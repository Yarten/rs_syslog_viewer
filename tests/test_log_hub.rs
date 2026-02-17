use rs_syslog_viewer::app::{LogHub, LogHubData};
use rs_syslog_viewer::log::{Config, Index, LogLine};
use std::collections::BTreeSet;

mod common;

#[tokio::test]
async fn test_log_hub() {
  let names = ["test", "user"];
  let root = common::get_test_root();

  let mut true_content = names
    .iter()
    .map(|name| common::read_all_files_as_lines(&root, name))
    .flatten()
    .flatten()
    .collect::<Vec<LogLine>>();
  true_content.sort_by(LogLine::is_older);
  let true_reversed_content: Vec<LogLine> = true_content.iter().rev().cloned().collect();

  let mut log_hub = LogHub::open(
    &root,
    names
      .iter()
      .map(|name| (name.to_string(), Config::default()))
      .collect(),
  );

  for i in 0..5 {
    tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
    let mut data = log_hub.data().await;
    let first_index = data.first_index();
    data.try_load_older_logs(first_index);
  }

  let data = log_hub.data().await;
  let content: Vec<LogLine> = common::collect_lines(data.iter_forward_from_head());
  let reversed_content: Vec<LogLine> = common::collect_lines(data.iter_backward_from_tail());

  assert_eq!(&content, &true_content);
  assert_eq!(&reversed_content, &true_reversed_content);
}
