use crate::file::{
  Event,
};
use crate::log::{
  LogFileContent,
};

#[derive(Default)]
pub struct LogFile {
  content: LogFileContent,
}

impl LogFile {
  fn test() {

  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn test1() {
    let x = vec![1; 5];
    x.iter().next_back();
    for (i, j) in x.iter().enumerate().nth(3) {
      println!("x[{}] = {}", i, j);
    }
  }
}