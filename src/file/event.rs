//! 文件事件定义，包括：
//! 1. 读取的新的一行字符串，新行可能从头部插入，也可以从尾部插入；
//! 2. 文件的重命名；
//! 3. 文件的删除。

use std::path::PathBuf;
use tokio::{io::Result, sync::mpsc};

#[derive(Debug)]
pub enum Event {
  NewHead(String),
  NewTail(String),
  Renamed(PathBuf),
  Removed,
}

impl Event {
  pub async fn send_head(tx: &mpsc::Sender<Event>, buffer: &[u8]) -> Result<()> {
    let line = Event::NewHead(String::from_utf8_lossy(buffer).to_string());
    if let Err(e) = tx.send(line).await {
      eprintln!("Failed to send head line: {}", e);
    }
    Ok(())
  }

  pub async fn send_tail(tx: &mpsc::Sender<Event>, buffer: &[u8]) -> Result<()> {
    let line = Event::NewTail(String::from_utf8_lossy(buffer).to_string());
    if let Err(e) = tx.send(line).await {
      eprintln!("Failed to send tail line: {}", e);
    }
    Ok(())
  }
}

