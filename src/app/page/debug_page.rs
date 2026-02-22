use crate::{
  app::controller::DebugController,
  debug::Item,
  ui::{Page, PageState, ViewPortRenderEx},
};
use chrono::Timelike;
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Style, Styled, Stylize},
  text::{self, Span},
  widgets::ListItem,
};
use std::{borrow::Cow, cell::RefCell, rc::Rc};

pub struct DebugPage {
  pub debug_controller: Rc<RefCell<DebugController>>,
}

impl Page for DebugPage {
  fn render(&self, area: Rect, buf: &mut Buffer, state: &PageState) {
    self
      .debug_controller
      .borrow_mut()
      .view_mut()
      .render(area, buf, state.focus, |(_, v)| self.render_item(v))
  }

  fn title(&'_ self) -> Cow<'_, str> {
    "Debug Logs".into()
  }
}

impl DebugPage {
  fn render_item<'a>(&self, item: &'a Item) -> ListItem<'a> {
    let mut line = text::Text::default();

    line.push_span(
      format!(
        "{:02}:{:02}:{:02}",
        item.date.hour(),
        item.date.minute(),
        item.date.second()
      )
      .cyan(),
    );

    line.push_span(Span::raw(" "));

    let color = if item.is_error {
      Color::Red
    } else {
      Color::White
    };
    line.push_span(Span::raw(&item.content).fg(color));

    ListItem::new(line)
  }
}
