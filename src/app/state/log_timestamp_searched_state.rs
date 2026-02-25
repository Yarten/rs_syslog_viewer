use super::log_state_kit::LogStateKit;
use crate::app::controller::log_controller::Error;
use crate::ui::ViewPortEx;
use crate::{
  app::{StateBuilder, ViewPortStateEx, controller::LogController},
  ui::{KeyEventEx, State},
};
use crossterm::event::{KeyCode, KeyEvent};
use std::{cell::RefCell, rc::Rc};

/// 在已经设置完时间戳搜索条件的情况下，进行时间戳搜索与导航
pub struct LogTimestampSearchedState {
  kit: LogStateKit,
}

impl LogTimestampSearchedState {
  pub fn new(log_controller: Rc<RefCell<LogController>>) -> Self {
    Self {
      kit: LogStateKit::new(log_controller, "log timestamp searched"),
    }
  }
}

impl StateBuilder for LogTimestampSearchedState {
  fn build(self) -> State {
    let c1 = self.kit.log_controller.clone();
    let c2 = c1.clone();
    let c3 = c1.clone();

    self
      .kit
      .action(KeyEvent::simple(KeyCode::Char(']')), move |ctrl| {
        ctrl.next_timestamp_search()
      })
      .action(KeyEvent::simple(KeyCode::Char('[')), move |ctrl| {
        ctrl.prev_timestamp_search()
      })
      .error(|e| match e {
        Error::TimestampSearchFormatError(msg) => Some(msg),
        Error::NextTimestampSearchNotFound => {
          Some("No next log is found. (use [ to find previous one)".to_string())
        }
        Error::PrevTimestampSearchNotFound => {
          Some("No previous log is found. (use ] to find next one)".to_string())
        }
        _ => None,
      })
      .state
      .view_port(c1, true)
      .enter_action(move |pager| {
        let mut ctrl = c2.borrow_mut();
        pager.status().set_tips(format!(
          "Use ][ to navigate searching '{}'",
          ctrl.get_search_timestamp()
        ));
        ctrl.view_mut().ui_mut().do_not_follow();
        ctrl.search_timestamp()
      })
      .leave_action(move |_| c3.borrow_mut().set_search_timestamp(None))
  }
}
