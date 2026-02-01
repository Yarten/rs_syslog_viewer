//! 从尾部读取文件，会打开两个异步流程，一个从尾部往前读取，一个从尾部
//! 持续监听新行的增加，往尾部发送新行。

use crate::file::{
  Event, Reader,
  reader::{Config, ReadDirection, State},
  watcher::{ChangedEvent, MetadataEvent},
};
use anyhow::Result;
use std::{os::fd::AsRawFd, path::Path};
use tokio::{fs::File, sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// 从尾部开始读取内容的文件读取器
pub struct TailReader {
  config: Config,

  /// 文件对象（保持打开状态，确保 fd 不变更）
  file: File,

  /// 往头部读取内容的状态
  head_state: State,

  /// 往尾部读取内容的状态
  tail_state: State,

  /// 用于控制读取取消的 token
  cancel_token: CancellationToken,

  /// 用于收发事件的通道
  tx: mpsc::Sender<Event>,
  rx: mpsc::Receiver<Event>,

  /// 向前读取的 join handler
  jh_reading_head: Option<JoinHandle<()>>,

  /// 向后读取的 join handler
  jh_reading_tail: Option<JoinHandle<()>>,
}

impl Reader for TailReader {
  async fn open(path: &Path, config: Config) -> Result<Self> {
    // 打开文件，并一直保证它打开，从而使 fd 不会回收，
    // 无论文件如何重命名，我们都能找到它
    let file = File::open(path).await?;
    let fd = file.as_raw_fd();

    // 创建通信通道
    let (tx, rx) = mpsc::channel::<Event>(config.channel_size);

    // 初始化用于读取的状态
    let head_state = State::new_tail(path, fd, config.buffer_size, tx.clone()).await?;
    let tail_state = State::new_tail(path, fd, config.buffer_size, tx.clone()).await?;

    // 返回文件读取器
    Ok(TailReader {
      config,
      file,
      head_state,
      tail_state,
      cancel_token: CancellationToken::new(),
      tx,
      rx,
      jh_reading_head: None,
      jh_reading_tail: None,
    })
  }

  async fn start(&mut self) -> Result<()> {
    // 初始化读取尾部一些内容
    self.init_read().await?;

    // 开始往头部方向读取
    self.jh_reading_head = Some(self.spawn_reading_head());

    // 开始往尾部方向读取
    self.jh_reading_tail = Some(self.spawn_watching_change());

    Ok(())
  }

  async fn stop(&mut self) -> Result<()> {
    self.cancel_token.cancel();
    if let Some(jh) = self.jh_reading_head.take() {
      jh.await?;
    }
    if let Some(jt) = self.jh_reading_tail.take() {
      jt.await?;
    }
    Ok(())
  }

  async fn changed(&mut self) -> Option<Event> {
    self.rx.recv().await
  }
}

impl TailReader {
  /// 首次读取，从尾部读取一个缓冲区大小的内容，尝试找出潜在的不完整尾行，放入尾部不完整缓冲区中。
  async fn init_read(&mut self) -> Result<()> {
    // 用于读取的缓存
    let mut buffer = vec![0; self.config.buffer_size as usize];

    // 从尾部读取一部分内容（首次读取），tail state 和 head state 已经在构建时设置好了初始读取的位置。
    // 这里将分成三个部分，头部属于 head state 的一部分，中间可以发送追加，尾部属于 tail state 的一部分。
    // head state 和 tail state 的一部分，将在未来向前、向后读取新内容时，和新内容拼接在一起
    if let Some(parts) =
      Self::read_buffer(&mut buffer, &mut self.tail_state, ReadDirection::Tail).await?
    {
      if let Some(tail_part) = parts.tail {
        // 处理头部
        Self::update_head_line(&mut self.head_state, parts.head).await?;

        // 中间部分，都是完整行，我们将其发射出去
        for line_buffer in parts.middle {
          self.tail_state.send_tail_for(line_buffer).await?;
        }

        // 处理尾部
        Self::update_tail_line(&mut self.tail_state, tail_part, parts.tail_is_end).await?;
      } else {
        // 尾部不存在，说明我们只读到一行。
        // 如果已经到达头部首字符，那么说明这个行是完整的；否则，这个行不完整，加入头部方向搜索。
        // 我们不考虑它也是尾行，且尾行也不完整的情况，此处只能截断处理。
        Self::update_head_line(&mut self.head_state, parts.head).await?;
      }
    }

    Ok(())
  }

  fn spawn_reading_head(&mut self) -> JoinHandle<()> {
    // 取出头部方向的状态数据
    let mut state = State::default();
    std::mem::swap(&mut state, &mut self.head_state);

    // 准备 cancel token
    let cancel_token = self.cancel_token.clone();

    // 导出 config
    let config = self.config.clone();

    // 启动新协程，对头部方向的内容进行读取
    tokio::task::spawn(async move {
      // 用于读取的缓存
      let mut buffer = vec![0; config.buffer_size as usize];

      // 一直读取，直至到达文件头部
      while !state.has_reached_head() && !cancel_token.is_cancelled() {
        if let Err(e) = Self::read_head_lines(&mut buffer, &mut state).await {
          eprintln!("Error while reading head lines: {e}");
          break;
        }
      }
    })
  }

  fn spawn_watching_change(&mut self) -> JoinHandle<()> {
    // 取出尾部方向的状态数据
    let mut state = State::default();
    std::mem::swap(&mut state, &mut self.tail_state);

    // 准备 cancel token
    let cancel_token = self.cancel_token.clone();

    // 导出 config
    let config = self.config.clone();

    // 取出事件发送通道，用于发送 metadata 变化事件
    let tx = self.tx.clone();

    // 启动新协程，监控文件变化
    tokio::spawn(async move {
      // 创建文件系统监视器
      let mut watcher = match state.watcher(config.poll_interval) {
        Ok(w) => w,
        Err(e) => {
          eprintln!("Failed to watch watcher: {e}");
          return;
        }
      };

      // 用于读取的缓存
      let mut buffer = vec![0; config.buffer_size as usize];

      // 循环监听
      'watch_loop: loop {
        tokio::select! {
          // 外部的取消信号
          _ = cancel_token.cancelled() => { break 'watch_loop; },

          // 监控文件变化
          res = watcher.changed() => match res {
            // 文件元数据变更，可能是重命名或者被删除。如果被删除，则结束监听
            Ok(ChangedEvent::Metadata(event)) => {
              if (event.send(&tx).await) {
                break 'watch_loop;
              }
            },

            // 文件变更，对于我们从尾部读取的情况来说，就是尾部新增了内容
            Ok(ChangedEvent::Content) => {
              if let Err(e) = Self::read_tail_lines(&mut buffer, &mut state).await {
                eprintln!("Error while reading tail lines: {e}");
                break 'watch_loop;
              }
            },

            // 出现错误则报错退出
            Err(e) => {
              eprintln!("Failed to watch watcher: {e}");
              break 'watch_loop;
            }
          }
        }
      }

      // 确保向前读取内容的流程也停止
      cancel_token.cancel();
    })
  }
}
