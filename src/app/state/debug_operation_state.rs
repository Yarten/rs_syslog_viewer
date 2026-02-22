use crate::{
  app::{StateBuilder, ViewPortStateEx, controller::DebugController},
  ui::State,
};
use std::{cell::RefCell, rc::Rc};

/// 处理调试日志浏览导航的状态
pub struct DebugOperationState {
  /// 调试数据控制器
  debug_controller: Rc<RefCell<DebugController>>,

  /// 被构建的状态
  state: State,
}

impl DebugOperationState {
  pub fn new(debug_controller: Rc<RefCell<DebugController>>) -> Self {
    Self {
      debug_controller,
      state: State::new("debug operation"),
    }
  }
}

impl StateBuilder for DebugOperationState {
  fn build(self) -> State {
    self.state.view_port(self.debug_controller)
  }
}
