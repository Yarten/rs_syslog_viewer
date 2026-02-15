//! 从头部正向读取文件内容，用于已有的、不会被更新的文件的读取

use crate::file::{
  Event, Reader,
  reader::{self, ReaderBase},
  reader::{Config, State},
  watcher::{ChangedEvent, MetadataEvent},
};
use anyhow::Result;
use std::{os::fd::AsRawFd, path::Path};
use tokio::{fs::File, sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// 从头部开始读取内容的文件读取器
pub struct HeadReader {
  config: Config,

  /// 文件对象（保持打开状态，确保 fd 不变更）
  file: File,

  /// 读取文件的状态
  state: State,

  /// 用于控制读取取消的 token
  cancel_token: CancellationToken,

  /// 用于收发事件的通道
  tx: mpsc::Sender<Event>,
  rx: mpsc::Receiver<Event>,

  /// 监听文件路径变化的 join handler
  jh_watching: Option<JoinHandle<()>>,

  /// 异步读取的 join handler
  jh_reading: Option<JoinHandle<()>>,
}

impl Reader for HeadReader {
  async fn open(path: &Path, config: Config) -> Result<Self> {
    // 打开文件，并一直保证它打开，从而使 fd 不会回收，
    // 无论文件如何重命名，我们都能找到它
    let file = File::open(path).await?;
    let fd = file.as_raw_fd();

    // 创建通信通道
    let (tx, rx) = mpsc::channel::<Event>(config.channel_size);

    // 初始化读取状态数据
    let state = State::new_head(path, fd, config.buffer_size, tx.clone()).await?;

    // 返回文件读取器
    Ok(HeadReader {
      config,
      file,
      state,
      cancel_token: CancellationToken::new(),
      tx,
      rx,
      jh_watching: None,
      jh_reading: None,
    })
  }
}

impl ReaderBase for HeadReader {
  async fn start(&mut self) -> Result<()> {
    self.jh_watching = Some(self.spawn_watching()?);
    self.jh_reading = Some(self.spawn_reading());
    Ok(())
  }

  async fn stop(&mut self) -> Result<()> {
    self.cancel_token.cancel();
    if let Some(jh) = self.jh_watching.take() {
      jh.await?;
    }
    if let Some(jh) = self.jh_reading.take() {
      jh.await?;
    }
    Ok(())
  }

  async fn changed(&mut self) -> Option<Vec<Event>> {
    reader::poll_events(&mut self.rx, self.config.recv_buffer_size).await
  }
}

impl HeadReader {
  /// 开启一个对文件元数据的异步监听
  fn spawn_watching(&mut self) -> Result<JoinHandle<()>> {
    // 导出 config
    let config = self.config.clone();

    // 创建文件系统监视器，监控重命名或删除事件，忽略变更事件
    let mut watcher = self.state.watcher(config.poll_interval)?;

    // 准备 cancel token
    let cancel_token = self.cancel_token.clone();

    // 取出事件发送通道，用于发送 metadata 变化事件
    let tx = self.tx.clone();

    Ok(tokio::spawn(async move {
      'watch_loop: loop {
        tokio::select! {
          // 外部的取消信号
          _ = cancel_token.cancelled() => { break 'watch_loop; },

          // 监控文件的路径变化
          res = watcher.changed() => match res {
            Err(e) => {
              eprintln!("watcher error: {:?}", e);
              break 'watch_loop;
            },
            Ok(ChangedEvent::Metadata(event)) => {
              if (event.send(&tx).await) {
                  break 'watch_loop;
              }
            },
            _ => {}
          },
        }
      }
    }))
  }

  fn spawn_reading(&mut self) -> JoinHandle<()> {
    // 取出状态数据
    let mut state = State::default();
    std::mem::swap(&mut state, &mut self.state);

    // 准备 cancel token
    let cancel_token = self.cancel_token.clone();

    // 导出 config
    let config = self.config.clone();

    // 启动新协程，一直读取文件，直到读取不到内容
    tokio::spawn(async move {
      // 用于读取的缓存
      let mut buffer = vec![0; config.buffer_size as usize];

      // 一直读取到文件结尾
      while !cancel_token.is_cancelled() {
        // 记录上一次读取的位置
        let last_position = state.position();

        if let Err(e) = reader::read_tail_lines(&mut buffer, &mut state).await {
          eprintln!("Error while reading tail lines: {e}");
          break;
        }

        // 没有读取到新内容，则退出
        if state.position() == last_position {
          break;
        }
      }
    })
  }
}
