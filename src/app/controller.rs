use crate::{app::LogHubRef, ui::ViewPort};

mod debug_controller;
mod log_controller;
mod tag_controller;

pub use debug_controller::DebugController;
pub use log_controller::LogController;
pub use tag_controller::TagController;

/// 维护一个页面所需的操作接口、数据接口的逻辑控制器，实现 App 功能
pub trait Controller {
  /// 在 App 主处理循环中，对日志数据进行一次逻辑控制处理，
  /// 需要结合其他控制信息，实现功能，找出渲染所需的数据。
  fn run_once(&mut self, data: &mut LogHubRef);

  /// 返回是否应该结束程序
  fn should_quit(&self) -> bool {
    false
  }

  /// 导航用的展示区 UI 数据，可供 State 结合导航相关按键，进行响应处理，
  /// 由 [ViewPortStateEx] 提供能力扩展。
  fn view_port(&mut self) -> Option<&mut ViewPort> {
    None
  }
}
