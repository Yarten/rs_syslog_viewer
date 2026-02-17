use crate::log::{Config, DataBoard, Index as LogIndex, LogLine, RotatedLog};
use std::ops::DerefMut;
use std::{cmp::Ordering, collections::HashMap, ops::Deref, path::Path, sync::Arc};
use tokio::task::{self, JoinHandle};
use tokio_util::sync::CancellationToken;

/// 所有日志文件的索引
pub struct Index {
  indexes: Vec<LogIndex>,
}

/// 若有的日志文件，支持内容的查找操作，以及标记操作，
pub struct LogHubData {
  /// 所有的被跟踪的系统日志
  logs: Vec<RotatedLog>,

  /// 数据看板，代表所有日志的统计数据，由所有日志更新时一同更新
  data_board: Arc<DataBoard>,
}

pub struct LogHub {
  /// 数据内容，其中的内容不是总有效。
  /// 在通过 `data()` 函数获取操作接口之前，它们都在异步的流程中刷新自己的状态
  logs_data: LogHubData,

  /// 各个日志异步刷新的流程句柄
  log_handles: Vec<JoinHandle<(usize, RotatedLog)>>,

  /// 控制异步流程是否终止的 token
  stop_token: CancellationToken,
}

impl LogHub {
  /// 基于给定的系统日志存储根目录，以及已知的系统日志名称（文件名，不含后缀），
  /// 创建本对象
  pub fn open(root: &Path, names: HashMap<String, Config>) -> Self {
    // 创建各个系统日志对象，组成有序的数组，该顺序在整个进程内都不会再改变
    let logs: Vec<RotatedLog> = names
      .into_iter()
      .map(|(name, config)| RotatedLog::new(root.join(name + ".log"), config))
      .collect();

    // 创建本 hub 对象
    let mut hub = Self {
      logs_data: LogHubData {
        logs,
        data_board: Arc::new(DataBoard::default()),
      },
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
    LogHubDataGuard { hub: self }
  }

  /// 将所有的系统日志发送到异步流程中，执行状态更新
  fn spawn_updating(&mut self) {
    // 取出日志对象们
    let logs = std::mem::take(&mut self.logs_data.logs);

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
          self.logs_data.data_board.clone(),
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
    self.logs_data.logs = results.into_iter().map(|(_, log)| log).collect();
  }

  /// 异步刷新某个系统日志的流程
  async fn update(
    index: usize,
    mut log: RotatedLog,
    data_board: Arc<DataBoard>,
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
}

impl<'a> Deref for LogHubDataGuard<'a> {
  type Target = LogHubData;

  fn deref(&self) -> &Self::Target {
    &self.hub.logs_data
  }
}

impl<'a> DerefMut for LogHubDataGuard<'a> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.hub.logs_data
  }
}

impl Drop for LogHubDataGuard<'_> {
  fn drop(&mut self) {
    self.hub.spawn_updating();
  }
}

/// 打破 rust 对被索引日志的引用生命周期检查
macro_rules! unsafe_log_ref {
    ($var:ident) => {$var};
    ($var:ident, mut) => {unsafe { &mut *(*$var as *mut LogLine) }}
}

/// 同时处理所有系统日志的向量迭代器
macro_rules! define_iterator {
  ($name:ident $(, $mut_flag:tt)?) => {
    pub struct $name<'a, I, F>
    where
      I: Iterator<Item = (LogIndex, &'a $($mut_flag)? LogLine)>,
      F: Fn(&LogLine, &LogLine) -> Ordering,
    {
      iters: Vec<(I, Option<(LogIndex, &'a $($mut_flag)? LogLine)>)>,
      cmp: F,
    }

    impl<'a, I, F> Iterator for $name<'a, I, F>
    where
      I: Iterator<Item = (LogIndex, &'a $($mut_flag)? LogLine)>,
      F: Fn(&LogLine, &LogLine) -> Ordering,
    {
      type Item = (Index, &'a $($mut_flag)? LogLine);

      fn next(&mut self) -> Option<Self::Item> {
        // 所有日志的索引向量
        let mut index = Index {
          indexes: Vec::with_capacity(self.iters.len()),
        };

        // 记录极值元素
        let mut min_elem: Option<(usize, &'a $($mut_flag)? LogLine)> = None;

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
              let log = unsafe_log_ref!(log $(, $mut_flag)?);
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
  };
}

define_iterator!(Iter);
define_iterator!(IterMut, mut);


impl LogHubData {
  pub fn get(&'_ self, index: Index) -> Option<&'_ LogLine> {
    self
      .iter_forward_from(index)
      .next()
      .and_then(|(_, log)| Some(log))
  }

  /// 获取从指定索引处，开始正向遍历的迭代器
  pub fn iter_forward_from(&'_ self, index: Index) -> impl Iterator<Item = (Index, &'_ LogLine)> {
    Iter {
      iters: index
        .indexes
        .iter()
        .zip(self.logs.iter())
        .map(|(idx, log)| (log.iter_forward_from(*idx), None))
        .collect(),
      cmp: LogLine::is_older,
    }
  }

  /// 获取从指定索引处，开始逆向遍历的迭代器
  pub fn iter_backward_from(&'_ self, index: Index) -> impl Iterator<Item = (Index, &'_ LogLine)> {
    Iter {
      iters: index
        .indexes
        .iter()
        .zip(self.logs.iter())
        .map(|(idx, log)| (log.iter_backward_from(*idx), None))
        .collect(),
      cmp: LogLine::is_newer,
    }
  }

  /// 获取从第一条日志开始正向遍历的迭代器
  pub fn iter_forward_from_head(&'_ self) -> impl Iterator<Item = (Index, &'_ LogLine)> {
    self.iter_forward_from(self.first_index())
  }

  /// 获取从最后一条日志开始逆向遍历的迭代器
  pub fn iter_backward_from_tail(&'_ self) -> impl Iterator<Item = (Index, &'_ LogLine)> {
    self.iter_backward_from(self.last_index())
  }

  /// 获取从指定索引处，开始正向遍历的可变迭代器
  pub fn iter_mut_forward_from(&'_ mut self, index: Index) -> impl Iterator<Item = (Index, &'_ mut LogLine)> {
    IterMut {
      iters: index
        .indexes
        .iter()
        .zip(self.logs.iter_mut())
        .map(|(idx, log)| (log.iter_mut_forward_from(*idx), None))
        .collect(),
      cmp: LogLine::is_older,
    }
  }

  /// 获取从指定索引处，开始逆向遍历的可变迭代器
  pub fn iter_mut_backward_from(&'_ mut self, index: Index) -> impl Iterator<Item = (Index, &'_ mut LogLine)> {
    IterMut {
      iters: index
        .indexes
        .iter()
        .zip(self.logs.iter_mut())
        .map(|(idx, log)| (log.iter_mut_backward_from(*idx), None))
        .collect(),
      cmp: LogLine::is_newer,
    }
  }

  /// 获取从第一条日志开始正向遍历的可变迭代器
  pub fn iter_mut_forward_from_head(&'_ mut self) -> impl Iterator<Item = (Index, &'_ mut LogLine)> {
    self.iter_mut_forward_from(self.first_index())
  }

  /// 获取从最后一条日志开始逆向遍历的可变迭代器
  pub fn iter_mut_backward_from_tail(&'_ mut self) -> impl Iterator<Item = (Index, &'_ mut LogLine)> {
    self.iter_mut_backward_from(self.last_index())
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
}
