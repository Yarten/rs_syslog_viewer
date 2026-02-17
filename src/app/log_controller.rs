use crate::app::{Controller, LogHubData};
use crate::log::LogLine;
use std::collections::VecDeque;

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
  fn clear(&mut self) -> &mut Self {
    self.logs.clear();
    self
  }

  fn cursor_to_last(&mut self) -> &mut Self {
    self.cursor = self.height.saturating_sub(1);
    self
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
}

impl LogController {
  pub fn new() -> Self {
    Self {
      control: Control::Follow,
      view_port: Default::default(),
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

impl Controller for LogController {
  fn run_once(&mut self, data: &LogHubData) {
    match self.control {
      Control::Follow => self.fill_latest_logs(data),
    }
  }
}

impl LogController {
  fn fill_latest_logs(&mut self, data: &LogHubData) {}
}
