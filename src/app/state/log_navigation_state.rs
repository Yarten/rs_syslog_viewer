use crate::{
  app::{StateBuilder, ViewPortStateEx, controller::LogController},
  ui::State,
};
use std::{cell::RefCell, rc::Rc};

/// 处理日志浏览导航的状态
pub struct LogNavigationState {
  /// 日志数据控制器
  log_controller: Rc<RefCell<LogController>>,

  /// 被构建的状态
  state: State,
}

impl LogNavigationState {
  pub fn new(log_controller: Rc<RefCell<LogController>>) -> Self {
    Self {
      log_controller,
      state: State::new("log navigation"),
    }
  }
}

impl StateBuilder for LogNavigationState {
  fn build(self) -> State {
    self.state.view_port(self.log_controller)
  }
}
