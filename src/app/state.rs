use crate::ui::State;

mod log_navigation_state;

pub trait StateBuilder {
  /// 构建 sm 的一个状态
  fn build(self) -> State;
}

pub use log_navigation_state::LogNavigationState;
