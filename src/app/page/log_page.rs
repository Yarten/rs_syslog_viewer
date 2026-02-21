use crate::{
  app::controller::LogController,
  log::LogLine,
  ui::{Page, ViewPortController},
};
use ratatui::style::{Color, Style};
use ratatui::widgets::{List, ListState};
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  text::{self, Span},
  widgets::{ListItem, StatefulWidget},
};
use std::{borrow::Cow, cell::RefCell, rc::Rc};

pub struct LogPage {
  /// 本页面渲染依据的状态数据
  pub log_controller: Rc<RefCell<LogController>>,
}

impl Page for LogPage {
  fn render(&self, area: Rect, buf: &mut Buffer, input: Option<String>) {
    let mut ctrl = self.log_controller.borrow_mut();

    // 构建 list state
    let mut state = ListState::default();
    state.select(Some(ctrl.view().cursor()));

    // 组装日志
    let logs: Vec<ListItem> = ctrl
      .view()
      .items()
      .iter()
      .map(|(_, i)| self.render_log_line(i))
      .collect();

    // 渲染
    List::new(logs)
      .highlight_style(Style::default().bg(Color::Yellow))
      .render(area, buf, &mut state);

    // 由于现在访问得到的 controller 数据都是基于之前的事实计算的，
    // 因此，我们只能在渲染的最后，再给 controller 更新最新的窗口大小
    ctrl.view_mut().set_height(area.height as usize);
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
