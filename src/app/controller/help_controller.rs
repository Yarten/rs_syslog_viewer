use crate::ui::CursorExpectation;
use crate::{
  app::{Controller, LogHubRef},
  log::LogDirection,
};

#[derive(Clone)]
pub enum HelpLine {
  Title(&'static str),
  Item(&'static str),
  Separator,
}

/// 展示区里维护的帮助内容条目（逐行设置）
type Item = (usize, HelpLine);

// 展示区数据维护器
crate::view_port!(ViewPort, Item);

impl ViewPort {
  fn fill(&mut self, data: &[HelpLine], mut index: usize) {
    index = index.min(data.len().saturating_sub(1));

    let mut iter_down = data.iter().enumerate().skip(index);
    let mut iter_up = data.iter().enumerate().take(index).rev();
    self.do_fill(|dir| match dir {
      LogDirection::Forward => iter_down.next().map(|(a, b)| (a, b.clone())),
      LogDirection::Backward => iter_up.next().map(|(a, b)| (a, b.clone())),
    });
  }
}

/// 帮助页面展示的内容
pub struct HelpController {
  /// 展示区里的数据M
  view_port: ViewPort,

  /// 所有预定好的帮助信息
  help_lines: Vec<HelpLine>,
}

impl HelpController {
  pub fn view_mut(&mut self) -> &mut ViewPort {
    &mut self.view_port
  }
}

impl Default for HelpController {
  fn default() -> Self {
    Self {
      view_port: ViewPort::default(),
      help_lines: vec![
        // 通用说明
        HelpLine::Title("Common Operation"),
        HelpLine::Item("case insensitive"),
        HelpLine::Item("use ◄ ▲ ▼ ► to navigate"),
        HelpLine::Item("press 'enter' to confirm"),
        HelpLine::Item("press 'esc' to cancel"),
        HelpLine::Item("press 'q' to quit, finally will ask y/n to confirm or cancel"),
        HelpLine::Item("press 'ctrl c' to quit anywhere without asking"),
        HelpLine::Separator,
        // 标签页说明
        HelpLine::Title("Tags Filter"),
        HelpLine::Item("press 't' to search tags"),
        HelpLine::Item("press 'ctrl y' to set all"),
        HelpLine::Item("press 'ctrl n' to unset all"),
        HelpLine::Item("press 'ctrl h' to reverse all"),
        HelpLine::Item("press 'ctrl t' to toggle the filter page"),
        HelpLine::Item("press 'alt t' to toggle the fullscreen filter page"),
        HelpLine::Separator,
        // 日志页说明
        HelpLine::Title("Logs View Port"),
        HelpLine::Item("press 'm' to mark or unmark"),
        HelpLine::Item("press '/' to search by content"),
        HelpLine::Item("press '?' to search by timestamp (see bellow)"),
        HelpLine::Item("press '[' to jump to prev log"),
        HelpLine::Item("press ']' to jump to next log"),
        HelpLine::Separator,
        // 时间戳规则说明
        HelpLine::Title("Timestamp Condition Syntax"),
        HelpLine::Item(
          "Perform fuzzy matching using the highest precision unit appeared in conditions",
        ),
        HelpLine::Item("use ',' to separate conditions (AND rule)"),
        HelpLine::Item("date: 2025.11.12, 2025-11-12, 11-12"),
        HelpLine::Item("time: 11:12:13, 11:12"),
        HelpLine::Item("timepoint: {date}, {time}, {data} {time}, {time} {date}"),
        HelpLine::Item("duration: 3d 4h 5m 6s, 4h5s (missing some is ok, but order is strict)"),
        HelpLine::Separator,
        HelpLine::Item("'= {duration}, {duration}': equal to the timepoint from now - duration"),
        HelpLine::Item("'> {duration}': earlier than the timepoint from now - duration"),
        HelpLine::Item("'< {duration}': later than the timepoint from now - duration"),
        HelpLine::Separator,
        HelpLine::Item("'= {timepoint}, {timepoint}': equal to the timepoint"),
        HelpLine::Item("'> {timepoint}': later than the timepoint"),
        HelpLine::Item("'< {timepoint}': earlier than the timepoint"),
        HelpLine::Item("'{timepoint} ~ {timepoint}': time range"),
        HelpLine::Separator,
        // 调试页面规则说明
        HelpLine::Title("Debug Logs"),
        HelpLine::Item("press 'd' to open and focus the debug page"),
        HelpLine::Item("press 'ctrl d' to toggle the debug page"),
        HelpLine::Item("press 'alt d' to toggle the fullscreen filter page"),
      ],
    }
  }
}

impl Controller for HelpController {
  fn run_once(&mut self, _: &mut LogHubRef) {
    // 响应调试区的控制
    let (cursor_index, cursor_expectation) = self
      .view_port
      .apply()
      .map(|((i, _), e)| (*i, e))
      .unwrap_or((0, CursorExpectation::None));

    // 处理光标越界加载期望
    let cursor_index = match cursor_expectation {
      CursorExpectation::None => cursor_index,
      CursorExpectation::MoreUp => (cursor_index as isize - 1).max(0) as usize,
      CursorExpectation::MoreDown => cursor_index.saturating_add(1),
    };

    // 取出数据，填充展示区，并启发数据区最大数量，以及顶层数据在整体中的索引，以展示纵向滚动条
    self.view_port.fill(&self.help_lines, cursor_index);
    self.view_port.ui.update_vertical_scroll_state(
      self.help_lines.len(),
      self
        .view_port
        .data
        .front()
        .map(|(idx, _)| *idx)
        .unwrap_or(0),
    )
  }

  fn view_port(&mut self) -> Option<&mut ViewPortBase> {
    Some(self.view_port.ui_mut())
  }
}
