use crate::{
  app::{StateBuilder, controller::AppController},
  ui::{KeyEventEx, State},
};
use crossterm::event::{KeyCode, KeyEvent};
use std::{cell::RefCell, rc::Rc};

pub struct QuitState {
  /// 程序控制器
  app_controller: Rc<RefCell<AppController>>,

  /// 被构建的状态
  state: State,
}

impl QuitState {
  pub fn new(app_controller: Rc<RefCell<AppController>>) -> Self {
    Self {
      app_controller,
      state: State::new("quit"),
    }
  }
}

impl StateBuilder for QuitState {
  fn build(self) -> State {
    let ctrl = self.app_controller;
    self
      .state
      .enter_action(|pager| pager.status().set_error("Quit or not ? Y/n"))
      .action(KeyEvent::simple(KeyCode::Char('y')), move |_| {
        ctrl.borrow_mut().quit();
      })
  }
}
