use crate::log::{Config, DataBoard, Index as LogIndex, LogDirection, LogLine, RotatedLog};
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
  /// 各个文件此时标记的日志索引
  indexes: Vec<LogIndex>,

  /// 当前选择的是具体哪一份文件的日志。
  /// 同一组索引由于遍历方向的不同，会导致指向的数据不同，因此需要
  /// 本字段进行精确地指示。
  selection: usize,
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
  /// 存储了：
  /// 1. 最后一次有效结果的索引，使用出发时的索引进行初始化，迭代过程中逐渐更新；
  /// 2. 迭代器；
  /// 3. 上一次迭代器取出、但未被采纳的结果
  iters: Vec<(LogIndex, I, Option<(LogIndex, &'a mut LogLine)>)>,

  /// 比较两个日志，若返回 Less，表示左边日志优先取用
  cmp: F,

  /// 遍历开始时的日志选择。第一次遍历，需要将迭代器一直跳转到指定选择上。
  /// 若本字段数值大于 iters 的元素数量，则不处理。命中第一次后，它的值被设置为一个足够大的值。
  init_selection: usize,

  /// 遍历的方向，会影响遍历各个文件的顺序
  direction: LogDirection,
}

impl<'a, I, F> Iter<'a, I, F>
where
  I: Iterator<Item = (LogIndex, &'a mut LogLine)>,
  F: Fn(&LogLine, &LogLine) -> Ordering,
{
  fn next_one(&mut self) -> Option<LogItem<'a>> {
    // TODO: 1. Index 补充 selection；2. 初次遍历需要找到它；3. forward，backward iters 迭代方向相反

    let (index, min_elem) = match self.direction {
      LogDirection::Forward => Self::find_extremum(&self.cmp, self.iters.iter_mut().enumerate()),
      LogDirection::Backward => {
        Self::find_extremum(&self.cmp, self.iters.iter_mut().enumerate().rev())
      }
    };

    // 若找到了极值，则需要将其取到的数据记录清掉，以便于下一个周期取新的进行比较。
    if let Some((nth, min_log)) = min_elem {
      // 清理的同时，还需要记录最后一次有效的索引
      let iter = &mut self.iters[nth];
      if let Some((last_index, _)) = iter.2.take() {
        iter.0 = last_index;
      }

      // 返回本次迭代的结果
      Some((index, min_log))
    } else {
      None
    }
  }

  /// 给定各个日志的迭代器的迭代器，寻找这一次迭代的极值
  fn find_extremum<'b, J>(cmp: &F, iters: J) -> (Index, Option<(usize, &'a mut LogLine)>)
  where
    J: Iterator<
        Item = (
          usize,
          &'b mut (LogIndex, I, Option<(LogIndex, &'a mut LogLine)>),
        ),
      > + ExactSizeIterator,
    I: 'b,
    'a: 'b,
  {
    // 所有日志的索引向量
    let mut index = Index {
      indexes: vec![LogIndex::zero(); iters.len()],
      selection: usize::MAX,
    };

    // 记录极值元素
    let mut min_elem: Option<(usize, &'a mut LogLine)> = None;

    // 找到所有日志中的极值
    for (nth, (last_index, i, elem)) in iters {
      if elem.is_none() {
        *elem = i.next();
      }

      if let Some((idx, log)) = elem {
        index.indexes[nth] = *idx;

        if match &min_elem {
          None => true,
          Some((_, min_log)) => (cmp)(log, min_log) == Ordering::Less,
        } {
          let log = crate::unsafe_ref!(LogLine, *log, mut);
          min_elem = Some((nth, log));
          index.selection = nth;
        }
      } else {
        // 该日志的迭代器取不出内容时，使用上一次有效的索引来代表它的索引
        index.indexes[nth] = *last_index;
      }
    }

    // 返回结果
    (index, min_elem)
  }
}

impl<'a, I, F> Iterator for Iter<'a, I, F>
where
  I: Iterator<Item = (LogIndex, &'a mut LogLine)>,
  F: Fn(&LogLine, &LogLine) -> Ordering,
{
  type Item = LogItem<'a>;

  fn next(&mut self) -> Option<Self::Item> {
    loop {
      return match self.next_one() {
        None => None,
        Some(item) => {
          if self.init_selection < self.iters.len() {
            if item.0.selection != self.init_selection {
              continue;
            }
            self.init_selection = self.iters.len();
          }
          Some(item)
        }
      };
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
        .into_iter()
        .zip(self.logs.iter_mut())
        .map(|(idx, log)| (idx, log.filtered_iter_forward_from(tags_ref, idx), None))
        .collect(),
      cmp: LogLine::is_older,
      init_selection: index.selection,
      direction: LogDirection::Forward,
    }
  }

  /// 获取从指定索引处，开始逆向遍历的迭代器
  pub fn iter_backward_from(&'_ mut self, index: Index) -> impl Iterator<Item = LogItem<'_>> {
    let tags_ref = self.data_board.get_tags();

    Iter {
      iters: index
        .indexes
        .into_iter()
        .zip(self.logs.iter_mut())
        .map(|(idx, log)| (idx, log.filtered_iter_backward_from(tags_ref, idx), None))
        .collect(),
      cmp: LogLine::is_newer,
      init_selection: index.selection,
      direction: LogDirection::Backward,
    }
  }

  /// 获取从第一条日志开始正向遍历的迭代器
  pub fn iter_forward_from_head(&'_ mut self) -> impl Iterator<Item = LogItem<'_>> {
    let index = self.first_index();
    self.iter_forward_from(index)
  }

  /// 获取从最后一条日志开始逆向遍历的迭代器
  pub fn iter_backward_from_tail(&'_ mut self) -> impl Iterator<Item = LogItem<'_>> {
    let index = self.last_index();
    self.iter_backward_from(index)
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
  pub fn first_index(&mut self) -> Index {
    let index = Index {
      indexes: self.logs.iter().map(|log| log.first_index()).collect(),
      selection: usize::MAX,
    };

    // 只有迭代过一次后，才能正确地找到第一条日志的 selection
    match self.iter_forward_from(index.clone()).next() {
      None => index,
      Some((index, _)) => index,
    }
  }

  /// 获取最新的日志索引（也即最后一个日志的索引）
  pub fn last_index(&mut self) -> Index {
    let index = Index {
      indexes: self.logs.iter().map(|log| log.last_index()).collect(),
      selection: usize::MAX,
    };

    // 只有迭代过一次后，才能正确地找到第一条日志的 selection
    match self.iter_backward_from(index.clone()).next() {
      None => index,
      Some((index, _)) => index,
    }
  }

  /// 尝试加载更旧的日志。将会从给定的日志索引中，找到已经顶到头的那些，
  /// 要求它们进行加载。
  pub fn try_load_older_logs(&mut self, index: &Index) {
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
