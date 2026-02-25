use crate::{
  app::Controller,
  ui::{KeyEventEx, State, ViewPort},
};
use crossterm::event::{KeyCode, KeyEvent};
use std::{cell::RefCell, rc::Rc};

mod debug_operation_state;
mod log_content_searched_state;
mod log_content_searching_state;
mod log_navigation_state;
mod log_state_kit;
mod log_timestamp_searched_state;
mod log_timestamp_searching_state;
mod quit_state;
mod tag_operation_state;

pub use debug_operation_state::DebugOperationState;
pub use log_content_searched_state::LogContentSearchedState;
pub use log_content_searching_state::LogContentSearchingState;
pub use log_navigation_state::LogNavigationState;
pub use log_timestamp_searched_state::LogTimestampSearchedState;
pub use log_timestamp_searching_state::LogTimestampSearchingState;
pub use quit_state::QuitState;
pub use tag_operation_state::TagOperationState;

pub trait StateBuilder {
  /// 构建 sm 的一个状态
  fn build(self) -> State;
}

/// 用于扩展 State 以支持数据导航能力
pub trait ViewPortStateEx {
  fn view_port(self, ctrl: Rc<RefCell<dyn Controller>>, scrollable: bool) -> Self;
}

impl ViewPortStateEx for State {
  fn view_port(mut self, ctrl: Rc<RefCell<dyn Controller>>, scrollable: bool) -> Self {
    fn action(
      state: State,
      code: KeyCode,
      ctrl: Rc<RefCell<dyn Controller>>,
      mut act: impl FnMut(&mut ViewPort) + 'static,
    ) -> State {
      state.action(KeyEvent::simple(code), move |_| {
        if let Some(view_port) = ctrl.borrow_mut().view_port() {
          act(view_port);
        }
      })
    }

    // 启用横向滚动条能力，配置相关按键事件。
    if scrollable {
      if let Some(view_port) = ctrl.borrow_mut().view_port() {
        view_port.enable_horizontal_scroll();
        self = action(self, KeyCode::Left, ctrl.clone(), |v| {
          v.want_scroll_horizontally(-1)
        });
        self = action(self, KeyCode::Right, ctrl.clone(), |v| {
          v.want_scroll_horizontally(1)
        });
      }
    }

    self = action(self, KeyCode::Up, ctrl.clone(), |v| v.want_move_cursor(-1));
    self = action(self, KeyCode::Down, ctrl.clone(), |v| v.want_move_cursor(1));
    self = action(self, KeyCode::PageUp, ctrl.clone(), |v| v.want_page_up());
    self = action(self, KeyCode::PageDown, ctrl.clone(), |v| {
      v.want_page_down()
    });
    self
  }
}
