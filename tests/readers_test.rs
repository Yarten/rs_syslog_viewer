use rs_syslog_viewer::file::{Event, HeadReader, Reader, TailReader, reader::Config};
use std::collections::LinkedList;
use std::path::Path;

mod common;

async fn read_file<R>(path: &Path, true_content: &LinkedList<String>)
where
  R: Reader,
{
  let mut reader = R::open(path, Config::default())
    .await
    .expect("Failed to create reader");

  reader.start().await.expect("Failed to start reader");

  let mut content = LinkedList::new();
  loop {
    tokio::select! {
      _ = tokio::time::sleep(tokio::time::Duration::from_millis(1000)) => {
        break;
      },
      Some(event) = reader.changed() => {
        match event {
          Event::NewHead(s) => {
            println!("Head changed {:?}", s);
            content.push_front(s);
          }
          Event::NewTail(s) => {
            println!("Tail changed {:?}", s);
            content.push_back(s);
          }
          Event::Renamed(_) => {}
          Event::Removed => {}
        }
      }
    }
  }

  reader.stop().await.expect("Failed to stop reader");

  assert_eq!(&content, true_content);
}

#[tokio::test]
async fn readers_test() {
  let log_path = common::get_test_log();
  let true_content = common::read_file_as_lines(&log_path);
  println!("Test tail reader ...");
  read_file::<TailReader>(&log_path, &true_content).await;
  println!("Test head reader ...");
  read_file::<HeadReader>(&log_path, &true_content).await;
}
