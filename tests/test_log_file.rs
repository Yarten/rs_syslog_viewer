use rs_syslog_viewer::log::{DataBoard, Event, LogFile, LogLine};
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

mod common;

#[tokio::test]
async fn test_log_file() {
  let log_path = common::get_test_log();

  let true_content: Vec<LogLine> = common::read_file_as_lines(&log_path)
    .into_iter()
    .map(|line| LogLine::new(line))
    .collect();
  let true_reversed_content: Vec<LogLine> = true_content.iter().rev().cloned().collect();
  let true_tags: BTreeSet<String> = common::all_tags(&true_content);

  let data_board = Arc::new(DataBoard::default());
  let mut log_file = LogFile::open(log_path, false)
    .await
    .expect("Could not open log file");

  loop {
    tokio::select! {
      _ = tokio::time::sleep(tokio::time::Duration::from_millis(1000)) => {
        break;
      },
      _ = log_file.update(data_board.clone()) => {}
    }
  }

  let content: Vec<LogLine> = common::collect_lines(log_file.data().iter_forward_from_head());
  let reversed_content: Vec<LogLine> =
    common::collect_lines(log_file.data().iter_backward_from_tail());
  let tags: BTreeSet<String> = data_board.get_tags().keys().cloned().collect();

  assert_eq!(&content, &true_content);
  assert_eq!(&reversed_content, &true_reversed_content);
  assert_eq!(&tags, &true_tags);
}
