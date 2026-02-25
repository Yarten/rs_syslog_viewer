use crate::log::LogDirection;
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  prelude::*,
  symbols,
  widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use std::collections::VecDeque;

/// 描述本帧内的控制
#[derive(Default, Copy, Clone)]
pub enum Control {
  /// 没有动作，光标将停在上一帧的位置
  #[default]
  Idle,

  /// 跟随最新日志
  Follow,

  /// 逐步移动日志
  MoveBySteps(isize),

  /// 往上翻页
  PageUp,

  /// 往下翻页
  PageDown,
}

#[derive(Default, Copy, Clone)]
struct VerticalScrollState {
  items_count: usize,
  position: usize,
}

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

  /// 当帧需要处理的控制
  pub control: Control,

  /// 横向滚动条当前的位置。它的总长度将动态计算，位置也会动态钳制。
  /// 如果位置设置为 None，则没有横向滚动能力。
  horizontal_scroll_position: Option<usize>,

  /// 纵向滚动条的状态，其中的内容数量以及滚动条位置需要外部设置。
  /// 如果条目数量为零，则不会展示纵向滚动条，但仍然具备纵向滚动能力。
  vertical_scroll_state: VerticalScrollState,
}

/// 扩展 view port 数据的能力，假设了 view port 数据是由 Key 和 Value 组成的元组
pub trait CursorEx {
  type Key;
  type Value;

  fn key(self) -> Option<Self::Key>;

  fn key_or(self, fallback: impl Fn() -> Self::Key) -> Self::Key
  where
    Self: Sized,
  {
    self.key().unwrap_or(fallback())
  }
}

/// 描述加载可视区域外的光标数据加载期望
#[derive(Copy, Clone)]
pub enum CursorExpectation {
  None,
  MoreUp,
  MoreDown,
}

/// 扩展了 [ViewPort] 对具体数据的操作能力，以及响应控制的能力。
pub trait ViewPortEx {
  /// 展示区每一行的数据内容
  type Item;

  /// 获取管理 UI 的数据
  fn ui_mut(&mut self) -> &mut ViewPort;
  fn ui(&self) -> &ViewPort;

  /// 获取用户数据
  fn data_mut(&mut self) -> &mut VecDeque<Self::Item>;
  fn data(&self) -> &VecDeque<Self::Item>;

  /// 获取控制量
  fn control_mut(&mut self) -> &mut Control;
  fn control(&self) -> Control;

  /// 在已有的数据范围内，应用本帧的控制，返回光标最终指向的数据，
  /// 一般返回的是上一帧的旧数据
  fn apply(&mut self) -> Option<(&Self::Item, CursorExpectation)> {
    let control = self.control();

    // 重置控制量
    match control {
      Control::Follow => {}
      _ => *self.control_mut() = Control::Idle,
    }

    // 响应控制
    match control {
      // 光标保持不动，返回光标指向的数据
      Control::Idle => self
        .data()
        .get(self.ui().cursor)
        .map(|v| (v, CursorExpectation::None)),

      // 将光标拉到最顶部，跟踪最新的数据。由于本类记录的数据是落后的，并不知道最新是什么数据，因此这里返回 None
      Control::Follow => {
        self.ui_mut().set_cursor_at_bottom();
        None
      }

      // 移动光标，返回光标指向的数据
      Control::MoveBySteps(n) => {
        let cursor = self.ui().cursor as isize + n;

        // 若光标移动位置后超出了数据范围，则会期望加载更多数据
        let cursor_expectation = if cursor < 0 {
          CursorExpectation::MoreUp
        } else if cursor as usize >= self.ui().data_count {
          CursorExpectation::MoreDown
        } else {
          CursorExpectation::None
        };

        self.ui_mut().set_cursor(cursor.max(0) as usize);
        self
          .data()
          .get(self.ui().cursor)
          .map(|v| (v, cursor_expectation))
      }

      // 向上翻页，光标置顶，返回视野里最顶层的数据，这样一来，视野里的顶层数据就会在底层
      Control::PageUp => {
        // 以下判断条件是测试出来的结果
        let cursor_expectation = if self.ui().height < 2 {
          CursorExpectation::MoreUp
        } else {
          CursorExpectation::None
        };
        self.ui_mut().set_cursor_at_bottom();
        self.data().front().map(|v| (v, cursor_expectation))
      }

      // 向下翻页，光标置底，返回视野里最底层的数据，这样一来，视野里的底层数据就会在顶层
      Control::PageDown => {
        // 以下判断条件是测试出来的结果
        let cursor_expectation = if self.ui().height < 3 {
          CursorExpectation::MoreDown
        } else {
          CursorExpectation::None
        };
        self.ui_mut().set_cursor_at_top();
        self.data().back().map(|v| (v, cursor_expectation))
      }
    }
  }

  /// 填充数据的辅助函数
  fn do_fill<F>(&mut self, mut f: F)
  where
    F: FnMut(LogDirection) -> Option<Self::Item>,
  {
    // 分开引用两部分的数据进行操作，不会有冲突
    let ui = crate::unsafe_ref!(Self, self, mut).ui_mut();
    let data = crate::unsafe_ref!(Self, self, mut).data_mut();

    // 清除已有的数据
    data.clear();

    // 使用 view port ui 的能力，逐一填充数据
    ui.fill(|dir| match (f)(dir) {
      None => false,
      Some(x) => {
        match dir {
          LogDirection::Forward => data.push_back(x),
          LogDirection::Backward => data.push_front(x),
        }
        true
      }
    });
  }
}

/// 扩展 [ViewPort] 渲染到终端的能力
pub trait ViewPortRenderEx: ViewPortEx {
  fn render(
    &mut self,
    mut area: Rect,
    buf: &mut Buffer,
    focus: bool,
    f: impl Fn(&Self::Item) -> Line,
  ) {
    // 组装渲染条目
    let mut items: Vec<Line> = self.data().iter().map(|i| f(i)).collect();

    // -----------------------------------------------------------
    // 调整并渲染横向滚动条的位置。该滚动条不一定想要渲染，取决于 UI 数据中是否记录了它先前的位置。
    let mut horizontal_scroll_position = self.ui().horizontal_scroll_position;

    if let Some(pos) = horizontal_scroll_position.as_mut() {
      // 计算当前行最大宽度
      let width = items.iter().map(|line| line.width()).max().unwrap_or(0) + 10;

      // 可滚动的范围
      let area_width = area.width as usize;
      let scroll_range = if width > area_width {
        width - area_width
      } else {
        0
      };

      // 确保滚动条位置在可滚动范围内
      *pos = scroll_range.saturating_sub(1).min(*pos);

      // 仅当内容宽度大于可渲染范围时，渲染滚动条。滚动条将重叠在底部边框上。
      if scroll_range > 0 {
        let mut state = ScrollbarState::new(scroll_range).position(*pos);
        Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
          .symbols(symbols::scrollbar::Set {
            track: "-",
            thumb: "▮",
            begin: "<",
            end: ">",
          })
          .thumb_style(Style::default().yellow())
          .track_style(Style::default().yellow())
          .begin_style(Color::Red)
          .end_style(Color::Red)
          .render(
            area.outer(Margin {
              vertical: 1,
              horizontal: 0,
            }),
            buf,
            &mut state,
          );
      }
    }

    // -----------------------------------------------------------
    // 调整并渲染纵向滚动条的位置
    let mut vertical_scroll_state = self.ui().vertical_scroll_state;
    {
      // 可滚动的范围
      let area_height = area.height as usize;
      let scroll_range = if vertical_scroll_state.items_count > area_height {
        vertical_scroll_state.items_count - area_height
      } else {
        0
      };

      // 确保滚动条位置在可滚动范围内
      vertical_scroll_state.position = vertical_scroll_state
        .position
        .min(scroll_range.saturating_sub(1));

      // 渲染滚动条。滚动条处于页面框内部，不和边框重叠
      if scroll_range > 0 {
        let mut state = ScrollbarState::new(scroll_range).position(vertical_scroll_state.position);
        Scrollbar::new(ScrollbarOrientation::VerticalRight).render(area, buf, &mut state);
        area.width = area.width.saturating_sub(1);
      }
    }

    // -----------------------------------------------------------
    // 高亮光标指向的数据
    if focus && let Some(line) = items.get_mut(self.ui().cursor) {
      // 若本行的宽度小于可视区的宽度，我们需要在其后方补充空白格，否则高亮区域没法横穿整个行，看起来会比较奇怪。
      // 本来用 List 渲染可以自动解决这个问题，但它不支持 scrollbar ，因此我们只能手动实现下。
      let line_width = line.width();
      if line_width < area.width as usize {
        line.push_span(Span::raw(" ".repeat(area.width as usize - line_width)));
      }

      line.style = line.style.bg(Color::Blue);
    }

    // -----------------------------------------------------------
    // 渲染展示区内容
    let mut content = Paragraph::new(items);
    if let Some(pos) = horizontal_scroll_position {
      content = content.scroll((0, pos as u16));
    }
    content.render(area, buf);

    // -----------------------------------------------------------
    // 更新 UI 数据
    let ui = self.ui_mut();

    // 更新滚动条位置
    ui.horizontal_scroll_position = horizontal_scroll_position;
    ui.vertical_scroll_state = vertical_scroll_state;

    // 由于现在访问得到的 controller 数据都是基于之前的事实计算的，
    // 因此，我们只能在渲染的最后，再给 controller 更新最新的窗口大小
    ui.set_height(area.height as usize);
  }
}

impl ViewPort {
  /// 启用横向滚动条
  pub fn enable_horizontal_scroll(&mut self) {
    self.horizontal_scroll_position = Some(0);
  }

  /// 手动设置纵向滚动条条目上限以及展示区首条数据的位置
  pub fn update_vertical_scroll_state(&mut self, items_count: usize, top_item_index: usize) {
    self.vertical_scroll_state.items_count = items_count;
    self.vertical_scroll_state.position = top_item_index;
  }

  /// 移动横向滚动条，向左为负，向右为正。
  /// 不用担心会不会超出内容范围，在渲染时会钳制
  pub fn want_scroll_horizontally(&mut self, steps: isize) {
    if let Some(pos) = self.horizontal_scroll_position.as_mut() {
      *pos = (*pos as isize + steps).max(0) as usize;
    }
  }

  /// 设置展示区高度，同时钳制光标位置，防止越界
  pub fn set_height(&mut self, height: usize) -> &mut Self {
    self.height = height;
    self.set_cursor(self.cursor)
  }

  /// 总是跟踪到最新的日志（退出导航模式）
  pub fn want_follow(&mut self) {
    self.control = Control::Follow;
  }

  /// 不要跟踪最新日志
  pub fn do_not_follow(&mut self) {
    self.control = Control::Idle;
  }

  /// 按步移动光标
  pub fn want_move_cursor(&mut self, steps: isize) {
    self.control = Control::MoveBySteps(steps);
  }

  /// 往上翻页
  pub fn want_page_up(&mut self) {
    self.control = Control::PageUp;
  }

  /// 往下翻页
  pub fn want_page_down(&mut self) {
    self.control = Control::PageDown;
  }
}

impl ViewPort {
  /// 直接设置光标位置，需要钳制它，防止越界
  fn set_cursor(&mut self, cursor: usize) -> &mut Self {
    self.cursor = cursor.clamp(
      0,
      self
        .height
        .saturating_sub(1)
        .min(self.data_count.saturating_sub(1)),
    );
    self
  }

  /// 将光标移动到展示区最顶部，和具体数据无关
  fn set_cursor_at_top(&mut self) -> &mut Self {
    self.cursor = 0;
    self
  }

  /// 将光标移动到展示区最底部，和具体数据无关
  fn set_cursor_at_bottom(&mut self) -> &mut Self {
    self.cursor = self.height.saturating_sub(1);
    self
  }

  /// 根据已经配置好的光标位置，从指定索引处的数据开始填充数据区，
  /// 我们总是要求在条件允许的情况下，光标实际展示的位置不要过于接近底部或顶部。
  ///
  /// 光标的位置总是用 forward 方向（也即往下的迭代方向）的迭代器进行插入。
  fn fill<F>(&mut self, mut f: F)
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

#[macro_export]
macro_rules! view_port {
  ($name:ident, $item_type:ty) => {
    use crate::{
      app::then::Then,
      ui::{
        ViewPort as ViewPortBase, ViewPortEx, ViewPortRenderEx,
        view_port::Control as ViewPortControl,
      },
    };
    use std::collections::VecDeque;

    /// 维护日志展示区的数据
    #[derive(Default)]
    pub struct $name {
      /// 展示区 UI 相关的数据
      ui: ViewPortBase,

      /// 日志行，从前往后对应展示区的日志从上往下
      data: VecDeque<$item_type>,
    }

    impl Then for $name {}
    impl ViewPortRenderEx for $name {}
    impl ViewPortEx for $name {
      type Item = $item_type;

      fn ui_mut(&mut self) -> &mut ViewPortBase {
        &mut self.ui
      }

      fn ui(&self) -> &ViewPortBase {
        &self.ui
      }

      fn data_mut(&mut self) -> &mut VecDeque<Self::Item> {
        &mut self.data
      }

      fn data(&self) -> &VecDeque<Self::Item> {
        &self.data
      }

      fn control_mut(&mut self) -> &mut ViewPortControl {
        &mut self.ui.control
      }

      fn control(&self) -> ViewPortControl {
        self.ui.control
      }
    }
  };
}
