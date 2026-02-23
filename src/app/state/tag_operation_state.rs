use crate::{
  app::{StateBuilder, ViewPortStateEx, controller::TagController},
  ui::{KeyEventEx, State},
};
use crossterm::event::{KeyCode, KeyEvent};
use std::{cell::RefCell, rc::Rc};

pub struct TagOperationState {
  /// 标签数据控制器
  tag_controller: Rc<RefCell<TagController>>,

  /// 被构建的状态
  state: State,
}

impl TagOperationState {
  pub fn new(tag_controller: Rc<RefCell<TagController>>) -> Self {
    Self {
      tag_controller,
      state: State::new("tag operation"),
    }
  }

  fn action(mut self, event: KeyEvent, mut act: impl FnMut(&mut TagController) + 'static) -> Self {
    let ctrl = self.tag_controller.clone();
    self.state = self.state.action(event, move |_| {
      act(&mut ctrl.borrow_mut());
    });
    self
  }
}

impl StateBuilder for TagOperationState {
  fn build(self) -> State {
    let c1 = self.tag_controller.clone();
    let c2 = c1.clone();
    let c3 = c1.clone();

    self
      .action(KeyEvent::simple(KeyCode::Enter), |ctrl| ctrl.toggle())
      .action(KeyEvent::ctrl('y'), |ctrl| ctrl.set_all())
      .action(KeyEvent::ctrl('n'), |ctrl| ctrl.unset_all())
      .action(KeyEvent::ctrl('h'), |ctrl| ctrl.toggle_all())
      .state
      .view_port(c1, false)
      .input("Tags", move |s| c2.borrow_mut().search(s.to_string()))
      .enter_action(move |pager| {
        pager
          .status()
          .reset_input(c3.borrow().get_curr_search().to_string());
      })
  }
}
