use crate::log::LogDirection;

#[derive(Default)]
pub struct ViewPort {
  /// 展示区的高度，也即能够展示的日志行数量
  height: usize,

  /// 光标位置，指的是相对于 height 中的定位
  cursor: usize,

  /// 光标数据索引，指的是相对于 logs 中的定位，
  /// 它和 `cursor` 不一定重叠，特别是在新的一帧构建过程中，
  /// 因此，每帧渲染获取数据的时候，将进行光标重定位
  cursor_index: usize,

  /// 数据数量统计
  data_count: usize,
}

pub trait ViewPortEx {
  fn ui(&mut self) -> &mut ViewPort;

  fn set_height(&mut self, height: usize) -> &mut Self {
    self.ui().set_height(height);
    self
  }

  fn set_cursor(&mut self, cursor: usize) -> &mut Self {
    self.ui().set_cursor(cursor);
    self
  }

  fn set_cursor_at_top(&mut self) -> &mut Self {
    self.ui().set_cursor_at_top();
    self
  }

  fn set_cursor_at_bottom(&mut self) -> &mut Self {
    self.ui().set_cursor_at_bottom();
    self
  }
}

impl ViewPort {
  /// 光标的数据索引
  pub fn cursor(&self) -> usize {
    self.cursor
  }

  /// 设置展示区高度，同时钳制光标位置，防止越界
  pub fn set_height(&mut self, height: usize) -> &mut Self {
    self.height = height;
    self.set_cursor(self.cursor);
    self
  }

  /// 直接设置光标位置，需要钳制它，防止越界
  pub fn set_cursor(&mut self, cursor: usize) -> &mut Self {
    self.cursor = cursor.clamp(0, self.height.saturating_sub(1));
    self
  }

  /// 将光标移动到展示区最顶部，和具体数据无关
  pub fn set_cursor_at_top(&mut self) -> &mut Self {
    self.cursor = 0;
    self
  }

  /// 将光标移动到展示区最底部，和具体数据无关
  pub fn set_cursor_at_bottom(&mut self) -> &mut Self {
    self.cursor = self.height.saturating_sub(1);
    self
  }

  /// 根据已经配置好的光标位置，从指定索引处的数据开始填充数据区，
  /// 我们总是要求在条件允许的情况下，光标实际展示的位置不要过于接近底部或顶部。
  ///
  /// 光标的位置总是用 forward 方向（也即往下的迭代方向）的迭代器进行插入。
  pub fn fill<F>(&mut self, mut f: F)
  where
    F: FnMut(LogDirection) -> bool,
  {
    // 首先先清空数据
    self.data_count = 0;
    self.cursor_index = 0;

    // 展示区高度为空时，结束处理
    if self.height == 0 {
      return;
    }

    // 先取出光标所在日志行。如果连光标指向的数据都不存在，则结束处理
    if !self.push_back(&mut f) {
      return;
    }

    // 光标离上下边界最少这么多行
    let min_spacing = ((self.height as f64 * 0.2 + 1.0) as usize).min(5);

    // 将光标限制在中间这个范围内
    self.cursor = match (
      self.ideal_count_up() >= min_spacing,
      self.ideal_count_down() >= min_spacing,
    ) {
      // 光标离上边界过近，离下边界较远，那么将其向下调整
      (false, true) => min_spacing,

      // 光标离下边界过近，离上边界较远，那么将其向上调整
      (true, false) => self.height - min_spacing,

      // 光标处于中间，或者上下空间都不足，不移动光标
      _ => self.cursor,
    };

    // 按现在光标的理想位置，开始取数据。可能某一端的数据其实没有那么多，我们将在后文从另外一端补充
    self.push_some_front(self.ideal_count_up(), &mut f);
    self.push_some_back(self.ideal_count_down(), &mut f);

    // 检查上下两端的数据是否已经顶到头，如果某一端没有顶到头，则尝试从另外一边追加数据，
    // 尽量保证数据展示区是满屏展示的。
    // 也有可能两端的数据都不够，但已经都没有数据了，此时等于下方两个操作没有效果。
    // 我们会在最终调整 cursor，使其对齐到它真正的位置上
    let unfilled_spacing = self.height - self.data_count;

    // 顶部数据不够，底部来凑
    if self.current_count_up() < self.ideal_count_up() {
      self.push_some_back(unfilled_spacing, &mut f);
    }

    // 底部数据不够，顶部来凑
    if self.current_count_down() < self.ideal_count_down() {
      self.push_some_front(unfilled_spacing, &mut f);
    }

    // 更新光标的位置，和实际情况对齐
    self.cursor = self.cursor_index;
  }

  /// 光标往上区域应有的日志数量，仅和当前光标位置有关
  fn ideal_count_up(&self) -> usize {
    self.cursor
  }

  /// 光标往下区域应有的日志数量，仅和当前光标位置有关
  fn ideal_count_down(&self) -> usize {
    self.height - self.cursor - 1
  }

  /// 实际情况下，光标往上区域的数据数量
  fn current_count_up(&self) -> usize {
    self.cursor_index
  }

  /// 实际情况下，光标往下区域的数据数量
  fn current_count_down(&self) -> usize {
    self.data_count - self.cursor_index - 1
  }

  /// 在光标之上的区域插入一些数据
  fn push_some_front<F>(&mut self, count: usize, f: &mut F)
  where
    F: FnMut(LogDirection) -> bool,
  {
    for _ in 0..count {
      if !self.push_front(f) {
        break;
      }
    }
  }

  /// 在光标之下的区域插入一些数据
  fn push_some_back<F>(&mut self, count: usize, f: &mut F)
  where
    F: FnMut(LogDirection) -> bool,
  {
    for _ in 0..count {
      if !self.push_back(f) {
        break;
      }
    }
  }

  /// 在最顶部插入数据，也即日志的逆向方向插入
  fn push_front<F>(&mut self, f: &mut F) -> bool
  where
    F: FnMut(LogDirection) -> bool,
  {
    if (f)(LogDirection::Backward) {
      self.cursor_index += 1;
      self.data_count += 1;
      true
    } else {
      false
    }
  }

  /// 在最底部插入数据，也即日志的正向方向插入
  fn push_back<F>(&mut self, f: &mut F) -> bool
  where
    F: FnMut(LogDirection) -> bool,
  {
    if (f)(LogDirection::Forward) {
      self.data_count += 1;
      true
    } else {
      false
    }
  }
}