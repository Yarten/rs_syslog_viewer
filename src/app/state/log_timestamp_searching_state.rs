use super::log_state_kit::LogStateKit;
use crate::{
  app::{StateBuilder, ViewPortStateEx, controller::LogController},
  ui::State,
};
use std::{cell::RefCell, rc::Rc};

/// 搜索日志时间戳的状态，还在输入中
pub struct LogTimestampSearchingState {
  kit: LogStateKit,
}

impl LogTimestampSearchingState {
  pub fn new(log_controller: Rc<RefCell<LogController>>) -> Self {
    Self {
      kit: LogStateKit::new(log_controller, "log timestamp searching"),
    }
  }
}

impl StateBuilder for LogTimestampSearchingState {
  fn build(self) -> State {
    let c1 = self.kit.log_controller.clone();
    let c2 = c1.clone();

    self
      .kit
      .state
      .input("Timestamps", move |s| {
        c1.borrow_mut().set_search_timestamp(Some(s.to_string()))
      })
      .view_port(c2, true) // 输入状态下，其实横向滚动操作是无效的，这里仅展示下滚动条。
  }
}
