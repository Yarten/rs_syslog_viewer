use crate::{
  app::controller::LogController,
  log::LogLine,
  ui::{Page, PageState, ViewPortRenderEx},
};
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  text::{self, Span},
  widgets::ListItem,
};
use std::{borrow::Cow, cell::RefCell, rc::Rc};

pub struct LogPage {
  /// 本页面渲染依据的状态数据
  pub log_controller: Rc<RefCell<LogController>>,
}

impl Page for LogPage {
  fn render(&self, area: Rect, buf: &mut Buffer, state: &PageState) {
    self
      .log_controller
      .borrow_mut()
      .view_mut()
      .render(area, buf, state.focus, |(_, i)| self.render_log_line(i));
  }

  fn title(&'_ self) -> Cow<'_, str> {
    self.log_controller.borrow().logs_root().to_owned().into()
  }
}

impl LogPage {
  /// 为给定的日志行，创建可渲染的列表项
  fn render_log_line<'a>(&self, log: &'a LogLine) -> ListItem<'a> {
    let mut line = text::Line::default();

    match log {
      // 正常日志
      LogLine::Good(log) => {
        line.push_span(Span::raw(log.timestamp.to_rfc3339()));
        line.push_span(Span::raw(&log.tag));
        line.push_span(Span::raw(&log.message));
      }

      // 坏的日志
      LogLine::Bad(log) => line.push_span(Span::raw(&log.content)),
    }

    ListItem::new(line)
  }
}
