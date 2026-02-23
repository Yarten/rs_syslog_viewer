use crate::{
  app::controller::{LogController, log_controller::Error},
  ui::State,
};
use crossterm::event::KeyEvent;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
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

  /// 设置一个按键动作
  pub fn action(mut self, event: KeyEvent, act: impl Fn(&mut LogController) + 'static) -> Self {
    let ctrl = self.log_controller.clone();
    self.state = self.state.action(event, move |_| {
      act(&mut ctrl.borrow_mut());
    });
    self
  }

  /// 设置读取到错误时，在状态栏展示的错误信息
  pub fn errors(mut self, errors: &[(Error, &str)]) -> Self {
    let ctrl = self.log_controller.clone();
    let errors: BTreeMap<Error, String> = errors
      .iter()
      .map(|(error, msg)| (*error, msg.to_string()))
      .collect();
    let mut is_error_reset = true;

    self.state = self.state.manual_action(move |pager| {
      if let Some(error) = ctrl.borrow_mut().take_error()
        && let Some(msg) = errors.get(&error)
      {
        pager.status().set_error(msg);
        is_error_reset = false;
      } else if !is_error_reset {
        is_error_reset = true;
        pager.status().reset_error();
      }
    });
    self
  }
}
