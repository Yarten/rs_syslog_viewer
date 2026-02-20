use crate::log::{IterNextNth, LogLine};

/// 索引日志内容中的某一行日志，可以和日志内容的迭代器互相转换
#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub struct Index {
  chunk_index: usize,
  line_index: usize,
}

impl Index {
  pub(super) fn new(chunk_index: usize, line_index: usize) -> Self {
    Self {
      chunk_index,
      line_index,
    }
  }

  pub(super) fn zero() -> Self {
    Self::new(0, 0)
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

  /// 日志行数量
  fn len(&self) -> usize {
    self.lines.len()
  }

  /// 本 chunk 是否已经满，再插入会导致内存重分配
  fn is_full(&self) -> bool {
    self.lines.len() == self.lines.capacity()
  }

  /// 检查本 chunk 是正向序还是逆向序（从头部插入）
  fn is_reversed(&self) -> bool {
    self.reversed
  }

  /// 获取指定索引的数据
  fn get(&'_ self, i: usize) -> Option<&'_ LogLine> {
    self.lines.get(self.get_real_index(i))
  }

  /// 获取指定索引的可变数据
  fn get_mut<'a>(&mut self, i: usize) -> Option<&'a mut LogLine> {
    let i = self.get_real_index(i);

    if i >= self.lines.len() {
      None
    } else {
      let line = unsafe { &mut *(self.lines.get_unchecked_mut(i) as *mut LogLine) };
      Some(line)
    }
  }

  /// 给定逻辑上的数据索引，获取真实的数据索引
  fn get_real_index(&self, i: usize) -> usize {
    if self.reversed {
      self.lines.len().overflowing_sub(1).0.overflowing_sub(i).0
    } else {
      i
    }
  }
}

pub struct LogFileContent {
  chunks: Vec<Chunk>,
  chunk_capacity: usize,
}

impl LogFileContent {
  /// 新建日志内容
  pub fn new(chunk_capacity: usize) -> Self {
    Self {
      chunks: Vec::new(),
      chunk_capacity,
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
      None => true,
      Some(first) => !first.is_reversed() || first.is_full(),
    }
  }

  /// 检查是否应该在尾部插入新的 chunk
  fn should_extend_back(&self) -> bool {
    match self.chunks.last() {
      None => true,
      Some(last) => last.is_reversed() || last.is_full(),
    }
  }

  /// 是否本类维护的参数，新建 chunk
  fn new_chunk(&self, reserved: bool) -> Chunk {
    Chunk::new(self.chunk_capacity, reserved)
  }

  /// 获取指向第一条日志的索引
  pub fn first_index(&self) -> Index {
    Index::zero()
  }

  /// 获取指向最后一条日志的索引
  pub fn last_index(&self) -> Index {
    let chunk_index = self.chunks.len().saturating_sub(1);
    let line_index = match self.chunks.get(chunk_index) {
      None => 0,
      Some(chunk) => chunk.len().saturating_sub(1),
    };
    Index::new(chunk_index, line_index)
  }

  /// 将给定索引移动指定的步长。若移动结束时指向了有效的数据，则返回新的索引，
  /// 若移动结束时发现索引越界，则返回剩余需要移动的步长。
  pub fn step_index(&self, mut index: Index, mut n: isize) -> Result<Index, isize> {
    // 获取当前索引的 chunk，如果不存在，则终止处理
    let mut chunk = self.chunks.get(index.chunk_index).ok_or(n)?;

    loop {
      // 检查当前索引指向的条目在正确的数据范围内，如果否，则终止处理
      let chunk_size = chunk.len();
      if index.line_index >= chunk_size {
        break Err(n);
      }

      // 若 n 为 0，则结束处理，已经找到指定的日志索引
      if n == 0 {
        break Ok(index);
      }

      // 尝试更新行索引（可能会超出 chunk 范围）
      let next_line_index = index.line_index as isize + n;

      if next_line_index < 0 {
        // 若行超出 chunk 下界，则往前迭代 chunk，若没有前面已经没有 chunk，则终止处理
        n = next_line_index + 1;
        index.chunk_index = index.chunk_index.overflowing_sub(1).0;
        chunk = self.chunks.get(index.chunk_index).ok_or(n)?;
        index.line_index = chunk.len().saturating_sub(1);
      } else if next_line_index >= chunk_size as isize {
        // 若超出 chunk 上届，则往后迭代 chunk，若后边已经没有 chunk，则终止处理
        n = next_line_index - chunk_size as isize;
        index.chunk_index += 1;
        index.line_index = 0;
        chunk = self.chunks.get(index.chunk_index).ok_or(n)?;
      } else {
        // 移动之后刚好落在本 chunk 内，返回更新后的索引
        index.line_index = next_line_index as usize;
        break Ok(index);
      }
    }
  }

  /// 给定索引，获取日志行数据
  pub fn get(&self, index: Index) -> Option<&LogLine> {
    self.chunks.get(index.chunk_index)?.get(index.line_index)
  }

  /// 给定索引，获取可变的日志行数据
  pub fn get_mut<'a>(&mut self, index: Index) -> Option<&'a mut LogLine> {
    self
      .chunks
      .get_mut(index.chunk_index)?
      .get_mut(index.line_index)
  }
}

impl Default for LogFileContent {
  /// 创建默认 chunk 大小的日志内容
  fn default() -> Self {
    Self::new(512)
  }
}

// 定义迭代器及其获取接口
crate::define_all_iterators!(LogFileContent, Index);

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

    let mut iter = content.iter_forward_from(Index::new(0, 1));
    assert_eq!(
      iter.next(),
      Some((Index::new(0, 1), &LogLine::new("222".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(1, 0), &LogLine::new("aaa".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(1, 1), &LogLine::new("bbb".to_string())))
    );
    assert_eq!(iter.next(), None);

    let mut iter = content.iter_backward_from(Index::new(1, 0));
    assert_eq!(
      iter.next(),
      Some((Index::new(1, 0), &LogLine::new("aaa".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(0, 1), &LogLine::new("222".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(0, 0), &LogLine::new("111".to_string())))
    );
    assert_eq!(iter.next(), None);

    let mut iter = content.iter_forward_from_head();
    assert_eq!(
      iter.next(),
      Some((Index::new(0, 0), &LogLine::new("111".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(0, 1), &LogLine::new("222".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(1, 0), &LogLine::new("aaa".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(1, 1), &LogLine::new("bbb".to_string())))
    );
    assert_eq!(iter.next(), None);

    let mut iter = content.iter_backward_from_tail();
    assert_eq!(
      iter.next(),
      Some((Index::new(1, 1), &LogLine::new("bbb".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(1, 0), &LogLine::new("aaa".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(0, 1), &LogLine::new("222".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(0, 0), &LogLine::new("111".to_string())))
    );
    assert_eq!(iter.next(), None);

    let mut iter = content.iter_forward_from_head();
    assert_eq!(
      iter.next_nth(2).ok(),
      Some((Index::new(1, 0), &LogLine::new("aaa".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(1, 1), &LogLine::new("bbb".to_string())))
    );

    let mut iter = content.iter_backward_from_tail();
    assert_eq!(
      iter.next_nth(2).ok(),
      Some((Index::new(0, 1), &LogLine::new("222".to_string())))
    );
    assert_eq!(
      iter.next(),
      Some((Index::new(0, 0), &LogLine::new("111".to_string())))
    );

    let mut iter = content.iter_forward_from_head();
    assert_eq!(iter.next_nth(6), Err(2));

    let mut iter = content.iter_backward_from_tail();
    assert_eq!(iter.next_nth(6), Err(2));
  }
}
