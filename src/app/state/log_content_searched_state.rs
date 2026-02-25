use super::log_state_kit::LogStateKit;
use crate::app::controller::log_controller::Error;
use crate::{
  app::{StateBuilder, ViewPortStateEx, controller::LogController},
  ui::{KeyEventEx, State},
};
use crossterm::event::{KeyCode, KeyEvent};
use std::{cell::RefCell, rc::Rc};

/// 在已经搜索完成的结果中，进行导航的状态
pub struct LogContentSearchedState {
  kit: LogStateKit,
}

impl LogContentSearchedState {
  pub fn new(log_controller: Rc<RefCell<LogController>>) -> LogContentSearchedState {
    Self {
      kit: LogStateKit::new(log_controller, "log content searched"),
    }
  }
}

impl StateBuilder for LogContentSearchedState {
  fn build(self) -> State {
    let c1 = self.kit.log_controller.clone();
    let c2 = c1.clone();

    self
      .kit
      .action(KeyEvent::simple(KeyCode::Char(']')), move |ctrl| {
        ctrl.next_content_search()
      })
      .action(KeyEvent::simple(KeyCode::Char('[')), move |ctrl| {
        ctrl.prev_content_search()
      })
      .error(|e| match e {
        Error::NextContentSearchNotFound => {
          Some("No next log is found. (use [ to find previous one)".to_string())
        }
        Error::PrevContentSearchNotFound => {
          Some("No previous log is found. (use ] to find next one)".to_string())
        }
        _ => None,
      })
      .state
      .view_port(c1, true)
      .enter_action(move |pager| {
        pager.status().set_tips(format!(
          "Use ][ to navigate searching '{}'",
          c2.borrow().get_search_content()
        ))
      })
  }
}
