use std::{
  collections::{HashSet, VecDeque},
  fs,
  path::PathBuf,
};
use crate::log::{
  LogFile, Event,
  log_file_content::{Index as LogFileContentIndex},
};


/// 索引某一个系统日志中的某一行
pub struct Index {
  /// `RotatedLog` 中的日志文件数组的索引
  file_index: usize,

  /// 指向了某一份 `LogFile` 后的其中一行的索引
  line_index: LogFileContentIndex
}

/// 维护一组由 syslog 滚动的系统日志，
/// 这些日志是按需加载的，但总是从最新的一份开始
pub struct RotatedLog {
  /// 日志文件路径，不带被回滚时的后缀别名，一般指向最新的、正在被更新的日志
  path: PathBuf,

  /// 有序的多份日志文件，序号越靠前
  log_files: VecDeque<LogFile>,

  /// 持续收集的日志标签，用于检查是否有新的，如果出现新的，向上一层报告
  tags: HashSet<String>,

  /// 期望加载上一个日志
  want_older_log: bool
}

impl RotatedLog {
  /// 创建新的一组系统日志文件维护实例，给定的 `path` 参数是未带回滚后缀的路径，
  /// 本类会自动在相同目录下，扫描它的被滚动的其他日志。
  fn new(path: PathBuf, possible_max_rotated_count: usize) -> Self {
    Self {
      path,
      log_files: VecDeque::with_capacity(possible_max_rotated_count),
      tags: HashSet::new(),
      want_older_log: false
    }
  }

  /// 一个轮询周期内，在检查各个日志文件内容变更前，加载新的日志文件、
  /// 或者按照需求，加载耿旧的日志文件。
  ///
  /// 返回是否需要加入内容变更轮询，如果本系统日志还没有加载任何文件，则不参与轮询。
  ///
  /// 加载日志文件过程中有很多 await 点，它们并不能保证取消安全。
  pub async fn prepare(&mut self) -> bool {
    // 加载最新的日志。如果已经加载，则无事发生
    let _ = self.maybe_load_latest_log().await;

    // 根据需求，加载旧一点的一份日志
    let _ = self.maybe_load_older_log().await;

    !self.log_files.is_empty()
  }

  /// 处理日志内容的变更、文件的滚动与删除
  pub async fn update(&mut self) -> Option<Event> {
    // select 所有日志文件的事件
    let async_fns: Vec<_> =
      self
        .log_files
        .iter_mut()
        .map(|log_file| {Box::pin(log_file.update())})
        .collect();

    // 处理其中一个，其余取消处理
    let (result, index, _) = futures::future::select_all(async_fns).await;

    match result {
      None => return None,
      Some(result) => match result {
        Event::Tick => {}

        // 处理文件的删除
        Event::Removed => {
          if let Some(mut log_file) = self.log_files.remove(index) {
            let _ = log_file.close().await;
          }
        }
        Event::NewTag(_) => {}
      }
    }

    None
  }

  /// 若当前还未加载最新的日志文件，也即系统正在更新的那一份（如 x.log），则尝试加载它。
  /// 如果根本不存在正在更新的日志，而我们的日志文件一份都没有加载，那就找到最新的一份滚动日志文件，进行加载
  async fn maybe_load_latest_log(&mut self) -> Option<()> {
    // 目前已经加载的最新日志文件的路径
    let loaded_latest_path = match self.log_files.back() {
      None => &PathBuf::new(),
      Some(log_file ) => log_file.path()
    };

    // 快速路径判断：如果最新的路径等于系统正在更新的路径，则结束处理
    if loaded_latest_path == &self.path {
      return None
    }

    // 找到目录下，最新的一份日志文件（在 x.log, x.log.1, x.log.2 中找）
    let latest_path = self.find_latest_log_path()?;

    // 如果最新的这一份文件已经被加载，则结束处理
    if loaded_latest_path == &latest_path {
      return None
    }

    // 加载最新的文件
    self.log_files.push_back(
      self.open_log_file(latest_path).await?
    );

    None
  }

  /// 找到本系统日志最新的那一份
  fn find_latest_log_path(&self) -> Option<PathBuf> {
    // 最新路径的记录
    let mut latest_path: Option<PathBuf> = None;

    // 找到字典序最小的路径，即为最新的文件
    // （x.log, x.log.1, x.log.2 中，x.log 比 x.log.1 新，x.log.1 比 x.log.2 新）
    self.visit_log_paths(
      |path: PathBuf| {
        Self::update_with_latest_path(&mut latest_path, path);
      }
    );

    // 返回可能找到的最新文件路径
    latest_path
  }

  /// 如果有需要，尝试加载更老一点的日志，这份日志仅比目前已经加载的日志再老一点
  async fn maybe_load_older_log(&mut self) -> Option<()> {
    // 判断是否有设置想要加载一份老日志的标志
    if !self.want_older_log {
      return None
    }
    self.want_older_log = false;

    // 找到目录下，稍微旧一点的一份日志
    let older_path = self.find_older_log_path()?;

    // 加载这一份日志文件
    self.log_files.push_front(
      self.open_log_file(older_path).await?
    );

    None
  }

  fn find_older_log_path(&self) -> Option<PathBuf> {
    // 找出目前已经加载的最老文件。如果找不到，则不往后处理
    let loaded_oldest_path = self.log_files.front()?.path();

    // 记录下一个旧一点的路径
    let mut next_older_path: Option<PathBuf> = None;

    // 找到比已经加载的最老文件更老，但又在这些更老的文件中最新的那一个
    self.visit_log_paths(
      |path: PathBuf| {
        if &path > loaded_oldest_path {
          Self::update_with_latest_path(&mut next_older_path, path);
        }
      }
    );

    next_older_path
  }

  /// 遍历属于本系统日志的那些具体的文件，也即 x.log, x.log.1, x.log.2 等
  fn visit_log_paths(&self, mut func: impl FnMut(PathBuf)) -> Option<()> {
    // 日志的名称
    let log_name = self.path.file_name()?.to_str()?;

    // 遍历本系统日志目录下的所有文件
    for entry in fs::read_dir(self.path.parent()?).ok()? {
      let entry = entry.ok()?;

      // 跳过文件的情况（很少命中这种情况）
      if !entry.file_type().ok()?.is_file() {
        continue
      }

      // 找到有本系统日志名称前缀的文件，它们就是和本系统日志相关的文件，接着处理它们
      if entry.file_name().to_str()?.starts_with(&log_name) {
        func(entry.path());
      }
    }

    Some(())
  }

  /// 比较记录中的最新路径，与一个新的路径，如果新的路径比记录中的路径更加新，
  /// 则拿它来更新到记录中。
  fn update_with_latest_path(curr_latest_path: &mut Option<PathBuf>, new_path: PathBuf) {
    match curr_latest_path {
      None => {
        *curr_latest_path = Some(new_path);
      }
      Some(curr_latest_path) => {
        if *curr_latest_path > new_path {
          *curr_latest_path = new_path;
        }
      }
    }
  }

  /// 打开指定路径的日志文件
  async fn open_log_file(&self, path: PathBuf) -> Option<LogFile> {
    println!("load log file {:?}", path);
    match LogFile::open(path, true, self.tags.clone()).await {
      Ok(log_file) => {
        Some(log_file)
      }
      Err(e) => {
        eprintln!("failed to load log file: {}", e);
        None
      }
    }
  }
}