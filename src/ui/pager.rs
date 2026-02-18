use crate::ui::{StatusBar, status_bar};
use ratatui::layout::{Constraint, Layout, Position};
use ratatui::widgets::Block;
use ratatui::{
  Frame,
  buffer::Buffer,
  layout::{Alignment, Rect},
  style::{Color, Style},
  widgets::{BorderType, Borders, Paragraph, Widget},
};
use std::{
  borrow::Cow,
  collections::{HashMap, VecDeque},
};

/// 适用于本页面管理器的渲染接口
pub trait Page {
  /// 渲染一个组件。
  ///
  /// 我们在 ratatui 原来的参数基础上，补充了 `input` 参数，用于代表当前输入框的内容
  fn render(&self, area: Rect, buf: &mut Buffer, input: Option<String>);

  /// 本页面的标题名称
  fn title(&'_ self) -> Cow<'_, str>;
}

/// 若 root page 没有指定时，默认使用该页面
struct DefaultPage;

impl Page for DefaultPage {
  fn render(&self, area: Rect, buf: &mut Buffer, _: Option<String>) {
    Paragraph::new("welcome to viewer")
      .alignment(Alignment::Center)
      .render(area, buf);
  }

  fn title(&'_ self) -> Cow<'_, str> {
    "...".into()
  }
}

/// 测试用的页面
pub struct DemoPage {
  name: String,
}

impl DemoPage {
  pub fn new<T>(name: T) -> Self
  where
    T: Into<String>,
  {
    Self { name: name.into() }
  }
}

impl Page for DemoPage {
  fn render(&self, area: Rect, buf: &mut Buffer, input: Option<String>) {
    Paragraph::new(self.name.clone() + " Good")
      .style(Style::new().bg(Color::DarkGray).fg(Color::White))
      .alignment(Alignment::Center)
      .render(area, buf);
  }

  fn title(&'_ self) -> Cow<'_, str> {
    (&self.name).into()
  }
}

#[derive(Copy, Clone)]
pub struct PageTheme {
  borders: Borders,
  border_type: BorderType,
  border_style: Style,
  title_alignment: Alignment,
  title_style: Style,
}

impl PageTheme {
  pub fn full() -> Self {
    Self {
      borders: Borders::TOP | Borders::BOTTOM,
      border_type: BorderType::Plain,
      border_style: Style::new().white(),
      title_alignment: Alignment::Left,
      title_style: Style::new().white().bold(),
    }
  }

  pub fn half() -> Self {
    Self {
      borders: Borders::ALL,
      border_type: BorderType::Rounded,
      border_style: Style::new().white(),
      title_alignment: Alignment::Left,
      title_style: Style::new().white().bold(),
    }
  }
}

#[derive(Copy, Clone)]
pub struct Theme {
  /// 页面的背景色
  bg: Color,

  /// 全屏页面的风格
  full_page: PageTheme,

  /// 半屏页面的风格
  half_page: PageTheme,

  /// 状态栏的风格
  status_bar: status_bar::Theme,

  /// 半屏页面实际占据的百分比
  half_page_constraint: Constraint,
}

impl Default for Theme {
  fn default() -> Self {
    Self {
      bg: Color::DarkGray,
      full_page: PageTheme::full(),
      half_page: PageTheme::half(),
      status_bar: status_bar::Theme::default(),
      half_page_constraint: Constraint::Percentage(25),
    }
  }
}

/// 子页面的打开方式
#[derive(Copy, Clone)]
enum PageMode {
  /// 左半边渲染
  Left(usize),

  /// 右半边渲染
  Right(usize),

  /// 全屏渲染
  Full(usize),
}

impl PageMode {
  fn get_index(&self) -> usize {
    match self {
      PageMode::Left(i) => *i,
      PageMode::Right(i) => *i,
      PageMode::Full(i) => *i,
    }
  }
}

/// 页面管理器，管理多个子页面，支持这些字面半屏展示、全屏展示等，
/// 组织页面的重叠关系。
pub struct Pager {
  /// 根页面，如果没有任何子页面时，展示该页面
  root_page: Box<dyn Page>,

  /// 各个注册的子页面，使用自定义值索引它，后续要打开、关闭它们，都得用相同的索引值
  pages: HashMap<usize, Box<dyn Page>>,

  /// 位于底部的状态栏
  status_bar: StatusBar,

  /// 风格
  theme: Theme,

  /// 子页面的打开栈。
  /// 新打开的页面总是从 front 插入，如果有些页面在底部、而又被打开一次，将会被调整到最前面。
  /// 左边、右边、全屏总是只记录一个页面，也即在左边连续打开两个页面时，后打开的页面将顶掉前一个的记录，
  /// 本栈主要记录了最后一次操作的顺序，当用户想弹出子页面时，将按先入先出的顺序关闭。
  pages_stack: VecDeque<PageMode>,
}

impl Default for Pager {
  fn default() -> Self {
    Self::new(Theme::default())
  }
}

impl Pager {
  pub fn new(theme: Theme) -> Self {
    Self {
      root_page: Box::new(DefaultPage),
      pages: HashMap::new(),
      status_bar: StatusBar::new(theme.status_bar),
      theme,
      pages_stack: VecDeque::new(),
    }
  }

  pub fn add_page(mut self, index: usize, page: impl Page + 'static) -> Self {
    self.pages.insert(index, Box::new(page));
    self
  }

  pub fn add_page_as_root(mut self, page: impl Page + 'static) -> Self {
    self.root_page = Box::new(page);
    self
  }
}

impl Pager {
  pub fn status(&mut self) -> &mut StatusBar {
    &mut self.status_bar
  }

  /// 在左半边打开指定的子页面。
  pub fn open_left(&mut self, index: usize) {
    self.open_page(PageMode::Left(index));
  }

  /// 在右半边打开指定的子页面。
  pub fn open_right(&mut self, index: usize) {
    self.open_page(PageMode::Right(index));
  }

  /// 全屏打开指定的子页面
  pub fn open_full(&mut self, index: usize) {
    self.open_page(PageMode::Full(index));
  }

  /// 在左半边打开指定子页面。如果子页面已经处于打开状态，则变成关掉它。
  pub fn toggle_left(&mut self, index: usize) {
    self.toggle_page(PageMode::Left(index));
  }

  /// 在右半边打开指定子页面。如果子页面已经处于打开状态，则变成关掉它。
  pub fn toggle_right(&mut self, index: usize) {
    self.toggle_page(PageMode::Right(index));
  }

  /// 全屏打开指定的子页面。如果子页面已经处于打开状态，则变成关掉它。
  pub fn toggle_full(&mut self, index: usize) {
    self.toggle_page(PageMode::Full(index));
  }

  /// 关闭指定的子页面，返回是否关闭成功
  pub fn close(&mut self, index: usize) -> bool {
    for i in 0..self.pages_stack.len() {
      if self.pages_stack[i].get_index() == index {
        self.pages_stack.remove(i);

        // 由于在添加时，我们保证了去重，因此此处应该最多只能找到一个指定的页面
        return true;
      }
    }

    false
  }

  /// 关闭顶部的一个子页面，返回是否关闭成功
  pub fn close_top(&mut self) -> bool {
    self.pages_stack.pop_front().is_some()
  }

  /// 打开指定模式的页面。
  /// 如果当前最顶部打开了一个全屏子页面，那么：
  /// 1. 如果该子页面，正好是指定要打开的页面，则将其重新打开（变成半边页面、或者仍然保持全屏）；
  /// 2. 如果该子页面不是要打开的页面，则本次打开无效。
  ///
  /// 其余情况，总是能打开指定的页面。如果该页面之前已经被打开过，则删除它之前的打开记录，
  /// 这个操作等于将该子页面置顶。
  fn open_page(&mut self, new_page_mode: PageMode) {
    let index = new_page_mode.get_index();

    // 不处理被索引页面不存在的情况
    if !self.should_have_page(index) {
      return;
    }

    // 检查顶部页面是否全屏的，如果是，那么如果恰好是指定的子页面，则重新打开它，否则不能操作。
    if self.check_full_page(new_page_mode) {
      return;
    }

    // 其余情况，总是打开该子页面，并清空它更底部的打开状态
    self.open_page_or_move_to_top(new_page_mode);
  }

  /// 如果指定页面打开了，则将其关闭；反之，则将其打开
  fn toggle_page(&mut self, new_page_mode: PageMode) {
    if !self.close(new_page_mode.get_index()) {
      self.open_page(new_page_mode);
    }
  }

  /// 检查是否存在指定索引的页面
  fn should_have_page(&self, index: usize) -> bool {
    if self.pages.contains_key(&index) {
      true
    } else {
      eprintln!("failed to open the unexisted page ! ({index})");
      false
    }
  }

  /// 检查当前是否有全屏页面在展示，如果有，则仅该全屏页面是指定的页面时，
  /// 才能重新打开它（变小、或者不变）
  fn check_full_page(&mut self, new_page_mode: PageMode) -> bool {
    if let Some(page_mode) = self.pages_stack.front_mut()
      && let PageMode::Full(top_index) = page_mode.clone()
    {
      if top_index == new_page_mode.get_index() {
        *page_mode = new_page_mode;
      }
      return true;
    }

    false
  }

  /// 打开新页面，如果之前已经打开过它，则将其之前打开过的记录删除（等于是将子页面置顶）
  fn open_page_or_move_to_top(&mut self, page_mode: PageMode) {
    self.close(page_mode.get_index());
    self.pages_stack.push_front(page_mode);
  }
}

impl Pager {
  pub fn render(&self, frame: &mut Frame) {
    // 将整个页面分为核心展示部分（展示一些 Page），以及底部的状态栏
    let vertical = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]);
    let [main, bottom] = frame.area().layout(&vertical);

    // 渲染页面
    self.render_main(main, frame.buffer_mut());

    // 渲染状态栏
    self.status_bar.render(bottom, frame.buffer_mut());

    // 如果状态栏存在光标，则将其绘制出来
    if let Some(cursor_pos) = self.status_bar.get_cursor_position() {
      frame.set_cursor_position(Position::new(bottom.x + cursor_pos as u16, bottom.y));
    }
  }

  /// 渲染页面们。有以下几种情况：
  /// 1. 存在打开的全屏子页面，则只渲染它；
  /// 2. 有两个打开半边的子页面，那么将页面分成左、中、右，其中中间部分给根页面渲染；
  /// 3. 有一个打开半边的子页面，那么将页面分成两部分，取决于子页面的位置，将小的部分留给它渲染，大的留给根页面；
  /// 4. 如果没有打开的子页面，则全部空间用于渲染根页面。
  fn render_main(&self, area: Rect, buf: &mut Buffer) {
    // 置顶打开了一个全屏子页面，全部空间用于渲染它
    if let Some(PageMode::Full(index)) = self.pages_stack.front() {
      self.render_full_page(area, buf, &self.pages[index]);
      return;
    }

    // 寻找子页面的打开模式，用双元素元组表示，第0个代表左边页面，第1个代表右边页面
    let mut pattern: (Option<usize>, Option<usize>) = (None, None);

    for page_mode in self.pages_stack.iter() {
      match page_mode {
        PageMode::Left(index) => {
          if pattern.0.is_none() {
            pattern.0 = Some(*index);
          }
        }
        PageMode::Right(index) => {
          if pattern.1.is_none() {
            pattern.1 = Some(*index);
          }
        }
        PageMode::Full(_) => {
          unreachable!()
        }
      }
    }

    // 根据不同的子页面打开模式，进行不同的渲染
    match &pattern {
      // 左右两边渲染子页面，中间渲染根页面
      (Some(left_index), Some(right_index)) => {
        let horizontal = Layout::horizontal([
          self.theme.half_page_constraint,
          Constraint::Fill(1),
          self.theme.half_page_constraint,
        ]);
        let [left, main, right] = area.layout(&horizontal);
        self.render_half_page(left, buf, &self.pages[left_index]);
        self.render_full_page(main, buf, &self.root_page);
        self.render_half_page(right, buf, &self.pages[right_index]);
      }

      // 左边渲染子页面，右边渲染根页面
      (Some(left_index), None) => {
        let horizontal = Layout::horizontal([self.theme.half_page_constraint, Constraint::Fill(1)]);
        let [left, main] = area.layout(&horizontal);
        self.render_half_page(left, buf, &self.pages[left_index]);
        self.render_full_page(main, buf, &self.root_page);
      }

      // 右边渲染子页面，左边渲染根页面
      (None, Some(right_index)) => {
        let horizontal = Layout::horizontal([Constraint::Fill(1), self.theme.half_page_constraint]);
        let [main, right] = area.layout(&horizontal);
        self.render_full_page(main, buf, &self.root_page);
        self.render_half_page(right, buf, &self.pages[right_index]);
      }

      // 没有任何子页面打开，则直接渲染根页面
      (None, None) => {
        self.render_full_page(area, buf, &self.root_page);
      }
    }
  }

  /// 渲染全屏风格的页面
  fn render_full_page(&self, area: Rect, buf: &mut Buffer, page: &Box<dyn Page>) {
    let block = Block::new()
      .borders(self.theme.full_page.borders)
      .border_type(self.theme.full_page.border_type)
      .border_style(self.theme.full_page.border_style)
      .title_alignment(self.theme.full_page.title_alignment)
      .title_style(self.theme.full_page.title_style);
    self.render_page(area, buf, page, block);
  }

  /// 渲染半屏风格的页面
  fn render_half_page(&self, area: Rect, buf: &mut Buffer, page: &Box<dyn Page>) {
    let block = Block::new()
      .borders(self.theme.half_page.borders)
      .border_type(self.theme.half_page.border_type)
      .border_style(self.theme.half_page.border_style)
      .title_alignment(self.theme.half_page.title_alignment)
      .title_style(self.theme.half_page.title_style);
    self.render_page(area, buf, page, block);
  }

  fn render_page(&self, area: Rect, buf: &mut Buffer, page: &Box<dyn Page>, block: Block) {
    let block = block.title(page.title());
    let inner_area = block.inner(area);
    block.render(area, buf);
    page.render(inner_area, buf, self.status_bar.get_input());
  }
}
