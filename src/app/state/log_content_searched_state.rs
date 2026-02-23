use super::log_state_kit::LogStateKit;
use crate::app::controller::log_controller::Error;
use crate::ui::StatusBar;
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

fn set_search_info(status: &mut StatusBar, search: &str) {
  status.set_info(format!("Searching '{}' (use ][ to navigate)", search))
}

impl StateBuilder for LogContentSearchedState {
  fn build(self) -> State {
    let c1 = self.kit.log_controller.clone();
    let c2 = c1.clone();
    let c3 = c1.clone();

    // 在搜索内容没有匹配时报错，用户继续操作时，会重新设置提示词，
    // 用这个标志标识提示词已经设置，避免重复设置。
    let mut info_is_set = false;

    self
      .kit
      .action(KeyEvent::simple(KeyCode::Char(']')), move |ctrl| {
        ctrl.next_content_search()
      })
      .action(KeyEvent::simple(KeyCode::Char('[')), move |ctrl| {
        ctrl.prev_content_search()
      })
      .state
      .view_port(c1, true)
      .enter_action(move |pager| {
        set_search_info(pager.status(), c2.borrow().get_search_content());
      })
      .manual_action(move |pager| {
        if let Some(error) = c3.borrow_mut().take_error() {
          let status = pager.status();
          match error {
            Error::NextContentSearchNotFound => {
              status.set_error("No next log is found. (use [ to find previous one)")
            }
            Error::PrevContentSearchNotFound => {
              status.set_error("No previous log is found. (use ] to find next one)")
            }
          }
          info_is_set = false;
        } else if !info_is_set {
          set_search_info(pager.status(), c3.borrow().get_search_content());
          info_is_set = true;
        }
      })
  }
}
