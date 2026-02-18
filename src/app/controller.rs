use crate::app::LogHubData;

/// 维护一个页面所需的操作接口、数据接口的逻辑控制器，实现 App 功能
pub trait Controller {
  /// 在 App 主处理循环中，对日志数据进行一次逻辑控制处理，
  /// 需要结合其他控制信息，实现功能，找出渲染所需的数据。
  fn run_once(&mut self, data: &mut LogHubData);

  /// 返回是否应该结束程序
  fn should_quit(&self) -> bool {
    false
  }
}
