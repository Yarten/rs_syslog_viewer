use color_eyre::Result;
use crossterm::event::{self, KeyCode};
use ratatui::{
  DefaultTerminal,
  buffer::Buffer,
  layout::{Alignment, Rect},
  style::{Color, Style},
  widgets::{Paragraph, Widget},
};
use rs_syslog_viewer::ui::{Page, Pager};

struct DemoPage {
  name: String,
}

impl DemoPage {
  fn new(name: String) -> Self {
    Self { name }
  }
}

impl Page for DemoPage {
  fn render(&self, area: Rect, buf: &mut Buffer, input: Option<String>) {
    Paragraph::new(self.name.clone() + " Good")
      .style(Style::new().bg(Color::DarkGray).fg(Color::White))
      .alignment(Alignment::Center)
      .render(area, buf);
  }

  fn title(&self) -> &str {
    &self.name
  }
}

fn run(terminal: &mut DefaultTerminal, mut pager: Pager) -> Result<()> {
  let mut input_mode = false;
  loop {
    terminal.draw(|frame| pager.render(frame))?;

    if let Some(key) = event::read()?.as_key_press_event() {
      if input_mode {
        match key.code {
          KeyCode::Enter => {
            let input = pager.status().get_input().unwrap();
            pager.status().set_info(input);
            input_mode = false;
          }
          KeyCode::Char(to_insert) => pager.status().enter_char(to_insert),
          KeyCode::Backspace => pager.status().delete_char(),
          KeyCode::Left => pager.status().move_cursor_left(),
          KeyCode::Right => pager.status().move_cursor_right(),
          KeyCode::Esc => {
            pager.status().set_info("".to_owned());
            input_mode = false;
          }
          _ => {}
        }
      } else {
        match key.code {
          KeyCode::Char('e') => {
            pager.status().set_input("Input".to_owned());
            input_mode = true;
          }
          KeyCode::Char('q') => return Ok(()),
          KeyCode::Char('1') => pager.toggle_left(1),
          KeyCode::Char('2') => pager.toggle_right(2),
          KeyCode::Char('3') => pager.toggle_full(3),
          KeyCode::Esc => {
            pager.close_top();
          }
          _ => {}
        }
      }
    }
  }
}

fn main() -> Result<()> {
  color_eyre::install()?;

  let mut pager = Pager::default();
  pager.add_page_as_root(Box::new(DemoPage::new("Root".to_owned())));
  pager.add_page(1, Box::new(DemoPage::new("Aaaa".to_owned())));
  pager.add_page(2, Box::new(DemoPage::new("Bbbb".to_owned())));
  pager.add_page(3, Box::new(DemoPage::new("Cccc".to_owned())));

  ratatui::run(|terminal| run(terminal, pager))
}
