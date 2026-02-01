use chrono::{DateTime, FixedOffset};
use std::iter::Enumerate;
use crate::log::LogLine;

/// 索引日志内容中的某一行日志，可以和日志内容的迭代器互相转换
#[derive(PartialEq, Debug)]
pub struct Index {
  chunk_index: usize,
  line_index: usize,
}

impl Index {
  pub fn new(chunk_index: usize, line_index: usize) -> Self {
    Self { chunk_index, line_index }
  }
}

/// 维护连续的一个日志行 buffer
struct Chunk {
  /// 存储的数据
  lines: Vec<LogLine>,

  /// 实际存储上，我们总是从索引 0 往后插入，但实际含义上，插入的数据顺序可以是颠倒的
  reversed: bool,
}

impl Chunk {
  /// 新建一个 chunk，将预分配内容，并指定元素顺序。
  /// 我们总是从 lines 的尾部添加新元素，但根据 reversed 参数的值
  /// 来定义这些值逻辑上的真实阅读顺序
  fn new(capacity: usize, reversed: bool) -> Self {
    Self {
      lines: Vec::with_capacity(capacity),
      reversed,
    }
  }

  /// 插入新元素
  fn push(&mut self, line: LogLine) {
    self.lines.push(line);
  }

  /// 本 chunk 是否是空的
  fn is_empty(&self) -> bool {
    self.lines.is_empty()
  }

  /// 本 chunk 是否已经满，再插入会导致内存重分配
  fn is_full(&self) -> bool {
    self.lines.len() == self.lines.capacity()
  }

  /// 检查本 chunk 是正向序还是逆向序（从头部插入）
  fn is_reversed(&self) -> bool {
    self.reversed
  }

  /// 获得双向迭代器
  fn iter(&'_ self) -> ChunkIter<'_> {
    ChunkIter {
      lines_iter: self.lines.iter().enumerate(),
      reversed: self.reversed,
      lines_count: self.lines.len(),
    }
  }

  /// 获取最早的日志时间点（如果日志全都是损坏的，或者没有日志记录，则返回 None）
  /// 由于系统日志的时间来自当时的系统事件，而系统时间可能由于 RTC 或 PTP 的因素，
  /// 导致时间值准确的，因此无法假定 first time 一定小于 last time
  fn first_time(&self) -> Option<DateTime<FixedOffset>> {
    todo!()
  }
}

struct ChunkIter<'a> {
  lines_iter: Enumerate<std::slice::Iter<'a, LogLine>>,
  reversed: bool,
  lines_count: usize,
}

struct ChunkIter2<'a, I>
where
  I: DoubleEndedIterator<Item = (usize, &'a LogLine)>,
{
  lines_iter: I,
  reversed: bool,
  lines_count: usize,
}

impl<'a> ChunkIter<'a> {
  fn reverse_indexed_data(lines_count: usize, elem: Option<(usize, &'a LogLine)>) -> Option<(usize, &'a LogLine)> {
    match elem {
      None => { None },
      Some((index, data)) => {
        Some((lines_count - 1 - index, data))
      }
    }
  }
}

impl<'a> Iterator for ChunkIter<'a> {
  type Item = (usize, &'a LogLine);

  fn next(&mut self) -> Option<Self::Item> {
    if self.reversed {
      Self::reverse_indexed_data(self.lines_count, self.lines_iter.next_back())
    } else {
      self.lines_iter.next()
    }
  }
}

impl<'a> DoubleEndedIterator for ChunkIter<'a> {
  fn next_back(&mut self) -> Option<Self::Item> {
    if self.reversed {
      Self::reverse_indexed_data(self.lines_count, self.lines_iter.next())
    } else {
      self.lines_iter.next_back()
    }
  }
}

pub struct LogFileContent {
  chunks: Vec<Box<Chunk>>,
  chunk_capacity: usize,
}

impl LogFileContent {
  /// 新建日志内容
  pub fn new(chunk_capacity: usize) -> Self {
    Self {
      chunks: Vec::new(),
      chunk_capacity
    }
  }

  /// 文件内容是否为空
  pub fn is_empty(&self) -> bool {
    self.chunks.is_empty()
  }

  /// 在头部插入新日志行
  pub fn push_front(&mut self, line: LogLine) {
    if self.should_extend_front() {
      self.chunks.insert(0, self.new_chunk(true));
    }

    if let Some(chunk) = self.chunks.first_mut() {
      chunk.push(line);
    }
  }

  /// 在尾部插入新日志行
  pub fn push_back(&mut self, line: LogLine) {
    if self.should_extend_back() {
      self.chunks.push(self.new_chunk(false));
    }

    if let Some(chunk) = self.chunks.last_mut() {
      chunk.push(line);
    }
  }

  /// 检查是否应该在头部插入新的 chunk
  fn should_extend_front(&self) -> bool {
    match self.chunks.first() {
      None => { true }
      Some(first) => {
        !first.is_reversed() || first.is_full()
      }
    }
  }

  /// 检查是否应该在尾部插入新的 chunk
  fn should_extend_back(&self) -> bool {
    match self.chunks.last() {
      None => { true },
      Some(last) => {
        last.is_reversed() || last.is_full()
      }
    }
  }

  /// 是否本类维护的参数，新建 chunk
  fn new_chunk(&self, reserved: bool) -> Box<Chunk> {
    Box::new(Chunk::new(self.chunk_capacity, reserved))
  }

  /// 获得迭代器，支持双向搜索
  fn iter(&'_ self) -> Iter<'_> {
    Iter {
      chunks_iter: self.chunks.iter().enumerate(),
      chunk_index: 0,
      lines_iter: None,
    }
  }

  // fn iter_by_index(&'_ self, index: &Index) -> Iter<'_> {
  //
  // }
}

impl Default for LogFileContent {
  /// 创建默认 chunk 大小的日志内容
  fn default() -> Self {
    Self::new(512)
  }
}

struct Iter<'a> {
  chunks_iter: Enumerate<std::slice::Iter<'a, Box<Chunk>>>,
  chunk_index: usize,
  lines_iter: Option<ChunkIter<'a>>,
}

impl<'a> Iterator for Iter<'a> {
  type Item = (Index, &'a LogLine);

  fn next(&mut self) -> Option<Self::Item> {
    loop {
      match self.lines_iter.as_mut() {
        None => match self.chunks_iter.next() {
          None => {
            break None
          },
          Some((chunk_index, chunk)) => {
            self.lines_iter = Some(chunk.iter());
            self.chunk_index = chunk_index;
            continue
          }
        },
        Some(lines_iter) => match lines_iter.next() {
          None => {
            self.lines_iter = None;
            continue
          }
          Some((line_index, line)) => {
            break Some((Index::new(self.chunk_index, line_index), line));
          }
        }
      }
    }
  }
}


impl<'a> DoubleEndedIterator for Iter<'a> {
  fn next_back(&mut self) -> Option<Self::Item> {
    loop {
      match self.lines_iter.as_mut() {
        None => match self.chunks_iter.next_back() {
          None => {
            break None
          },
          Some((chunk_index, chunk)) => {
            self.lines_iter = Some(chunk.iter());
            self.chunk_index = chunk_index;
            continue
          }
        },
        Some(lines_iter) => match lines_iter.next_back() {
          None => {
            self.lines_iter = None;
            continue
          },
          Some((line_index, line)) => {
            break Some((Index::new(self.chunk_index, line_index), line));
          }
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_log_file_content() {
    let mut content = LogFileContent::default();
    content.push_back(LogLine::new("aaa".to_string()));
    content.push_back(LogLine::new("bbb".to_string()));
    content.push_front(LogLine::new("222".to_string()));
    content.push_front(LogLine::new("111".to_string()));

    let mut iter = content.iter();
    assert_eq!(iter.next(), Some((Index::new(0, 0), &LogLine::new("111".to_string()))));
    assert_eq!(iter.next(), Some((Index::new(0, 1), &LogLine::new("222".to_string()))));
    assert_eq!(iter.next(), Some((Index::new(1, 0), &LogLine::new("aaa".to_string()))));
    assert_eq!(iter.next(), Some((Index::new(1, 1), &LogLine::new("bbb".to_string()))));
    assert_eq!(iter.next(), None);

    let mut iter = content.iter();
    assert_eq!(iter.next_back(), Some((Index::new(1, 1), &LogLine::new("bbb".to_string()))));
    assert_eq!(iter.next_back(), Some((Index::new(1, 0), &LogLine::new("aaa".to_string()))));
    assert_eq!(iter.next_back(), Some((Index::new(0, 1), &LogLine::new("222".to_string()))));
    assert_eq!(iter.next_back(), Some((Index::new(0, 0), &LogLine::new("111".to_string()))));
    assert_eq!(iter.next_back(), None);
  }
}