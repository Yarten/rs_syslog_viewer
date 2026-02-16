use rs_syslog_viewer::log::LogLine;
use std::collections::BTreeSet;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub fn get_test_root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

pub fn get_test_log() -> PathBuf {
  get_test_root().join("test.log")
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

pub fn read_all_files_as_lines(root: &Path, name: &str) -> Option<Vec<LogLine>> {
  let log_name = name.to_string() + ".log";
  let mut log_paths: BTreeSet<PathBuf> = BTreeSet::new();

  for entry in fs::read_dir(root).ok()? {
    let entry = entry.ok()?;

    // 跳过文件的情况（很少命中这种情况）
    if !entry.file_type().ok()?.is_file() {
      continue;
    }

    // 找到有本系统日志名称前缀的文件，它们就是和本系统日志相关的文件，接着处理它们
    if entry.file_name().to_str()?.starts_with(&log_name) {
      log_paths.insert(entry.path());
    }
  }

  let lines: Vec<LogLine> = log_paths
    .into_iter()
    .rev()
    .map(|path| read_file_as_lines(path.as_path()))
    .flatten()
    .map(|line| LogLine::new(line))
    .collect();

  Some(lines)
}
