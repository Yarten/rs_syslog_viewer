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

  let true_tags: BTreeSet<String> = true_content
    .iter()
    .filter_map(|line| match line {
      LogLine::Good(line) => Some(line),
      LogLine::Bad(_) => None,
    })
    .map(|x| x.tag.clone())
    .collect();

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

  let mut content: Vec<LogLine> = Vec::new();
  for (_, line) in log_file.iter_forward_from_head() {
    content.push(line.clone());
  }

  assert_eq!(&content, &true_content);
  assert_eq!(&*data_board.get_tags(), &true_tags);
}
