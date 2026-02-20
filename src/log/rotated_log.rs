use crate::log::{
  DataBoard, Event, IterNextNth, LogDirection, LogFile, LogLine, LogLink, data_board::TagsData,
  log_file_content::Index as LogFileIndex,
};
use std::{collections::VecDeque, fs, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

/// 索引某一个系统日志中的某一行
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Index {
  /// `RotatedLog` 中的日志文件数组的索引
  file_index: usize,

  /// 指向了某一份 `LogFile` 后的其中一行的索引
  line_index: LogFileIndex,
}

impl Index {
  fn new(file_index: usize, line_index: LogFileIndex) -> Self {
    Self {
      file_index,
      line_index,
    }
  }

  pub fn zero() -> Self {
    Self::new(0, LogFileIndex::zero())
  }
}

/// 日志文件的配置
pub struct Config {
  possible_max_rotated_count: usize,
}

impl Config {
  pub fn default() -> Self {
    Self {
      possible_max_rotated_count: 5,
    }
  }
}

/// 维护一组由 syslog 滚动的系统日志，
/// 这些日志是按需加载的，但总是从最新的一份开始
pub struct RotatedLog {
  /// 日志文件路径，不带被回滚时的后缀别名，一般指向最新的、正在被更新的日志
  path: PathBuf,

  /// 有序的多份日志文件，总是从 back 插入更新的日志，
  ///
  /// 也即，在数组中这些日志这么排序：\[x.log.2, x.log.1, x.log]
  log_files: VecDeque<LogFile>,

  /// 期望加载上一个日志
  want_older_log: bool,
}

impl RotatedLog {
  /// 创建新的一组系统日志文件维护实例，给定的 `path` 参数是未带回滚后缀的路径，
  /// 本类会自动在相同目录下，扫描它的被滚动的其他日志。
  pub fn new(path: PathBuf, config: Config) -> Self {
    Self {
      path,
      log_files: VecDeque::with_capacity(config.possible_max_rotated_count),
      want_older_log: false,
    }
  }

  /// 标记期望获得更旧一点的日志
  pub fn set_want_older_log(&mut self) {
    self.want_older_log = true;
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
  pub async fn update(&mut self, data_board: Arc<Mutex<DataBoard>>) {
    // select 所有日志文件的事件
    let async_fns: Vec<_> = self
      .log_files
      .iter_mut()
      .map(|log_file| Box::pin(log_file.update(data_board.clone())))
      .collect();

    // 处理其中一个，其余取消处理
    let (events, index, _) = futures::future::select_all(async_fns).await;

    // 处理该日志可能的删除事件
    if let Some(events) = events {
      for event in events {
        if let Event::Removed = event
          && let Some(mut log_file) = self.log_files.remove(index)
        {
          let _ = log_file.close().await;
        }
      }
    }
  }

  /// 若当前还未加载最新的日志文件，也即系统正在更新的那一份（如 x.log），则尝试加载它。
  /// 如果根本不存在正在更新的日志，而我们的日志文件一份都没有加载，那就找到最新的一份滚动日志文件，进行加载
  async fn maybe_load_latest_log(&mut self) -> Option<()> {
    // 目前已经加载的最新日志文件的路径
    let loaded_latest_path = match self.log_files.back() {
      None => &PathBuf::new(),
      Some(log_file) => log_file.path(),
    };

    // 快速路径判断：如果最新的路径等于系统正在更新的路径，则结束处理
    if loaded_latest_path == &self.path {
      return None;
    }

    // 找到目录下，最新的一份日志文件（在 x.log, x.log.1, x.log.2 中找）
    let latest_path = self.find_latest_log_path()?;

    // 如果最新的这一份文件已经被加载，则结束处理
    if loaded_latest_path == &latest_path {
      return None;
    }

    // 加载最新的文件
    self
      .log_files
      .push_back(self.open_log_file(latest_path).await?);

    None
  }

  /// 找到本系统日志最新的那一份
  fn find_latest_log_path(&self) -> Option<PathBuf> {
    // 最新路径的记录
    let mut latest_path: Option<PathBuf> = None;

    // 找到字典序最小的路径，即为最新的文件
    // （x.log, x.log.1, x.log.2 中，x.log 比 x.log.1 新，x.log.1 比 x.log.2 新）
    self.visit_log_paths(|path: PathBuf| {
      Self::update_with_latest_path(&mut latest_path, path);
    });

    // 返回可能找到的最新文件路径
    latest_path
  }

  /// 如果有需要，尝试加载更老一点的日志，这份日志仅比目前已经加载的日志再老一点
  async fn maybe_load_older_log(&mut self) -> Option<()> {
    // 判断是否有设置想要加载一份老日志的标志
    if !self.want_older_log {
      return None;
    }
    self.want_older_log = false;

    // 找到目录下，稍微旧一点的一份日志
    let older_path = self.find_older_log_path()?;

    // 加载这一份日志文件
    self
      .log_files
      .push_front(self.open_log_file(older_path).await?);

    None
  }

  fn find_older_log_path(&self) -> Option<PathBuf> {
    // 找出目前已经加载的最老文件。如果找不到，则不往后处理
    let loaded_oldest_path = self.log_files.front()?.path();

    // 记录下一个旧一点的路径
    let mut next_older_path: Option<PathBuf> = None;

    // 找到比已经加载的最老文件更老，但又在这些更老的文件中最新的那一个
    self.visit_log_paths(|path: PathBuf| {
      if &path > loaded_oldest_path {
        Self::update_with_latest_path(&mut next_older_path, path);
      }
    });

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
        continue;
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

    // 如果要求被加载的日志文件名称等于系统日志最新的那份文件名称，
    // 则我们认为我们在打开一份正在被实时更新的日志文件
    let is_rolling_log = &path == &self.path;

    // 打开这一份日志文件
    match LogFile::open(path, is_rolling_log).await {
      Ok(log_file) => Some(log_file),
      Err(e) => {
        eprintln!("failed to load log file: {}", e);
        None
      }
    }
  }
}

impl RotatedLog {
  /// 获取指向第一条日志的索引
  pub fn first_index(&self) -> Index {
    Index {
      file_index: 0,
      line_index: LogFileIndex::zero(),
    }
  }

  /// 获取指向最后一条日志的索引
  pub fn last_index(&self) -> Index {
    let file_index = self.log_files.len().saturating_sub(1);
    let line_index = match self.log_files.get(file_index) {
      None => LogFileIndex::zero(),
      Some(log_file) => log_file.data().last_index(),
    };
    Index {
      file_index,
      line_index,
    }
  }

  /// 将给定索引移动指定的步长。若移动结束时指向了有效的数据，则返回新的索引，
  /// 若移动结束时发现索引越界，则返回剩余需要移动的步长。
  pub fn step_index(&self, mut index: Index, mut n: isize) -> Result<Index, isize> {
    // 获取当前索引的 log_file，如果不存在，则终止处理
    let mut log_file = self.log_files.get(index.file_index).ok_or(n)?;

    // 是向前迭代、还是向后迭代
    let is_forward = n >= 0;

    loop {
      // 检查当前索引指向的条目在正确的数据范围内，如果否，则终止处理
      if log_file.data().get(index.line_index).is_none() {
        break Err(n);
      }

      // 若 n 为 0，则结束处理，已经找到指定的日志索引
      if n == 0 {
        break Ok(index);
      }

      // 尝试更新行索引（可能会超出 log_file 范围）
      match log_file.data().step_index(index.line_index, n) {
        // 移动了索引后，仍然命中了当前文件的日志行，则返回这个新索引
        Ok(line_index) => break Ok(Index::new(index.file_index, line_index)),

        // 移动了索引后，超出了文件的索引范围，我们需要继续往下或往上找文件
        Err(m) => {
          // 更新剩余步长
          n = m;

          // 往下或往上找文件，取决于步长的符号
          index.file_index = if is_forward {
            index.file_index + 1
          } else {
            index.file_index.overflowing_sub(1).0
          };

          // 找到新文件索引指向的文件数据，如果没找到，返回以剩余步长为信息的错误
          log_file = self.log_files.get(index.file_index).ok_or(n)?;

          // 如果是下翻文件，则从该新文件的头开始新的搜素。反之，从尾部开始搜索。
          index.line_index = if is_forward {
            log_file.data().first_index()
          } else {
            log_file.data().last_index()
          }
        }
      }
    }
  }

  /// 给定索引，获取日志行数据
  pub fn get(&self, index: Index) -> Option<&LogLine> {
    self
      .log_files
      .get(index.file_index)?
      .data()
      .get(index.line_index)
  }

  /// 给定索引，获取日志行数据
  pub fn get_mut<'a>(&mut self, index: Index) -> Option<&'a mut LogLine> {
    self
      .log_files
      .get_mut(index.file_index)?
      .data_mut()
      .get_mut(index.line_index)
  }
}

// 定义迭代器以及获取接口
crate::define_all_iterators!(RotatedLog, Index);

type ItemMut<'a> = (Index, &'a mut LogLine);

/// 带有标签过滤功能的迭代器，会在遍历过程中，建立缓存的快速跳转链路
pub struct FilteredIter<'a, 'b, I>
where
  I: Iterator<Item = ItemMut<'a>> + IterNextNth<Item = ItemMut<'a>>,
{
  data: &'a mut RotatedLog,
  iter: I,
  tags: &'b TagsData,
  index: Index,
  link: LogLink,

  /// 用于标记记录中的 index 及其 link 是否是已知无效的，
  /// 1. 在遍历开始时，是未知的；
  /// 2. 在遍历中间，跳转到有效日志行上，更新了记录时，是已知有效的；
  /// 3. 在遍历中间，跳转到匹配、但链接无效的日志行，虽然更新了记录，但是是已知无效的。
  /// 在遇到下一个有效日志行、或者遍历结束时，会从 index
  chain_begin_is_known_as_invalid: bool,
}

/// 用于特化取出指定索引开始遍历的迭代器（可能是正向，也可能是逆向）
pub trait IterFrom<'a> {
  type Iter: Iterator<Item = ItemMut<'a>>;

  fn data_iter_from(data: &mut RotatedLog, index: Index) -> Self::Iter;

  fn direction(&self) -> LogDirection;
}

impl<'a, 'b> IterFrom<'a> for FilteredIter<'a, 'b, ForwardIterMut<'a>> {
  type Iter = ForwardIterMut<'a>;

  fn data_iter_from(data: &mut RotatedLog, index: Index) -> Self::Iter {
    data.iter_mut_forward_from(index)
  }

  fn direction(&self) -> LogDirection {
    LogDirection::Forward
  }
}

impl<'a, 'b> IterFrom<'a> for FilteredIter<'a, 'b, BackwardIterMut<'a>> {
  type Iter = BackwardIterMut<'a>;

  fn data_iter_from(data: &mut RotatedLog, index: Index) -> Self::Iter {
    data.iter_mut_backward_from(index)
  }

  fn direction(&self) -> LogDirection {
    LogDirection::Backward
  }
}

impl<'a, 'b, I> FilteredIter<'a, 'b, I>
where
  I: Iterator<Item = ItemMut<'a>> + IterNextNth<Item = ItemMut<'a>>,
  Self: IterFrom<'a, Iter = I>,
{
  fn new(data: &'a mut RotatedLog, tags: &'b TagsData, index: Index) -> Self {
    let iter = Self::data_iter_from(data, index);
    Self {
      data,
      iter,
      tags,
      index,
      link: LogLink::default(),
      chain_begin_is_known_as_invalid: false,
    }
  }

  /// 检查给定的 link 是否有效
  fn is_link_valid(&self, link: LogLink) -> bool {
    self.tags.get_version() == link.ver
  }

  /// 检查指定日志是否被过滤
  fn is_filtered(&self, log: &LogLine) -> bool {
    match log.get_tag() {
      None => false,
      Some(tag) => !self.tags.get(tag),
    }
  }

  /// 从之前记录的（也即是上一次有效记录的第一跳无效记录）的日志行，
  /// 一路到给定的日志行为止（不包含该日志行），更新它们 link，使之指向本日志行指向的日志
  ///
  /// 以下是示意图：
  /// 1. 第一个 v 是上一次记录的有效数据，中间的 x 代表多次循环中找到的无效记录；
  /// 2. 第二个 v 是本次找到的有效记录，它的 link 指向了下一跳。
  ///
  /// ```
  /// #  -------!       -------!
  /// # |v|....|x|x|x|x|v|....|?|
  /// ```
  ///
  /// 接下来，我们将所有的 x 的 link 都更新正确，使它们指向新发现的有效日志，指向的日志：
  ///
  /// ```
  /// #  +------!       +------!
  /// # |v|....|x|x|x|x|v|....|?|
  /// #         | | | +--------^
  /// #         | | +----------^
  /// #         | +------------^
  /// #         +--------------^
  /// ```
  fn fix_links_all_the_way(&mut self, end_index: Option<Index>, mut farest_skip: usize) {
    // 若之前记录的 link 就是有效的，则结束处理
    if self.is_link_valid(self.link) {
      return;
    }

    // 从之前的索引位置一路更新到当前索引为止
    for (index, log) in Self::data_iter_from(self.data, self.index) {
      if Some(index) == end_index {
        break;
      }

      log.set_link(
        self.direction(),
        LogLink {
          ver: self.tags.get_version(),
          skip: farest_skip,
        },
      );

      // 随着离当前有效索引越来越近，要逐步缩短步长
      farest_skip -= 1;
    }
  }

  /// 记录新的起点索引及其跳转链接，基于这一行日志开始往后跳转、或者重铸跳转链
  fn begin_new_chain(&mut self, index: Index, log: &LogLine) {
    self.link = log.get_link(self.direction());
    self.index = index;
    self.chain_begin_is_known_as_invalid = !self.is_link_valid(self.link);
  }
}

impl<'a, 'b, I> Iterator for FilteredIter<'a, 'b, I>
where
  I: Iterator<Item = ItemMut<'a>> + IterNextNth<Item = ItemMut<'a>>,
  Self: IterFrom<'a, Iter = I>,
{
  type Item = ItemMut<'a>;

  fn next(&mut self) -> Option<Self::Item> {
    // 循环处理过程中，因为始终找不到匹配的日志行，且 link 一直过期，
    // 而不断累积的无效日志行跳过步长，
    // 我们从本次迭代开始的 index 一直统计到遇到匹配的日志行、或者有效的 link 为止
    let mut skip_sum = if self.chain_begin_is_known_as_invalid {
      1
    } else {
      0
    };

    loop {
      // 本轮处理应该跳过的步长，取决于上一次访问时的元素的 link 是否有效，
      // 如果有效，则取它记录的 skip，否则取零（也即不跳过任何数据，取下一个进行分析）
      let skip = if self.is_link_valid(self.link) {
        // 由于实际上这个 skip 代表的是上一个元素的，因此从当前元素进行跳转时，步长得 -1
        self.link.skip.saturating_sub(1)
      } else {
        0
      };

      // 取出下一跳的日志
      let curr_item = self.iter.next_nth(skip).ok();

      // 如果已经取不到数据，说明已经访问到末尾，我们需要刷新此前所有无效日志行，
      // 让它们的 link 指向一个越界的值，保证下次迭代可以快速结束。
      if curr_item.is_none() {
        self.fix_links_all_the_way(None, skip_sum); // 注意，如果此时 iter 再次被误用，可能会导致链接错误
        return None;
      }

      // 解包内容
      let (index, log) = curr_item?;

      // 若上一个元素的 link 是有效的，那么之后的 link 更新处理都从新元素开始
      if self.is_link_valid(self.link) {
        self.begin_new_chain(index, log);
      }

      // 检查本日志是否被过滤掉。如果没有被过滤掉，那么刷新此前所有无效日志行，让它们的 link 指向本日志，
      // 同时，之后的链路跳转从本日志重新开始建设，
      // 最后，返回本日志
      if !self.is_filtered(log) {
        self.fix_links_all_the_way(Some(index), skip_sum);
        self.begin_new_chain(index, log);
        break Some((index, log));
      }

      // 本行日志被过滤掉，还需要继续寻找下一跳日志
      // 检查该日志的 link 是否有效，如果有效，则刷新之前所有无效日志行的 link
      let next_link = log.get_link(self.direction());

      if self.is_link_valid(next_link) {
        // +1 是因为本日志也是无效的，需要跳过，统计在其中。
        self.fix_links_all_the_way(Some(index), skip_sum + 1 + next_link.skip);
        self.begin_new_chain(index, log);

        // 从本条日志开始，重新建立 link，虽然本条日志不匹配过滤规则，但是可以用于缩短遍历路径，
        // 事实上，这种情况比较少见，也即：处于快速跳转路径中，而日志规则又不匹配，
        // 可能会出现在随机开始的第一次遍历里，从这一次跳转到正确的快速路径上后，应该一路命中的都是正确的日志行
        skip_sum = 0;
      } else {
        // 本条日志 link 无效（当 tags 发生变更时，会让所有 link 无效），则继续下一次遍历寻找。
        // 由于此前的步骤，现在记录的 link 一定是无效的，换句话说，下一个处理循环中，只会向前走一步
        skip_sum += 1;
      }
    }
  }
}

impl RotatedLog {
  /// 获取从指定索引出发的、带有标签过滤功能的正向迭代器
  pub fn filtered_iter_forward_from<'a, 'b>(
    &'a mut self,
    tags: &'b TagsData,
    index: Index,
  ) -> FilteredIter<'a, 'b, ForwardIterMut<'a>>
  where
    'b: 'a,
  {
    FilteredIter::new(self, tags, index)
  }

  /// 获取从指定索引出发的、带有标签过滤功能的逆向迭代器
  pub fn filtered_iter_backward_from<'a, 'b>(
    &'a mut self,
    tags: &'b TagsData,
    index: Index,
  ) -> FilteredIter<'a, 'b, BackwardIterMut<'a>>
  where
    'b: 'a,
  {
    FilteredIter::new(self, tags, index)
  }

  /// 获取从头部出发的、带有标签过滤功能的正向迭代器
  pub fn filtered_iter_forward_from_head<'a, 'b>(
    &'a mut self,
    tags: &'b TagsData,
  ) -> FilteredIter<'a, 'b, ForwardIterMut<'a>>
  where
    'b: 'a,
  {
    self.filtered_iter_forward_from(tags, self.first_index())
  }

  /// 获取从尾部出发的、带有标签过滤功能的逆向迭代器
  pub fn filtered_iter_backward_from_tail<'a, 'b>(
    &'a mut self,
    tags: &'b TagsData,
  ) -> FilteredIter<'a, 'b, BackwardIterMut<'a>>
  where
    'b: 'a,
  {
    self.filtered_iter_backward_from(tags, self.last_index())
  }
}
