use crate::{
  app::{StateBuilder, ViewPortStateEx, controller::HelpController},
  ui::State,
};
use std::{cell::RefCell, rc::Rc};

/// 处理帮助页面的状态
pub struct HelpState {
  /// 帮助信息维护器
  help_controller: Rc<RefCell<HelpController>>,

  /// 被构建的状态
  state: State,
}

impl HelpState {
  pub fn new(help_controller: Rc<RefCell<HelpController>>) -> HelpState {
    Self {
      help_controller,
      state: State::new("help"),
    }
  }
}

impl StateBuilder for HelpState {
  fn build(self) -> State {
    self.state.view_port(self.help_controller, true)
  }
}
