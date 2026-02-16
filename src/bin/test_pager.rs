use color_eyre::Result;
use crossterm::event::{self, KeyCode};
use ratatui::DefaultTerminal;
use rs_syslog_viewer::ui::{DemoPage, Pager};

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
            pager.status().set_info("");
            input_mode = false;
          }
          _ => {}
        }
      } else {
        match key.code {
          KeyCode::Char('e') => {
            pager.status().set_input("Input");
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

  let pager = Pager::default()
    .add_page_as_root(Box::new(DemoPage::new("Root")))
    .add_page(1, Box::new(DemoPage::new("Aaaa")))
    .add_page(2, Box::new(DemoPage::new("Bbbb")))
    .add_page(3, Box::new(DemoPage::new("Cccc")));

  ratatui::run(|terminal| run(terminal, pager))
}
