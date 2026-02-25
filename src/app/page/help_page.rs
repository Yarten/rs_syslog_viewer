use crate::{
  app::{
    controller::{HelpController, help_controller::HelpLine},
    rich,
  },
  ui::{Page, PageState, ViewPortRenderEx},
};
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{Styled, Stylize},
  text::{Line, Span},
};
use std::{borrow::Cow, cell::RefCell, rc::Rc};

pub struct HelpPage {
  pub help_controller: Rc<RefCell<HelpController>>,
}

impl Page for HelpPage {
  fn render(&self, area: Rect, buf: &mut Buffer, state: &PageState) {
    self
      .help_controller
      .borrow_mut()
      .view_mut()
      .render(area, buf, true, |(_, v)| self.render_item(v))
  }

  fn title(&'_ self) -> Cow<'_, str> {
    "Help".into()
  }
}

impl HelpPage {
  fn render_item<'a>(&self, item: &HelpLine) -> Line<'a> {
    let mut line = Line::default();
    match *item {
      HelpLine::Title(content) => {
        line.push_span(Span::raw(content).white().bold().underlined());
      }
      HelpLine::Item(content) => {
        line.push_span(Span::raw("• ").cyan().bold());
        rich(&mut line, content, "");
        // line.push_span(Span::raw(content).gray());
      }
      HelpLine::Separator => {
        line.push_span("");
      }
    }

    line
  }
}
