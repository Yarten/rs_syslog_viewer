use crate::{
  app::{StateBuilder, controller::LogController},
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

  /// 添加一个按键动作，控制日志的展示
  fn action(mut self, event: KeyEvent, mut act: impl FnMut(&mut LogController) + 'static) -> Self {
    let ctrl = self.log_controller.clone();
    self.state = self.state.action(event, move |_| {
      act(&mut ctrl.borrow_mut());
    });
    self
  }
}

impl StateBuilder for LogNavigationState {
  fn build(self) -> State {
    self
      .action(KeyEvent::simple(KeyCode::Up), |ctrl| ctrl.move_by_steps(-1))
      .action(KeyEvent::simple(KeyCode::Down), |ctrl| {
        ctrl.move_by_steps(1)
      })
      .action(KeyEvent::simple(KeyCode::PageUp), |ctrl| ctrl.page_up())
      .action(KeyEvent::simple(KeyCode::PageDown), |ctrl| ctrl.page_down())
      .state
  }
}
