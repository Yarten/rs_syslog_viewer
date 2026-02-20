use itertools::Itertools;
use rs_syslog_viewer::log::{Config, DataBoard, Index, IterNextNth, LogLine, RotatedLog};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

mod common;

fn postfix(log_path: &Path, n: i32) -> PathBuf {
  let mut path = log_path.to_path_buf();
  path.set_file_name(format!(
    "{}.{}",
    path.file_name().unwrap().to_str().unwrap(),
    n
  ));
  path
}

#[tokio::test]
async fn test_rotated_log() {
  let log_path = common::get_test_log();

  // 真值
  let true_content: Vec<LogLine> =
    common::read_all_files_as_lines(&common::get_test_root(), "test").unwrap();
  let true_reversed_content: Vec<LogLine> = true_content.iter().rev().cloned().collect();
  let true_tags: BTreeSet<String> = common::all_tags(&true_content);

  // 测试核心功能，读取数据
  let data_board = Arc::new(Mutex::new(DataBoard::default()));
  let mut log = RotatedLog::new(log_path.clone(), Config::default());

  let start = Instant::now();

  while start.elapsed() < Duration::from_secs(2) {
    assert!(log.prepare().await);

    tokio::select! {
      _ = tokio::time::sleep(Duration::from_millis(300)) => {
        log.set_want_older_log();
      },
      _ = log.update(data_board.clone()) => {}
    }
  }

  // 测试迭代器
  let content: Vec<LogLine> = common::collect_lines(log.iter_forward_from_head());
  let reversed_content: Vec<LogLine> = common::collect_lines(log.iter_backward_from_tail());
  let tags: BTreeSet<String> = data_board
    .lock()
    .await
    .get_tags()
    .ordered()
    .keys()
    .cloned()
    .collect();

  assert_eq!(&content, &true_content);
  assert_eq!(&reversed_content, &true_reversed_content);
  assert_eq!(&tags, &true_tags);

  // 测试跳跃式访问
  let mut iter = log.iter_forward_from_head();
  let mut n_sum = 0;

  for n in [10, 50, 55, 100, 150, 500] {
    n_sum += n;
    let true_log = true_content.get(n_sum);
    n_sum += 1;

    match iter.next_nth(n) {
      Ok((_, log)) => {
        assert_eq!(Some(log), true_log);
      }
      Err(_) => {
        assert_eq!(None, true_log);
      }
    }
  }

  let mut iter = log.iter_backward_from_tail();
  let mut n_sum = true_content.len() - 1;

  for n in [10, 50, 55, 100, 150, 500] {
    n_sum = n_sum.overflowing_sub(n).0;
    let true_log = true_content.get(n_sum);
    n_sum = n_sum.overflowing_sub(1).0;

    match iter.next_nth(n) {
      Ok((_, log)) => {
        assert_eq!(Some(log), true_log);
      }
      Err(_) => {
        assert_eq!(None, true_log);
      }
    }
  }

  // 测试过滤访问
  for k in 0..=true_tags.len() {
    for comb in true_tags.iter().combinations(k) {
      // 本次选中的标签组合
      let comb: HashSet<String> = comb.into_iter().cloned().collect();

      // 更新数据黑板中的标记记录
      let mut data_board = data_board.lock().await;
      let mut tags = data_board.get_tags_mut();
      tags.update_version();

      let all_tags: HashSet<String> = tags.ordered().keys().cloned().collect();
      for tag in all_tags {
        if comb.contains(&tag) {
          tags.set(&tag);
        } else {
          tags.unset(&tag);
        }
      }

      // 构建真值
      let true_filtered_content: Vec<LogLine> = true_content
        .iter()
        .filter_map(|l| match l.get_tag() {
          None => Some(l),
          Some(tag) => match comb.contains(tag) {
            false => None,
            true => Some(l),
          },
        })
        .cloned()
        .collect();
      let true_reversed_filtered_content: Vec<LogLine> =
        true_filtered_content.iter().rev().cloned().collect();

      // 分析遍历结果是否正确，每次遍历都至少进行两次，第一次没有缓存、之后都有缓存
      for _ in 0..3 {
        let filtered_content: Vec<LogLine> =
          common::collect_mut_lines(log.filtered_iter_forward_from_head(&tags));
        assert_eq!(filtered_content, true_filtered_content);

        let filtered_content: Vec<LogLine> =
          common::collect_mut_lines(log.filtered_iter_backward_from_tail(&tags));
        assert_eq!(filtered_content, true_reversed_filtered_content);
      }
    }
  }
}
