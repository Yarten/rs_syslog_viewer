use super::log_file_content::{BackwardIter, ForwardIter, Index, LogFileContent};
use crate::file::{
  Event, HeadReader, TailReader,
  reader::{self, Reader, ReaderBase},
};
use crate::log::{DataBoard, Event as LogEvent, LogLine};
use anyhow::Result;
use enum_dispatch::enum_dispatch;
use std::{
  collections::HashSet,
  path::{Path, PathBuf},
  sync::Arc,
};

/// 不同类型的 reader
#[enum_dispatch(ReaderBase)]
enum AnyReader {
  Head(HeadReader),
  Tail(TailReader),
}

/// 维护一份日志文件的内容读取、名称变更与删除，同时提供一些只读查询接口
pub struct LogFile {
  /// 日志的文件路径
  path: PathBuf,

  /// 日志内容
  content: LogFileContent,

  /// 文件内容读取器
  reader: AnyReader,
}

impl LogFile {
  /// 打开指定文件，并开始异步读取内容，监听它的变化。
  ///
  /// 使用 `latest` 参数指明该文件是否是最新的、正在被系统更新的文件，是我们将持续追踪它的最新内容,
  /// 否则一次性读完内容后，就会自动结束异步读取流程。
  ///
  /// `tags` 参数是之前历史上已经查询出来的一些标签记录，在打开新日志时，它可以用于去重。
  pub async fn open(path: PathBuf, latest: bool) -> Result<LogFile> {
    let config = reader::Config::default();
    let mut reader = if latest {
      AnyReader::Tail(TailReader::open(&path, config).await?)
    } else {
      AnyReader::Head(HeadReader::open(&path, config).await?)
    };

    reader.start().await?;

    Ok(LogFile {
      path,
      content: LogFileContent::default(),
      reader,
    })
  }

  /// 处理一次文件内容的变更检查与处理
  ///
  /// # Cancel Safety
  /// 本函数保证，当 await 被取消时，没有副作用。
  pub async fn update(&mut self, data_board: Arc<DataBoard>) -> Option<Vec<LogEvent>> {
    if let Some(events) = self.reader.changed().await {
      // 处理多个日志底层事件，消化掉内容新增事件，并向数据看板更新可能的新增标签，
      // 消化掉更名事件，
      // 如果是删除事件，则直接向调用者透传。
      events
        .into_iter()
        .filter_map(|event| match event {
          Event::NewHead(s) => {
            let new_log = LogLine::new(s);
            self.update_tag(&new_log, &data_board);
            self.content.push_front(new_log);
            None
          }
          Event::NewTail(s) => {
            let new_log = LogLine::new(s);
            self.update_tag(&new_log, &data_board);
            self.content.push_back(new_log);
            None
          }
          Event::Renamed(new_path) => {
            self.path = new_path;
            None
          }
          Event::Removed => Some(LogEvent::Removed),
        })
        .collect::<Vec<LogEvent>>()
        .into()
    } else {
      // 无法读到新的变更，代表本阅读器已经出错
      None
    }
  }

  /// 关闭本日志的异步监听流程
  ///
  /// # Cancel Safety
  /// 本函数保证 await 被终止时，没有副作用，异步的流程仍然会在后台完全结束。
  pub async fn close(&mut self) -> Result<()> {
    Ok(self.reader.stop().await?)
  }

  pub fn data(&self) -> &LogFileContent {
    &self.content
  }

  pub fn data_mut(&mut self) -> &mut LogFileContent {
    &mut self.content
  }

  pub fn path(&self) -> &PathBuf {
    &self.path
  }

  /// 检查给定的新日志行，是否带来了新的、未知的日志标签
  fn update_tag(&mut self, log: &LogLine, data_board: &DataBoard) {
    if let LogLine::Good(log) = log {
      data_board.update_tag(&log.tag);
    }
  }
}
