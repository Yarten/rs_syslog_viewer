use crate::{app::controller::LogController, ui::State};
use crossterm::event::KeyEvent;
use std::{cell::RefCell, rc::Rc};

pub(super) struct LogStateKit {
  /// 日志数据控制器
  pub log_controller: Rc<RefCell<LogController>>,

  /// 被构建的状态
  pub state: State,
}

impl LogStateKit {
  pub fn new(log_controller: Rc<RefCell<LogController>>, state_name: &str) -> Self {
    Self {
      log_controller,
      state: State::new(state_name),
    }
  }

  pub fn action(mut self, event: KeyEvent, act: impl Fn(&mut LogController) + 'static) -> Self {
    let ctrl = self.log_controller.clone();
    self.state = self.state.action(event, move |_| {
      act(&mut ctrl.borrow_mut());
    });
    self
  }
}
