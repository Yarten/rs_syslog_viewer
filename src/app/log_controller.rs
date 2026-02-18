use crate::{
  app::{Controller, Index, LogHubData},
  log::LogLine,
};
use std::sync::Arc;
use std::{collections::VecDeque, path::PathBuf};

/// 维护日志展示区的数据
#[derive(Default)]
struct ViewPort {
  /// 展示区的高度，也即能够展示的日志行数量
  height: usize,

  /// 日志行，从前往后对应展示区的日志从上往下
  logs: VecDeque<LogLine>,

  /// 光标位置，指的是相对于 height 中的定位
  cursor: usize,

  /// 光标数据索引，指的是相对于 logs 中的定位，
  /// 它和 `cursor` 不一定重叠，特别是在新的一帧构建过程中，
  /// 因此，每帧渲染获取数据的时候，将进行光标重定位
  cursor_index: usize,
}

impl ViewPort {
  /// 将光标移动到展示区最底部
  fn cursor_to_last(&mut self) -> &mut Self {
    self.cursor = self.height.saturating_sub(1);
    self
  }

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

    // 我们先从光标指向的数据，开始正向移动（也即往下、往更新的日志去移动）
    let mut iter_down = data.iter_forward_from(index.clone());

    // 先取出光标所在日志行。如果连光标指向的数据都不存在，则结束处理
    match iter_down.next() {
      None => return,
      Some((_, log)) => self.logs.push_back(log.clone()),
    }

    // 光标离上下边界最少这么多行
    let min_spacing = (self.height as f64 * 0.2 + 1.0) as usize;

    // 尽量往下取这么多数据，让光标所处的位置比较靠向中间。
    // 如果本来光标离底部较远，为了避免光标在页面中瞎移动，我们则要取出足够多的数据支撑它。
    self.push_some_back(
      &mut iter_down,
      min_spacing.max(self.height - self.cursor - 1),
    );

    // TODO: 引入 tag 的过滤（包装一下 iter，用 skip while ？）

    // 接着，我们从光标之上，开始逆向移动（也即往上、往更旧的日志去移动），
    // 需要跳过第一个数据（也即光标所在的数据）
    let mut iter_up = data.iter_backward_from(index.clone()).skip(1);

    // 和下取日志一样，我们尽量往上取一些数据，让光标位置离顶部足够远，
    // 或者保证光标位置不要随便移动
    self.push_some_front(&mut iter_up, min_spacing.max(self.cursor));

    // 检查上下两端的数据是否已经顶到头，如果某一端没有顶到头，则尝试从另外一边追加数据，
    // 尽量保证数据展示区是满屏展示的。
    // 也有可能两端的数据都不够，但已经都没有数据了，此时等于下方两个操作没有效果。
    // 我们会在最终调整 cursor，使其对齐到它真正的位置上
    let unfilled_spacing = self.height - self.logs.len();

    // 底部数据不够，顶部来凑
    if self.logs.len() - self.cursor_index < self.height - self.cursor {
      self.push_some_front(&mut iter_up, unfilled_spacing);
    }

    // 顶部数据不够，底部来凑
    if self.cursor_index < self.cursor {
      self.push_some_back(&mut iter_down, unfilled_spacing);
    }

    // 更新光标的位置，和实际情况对齐
    self.cursor = self.cursor_index;
  }

  /// 在光标之上的区域插入一些数据
  fn push_some_front<'a, I>(&mut self, iter_up: &mut I, count: usize)
  where
    I: Iterator<Item = (Index, &'a LogLine)>,
  {
    for _ in 0..count {
      match iter_up.next() {
        None => break,
        Some((_, log)) => self.push_front(log),
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
        Some((_, log)) => self.push_back(log),
      }
    }
  }

  /// 在最顶部插入数据
  fn push_front(&mut self, data: &LogLine) {
    self.logs.push_front(data.clone());
    self.cursor_index += 1;
  }

  /// 在最底部插入数据
  fn push_back(&mut self, data: &LogLine) {
    self.logs.push_back(data.clone());
  }
}

/// 描述本帧内的控制
enum Control {
  /// 跟随最新日志
  Follow,
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
    self.view_port.height = height;
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
  pub fn logs(&self) -> &VecDeque<LogLine> {
    &self.view_port.logs
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

    match self.control {
      Control::Follow => self.fill_latest_logs(data),
    }
  }
}

impl LogController {
  fn fill_latest_logs(&mut self, data: &LogHubData) {
    self
      .view_port
      .cursor_to_last()
      .fill(data, data.last_index());
  }
}
