use itertools::Itertools;
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Style, Stylize},
  text::{Span, Text},
  widgets::Widget,
};
use std::borrow::Cow;

/// 状态栏展示的模式
enum Mode {
  /// 一般的提示信息
  Tips,

  /// 输入模式
  Input,
}

#[derive(Copy, Clone)]
pub struct Theme {
  pub bg: Color,
  pub prefix: Style,
  pub info: Style,
  pub error: Style,
  pub prompt: Style,
  pub input: Style,
}

impl Default for Theme {
  fn default() -> Self {
    Self {
      bg: Color::White,
      prefix: Style::new().black().bold(),
      info: Style::new().green().bold(),
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

  /// 展示的错误信息。存在错误时，优先展示错误。但如果随后设置了 info 或者键入输入，
  /// 错误将被清除。
  critical_message: String,

  /// 输入的内容
  input: String,

  /// 下一个要插入的字符的位置
  input_index: usize,

  /// 光标的渲染位置（相对于可输入范围的相对位置）
  cursor_index: usize,

  /// 本状态栏的主题
  theme: Theme,
}

impl StatusBar {
  pub fn new(theme: Theme) -> Self {
    Self {
      mode: Mode::Tips,
      message: String::new(),
      critical_message: String::new(),
      input: String::new(),
      input_index: 0,
      cursor_index: 0,
      theme,
    }
  }

  pub fn set_tips<T>(&mut self, message: T)
  where
    T: Into<String>,
  {
    self.mode = Mode::Tips;
    self.message = message.into();
    self.reset_error();
  }

  pub fn set_critical<T>(&mut self, message: T)
  where
    T: Into<String>,
  {
    self.critical_message = message.into();
  }

  pub fn set_input<T>(&mut self, message: T)
  where
    T: Into<String>,
  {
    self.mode = Mode::Input;
    self.message = message.into() + ": ";
    self.reset_input(String::new());
    self.reset_error();
  }

  pub fn reset_input(&mut self, input: String) {
    self.input = input;
    self.input_index = self.input.chars().count();
    self.cursor_index = self.input_index;
    self.reset_error();
  }

  /// 清空错误，返回是否真的有错误被清空
  pub fn reset_error(&mut self) -> bool {
    if !self.critical_message.is_empty() {
      self.critical_message.clear();
      true
    } else {
      false
    }
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
    if self.reset_error() {
      return;
    }

    let index = self.byte_index();
    self.input.insert(index, new_char);
    self.move_cursor_right();
  }

  pub fn delete_char(&mut self) -> bool {
    if self.reset_error() {
      return false;
    }

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
    if self.reset_error() {
      return;
    }

    let cursor_moved_left = self.input_index.saturating_sub(1);
    let last_input_index = self.input_index;
    self.input_index = self.clamp_cursor(cursor_moved_left);
    if last_input_index != self.input_index {
      self.cursor_index = self.cursor_index.saturating_sub(1);
    }
  }

  pub fn move_cursor_right(&mut self) {
    if self.reset_error() {
      return;
    }

    let cursor_moved_right = self.input_index.saturating_add(1);
    let last_input_index = self.input_index;
    self.input_index = self.clamp_cursor(cursor_moved_right);
    if last_input_index != self.input_index {
      self.cursor_index += 1; // 在渲染时再被 clamp
    }
  }

  fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
    new_cursor_pos.clamp(0, self.input.chars().count())
  }
}

const INFO_PREFIX: &str = " # ";
const ERROR_PREFIX: &str = " ! ";
const INPUT_PREFIX: &str = " $ ";

impl StatusBar {
  /// 渲染状态栏，返回光标位置，由外层调用者渲染
  pub fn render(&mut self, area: Rect, buf: &mut Buffer) -> Option<usize> {
    let mut text = Text::default().bg(self.theme.bg);
    let mut cursor_position = None;

    if !self.critical_message.is_empty() {
      text.push_span(Span::styled(ERROR_PREFIX, self.theme.prefix));
      text.push_span(Span::styled(&self.critical_message, self.theme.error));
    } else {
      match self.mode {
        Mode::Tips => {
          text.push_span(Span::styled(INFO_PREFIX, self.theme.prefix));
          text.push_span(Span::styled(&self.message, self.theme.info));
        }
        Mode::Input => {
          text.push_span(Span::styled(INPUT_PREFIX, self.theme.prefix));
          text.push_span(Span::styled(&self.message, self.theme.prompt));

          // 供输入内容展示的最大宽度，如果输入超过这个宽度，我们需要省略内容
          let max_width = area.width as isize
            - 1
            - INPUT_PREFIX.len() as isize
            - self.message.chars().count() as isize;

          // 仅有一点宽度时，才渲染输入的内容与光标
          if max_width > 0 {
            let max_width = max_width as usize;

            // 对于超出可视范围的数据进行缩略，更新光标渲染的位置
            let (rendered_input, cursor_index) = self.omit_some_input_by_cursor(max_width);
            cursor_position = Some(cursor_index);

            // 对最终结果进行渲染
            text.push_span(Span::styled(rendered_input, self.theme.input));
          }
        }
      }
    }

    text.render(area, buf);

    match cursor_position {
      None => None,
      Some(cursor_index) => {
        self.cursor_index = cursor_index; // 因为 text 中引用了 self.input，因此只能在它渲染完毕后更新索引
        Some(INPUT_PREFIX.len() + self.message.chars().count() + self.cursor_index)
      }
    }
  }

  /// 当输入框的宽度不够时，根据光标的位置，对内容进行选择性缩略
  fn omit_some_input_by_cursor(&'_ self, max_width: usize) -> (Cow<'_, str>, usize) {
    // 获取输入的字符数量
    let curr_width = self.input.chars().count();

    // 若输入内容可以容得下，则不做特殊处理
    if curr_width <= max_width {
      return (Cow::from(&self.input), self.input_index);
    }

    // 取出字符串数组，对完整的字符而非字节进行操作
    let chars = self.input.chars().collect_vec();

    // 确保光标的渲染位置不会超出可展示的区域
    let cursor_index = self.cursor_index.min(max_width - 1);

    // 如果宽度太短，则不展示表示缩略的 ..
    // 此处 max_width 不会为零。
    if max_width <= 6 {
      return (
        Cow::from(String::from_iter(
          &chars[self.input_index - cursor_index
            ..(self.input_index + max_width - cursor_index).min(chars.len())],
        )),
        cursor_index,
      );
    }

    // 取出输入内容的所有字符，我们将取出部分字符，和 .. 组成新的字符串返回
    let head_omit_pos = max_width - 2;
    let tail_omit_pos = curr_width + 2 - max_width;

    if self.input_index < head_omit_pos {
      // 光标接近输入内容的开端，因此在末尾展示省略
      (
        Cow::from(String::from_iter(&chars[..head_omit_pos]) + ".."),
        self.input_index,
      )
    } else if self.input_index >= tail_omit_pos {
      // 光标接近输入内容的末端，因此在开头展示省略。
      (
        Cow::from(String::from("..") + &String::from_iter(&chars[tail_omit_pos..])),
        // +2 代表 .. ，光标位置在其之右
        self.input_index - tail_omit_pos + 2,
      )
    } else {
      // 确保光标不会落到两边 .. 的位置上
      let cursor_index = cursor_index.clamp(2, max_width - 2 - 1);
      (
        Cow::from(
          String::from("..")
            + &String::from_iter(
              &chars[self.input_index - cursor_index + 2
                ..self.input_index + max_width - cursor_index - 2],
            )
            + "..",
        ),
        cursor_index,
      )
    }
  }
}
