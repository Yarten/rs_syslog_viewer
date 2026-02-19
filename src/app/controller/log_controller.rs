use crate::{
  app::{Controller, Index, LogHubData},
  log::LogLine,
};
use std::sync::Arc;
use std::{collections::VecDeque, path::PathBuf};

/// 展示区里维护的数据条目
type Item = (Index, LogLine);

/// 为 cursor 返回的数据类型做一些能力扩展，方便取用数据
trait CursorEx {
  fn index(self) -> Option<Index>;

  fn index_or(self, fallback: impl Fn() -> Index) -> Index
  where
    Self: Sized,
  {
    self.index().unwrap_or((fallback)())
  }
}

impl CursorEx for Option<&Item> {
  fn index(self) -> Option<Index> {
    self.map(|x| x.0.clone())
  }
}

/// 维护日志展示区的数据
#[derive(Default)]
struct ViewPort {
  /// 展示区的高度，也即能够展示的日志行数量
  height: usize,

  /// 日志行，从前往后对应展示区的日志从上往下
  logs: VecDeque<Item>,

  /// 光标位置，指的是相对于 height 中的定位
  cursor: usize,

  /// 光标数据索引，指的是相对于 logs 中的定位，
  /// 它和 `cursor` 不一定重叠，特别是在新的一帧构建过程中，
  /// 因此，每帧渲染获取数据的时候，将进行光标重定位
  cursor_index: usize,
}

impl ViewPort {
  /// 简单地进行支持级联调用
  fn then<F, T>(&self, f: F) -> T
  where
    F: FnOnce() -> T,
  {
    (f)()
  }

  /// 设置展示区高度，同时钳制光标位置，防止越界
  fn set_height(&mut self, height: usize) -> &mut Self {
    self.height = height;
    self.set_cursor(self.cursor);
    self
  }

  /// 直接设置光标位置，需要钳制它，防止越界
  fn set_cursor(&mut self, cursor: usize) -> &mut Self {
    self.cursor = cursor.clamp(0, self.height.saturating_sub(1));
    self
  }

  /// 将光标移动到展示区最顶部，和具体数据无关
  fn set_cursor_at_top(&mut self) -> &mut Self {
    self.cursor = 0;
    self
  }

  /// 将光标移动到展示区最底部，和具体数据无关
  fn set_cursor_at_bottom(&mut self) -> &mut Self {
    self.cursor = self.height.saturating_sub(1);
    self
  }

  /// 顶部的数据
  fn top(&self) -> Option<&Item> {
    self.logs.front()
  }

  /// 底部的数据
  fn bottom(&self) -> Option<&Item> {
    self.logs.back()
  }

  /// 获取光标指向的数据
  fn cursor(&self) -> Option<&Item> {
    self.logs.get(self.cursor)
  }

  /// 移动光标指定步长，并返回指向的数据（旧的，上一帧的内容）
  fn move_cursor(&mut self, steps: isize) -> Option<&Item> {
    self.set_cursor((self.cursor as isize + steps).max(0) as usize);
    self.logs.get(self.cursor)
  }
}

impl ViewPort {
  /// 根据已经配置好的光标位置，从指定索引处的日志开始填充数据区，
  /// 我们总是要求在条件允许的情况下，光标实际展示的位置不要过于接近底部或顶部
  fn fill(&mut self, data: &LogHubData, index: Index) {
    // 首先先清空数据
    self.logs.clear();
    self.cursor_index = 0;

    // 展示区高度为空时，结束处理
    if self.height == 0 {
      return;
    }

    // 光标往下区域的数据迭代器
    let mut iter_down = data.iter_forward_from(index.clone());

    // 先取出光标所在日志行。如果连光标指向的数据都不存在，则结束处理
    match iter_down.next() {
      None => return,
      Some(x) => self.push_back(x),
    }

    // 光标往上区域的数据迭代器，
    // 需要跳过第一个数据（也即光标所在的数据）
    let mut iter_up = data.iter_backward_from(index.clone()).skip(1);

    // TODO: 引入 tag 的过滤（包装一下 iter，用 skip while ？）

    // 光标离上下边界最少这么多行
    let min_spacing = ((self.height as f64 * 0.2 + 1.0) as usize).min(5);

    // 将光标限制在中间这个范围内
    self.cursor = match (
      self.ideal_count_up() >= min_spacing,
      self.ideal_count_down() >= min_spacing,
    ) {
      // 光标离上边界过近，离下边界较远，那么将其向下调整
      (false, true) => min_spacing,

      // 光标离下边界过近，离上边界较远，那么将其向上调整
      (true, false) => self.height - min_spacing,

      // 光标处于中间，或者上下空间都不足，不移动光标
      _ => self.cursor,
    };

    // 按现在光标的理想位置，开始取数据。可能某一端的数据其实没有那么多，我们将在后文从另外一端补充
    self.push_some_front(&mut iter_up, self.ideal_count_up());
    self.push_some_back(&mut iter_down, self.ideal_count_down());

    // 检查上下两端的数据是否已经顶到头，如果某一端没有顶到头，则尝试从另外一边追加数据，
    // 尽量保证数据展示区是满屏展示的。
    // 也有可能两端的数据都不够，但已经都没有数据了，此时等于下方两个操作没有效果。
    // 我们会在最终调整 cursor，使其对齐到它真正的位置上
    let unfilled_spacing = self.height - self.logs.len();

    // 顶部数据不够，底部来凑
    if self.current_count_up() < self.ideal_count_up() {
      self.push_some_back(&mut iter_down, unfilled_spacing);
    }

    // 底部数据不够，顶部来凑
    if self.current_count_down() < self.ideal_count_down() {
      self.push_some_front(&mut iter_up, unfilled_spacing);
    }

    // 更新光标的位置，和实际情况对齐
    self.cursor = self.cursor_index;
  }

  /// 光标往上区域应有的日志数量，仅和当前光标位置有关
  fn ideal_count_up(&self) -> usize {
    self.cursor
  }

  /// 光标往下区域应有的日志数量，仅和当前光标位置有关
  fn ideal_count_down(&self) -> usize {
    self.height - self.cursor - 1
  }

  /// 实际情况下，光标往上区域的数据数量
  fn current_count_up(&self) -> usize {
    self.cursor_index
  }

  /// 实际情况下，光标往下区域的数据数量
  fn current_count_down(&self) -> usize {
    self.logs.len() - self.cursor_index - 1
  }

  /// 在光标之上的区域插入一些数据
  fn push_some_front<'a, I>(&mut self, iter_up: &mut I, count: usize)
  where
    I: Iterator<Item = (Index, &'a LogLine)>,
  {
    for _ in 0..count {
      match iter_up.next() {
        None => break,
        Some(x) => self.push_front(x),
      }
    }
  }

  /// 在光标之上的区域插入一些数据
  fn push_some_back<'a, I>(&mut self, iter_down: &mut I, count: usize)
  where
    I: Iterator<Item = (Index, &'a LogLine)>,
  {
    for _ in 0..count {
      match iter_down.next() {
        None => break,
        Some(x) => self.push_back(x),
      }
    }
  }

  /// 在最顶部插入数据
  fn push_front(&mut self, x: (Index, &LogLine)) {
    self.logs.push_front((x.0, x.1.clone()));
    self.cursor_index += 1;
  }

  /// 在最底部插入数据
  fn push_back(&mut self, x: (Index, &LogLine)) {
    self.logs.push_back((x.0, x.1.clone()));
  }
}

/// 描述本帧内的控制
enum Control {
  /// 没有动作，光标将停在上一帧的位置
  Idle,

  /// 跟随最新日志
  Follow,

  /// 逐步移动日志
  MoveBySteps(isize),

  /// 往上翻页
  PageUp,

  /// 往下翻页
  PageDown,
}

pub struct LogController {
  /// 当帧需要处理的控制
  control: Control,

  ///展示区的数据
  view_port: ViewPort,

  /// 日志的根目录
  log_files_root: Option<Arc<PathBuf>>,
}

impl LogController {
  pub fn new() -> Self {
    Self {
      control: Control::Follow,
      view_port: Default::default(),
      log_files_root: Default::default(),
    }
  }

  /// 总是跟踪到最新的日志（退出导航模式）
  pub fn follow(&mut self) {
    self.control = Control::Follow;
  }

  /// 更新日志区高度
  pub fn set_height(&mut self, height: usize) {
    self.view_port.set_height(height);
  }

  /// 按步移动光标
  pub fn move_by_steps(&mut self, steps: isize) {
    self.control = Control::MoveBySteps(steps);
  }

  /// 往上翻页
  pub fn page_up(&mut self) {
    self.control = Control::PageUp;
  }

  /// 往下翻页
  pub fn page_down(&mut self) {
    self.control = Control::PageDown;
  }
}

impl LogController {
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

  /// 展示的日志条目
  pub fn logs(&self) -> impl Iterator<Item = &LogLine> {
    self.view_port.logs.iter().map(|x| &x.1)
  }

  /// 光标所在的行索引
  pub fn cursor(&self) -> usize {
    self.view_port.cursor
  }
}

impl Controller for LogController {
  fn run_once(&mut self, data: &mut LogHubData) {
    // 记录日志根目录
    self.log_files_root = Some(data.data_board().get_root_path().clone());

    // TODO: 刷新上一帧 index 在这一帧的值，根据各个 log file 的增删情况来近似更新
    // 取出变更历史，进行 fix(index)

    // 取出当前光标应指向的数据索引，同时，对光标的位置完成配置
    let cursor_index: Index = match self.control {
      Control::Idle => self.view_port.cursor().index_or(|| data.last_index()),

      Control::Follow => self
        .view_port
        .set_cursor_at_bottom()
        .then(|| data.last_index()),

      Control::MoveBySteps(n) => self.view_port.move_cursor(n).index_or(|| data.last_index()),

      Control::PageUp => self
        .view_port
        .set_cursor_at_bottom()
        .top()
        .index_or(|| data.last_index()),

      Control::PageDown => self
        .view_port
        .set_cursor_at_top()
        .bottom()
        .index_or(|| data.last_index()),
    };

    // 基于当前的光标位置，及其指向的数据索引，填充整个展示区
    self.view_port.fill(data, cursor_index);

    // 重置控制量
    match self.control {
      Control::Follow => {}
      _ => self.control = Control::Idle,
    }
  }
}
