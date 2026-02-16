use rs_syslog_viewer::log::{Config, DataBoard, LogLine, RotatedLog};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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

  let true_content: Vec<LogLine> = common::read_file_as_lines(&postfix(&log_path, 2))
    .into_iter()
    .chain(common::read_file_as_lines(&postfix(&log_path, 1)).into_iter())
    .chain(common::read_file_as_lines(&log_path).into_iter())
    .map(|line| LogLine::new(line))
    .collect();
  let true_reversed_content: Vec<LogLine> = true_content.iter().rev().cloned().collect();
  let true_tags: BTreeSet<String> = common::all_tags(&true_content);

  let data_board = Arc::new(DataBoard::default());
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

  let content: Vec<LogLine> = common::collect_lines(log.iter_forward_from_head());
  let reversed_content: Vec<LogLine> = common::collect_lines(log.iter_backward_from_tail());
  let tags: BTreeSet<String> = data_board.get_tags().keys().cloned().collect();

  assert_eq!(&content, &true_content);
  assert_eq!(&reversed_content, &true_reversed_content);
  assert_eq!(&tags, &true_tags);
}
