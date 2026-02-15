use rs_syslog_viewer::log::LogLine;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub fn get_test_log() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/test.log")
}

pub fn read_file_as_lines(filename: &Path) -> Vec<String> {
  let file = File::open(filename).expect("file not found");
  let reader = BufReader::new(file);
  reader.lines().flatten().collect()
}

pub fn all_tags(content: &Vec<LogLine>) -> BTreeSet<String> {
  content
    .iter()
    .filter_map(|line| match line {
      LogLine::Good(line) => Some(line),
      LogLine::Bad(_) => None,
    })
    .map(|x| x.tag.clone())
    .collect()
}

pub fn collect_lines<'a, I>(i: impl Iterator<Item = (I, &'a LogLine)>) -> Vec<LogLine> {
  i.map(|(_, line)| line.clone()).collect()
}
