use crate::{
  app::{controller::TagController, rich},
  ui::{Page, PageState, ViewPortRenderEx},
};
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Style, Styled},
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
    let mut tag_controller = self.tag_controller.borrow_mut();
    let search = crate::unsafe_ref!(str, tag_controller.get_curr_search());

    tag_controller
      .view_mut()
      .render(area, buf, state.focus, |(k, v)| {
        self.render_tag(k, *v, search)
      });
  }

  fn title(&'_ self) -> Cow<'_, str> {
    "Tags Filter".into()
  }
}

impl TagPage {
  fn render_tag<'a>(&self, tag: &'a str, state: bool, search: &str) -> ListItem<'a> {
    let mut line = text::Line::default();

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
    rich(&mut line, tag, search);

    ListItem::new(line)
  }
}
