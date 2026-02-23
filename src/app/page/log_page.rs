use crate::app::controller::log_controller::{PidStyle, TagStyle, TimestampStyle};
use crate::{
  app::{
    controller::{LogController, log_controller::Style},
    rich,
  },
  log::LogLine,
  ui::{Page, PageState, ViewPortRenderEx},
};
use chrono::{DateTime, FixedOffset};
use ratatui::{
  buffer::Buffer,
  layout::Rect,
  prelude::*,
  text::{self, Span},
  widgets::ListItem,
};
use std::{borrow::Cow, cell::RefCell, rc::Rc};

pub struct Config {
  short_tag_len: usize,
  long_tag_len: usize,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      short_tag_len: 10,
      long_tag_len: 18,
    }
  }
}

pub struct LogPage {
  /// 本页面渲染依据的状态数据
  pub log_controller: Rc<RefCell<LogController>>,

  /// 渲染配置
  pub config: Config,
}

impl Page for LogPage {
  fn render(&self, area: Rect, buf: &mut Buffer, state: &PageState) {
    let style = *self.log_controller.borrow().style();
    let search = crate::unsafe_ref!(str, self.log_controller.borrow().get_search_content());

    self
      .log_controller
      .borrow_mut()
      .view_mut()
      .render(area, buf, state.focus, |(_, i)| {
        self.render_log_line(i, style, search)
      });
  }

  fn title(&'_ self) -> Cow<'_, str> {
    self.log_controller.borrow().logs_root().to_owned().into()
  }
}

impl LogPage {
  /// 为给定的日志行，创建可渲染的列表项
  fn render_log_line<'a>(&self, log: &'a LogLine, style: Style, search: &str) -> Line<'a> {
    let mut line = Line::default();

    match log {
      // 正常日志
      LogLine::Good(log) => {
        line.push_span(self.get_timestamp_span(&style, &log.timestamp).cyan());
        line.push_span(Span::raw(" "));

        if let Some(span) = self.get_tag_span(&style, &log.tag) {
          line.push_span(span.magenta());
          line.push_span(Span::raw(" "));
        }

        if let Some(span) = self.get_pid_span(&style, log.pid) {
          line.push_span(Span::raw("[").bold().white());
          line.push_span(span.yellow());
          line.push_span(Span::raw("]").bold().white());
          line.push_span(Span::raw(" "));
        }

        rich(&mut line, &log.message, search);
      }

      // 坏的日志
      LogLine::Bad(log) => line.push_span(Span::raw(&log.content).on_red()),
    }

    line
  }

  fn get_timestamp_span<'a>(&self, style: &Style, dt: &DateTime<FixedOffset>) -> Span<'a> {
    let timestamp_str = match style.timestamp_style {
      TimestampStyle::Full => dt.to_rfc3339(),
      TimestampStyle::Time => dt.format("%H:%M:%S%.3f").to_string(),
      TimestampStyle::MonthDayTime => dt.format("%m-%d|%H:%M:%S%.3f").to_string(),
      TimestampStyle::RoughTime => dt.format("%H:%M:%S").to_string(),
    };
    Span::raw(timestamp_str)
  }

  fn get_tag_span<'a>(&self, style: &Style, tag: &'a str) -> Option<Span<'a>> {
    let span = match style.tag_style {
      TagStyle::Full => Span::raw(tag),
      TagStyle::OmitLeft => {
        if tag.len() <= self.config.short_tag_len {
          Span::raw(tag)
        } else {
          Span::raw(String::from("..") + &tag[tag.len() - self.config.short_tag_len..])
        }
      }
      TagStyle::OmitRight => {
        if tag.len() <= self.config.short_tag_len {
          Span::raw(tag)
        } else {
          Span::raw(tag[tag.len() - self.config.short_tag_len..].to_string() + "..")
        }
      }
      TagStyle::OmitMiddle => {
        if tag.len() <= self.config.long_tag_len {
          Span::raw(tag)
        } else {
          let half_len = self.config.long_tag_len / 2;
          Span::raw(tag[..half_len].to_string() + ".." + &tag[tag.len() - half_len..])
        }
      }
      TagStyle::Hidden => return None,
    };
    Some(span)
  }

  fn get_pid_span<'a>(&self, style: &Style, pid: i32) -> Option<Span<'a>> {
    match style.pid_style {
      PidStyle::Shown => Some(Span::raw(pid.to_string())),
      PidStyle::Hidden => None,
    }
  }
}
