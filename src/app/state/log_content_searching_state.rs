use super::log_state_kit::LogStateKit;
use crate::ui::ViewPortEx;
use crate::{
  app::{StateBuilder, ViewPortStateEx, controller::LogController},
  ui::State,
};
use std::{cell::RefCell, rc::Rc};

/// 搜索日志的状态，还在输入中
pub struct LogContentSearchingState {
  kit: LogStateKit,
}

impl LogContentSearchingState {
  pub fn new(log_controller: Rc<RefCell<LogController>>) -> LogContentSearchingState {
    Self {
      kit: LogStateKit::new(log_controller, "log content searching"),
    }
  }
}

impl StateBuilder for LogContentSearchingState {
  fn build(self) -> State {
    let c1 = self.kit.log_controller.clone();
    let c2 = c1.clone();
    let c3 = c1.clone();

    self
      .kit
      .state
      .input("Logs", move |s| {
        c1.borrow_mut().search_content(Some(s.to_string()))
      })
      .view_port(c2, true) // 输入状态下，其实横向滚动操作是无效的，这里仅展示下滚动条。
      .enter_action(move |pager| {
        let mut ctrl = c3.borrow_mut();
        ctrl.view_mut().ui_mut().do_not_follow();
        pager
          .status()
          .reset_input(ctrl.get_search_content().to_string())
      })
  }
}
