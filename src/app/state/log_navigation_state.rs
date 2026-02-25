use super::log_state_kit::LogStateKit;
use crate::app::controller::log_controller::Error;
use crate::ui::ViewPortEx;
use crate::{
  app::{StateBuilder, ViewPortStateEx, controller::LogController},
  ui::{KeyEventEx, State},
};
use crossterm::event::{KeyCode, KeyEvent};
use std::{cell::RefCell, rc::Rc};

/// 处理日志浏览导航的状态
pub struct LogNavigationState {
  kit: LogStateKit,
}

impl LogNavigationState {
  pub fn new(log_controller: Rc<RefCell<LogController>>) -> Self {
    Self {
      kit: LogStateKit::new(log_controller, "log navigation"),
    }
  }
}

impl StateBuilder for LogNavigationState {
  fn build(self) -> State {
    let c1 = self.kit.log_controller.clone();

    self
      .kit
      .action(KeyEvent::simple(KeyCode::Char('1')), |ctrl| {
        ctrl.style_mut().next()
      })
      .action(KeyEvent::simple(KeyCode::Char('2')), |ctrl| {
        ctrl.style_mut().timestamp_style.next()
      })
      .action(KeyEvent::simple(KeyCode::Char('3')), |ctrl| {
        ctrl.style_mut().tag_style.next()
      })
      .action(KeyEvent::simple(KeyCode::Char('4')), |ctrl| {
        ctrl.style_mut().pid_style.next()
      })
      .action(KeyEvent::simple(KeyCode::Char('f')), |ctrl| {
        ctrl.view_mut().ui_mut().want_follow()
      })
      .action(KeyEvent::simple(KeyCode::Char('m')), |ctrl| {
        ctrl.toggle_mark()
      })
      .action(KeyEvent::simple(KeyCode::Char('[')), |ctrl| {
        ctrl.prev_mark()
      })
      .action(KeyEvent::simple(KeyCode::Char(']')), |ctrl| {
        ctrl.next_mark()
      })
      .error(|e| match e {
        Error::NextMarkedNotFound => {
          Some("No next marked log is found. (use [ to find previous one)".to_string())
        }
        Error::PrevMarkedNotFound => {
          Some("No previous marked log is found. (use ] to find next one)".to_string())
        }
        _ => None,
      })
      .state
      .view_port(c1, true)
  }
}
