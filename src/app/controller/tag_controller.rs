use crate::{
  app::{Controller, LogHubRef},
  log::LogDirection,
  ui::CursorEx,
};
use std::collections::BTreeMap;

/// 展示区里维护的数据条目
type Item = (String, bool);

impl CursorEx for Option<&Item> {
  type Key = String;
  type Value = bool;

  fn key(self) -> Option<String> {
    self.map(|x| x.0.clone())
  }
}

// 定义标签展示区的可视化数据
crate::view_port!(ViewPort, Item);

impl ViewPort {
  /// 根据已经配置好的光标位置，从指定键值处，开始填充数据
  fn fill(&mut self, tags: &BTreeMap<String, bool>, cursor: String) {
    let mut iter_down = tags.range(cursor.clone()..);
    let mut iter_up = tags.range(..cursor).rev();

    self.do_fill(|dir| match dir {
      LogDirection::Forward => iter_down.next().map(|(a, b)| (a.clone(), b.clone())),
      LogDirection::Backward => iter_up.next().map(|(a, b)| (a.clone(), b.clone())),
    })
  }
}

/// 描述本帧内的控制
#[derive(Default)]
enum Control {
  /// 没有动作，光标将停在上一帧的位置
  #[default]
  Idle,

  /// 变更光标所在行的标签激活状态
  Toggle,

  /// 搜索范围内的所有标签激活
  SetAll,

  /// 搜索范围内的所有标签关闭
  UnsetAll,

  /// 搜索范围内的所有标签反选
  ToggleAll,
}

/// 标签展示区的控制器
#[derive(Default)]
pub struct TagController {
  /// 当帧需要处理的控制
  control: Control,

  ///展示区的数据
  view_port: ViewPort,

  /// 和搜索匹配的标签集
  matched_tags: BTreeMap<String, bool>,

  /// 不和搜索匹配的标签集
  unmatched_tags: BTreeMap<String, bool>,

  /// 上一帧的搜索
  last_search: String,

  /// 本帧的搜索，将对比前后两帧的搜索内容，尽可能优化查找过程
  curr_search: String,
}

impl Controller for TagController {
  fn run_once(&mut self, data: &mut LogHubRef) {}
}
