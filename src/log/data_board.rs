use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::{collections::BTreeMap, path::PathBuf};

/// 从日志中发现的标签集合，用于过滤日志，布尔值代表是否选中
#[derive(Default)]
pub struct TagsData {
  /// 更新集，新的标签会添加在这里，默认值为 true
  updated_tags: HashSet<String>,

  /// 用于快速查重的记录
  hashed_tags: HashMap<String, bool>,

  /// 标签版本信息
  ver: usize,
}

impl TagsData {
  pub fn all(&self) -> &HashMap<String, bool> {
    &self.hashed_tags
  }

  pub fn set(&mut self, tag: &str) {
    self.set_value(tag, true);
  }

  pub fn unset(&mut self, tag: &str) {
    self.set_value(tag, false);
  }

  pub fn toggle(&mut self, tag: &str) {
    self.set_value(tag, !self.get(tag));
  }

  pub fn contains(&self, tag: &str) -> bool {
    self.hashed_tags.contains_key(tag)
  }

  pub fn get(&self, tag: &str) -> bool {
    match self.hashed_tags.get(tag) {
      Some(value) => *value,
      None => false,
    }
  }

  pub fn insert_new(&mut self, tag: &str) {
    self.hashed_tags.insert(tag.to_string(), true);
    self.updated_tags.insert(tag.to_string());
  }

  pub fn get_version(&self) -> usize {
    self.ver
  }

  pub fn update_version(&mut self) {
    self.ver += 1;
  }

  pub fn take_updated(&mut self) -> HashSet<String> {
    std::mem::take(&mut self.updated_tags)
  }

  fn set_value(&mut self, tag: &str, value: bool) {
    if let Some(flag) = self.hashed_tags.get_mut(tag) {
      *flag = value;
    }
  }
}

/// 记录着贯穿整个 viewer 的统计数据
#[derive(Default)]
pub struct DataBoard {
  /// 日志的标签数据
  tags: TagsData,

  /// 日志文件所在的根目录
  log_files_root: Arc<PathBuf>,
}

impl DataBoard {
  pub fn new(log_files_root: PathBuf) -> Self {
    Self {
      log_files_root: Arc::new(log_files_root),
      ..DataBoard::default()
    }
  }
}

impl DataBoard {
  /// 记录潜在可能得首次出现的日志标签
  pub fn update_tag(&mut self, new_tag: &str) {
    if !self.tags.contains(new_tag) {
      self.tags.insert_new(new_tag);
    }
  }

  /// 获取所有的日志标签的容器
  pub fn get_tags(&self) -> &TagsData {
    &self.tags
  }

  /// 获取所有的额日志标签的容器，但是可以修改
  pub fn get_tags_mut(&mut self) -> &mut TagsData {
    &mut self.tags
  }

  /// 获取日志所在的根目录
  pub fn get_root_path(&self) -> Arc<PathBuf> {
    self.log_files_root.clone()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::ops::DerefMut;

  #[test]
  fn test_tags_data() {
    let mut db = DataBoard::default();
    db.update_tag("test1");
    db.update_tag("test2");
    db.update_tag("test3");

    let mut true_tags: HashMap<String, bool> = HashMap::new();
    true_tags.insert("test1".to_string(), true);
    true_tags.insert("test2".to_string(), true);
    true_tags.insert("test3".to_string(), true);

    assert_eq!(db.get_tags().all(), &true_tags);
    assert_eq!(db.get_tags().get("test2"), true);

    db.get_tags_mut().unset("test3");
    assert_eq!(db.get_tags().get("test3"), false);
    db.get_tags_mut().set("test3");
    assert_eq!(db.get_tags().get("test3"), true);
    db.get_tags_mut().toggle("test3");
    assert_eq!(db.get_tags().get("test3"), false);
    db.get_tags_mut().toggle("test3");
    assert_eq!(db.get_tags().get("test3"), true);
  }
}
