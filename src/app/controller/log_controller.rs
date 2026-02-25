use crate::{
  app::{Controller, Index, LogHubRef, LogItem, TimeMatcher},
  log::{LogDirection, LogLine},
  ui::CursorExpectation,
};
use std::{path::PathBuf, sync::Arc};

/// 描述一条日志的其他属性，表征 viewer 其他渲染需求
#[derive(Default)]
pub struct Properties {
  pub timestamp_matched: bool,
}

/// 展示区里维护的数据条目
type Item = (Index, LogLine, Properties);

// 定义日志展示区的可视化数据
crate::view_port!(ViewPort, Item);

impl ViewPort {
  /// 根据已经配置好的光标位置，从指定索引处的日志开始填充数据区
  fn fill(&mut self, data: &mut LogHubRef, index: Index) {
    // 从指定索引位置处，取出正向与逆向的迭代器
    let (mut iter_down, mut iter_up) = data.iter_at(index);
    iter_up.next(); // 光标位置默认用的 iter_down 迭代器插入，因此 iter_up 需要先跳过这一行。

    // 使用 view port ui 的能力，逐一填充数据
    self.do_fill(|dir| match dir {
      LogDirection::Forward => iter_down.next().map(Self::map_into_item),
      LogDirection::Backward => iter_up.next().map(Self::map_into_item),
    })
  }

  fn map_into_item(item: (Index, &mut LogLine)) -> Item {
    (item.0, item.1.clone(), Properties::default())
  }
}

/// 时间戳展示风格
#[derive(Default, PartialEq, Copy, Clone)]
pub enum TimestampStyle {
  /// 完整时间戳信息
  Full,

  /// 不展示日期，仅展示时间，精确到毫秒
  Time,

  /// 展示月日以及时间，精确到毫秒
  #[default]
  MonthDayTime,

  /// 仅展示时分秒
  RoughTime,
}

impl TimestampStyle {
  pub fn next(&mut self) {
    *self = match self {
      TimestampStyle::Full => TimestampStyle::Time,
      TimestampStyle::Time => TimestampStyle::MonthDayTime,
      TimestampStyle::MonthDayTime => TimestampStyle::RoughTime,
      TimestampStyle::RoughTime => TimestampStyle::Full,
    }
  }
}

/// 标签展示风格
#[derive(Default, PartialEq, Copy, Clone)]
pub enum TagStyle {
  /// 完整展示
  #[default]
  Full,

  /// 过长在左边省略
  OmitLeft,

  /// 过长在右边省略
  OmitRight,

  /// 过长在中间省略
  OmitMiddle,

  /// 不展示
  Hidden,
}

impl TagStyle {
  pub fn next(&mut self) {
    *self = match self {
      TagStyle::Full => TagStyle::OmitLeft,
      TagStyle::OmitLeft => TagStyle::OmitRight,
      TagStyle::OmitRight => TagStyle::OmitMiddle,
      TagStyle::OmitMiddle => TagStyle::Hidden,
      TagStyle::Hidden => TagStyle::Full,
    }
  }
}

/// PID 展示风格
#[derive(Default, PartialEq, Copy, Clone)]
pub enum PidStyle {
  /// 展示
  Shown,

  /// 不展示
  #[default]
  Hidden,
}

impl PidStyle {
  pub fn next(&mut self) {
    *self = match self {
      PidStyle::Shown => PidStyle::Hidden,
      PidStyle::Hidden => PidStyle::Shown,
    }
  }
}

/// 日志各项内容展示风格配置
#[derive(Default, PartialEq, Copy, Clone)]
pub struct Style {
  pub timestamp_style: TimestampStyle,
  pub tag_style: TagStyle,
  pub pid_style: PidStyle,
  type_index: usize,
}

impl Style {
  pub fn next(&mut self) {
    let style = match self.type_index {
      0 => Style {
        timestamp_style: TimestampStyle::MonthDayTime,
        tag_style: TagStyle::Full,
        pid_style: PidStyle::Hidden,
        type_index: 0,
      },
      1 => Style {
        timestamp_style: TimestampStyle::Time,
        tag_style: TagStyle::OmitLeft,
        pid_style: PidStyle::Hidden,
        type_index: 1,
      },
      2 => Style {
        timestamp_style: TimestampStyle::RoughTime,
        tag_style: TagStyle::Hidden,
        pid_style: PidStyle::Hidden,
        type_index: 2,
      },
      3 => Style {
        timestamp_style: TimestampStyle::Full,
        tag_style: TagStyle::Full,
        pid_style: PidStyle::Shown,
        type_index: 3,
      },
      _ => {
        unreachable!()
      }
    };

    // 若当前选择的索引对应的风格与预设相同，则应用下一个风格，
    // 否则先回归本索引应有的风格。
    if style == *self {
      self.type_index += 1;
      if self.type_index > 3 {
        self.type_index = 0;
      }
      self.next();
    } else {
      *self = style;
    }
  }
}

/// 日志区的控制量
enum Control {
  Idle,

  /// 对光标指向的数据切换 mark 状态
  ToggleMark,

  /// 定位下一条被 mark 的日志
  NextMarked,

  /// 定位上一条被 mark 的日志
  PrevMarked,

  /// 定位最近的符合搜索结果的日志
  LocateContentSearch,

  /// 下一条符合搜索结果的日志
  NextContentSearch,

  /// 上一条符合搜索结果的日志
  PrevContentSearch,

  /// 定位最近的符合搜索结果的日志
  LocateTimestampSearch,

  /// 下一条符合搜索结果的日志
  NextTimestampSearch,

  /// 上一条符合搜索结果的日志
  PrevTimestampSearch,
}

/// 控制器的报错信息
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Error {
  // 跳转标记时的相关错误
  NextMarkedNotFound,
  PrevMarkedNotFound,

  // 内容搜索相关错误
  NextContentSearchNotFound,
  PrevContentSearchNotFound,

  // 时间戳搜索相关错误
  NextTimestampSearchNotFound,
  PrevTimestampSearchNotFound,
  TimestampSearchFormatError(String),
}

/// 日志展示区的控制器
pub struct LogController {
  ///展示区的数据
  view_port: ViewPort,

  /// 日志的根目录
  log_files_root: Option<Arc<PathBuf>>,

  /// 日志展示风格
  style: Style,

  /// 本帧控制
  control: Control,

  /// 本帧报错
  error: Option<Error>,

  /// 搜索的内容。为 None 时，说明当前不处于搜索状态。
  content_search: Option<String>,

  /// 搜索时间戳的指令，本字段仅记录
  timestamp_search: String,

  /// 时间戳匹配器，仅进入搜索状态时有值。如果给定的搜索指令错误，会记录它
  /// 生成时的错误信息
  timestamp_matcher: Option<Result<TimeMatcher, String>>,
}

impl Default for LogController {
  fn default() -> Self {
    let mut res = Self {
      view_port: Default::default(),
      log_files_root: Default::default(),
      style: Default::default(),
      control: Control::Idle,
      error: None,
      content_search: None,
      timestamp_search: String::new(),
      timestamp_matcher: None,
    };

    // 默认跟踪最新日志
    res.view_port.ui.want_follow();
    res
  }
}

impl LogController {
  pub fn view_mut(&mut self) -> &mut ViewPort {
    &mut self.view_port
  }
  pub fn view(&self) -> &ViewPort {
    &self.view_port
  }

  pub fn style_mut(&mut self) -> &mut Style {
    &mut self.style
  }
  pub fn style(&self) -> &Style {
    &self.style
  }

  pub fn take_error(&mut self) -> Option<Error> {
    self.error.take()
  }

  /// 日志所处根目录
  pub fn logs_root(&self) -> &str {
    if let Some(root) = &self.log_files_root
      && let Some(root) = root.to_str()
    {
      root
    } else {
      "logs"
    }
  }

  pub fn toggle_mark(&mut self) {
    self.control = Control::ToggleMark;
  }

  pub fn next_mark(&mut self) {
    self.control = Control::NextMarked;
  }

  pub fn prev_mark(&mut self) {
    self.control = Control::PrevMarked;
  }

  /// 设置搜索的内容，或者设置不搜索。
  pub fn search_content(&mut self, search: Option<String>) {
    self.content_search = search;
    self.control = Control::LocateContentSearch;
  }

  /// 跳转到下一条搜索匹配的日志
  pub fn next_content_search(&mut self) {
    self.control = Control::NextContentSearch;
  }

  /// 跳转到上一条搜索匹配的日志
  pub fn prev_content_search(&mut self) {
    self.control = Control::PrevContentSearch;
  }

  /// 获取搜素中的内容
  pub fn get_search_content(&self) -> &str {
    static EMPTY: String = String::new();
    self.content_search.as_ref().unwrap_or(&EMPTY)
  }

  /// 设置搜索的时间戳条件，或者设置不搜索
  pub fn set_search_timestamp(&mut self, search: Option<String>) {
    match search {
      None => {
        self.timestamp_matcher = None;
      }
      Some(cmd) => {
        self.timestamp_search = cmd;
      }
    }
  }

  /// 搜索时间戳最近匹配的日志
  pub fn search_timestamp(&mut self) {
    self.control = Control::LocateTimestampSearch;

    // 创建匹配器，解析搜索指令，如果出错，记录成错误
    let mut tm = TimeMatcher::new();
    match tm.parse(&self.timestamp_search) {
      Ok(_) => {
        self.timestamp_matcher = Some(Ok(tm));
      }
      Err(msg) => {
        self.timestamp_matcher = Some(Err(msg));
      }
    }
  }

  /// 跳转到下一条时间戳匹配的日志
  pub fn next_timestamp_search(&mut self) {
    self.control = Control::NextTimestampSearch;
  }

  /// 跳转到上一条时间戳匹配的日志
  pub fn prev_timestamp_search(&mut self) {
    self.control = Control::PrevTimestampSearch;
  }

  /// 获取时间戳条件
  pub fn get_search_timestamp(&self) -> &str {
    &self.timestamp_search
  }
}

/// 辅助进行日志条件搜索
struct Searcher<'a, 'b> {
  data: &'a mut LogHubRef<'b>,
  index: Index,
  error: Option<Error>,
}

impl<'a, 'b> Searcher<'a, 'b> {
  fn new(data: &'a mut LogHubRef<'b>, index: Index) -> Self {
    Self {
      data,
      index,
      error: None,
    }
  }

  fn nearest<F>(&mut self, matcher: F) -> Index
  where
    F: Fn(&LogLine) -> bool,
  {
    let index = std::mem::take(&mut self.index);
    let (iter_down, mut iter_up) = self.data.iter_at(index.clone());
    iter_up.next();

    Self::search_first_matched(iter_down, &matcher)
      .or(Self::search_first_matched(iter_up, &matcher))
      .unwrap_or_else(|| index)
  }

  fn next<F>(&mut self, matcher: F, error: Error) -> Index
  where
    F: Fn(&LogLine) -> bool,
  {
    let index = std::mem::take(&mut self.index);
    let mut iter_down = self.data.iter_forward_from(index.clone());
    iter_down.next();

    match Self::search_first_matched(iter_down, matcher) {
      Some(index) => index,
      None => {
        self.error = Some(error);
        index
      }
    }
  }

  fn prev<F>(&mut self, matcher: F, error: Error) -> Index
  where
    F: Fn(&LogLine) -> bool,
  {
    let index = std::mem::take(&mut self.index);
    let mut iter_up = self.data.iter_backward_from(index.clone());
    iter_up.next();

    match Self::search_first_matched(iter_up, matcher) {
      Some(index) => index,
      None => {
        self.error = Some(error);
        index
      }
    }
  }

  fn search_first_matched<'c, I, F>(iter: I, matcher: F) -> Option<Index>
  where
    I: Iterator<Item = LogItem<'c>>,
    F: Fn(&LogLine) -> bool,
  {
    for (index, log) in iter {
      if matcher(&log) {
        return Some(index);
      }
    }

    None
  }
}

impl LogController {
  fn mark_matcher(&self) -> impl Fn(&LogLine) -> bool {
    LogLine::is_marked
  }

  fn content_matcher(&self) -> impl Fn(&LogLine) -> bool {
    |log: &LogLine| log.get_content().contains(&self.get_search_content())
  }

  fn get_time_matcher(&self) -> Option<&TimeMatcher> {
    match self.timestamp_matcher.as_ref() {
      Some(Ok(tm)) => Some(tm),
      _ => None,
    }
  }

  fn timestamp_matcher(&self, tm: &TimeMatcher) -> impl Fn(&LogLine) -> bool {
    move |log: &LogLine| match log.get_timestamp() {
      None => false,
      Some(dt) => tm.is_matched(dt),
    }
  }

  /// 定位光标指向的数据索引。因为可能标签过滤规则的变化，会导致原来光标指向的数据不可见了
  fn ensure_cursor_valid(data: &mut LogHubRef, index: Index) -> Index {
    let (mut iter_down, mut iter_up) = data.iter_at(index.clone());
    match iter_down.next() {
      None => {}
      Some((index, _)) => return index,
    }
    match iter_up.next() {
      None => {}
      Some((index, _)) => return index,
    }
    index
  }

  /// 处理光标越界期望
  fn process_cursor_expectation(
    data: &mut LogHubRef,
    index: Index,
    expectation: CursorExpectation,
  ) -> Index {
    match expectation {
      CursorExpectation::None => index,
      CursorExpectation::MoreUp => {
        let mut iter_up = data.iter_backward_from(index.clone());
        iter_up.next();
        iter_up.next().map(|(index, _)| index).unwrap_or(index)
      }
      CursorExpectation::MoreDown => {
        let mut iter_down = data.iter_forward_from(index.clone());
        iter_down.next();
        iter_down.next().map(|(index, _)| index).unwrap_or(index)
      }
    }
  }

  /// 设置时间戳过滤属性。仅时间戳过滤状态启用时有效
  fn set_timestamp_matching_properties(&mut self) {
    match self.timestamp_matcher.as_ref() {
      None => {}
      Some(Err(msg)) => {
        self.error = Some(Error::TimestampSearchFormatError(msg.clone()));
      }
      Some(Ok(tm)) => self.view_port.data.iter_mut().for_each(|(_, log, props)| {
        props.timestamp_matched = match log.get_timestamp() {
          None => false,
          Some(dt) => tm.is_matched(dt),
        }
      }),
    }
  }
}

impl Controller for LogController {
  fn run_once(&mut self, data: &mut LogHubRef) {
    // 记录日志根目录
    self.log_files_root = Some(data.data_board().get_root_path().clone());

    // TODO: 刷新上一帧 index 在这一帧的值，根据各个 log file 的增删情况来近似更新
    // 取出变更历史，进行 fix(index)

    // 取出当前光标应指向的数据索引，同时，对光标的位置完成配置
    let (cursor_index, cursor_expectation) = self
      .view_port
      .apply()
      .map(|((i, ..), e)| (i.clone(), e))
      .unwrap_or_else(|| (data.last_index(), CursorExpectation::None));

    // 重定位索引，确保它光标总是指向可见的数据
    let cursor_index = Self::ensure_cursor_valid(data, cursor_index);

    // 处理光标越界的期望
    let mut cursor_index = Self::process_cursor_expectation(data, cursor_index, cursor_expectation);

    // 响应控制
    match self.control {
      Control::Idle => {}
      Control::ToggleMark => {
        if let Some(log) = data.get(cursor_index.clone()) {
          log.toggle_mark();
        }
      }
      _ => {
        // 处理搜索
        let mut searcher = Searcher::new(data, cursor_index.clone());
        cursor_index = match self.control {
          Control::NextMarked => searcher.next(self.mark_matcher(), Error::NextMarkedNotFound),
          Control::PrevMarked => searcher.prev(self.mark_matcher(), Error::PrevMarkedNotFound),
          Control::LocateContentSearch => searcher.nearest(self.content_matcher()),
          Control::NextContentSearch => {
            searcher.next(self.content_matcher(), Error::NextContentSearchNotFound)
          }
          Control::PrevContentSearch => {
            searcher.prev(self.content_matcher(), Error::PrevContentSearchNotFound)
          }
          Control::LocateTimestampSearch => self.get_time_matcher().map_or(cursor_index, |tm| {
            searcher.nearest(self.timestamp_matcher(&tm))
          }),
          Control::NextTimestampSearch => self.get_time_matcher().map_or(cursor_index, |tm| {
            searcher.next(
              self.timestamp_matcher(&tm),
              Error::NextTimestampSearchNotFound,
            )
          }),
          Control::PrevTimestampSearch => self.get_time_matcher().map_or(cursor_index, |tm| {
            searcher.prev(
              self.timestamp_matcher(&tm),
              Error::PrevTimestampSearchNotFound,
            )
          }),
          _ => {
            unreachable!()
          }
        };

        self.error = searcher.error;
      }
    }
    self.control = Control::Idle;

    // 基于当前的光标位置，及其指向的数据索引，填充整个展示区
    self.view_port.fill(data, cursor_index);

    // 设置时间戳过滤结果（如果有的话）
    self.set_timestamp_matching_properties();

    // 如果存在数据顶到头，触发更老的日志加载
    let first_index = self
      .view()
      .data
      .front()
      .map(|(first_index, ..)| first_index.clone())
      .unwrap_or(data.first_index());
    data.try_load_older_logs(&first_index);
  }

  fn view_port(&mut self) -> Option<&mut ViewPortBase> {
    Some(&mut self.view_port.ui)
  }
}
