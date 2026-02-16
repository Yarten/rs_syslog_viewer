use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, MutexGuard};

/// 记录着贯穿整个 viewer 的统计数据
#[derive(Default)]
pub struct DataBoard {
  /// 从日志中发现的标签集合，有序，用于过滤日志，布尔值代表是否选中
  ordered_tags: Mutex<BTreeMap<String, bool>>,

  /// 用于快速查重的标签集合，无序
  hashed_tags: Mutex<HashSet<String>>,
}

impl DataBoard {
  /// 记录潜在可能得首次出现的日志标签
  pub fn update_tag(&self, new_tag: &String) {
    let mut ordered_tags = self.ordered_tags.lock().unwrap();
    let mut hashed_tags = self.hashed_tags.lock().unwrap();

    // 我们大多数情况是查询，少量情况是插入，使用 HashSet 查询更快一些，
    // 但我们又希望展示时有序，因此组合了两种数据结构
    if !hashed_tags.contains(new_tag) {
      ordered_tags.insert(new_tag.clone(), true);
      hashed_tags.insert(new_tag.clone());
    }
  }

  /// 获取所有的日志标签（有序）
  pub fn get_tags(&'_ self) -> MutexGuard<'_, BTreeMap<String, bool>> {
    self.ordered_tags.lock().unwrap()
  }
}
