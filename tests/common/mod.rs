use std::collections::LinkedList;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub fn get_test_log() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/test.log")
}

pub fn read_file_as_lines(filename: &Path) -> LinkedList<String> {
  let file = File::open(filename).expect("file not found");
  let reader = BufReader::new(file);
  reader.lines().flatten().collect()
}
