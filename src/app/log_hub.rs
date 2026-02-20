use crate::log::{Config, DataBoard, Index as LogIndex, LogLine, RotatedLog};
use std::path::PathBuf;
use std::{
  cmp::Ordering,
  collections::HashMap,
  ops::{Deref, DerefMut},
  sync::Arc,
};
use tokio::{
  sync::{Mutex, MutexGuard},
  task::{self, JoinHandle},
};
use tokio_util::sync::CancellationToken;

/// 所有日志文件的索引
#[derive(Clone)]
pub struct Index {
  indexes: Vec<LogIndex>,
}

/// 日志文件，支持内容的查找操作，以及标记操作，
pub struct LogHubRef<'a> {
  /// 所有的被跟踪的系统日志
  logs: &'a mut Vec<RotatedLog>,

  /// 数据看板，代表所有日志的统计数据，由所有日志更新时一同更新
  data_board: &'a mut DataBoard,
}

pub struct LogHub {
  /// 数据内容，其中的内容不是总有效。
  /// 在通过 `data()` 函数获取操作接口之前，它们都在异步的流程中刷新自己的状态
  logs: Vec<RotatedLog>,

  /// 数据黑板，统计了日志的全局信息，在日志内容变化时同步更新
  data_board: Arc<Mutex<DataBoard>>,

  /// 各个日志异步刷新的流程句柄
  log_handles: Vec<JoinHandle<(usize, RotatedLog)>>,

  /// 控制异步流程是否终止的 token
  stop_token: CancellationToken,
}

impl LogHub {
  /// 基于给定的系统日志存储根目录，以及已知的系统日志名称（文件名，不含后缀），
  /// 创建本对象
  pub fn open(root: PathBuf, names: HashMap<String, Config>) -> Self {
    // 创建各个系统日志对象，组成有序的数组，该顺序在整个进程内都不会再改变
    let logs: Vec<RotatedLog> = names
      .into_iter()
      .map(|(name, config)| RotatedLog::new(root.join(name + ".log"), config))
      .collect();

    // 创建本 hub 对象
    let mut hub = Self {
      logs,
      data_board: Arc::new(Mutex::new(DataBoard::new(root))),
      log_handles: Vec::new(),
      stop_token: CancellationToken::new(),
    };

    // 启动异步刷新流程
    hub.spawn_updating();

    // 返回该 hub 对象
    hub
  }

  /// 停止所有异步刷新活动
  pub async fn close(&mut self) {
    self.stop_updating().await;
  }

  /// 停止异步刷新活动，返回数据访问接口。
  /// 等该接口析构时，继续执行异步刷新活动
  pub async fn data(&'_ mut self) -> LogHubDataGuard<'_> {
    self.stop_updating().await;
    LogHubDataGuard::new(self).await
  }

  /// 将所有的系统日志发送到异步流程中，执行状态更新
  fn spawn_updating(&mut self) {
    // 取出日志对象们
    let logs = std::mem::take(&mut self.logs);

    // 新建 token
    self.stop_token = CancellationToken::new();

    // 创建带索引的异步任务
    self.log_handles = logs
      .into_iter()
      .enumerate()
      .map(|(index, log)| {
        task::spawn(Self::update(
          index,
          log,
          self.data_board.clone(),
          self.stop_token.clone(),
        ))
      })
      .collect();
  }

  /// 停止所有在异步执行的系统日志刷新流程
  async fn stop_updating(&mut self) {
    // 触发所有流程结束
    self.stop_token.cancel();

    // 取出句柄
    let handlers = std::mem::take(&mut self.log_handles);

    // 收集并排序结果
    let mut results: Vec<(usize, RotatedLog)> = futures::future::join_all(handlers)
      .await
      .into_iter()
      .map(|handle| handle.expect("task panicked"))
      .collect();
    results.sort_by_key(|&(index, _)| index);

    // 将日志对象放回本类
    self.logs = results.into_iter().map(|(_, log)| log).collect();
  }

  /// 异步刷新某个系统日志的流程
  async fn update(
    index: usize,
    mut log: RotatedLog,
    data_board: Arc<Mutex<DataBoard>>,
    stop_token: CancellationToken,
  ) -> (usize, RotatedLog) {
    if log.prepare().await {
      loop {
        tokio::select! {
          _ = stop_token.cancelled() => break,
          _ = log.update(data_board.clone()) => {}
        }
      }
    }

    (index, log)
  }
}

/// 导出日志数据操作器，在声明周期结束时，自动开始异步的更新流程
pub struct LogHubDataGuard<'a> {
  hub: &'a mut LogHub,
  data: LogHubRef<'a>,
  _data_board_guard: MutexGuard<'a, DataBoard>,
}

impl<'a> LogHubDataGuard<'a> {
  async fn new(hub: &'a mut LogHub) -> Self {
    let mut data_board_guard = crate::unsafe_ref!(LogHub, hub).data_board.lock().await;
    let data_board = crate::unsafe_ref!(DataBoard, data_board_guard.deref_mut(), mut);
    let logs = &mut crate::unsafe_ref!(LogHub, hub, mut).logs;

    Self {
      hub,
      data: LogHubRef { logs, data_board },
      _data_board_guard: data_board_guard,
    }
  }
}

impl<'a> Deref for LogHubDataGuard<'a> {
  type Target = LogHubRef<'a>;

  fn deref(&self) -> &Self::Target {
    &self.data
  }
}

impl<'a> DerefMut for LogHubDataGuard<'a> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.data
  }
}

impl Drop for LogHubDataGuard<'_> {
  fn drop(&mut self) {
    self.hub.spawn_updating();
  }
}

/// 在所有日志中（包括所有的日志组，以及所有滚动文件），准确地关联其中一行日志
/// 的索引及其数据。
pub type LogItem<'a> = (Index, &'a mut LogLine);

/// 遍历处理所有日志文件，按时间顺序或逆序地逐一取出日志行
pub struct Iter<'a, I, F>
where
  I: Iterator<Item = (LogIndex, &'a mut LogLine)>,
  F: Fn(&LogLine, &LogLine) -> Ordering,
{
  iters: Vec<(I, Option<(LogIndex, &'a mut LogLine)>)>,
  cmp: F,
}

impl<'a, I, F> Iterator for Iter<'a, I, F>
where
  I: Iterator<Item = (LogIndex, &'a mut LogLine)>,
  F: Fn(&LogLine, &LogLine) -> Ordering,
{
  type Item = LogItem<'a>;

  fn next(&mut self) -> Option<Self::Item> {
    // 所有日志的索引向量
    let mut index = Index {
      indexes: Vec::with_capacity(self.iters.len()),
    };

    // 记录极值元素
    let mut min_elem: Option<(usize, &'a mut LogLine)> = None;

    // 找到所有日志中的极值
    for (nth, (i, elem)) in self.iters.iter_mut().enumerate() {
      if elem.is_none() {
        *elem = i.next();
      }

      if let Some((idx, log)) = elem {
        index.indexes.push(*idx);

        if match &min_elem {
          None => true,
          Some((_, min_log)) => (self.cmp)(log, min_log) == Ordering::Less,
        } {
          let log = crate::unsafe_ref!(LogLine, *log, mut);
          min_elem = Some((nth, log))
        }
      } else {
        index.indexes.push(LogIndex::zero());
      }
    }

    // 若找到了极值，则需要将其取到的数据记录清掉，以便于下一个周期取新的进行比较
    if let Some((nth, min_log)) = min_elem {
      self.iters[nth].1 = None;
      Some((index, min_log))
    } else {
      None
    }
  }
}

impl<'a> LogHubRef<'a> {
  pub fn get(&'_ mut self, index: Index) -> Option<&'_ mut LogLine> {
    self
      .iter_forward_from(index)
      .next()
      .and_then(|(_, log)| Some(log))
  }

  /// 获取从指定索引处，开始正向遍历的迭代器
  pub fn iter_forward_from(&'_ mut self, index: Index) -> impl Iterator<Item = LogItem<'_>> {
    let tags_ref = self.data_board.get_tags();

    Iter {
      iters: index
        .indexes
        .iter()
        .zip(self.logs.iter_mut())
        .map(|(idx, log)| (log.filtered_iter_forward_from(tags_ref, *idx), None))
        .collect(),
      cmp: LogLine::is_older,
    }
  }

  /// 获取从指定索引处，开始逆向遍历的迭代器
  pub fn iter_backward_from(&'_ mut self, index: Index) -> impl Iterator<Item = LogItem<'_>> {
    let tags_ref = self.data_board.get_tags();

    Iter {
      iters: index
        .indexes
        .iter()
        .zip(self.logs.iter_mut())
        .map(|(idx, log)| (log.filtered_iter_backward_from(tags_ref, *idx), None))
        .collect(),
      cmp: LogLine::is_newer,
    }
  }

  /// 获取从第一条日志开始正向遍历的迭代器
  pub fn iter_forward_from_head(&'_ mut self) -> impl Iterator<Item = LogItem<'_>> {
    self.iter_forward_from(self.first_index())
  }

  /// 获取从最后一条日志开始逆向遍历的迭代器
  pub fn iter_backward_from_tail(&'_ mut self) -> impl Iterator<Item = LogItem<'_>> {
    self.iter_backward_from(self.last_index())
  }

  /// 获取指定索引处的迭代器，同时返回（正向，逆向）两种
  pub fn iter_at(
    &'_ mut self,
    index: Index,
  ) -> (
    impl Iterator<Item = LogItem<'_>>,
    impl Iterator<Item = LogItem<'_>>,
  ) {
    let forward_iter = crate::unsafe_ref!(LogHubRef, self, mut).iter_forward_from(index.clone());
    let backward_iter = crate::unsafe_ref!(LogHubRef, self, mut).iter_backward_from(index);
    (forward_iter, backward_iter)
  }

  /// 获取指向首条日志的索引
  pub fn first_index(&self) -> Index {
    Index {
      indexes: self.logs.iter().map(|log| log.first_index()).collect(),
    }
  }

  /// 获取最新的日志索引（也即最后一个日志的索引）
  pub fn last_index(&self) -> Index {
    Index {
      indexes: self.logs.iter().map(|log| log.last_index()).collect(),
    }
  }

  /// 尝试加载更旧的日志。将会从给定的日志索引中，找到已经顶到头的那些，
  /// 要求它们进行加载。
  pub fn try_load_older_logs(&mut self, index: Index) {
    index
      .indexes
      .iter()
      .zip(self.logs.iter_mut())
      .for_each(|(idx, log)| {
        if idx == &log.first_index() {
          log.set_want_older_log();
        }
      });
  }

  /// 获取日志数据看板
  pub fn data_board(&mut self) -> &mut DataBoard {
    self.data_board
  }
}
