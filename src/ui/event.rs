#[derive(PartialEq, Clone, Copy)]
pub enum Event {
  /// 无按键事件发生
  Tick,

  /// 退出事件
  Quit,

  /// 有某个事件发生
  Some,
}
