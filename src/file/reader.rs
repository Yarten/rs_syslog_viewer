//! 读取文件，支持从头部开始读，也支持从尾部向头部读，并持续追踪最新的内容

use crate::file::{
  Event,
  watcher::{MetadataEvent, Watcher},
};
use ::anyhow::{Result, anyhow};
use std::{
  io::SeekFrom,
  os::fd::RawFd,
  path::{Path, PathBuf},
  time::Duration,
};
use tokio::{
  fs::File,
  io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt},
  sync::mpsc,
};

/// 读取文件所需的配置
#[derive(Clone)]
pub struct Config {
  pub buffer_size: u64,
  pub poll_interval: Duration,
  pub channel_size: usize,
}

impl Default for Config {
  fn default() -> Self {
    Config {
      buffer_size: 4096,
      poll_interval: Duration::from_millis(100),
      channel_size: 2000,
    }
  }
}

/// 读取过程中所需要的状态管理数据
#[derive(Default)]
pub struct State {
  // 上一次读取的文件偏移位置
  last_position: u64,

  // 上一次读取可能没有完整的行内容
  partial_buffer: Vec<u8>,

  // 文件的原始路径
  raw_path: PathBuf,

  // 文件的 fd 路径，我们监控这个路径，无论文件名如何重命名，它都不变
  fd_path: PathBuf,

  // 文件流
  file: Option<File>,

  // 发送行的通道
  tx: Option<mpsc::Sender<Event>>,
}

impl State {
  pub async fn new_head(
    path: &Path,
    fd: RawFd,
    buffer_size: u64,
    tx: mpsc::Sender<Event>,
  ) -> Result<Self> {
    // 基于给定的 fd 打开文件，这是 FileReader 先打开、并且一直持有的 fd，无论向前、向后读取，都使用该 fd，
    // 保证它们读到的同一份文件
    let fd_path = PathBuf::from(format!("/proc/self/fd/{}", fd));

    // 为本监控打开专用的文件流
    let file = File::open(&fd_path).await?;

    // 返回处于头部的状态数据
    Ok(Self {
      last_position: 0,
      partial_buffer: Vec::with_capacity(buffer_size as usize),
      raw_path: path.into(),
      fd_path,
      file: Some(file),
      tx: Some(tx),
    })
  }

  pub async fn new_tail(
    path: &Path,
    fd: RawFd,
    buffer_size: u64,
    tx: mpsc::Sender<Event>,
  ) -> Result<Self> {
    let mut new_state = Self::new_head(path, fd, buffer_size, tx).await?;

    // 从尾部往前跳一段距离，我们从这里开始向前、向后读取
    if let Some(file) = &mut new_state.file {
      let metadata = file.metadata().await?;
      new_state.last_position = metadata.len().saturating_sub(buffer_size);
      file.seek(SeekFrom::Start(new_state.last_position)).await?;
    }

    // 返回已经处于尾部的状态数据
    Ok(new_state)
  }

  /// 当前是否已经读取到了头部
  pub fn has_reached_head(&self) -> bool {
    self.last_position == 0
  }

  /// 返回当前文件的位置
  pub fn position(&self) -> u64 {
    self.last_position
  }

  /// 追加新内容到暂存区的头部，过长的部分将被截断
  pub fn save_head_partial(&mut self, buffer: &[u8]) {
    let curr_len = self.partial_buffer.len();
    let size = buffer.len().min(self.partial_buffer.capacity());
    let curr_len = (self.partial_buffer.capacity() - size).min(curr_len);
    self.partial_buffer.resize(curr_len + size, 0);
    self.partial_buffer.copy_within(..curr_len, size);
    self.partial_buffer[..size].copy_from_slice(&buffer[..size]);
  }

  /// 追加新内容到暂存区的尾部，多余部分将被截断
  pub fn save_tail_partial(&mut self, buffer: &[u8]) {
    let curr_len = self.partial_buffer.len();
    let size = self.partial_buffer.capacity() - curr_len;
    let size = size.min(buffer.len());
    self.partial_buffer.resize(curr_len + size, 0);
    self.partial_buffer[curr_len..].copy_from_slice(&buffer[..size]);
  }

  pub async fn send_head(&mut self) -> Result<()> {
    self.send_head_for(&self.partial_buffer).await?;
    self.partial_buffer.clear();
    Ok(())
  }

  pub async fn send_head_for(&self, buffer: &[u8]) -> Result<()> {
    if let Some(tx) = &self.tx {
      Event::send_head(tx, buffer).await?;
    }

    Ok(())
  }

  pub async fn send_tail(&mut self) -> Result<()> {
    self.send_tail_for(&self.partial_buffer).await?;
    self.partial_buffer.clear();
    Ok(())
  }

  pub async fn send_tail_for(&self, buffer: &[u8]) -> Result<()> {
    if let Some(tx) = &self.tx {
      Event::send_tail(tx, buffer).await?;
    }

    Ok(())
  }

  pub fn watcher(&self, poll_interval: Duration) -> Result<Watcher> {
    Watcher::new(&self.raw_path, &self.fd_path, poll_interval)
  }
}

/// 从文件中读取的缓冲区，基于换行符分割成前中后三个部分
pub struct BufferParts<'a> {
  pub head: Option<&'a [u8]>,
  pub middle: Vec<&'a [u8]>,
  pub tail: Option<&'a [u8]>,
  pub tail_is_end: bool,
}

/// 文件读取内容的方向
pub enum ReadDirection {
  Head,
  Tail,
}

/// 读取文件的接口定义
pub trait Reader: Sized {
  /// 给定需要被读取的文件路径，创建新的 reader
  async fn open(path: &Path, config: Config) -> Result<Self>;

  /// 开始读取文件
  async fn start(&mut self) -> Result<()>;

  /// 停止读取文件
  async fn stop(&mut self) -> Result<()>;

  /// 获取新的事件，包括新行添加、文件重命名，以及文件删除
  async fn changed(&mut self) -> Option<Event>;

  /// 往头部方向读取新的内容，将完整的行发送出去
  async fn read_head_lines(buffer: &mut Vec<u8>, state: &mut State) -> Result<()> {
    // 向前读取一部分内容
    // 这里分成三个部分，头部可能是不完整的一行，中间部分可以发送至头部插入，
    // 尾部和之前的缓存能连成完整的一行，第一个发送出去拼接
    if let Some(parts) = Self::read_buffer(buffer, state, ReadDirection::Head).await? {
      if let Some(tail_part) = parts.tail {
        // 若尾部存在，则将尾部和已有的缓存拼在一起，并发送为新的头行
        state.save_head_partial(tail_part);
        state.send_head().await?;

        // 遍历中间部分（得反向遍历，从后往前插入），这些都是完整行，我们将其发送出去
        for line_buffer in parts.middle.iter().rev() {
          state.send_head_for(line_buffer).await?;
        }
      }

      // 处理头部
      Self::update_head_line(state, parts.head).await?;
    }

    Ok(())
  }

  /// 往尾部方向读取新的内容，将完整的行发送出去
  async fn read_tail_lines(buffer: &mut Vec<u8>, state: &mut State) -> Result<()> {
    // 向后读取一部分内容，
    // 这里分成三个部分，头部和之前的缓存连在一起，组成完整的行。中间部分是完整的行。
    // 尾部如果包含换行符，则是完整的行，否则需要暂存起来，等待下次读取时拼接。
    // 我们不考虑只读到一行，仍然不完整的情况，此处只能截断处理（也就是直接发送出去）。
    if let Some(parts) = Self::read_buffer(buffer, state, ReadDirection::Tail).await? {
      if let Some(tail_part) = parts.tail {
        // 尾部存在，说明头部一定存在。作为完整行发送出去
        if let Some(head_part) = parts.head {
          state.save_tail_partial(head_part);
          state.send_tail().await?;
        }

        // 中间部分，都是完整行，我们将其发射出去
        for line_buffer in parts.middle {
          state.send_tail_for(line_buffer).await?;
        }

        // 处理尾部
        Self::update_tail_line(state, tail_part, parts.tail_is_end).await?;
      } else {
        // 尾部不存在，意味着我们只读到一行
        if let Some(head_part) = parts.head {
          // 作为尾部进行处理
          Self::update_tail_line(state, head_part, parts.tail_is_end).await?;
        }
      }
    }

    Ok(())
  }

  /// 处理给定方向的文件内容读取
  async fn read_buffer<'a>(
    buffer: &'a mut Vec<u8>,
    state: &mut State,
    read_direction: ReadDirection,
  ) -> Result<Option<BufferParts<'a>>> {
    let file = state.file.as_mut().unwrap();

    // 若往前读取，我们需要先将位置往前跳，准备好 buffer 大小
    if let ReadDirection::Head = read_direction {
      let current_position = state.last_position;

      // 已经头，则没有内容可以读了
      if current_position == 0 {
        return Ok(None);
      }

      // 向前跳一段距离
      state.last_position = current_position.saturating_sub(buffer.capacity() as u64);

      // 真正能读取的内容（如果接近头部，剩余的内容会不足一个 buffer 大小）
      let buffer_size = current_position - state.last_position;
      buffer.resize(buffer_size as usize, 0);

      // 文件流向前移动该距离
      file.seek(SeekFrom::Current(-(buffer_size as i64))).await?;
    }

    // 读取一段数据
    let bytes_read = file.read(buffer).await?;

    // 检查是否什么都没有读到
    if bytes_read == 0 {
      return Ok(None);
    }

    // 若往后读取，读取出内容后，我们要更新下位置
    match read_direction {
      ReadDirection::Head => {
        // 将文件指针往回拨，为下一次读取做好准备
        file.seek(SeekFrom::Current(-(bytes_read as i64))).await?;
      }
      ReadDirection::Tail => {
        // 更新尾部方向的位置。如果前后两次读取位置没变化，可以得知已经读取不到内容
        state.last_position += bytes_read as u64;
      }
    }

    // 将读取到的内容分成若干行，其中首行和尾行可能不完整
    let mut iter = buffer[..bytes_read].split(|&c| c == b'\n');
    let head_part = iter.next();
    let mut middle_part: Vec<_> = iter.collect();
    let tail_part = middle_part.pop();

    Ok(Some(BufferParts {
      head: head_part,
      middle: middle_part,
      tail: tail_part,
      tail_is_end: buffer[bytes_read - 1] == b'\n',
    }))
  }

  /// 处理读取 buffer 中的第一行，和已有的缓存拼接在一起，
  /// 如果当前位置已经到达 0，则说明该行已经完整，可以发送出去，从头部插入
  async fn update_head_line(head_state: &mut State, buffer: Option<&[u8]>) -> Result<()> {
    match buffer {
      Some(buffer) => {
        // 追加新内容
        head_state.save_head_partial(buffer);

        if head_state.last_position == 0 {
          // 已经读取到头部首字符了，那可以认为这个行是完整的了
          head_state.send_head().await?;
        }
      }
      None => {}
    }

    Ok(())
  }

  /// 处理读取 buffer 中的最后一行。
  /// 如果读取的最后一个字符是换行符，则将其作为完整行发送出去。否则暂存起来。
  async fn update_tail_line(
    tail_state: &mut State,
    buffer: &[u8],
    tail_is_end: bool,
  ) -> Result<()> {
    // 和已有的缓存拼接在一起
    tail_state.save_tail_partial(buffer);

    // 尾部是否完整，取决于读取的 buffer 最后一个字符是否为换行符。
    // 内容如果为空，说明这是全文最后的一个换行符，不构成完整的行
    if tail_is_end && !tail_state.partial_buffer.is_empty() {
      tail_state.send_tail().await?;
    }

    Ok(())
  }
}
