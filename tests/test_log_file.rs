use rs_syslog_viewer::log::{Event, LogFile, LogLine};
use std::collections::HashSet;

mod common;

#[tokio::test]
async fn test_log_file() {
  let log_path = common::get_test_log();

  let true_content: Vec<LogLine> = common::read_file_as_lines(&log_path)
    .into_iter()
    .map(|line| LogLine::new(line))
    .collect();

  let true_tags: HashSet<String> = true_content
    .iter()
    .map_while(|line| match line {
      LogLine::Good(line) => Some(line),
      LogLine::Bad(_) => None,
    })
    .map(|x| x.tag.clone())
    .collect();

  let mut log_file = LogFile::open(log_path, false, HashSet::new())
    .await
    .expect("Could not open log file");

  let mut tags = HashSet::new();
  loop {
    tokio::select! {
      _ = tokio::time::sleep(tokio::time::Duration::from_millis(1000)) => {
        break;
      },
      Some(event) = log_file.update() => {
        match event {
          Event::Tick => {}
          Event::Removed => {}
          Event::NewTag(new_tag) => {
            tags.insert(new_tag);
          }
        }
      }
    }
  }

  let mut content: Vec<LogLine> = Vec::new();
  for (index, line) in log_file.iter_forward_from_head() {
    content.push(line.clone());
  }

  assert_eq!(&content, &true_content);
  assert_eq!(&tags, &true_tags);
}
