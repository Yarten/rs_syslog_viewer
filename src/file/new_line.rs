//! 读取的新的一行字符串，新行可能从头部插入，也可以从尾部插入。

use tokio::{
    sync::{mpsc},
    io::{Result},
};

#[derive(Debug)]
pub enum NewLine {
    Head(String),
    Tail(String),
}

impl NewLine {
    pub async fn send_head(tx: &mpsc::Sender<NewLine>, buffer: &[u8]) -> Result<()> {
        let line = NewLine::Head(String::from_utf8_lossy(buffer).to_string());
        if let Err(e) = tx.send(line).await {
            eprintln!("Failed to send head line: {}", e);
        }
        Ok(())
    }

    pub async fn send_tail(tx: &mpsc::Sender<NewLine>, buffer: &[u8]) -> Result<()> {
        let line = NewLine::Tail(String::from_utf8_lossy(buffer).to_string());
        if let Err(e) = tx.send(line).await {
            eprintln!("Failed to send tail line: {}", e);
        }
        Ok(())
    }
}
