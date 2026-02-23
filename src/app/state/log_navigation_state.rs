use crate::{
  app::{StateBuilder, ViewPortStateEx, controller::LogController},
  ui::{KeyEventEx, State},
};
use crossterm::event::{KeyCode, KeyEvent};
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

  fn action(mut self, event: KeyEvent, act: impl Fn(&mut LogController) + 'static) -> Self {
    let ctrl = self.log_controller.clone();
    self.state = self.state.action(event, move |_| {
      act(&mut ctrl.borrow_mut());
    });
    self
  }
}

impl StateBuilder for LogNavigationState {
  fn build(self) -> State {
    let c1 = self.log_controller.clone();

    self
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
      .state
      .view_port(c1)
  }
}
