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

impl TagController {
  pub fn toggle(&mut self) {
    self.control = Control::Toggle;
  }

  pub fn set_all(&mut self) {
    self.control = Control::SetAll;
  }

  pub fn unset_all(&mut self) {
    self.control = Control::UnsetAll;
  }

  pub fn toggle_all(&mut self) {
    self.control = Control::ToggleAll;
  }

  pub fn search(&mut self, input: String) {
    self.curr_search = input;
  }

  pub fn get_curr_search(&self) -> &str {
    &self.curr_search
  }

  pub fn view_mut(&mut self) -> &mut ViewPort {
    &mut self.view_port
  }
}

impl Controller for TagController {
  fn run_once(&mut self, data: &mut LogHubRef) {
    // 响应列表区的操作，获取光标指向的数据
    let cursor_data = self.view_port.apply().cloned();

    // 响应选择控制
    self.apply_control(data, cursor_data.as_ref().map(|(k, _)| k));

    // 处理搜索的变更
    self.apply_search(data);

    // 更新标签版本
    data.data_board().get_tags_mut().update_version();

    // 填充数据
    self.view_port.fill(
      &self.matched_tags,
      // 保持上一帧光标的位置，或者重新取第一个标签为光标的位置
      cursor_data
        .map(|(k, _)| k)
        .or_else(|| self.matched_tags.first_key_value().map(|(k, _)| k.clone()))
        .unwrap_or(String::new()),
    )
  }

  fn view_port(&mut self) -> Option<&mut ViewPortBase> {
    Some(&mut self.view_port.ui)
  }
}

impl TagController {
  /// 对展示区内的所有标签（被过滤出来的、包括界面外的不可见项），处理它们的激活或关闭
  fn apply_control(&mut self, data: &mut LogHubRef, cursor_key: Option<&String>) {
    // 数据黑板中的标签记录
    let tags = data.data_board().get_tags_mut();

    match self.control {
      Control::Idle => {}
      Control::Toggle => {
        if let Some(key) = cursor_key {
          let value = self.matched_tags[key];
          *self.matched_tags.get_mut(key).unwrap() = !value;
          tags.toggle(key);
        }
      }
      Control::SetAll => {
        self.matched_tags.iter_mut().for_each(|(k, v)| {
          *v = true;
          tags.set(k);
        });
      }
      Control::UnsetAll => {
        self.matched_tags.iter_mut().for_each(|(k, v)| {
          *v = false;
          tags.unset(k);
        });
      }
      Control::ToggleAll => {
        self.matched_tags.iter_mut().for_each(|(k, v)| {
          *v = !*v;
          tags.toggle(k);
        });
      }
    }

    // 重置控制量
    self.control = Control::Idle;
  }

  /// 响应搜索信息的变更
  fn apply_search(&mut self, data: &mut LogHubRef) {
    if self.curr_search.len() > self.last_search.len()
      && self.curr_search.starts_with(&self.last_search)
    {
      // 搜搜字符串变长，条件更苛刻，检查是否有之前匹配的 key，现在不匹配了
      let matched_tags = std::mem::take(&mut self.matched_tags);
      self.matched_tags = matched_tags
        .into_iter()
        .filter_map(|(k, v)| {
          if k.find(&self.curr_search).is_none() {
            self.unmatched_tags.insert(k, v);
            None
          } else {
            Some((k, v))
          }
        })
        .collect();
    } else if self.curr_search.len() < self.last_search.len()
      && self.last_search.starts_with(&self.curr_search)
    {
      // 搜索字符串变短，条件根宽松，检查是否有之前不匹配的 key，现在匹配了
      let unmatched_tags = std::mem::take(&mut self.unmatched_tags);
      self.unmatched_tags = unmatched_tags
        .into_iter()
        .filter_map(|(k, v)| {
          if k.find(&self.curr_search).is_some() {
            self.matched_tags.insert(k, v);
            None
          } else {
            Some((k, v))
          }
        })
        .collect();
    } else if self.curr_search != self.last_search {
      // 若搜索字符串和之前的搜索字符串变化太大，那么需要重新分析所有的标签
      let matched_tags = std::mem::take(&mut self.matched_tags);
      let unmatched_tags = std::mem::take(&mut self.unmatched_tags);
      self.match_tags(matched_tags);
      self.match_tags(unmatched_tags);
    }

    // 取出新增的标签，根据搜索结果匹配到各个集合中
    self.match_tags(
      data
        .data_board()
        .get_tags_mut()
        .take_updated()
        .into_iter()
        .map(|k| (k, true))
        .collect(),
    );

    // 记录新的变更
    self.last_search = self.curr_search.clone();
  }

  /// 将搜索字符串匹配标签值，并根据结果加入到对应的集合中
  fn match_tags(&mut self, tags: BTreeMap<String, bool>) {
    tags.into_iter().for_each(|(k, v)| {
      if k.find(&self.curr_search).is_none() {
        self.unmatched_tags.insert(k, v);
      } else {
        self.matched_tags.insert(k, v);
      }
    });
  }
}
