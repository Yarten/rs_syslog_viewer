/// 日志事件的定义
pub enum Event {
  /// 普通的定周期事件，代表没有什么特殊的事情发生
  Tick,

  /// 代表本日志被删除
  Removed,
}
