use crate::{
  app::controller::TagController,
  ui::{Page, PageState, ViewPortRenderEx},
};
use ratatui::style::{Color, Style, Styled};
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  text::{self, Span},
  widgets::ListItem,
};
use std::{borrow::Cow, cell::RefCell, rc::Rc};

pub struct TagPage {
  /// 本页面渲染依据的状态数据
  pub tag_controller: Rc<RefCell<TagController>>,
}

impl Page for TagPage {
  fn render(&self, area: Rect, buf: &mut Buffer, state: &PageState) {
    self
      .tag_controller
      .borrow_mut()
      .view_mut()
      .render(area, buf, state.focus, |(k, v)| self.render_tag(k, *v));
  }

  fn title(&'_ self) -> Cow<'_, str> {
    "Tags Filter".into()
  }
}

impl TagPage {
  fn render_tag<'a>(&self, tag: &'a str, state: bool) -> ListItem<'a> {
    let mut line = text::Text::default();

    // 标识是否选中该标签的复选框
    let checkbox_style = Style::default().bg(Color::DarkGray).white().bold();
    line.push_span("[".set_style(checkbox_style));
    if state {
      line.push_span("x".set_style(checkbox_style.green()))
    } else {
      line.push_span(" ".set_style(checkbox_style))
    }
    line.push_span("]".set_style(checkbox_style));

    // 标签内容本身
    line.push_span(Span::raw(tag));

    ListItem::new(line)
  }
}
