use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Style, Stylize},
  text::{Span, Text},
  widgets::Widget,
};

/// 状态栏展示的模式
enum Mode {
  /// 一般的提示信息
  Info,

  /// 操作错误时的提示信息
  Error,

  /// 输入模式
  Input,
}

#[derive(Copy, Clone)]
pub struct Theme {
  pub bg: Color,
  pub info: Style,
  pub error: Style,
  pub prompt: Style,
  pub input: Style,
}

impl Default for Theme {
  fn default() -> Self {
    Self {
      bg: Color::White,
      info: Style::new().green(),
      error: Style::new().red().bold(),
      prompt: Style::new().dark_gray().bold(),
      input: Style::new().black(),
    }
  }
}

/// 渲染界面最底部的状态栏，展示提示或报错消息，有时候它也会成为输入框。
pub struct StatusBar {
  /// 展示的模式
  mode: Mode,

  /// 展示的信息。在 Input 模式下，它展示为前缀，还会自动后缀 ": "
  message: String,

  /// 输入的内容
  input: String,

  /// 输入光标在 input 中的位置
  input_index: usize,

  /// 本状态栏的主题
  theme: Theme,
}

impl StatusBar {
  pub fn new(theme: Theme) -> Self {
    Self {
      mode: Mode::Info,
      message: String::new(),
      input: String::new(),
      input_index: 0,
      theme,
    }
  }

  pub fn set_info<T>(&mut self, message: T)
  where
    T: Into<String>,
  {
    self.mode = Mode::Info;
    self.message = message.into();
  }

  pub fn set_error<T>(&mut self, message: T)
  where
    T: Into<String>,
  {
    self.mode = Mode::Error;
    self.message = message.into();
  }

  pub fn set_input<T>(&mut self, message: T)
  where
    T: Into<String>,
  {
    self.mode = Mode::Input;
    self.message = message.into() + ": ";
    self.reset_input(String::new());
  }

  pub fn reset_input(&mut self, input: String) {
    self.input = input;
    self.input_index = self.input.chars().count();
  }
}

impl StatusBar {
  pub fn get_input(&self) -> Option<&String> {
    if let Mode::Input = self.mode {
      Some(&self.input)
    } else {
      None
    }
  }

  pub fn enter_char(&mut self, new_char: char) {
    let index = self.byte_index();
    self.input.insert(index, new_char);
    self.move_cursor_right();
  }

  pub fn delete_char(&mut self) -> bool {
    let is_not_cursor_leftmost = self.input_index != 0;
    if is_not_cursor_leftmost {
      // Method "remove" is not used on the saved text for deleting the selected char.
      // Reason: Using remove on String works on bytes instead of the chars.
      // Using remove would require special care because of char boundaries.

      let current_index = self.input_index;
      let from_left_to_current_index = current_index - 1;

      // Getting all characters before the selected character.
      let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
      // Getting all characters after selected character.
      let after_char_to_delete = self.input.chars().skip(current_index);

      // Put all characters together except the selected one.
      // By leaving the selected one out, it is forgotten and therefore deleted.
      self.input = before_char_to_delete.chain(after_char_to_delete).collect();
      self.move_cursor_left();
    }

    // 返回是否有字符被删除
    is_not_cursor_leftmost
  }

  /// Returns the byte index based on the character position.
  ///
  /// Since each character in a string can contain multiple bytes, it's necessary to calculate
  /// the byte index based on the index of the character.
  fn byte_index(&self) -> usize {
    self
      .input
      .char_indices()
      .map(|(i, _)| i)
      .nth(self.input_index)
      .unwrap_or(self.input.len())
  }

  pub fn move_cursor_left(&mut self) {
    let cursor_moved_left = self.input_index.saturating_sub(1);
    self.input_index = self.clamp_cursor(cursor_moved_left);
  }

  pub fn move_cursor_right(&mut self) {
    let cursor_moved_right = self.input_index.saturating_add(1);
    self.input_index = self.clamp_cursor(cursor_moved_right);
  }

  fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
    new_cursor_pos.clamp(0, self.input.chars().count())
  }
}

impl StatusBar {
  pub fn render(&self, area: Rect, buf: &mut Buffer) {
    let mut text = Text::default().bg(self.theme.bg);
    match self.mode {
      Mode::Info => {
        text.push_span(Span::styled(&self.message, self.theme.info));
      }
      Mode::Error => {
        text.push_span(Span::styled(&self.message, self.theme.error));
      }
      Mode::Input => {
        text.push_span(Span::styled(&self.message, self.theme.prompt));
        text.push_span(Span::styled(&self.input, self.theme.input));
      }
    }

    text.render(area, buf);
  }

  pub fn get_cursor_position(&self) -> Option<usize> {
    if let Mode::Input = self.mode {
      Some(self.message.chars().count() + self.input_index)
    } else {
      None
    }
  }
}
