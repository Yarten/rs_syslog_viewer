use color_eyre::Result;
use crossterm::event::{self, KeyCode, KeyEvent};
use ratatui::DefaultTerminal;
use rs_syslog_viewer::ui::{DemoPage, KeyEventEx, Pager, State, StateMachine};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
  let quit_flag = Rc::new(RefCell::new(false));
  let quit_flag2 = quit_flag.clone();

  const IDLE_STATE: usize = 1;
  const EDIT_STATE: usize = 2;
  const QUIT_STATE: usize = 3;

  let idle_state = State::new("idle")
    .action(KeyEvent::ctrl('f'), |pager| {
      pager.status().set_error("F is active")
    })
    .action(KeyEvent::ctrl('a'), |pager| pager.toggle_left(1))
    .action(KeyEvent::ctrl('s'), |pager| pager.toggle_right(2))
    .action(KeyEvent::ctrl('d'), |pager| pager.toggle_full(3))
    .goto(KeyEvent::simple(KeyCode::Char('e')), EDIT_STATE)
    .goto_action(KeyEvent::simple(KeyCode::Esc), QUIT_STATE, |pager| {
      !pager.close_top()
    });

  let edit_state = State::new("edit")
    .input("Input")
    .goto_action(KeyEvent::simple(KeyCode::Enter), IDLE_STATE, |pager| {
      let input = pager.status().get_input().unwrap();
      pager.status().set_info(input);
      true
    })
    .goto_action(KeyEvent::simple(KeyCode::Esc), IDLE_STATE, |pager| {
      pager.status().set_error("Nothing !");
      true
    });

  let quit_state = State::new("quit")
    .enter_action(|pager| pager.status().set_info("Quit or not ? Y/n"))
    .goto_action(KeyEvent::simple(KeyCode::Char('n')), IDLE_STATE, |pager| {
      pager.status().set_info("give up quit");
      true
    })
    .action(KeyEvent::simple(KeyCode::Char('y')), move |_| {
      *quit_flag2.borrow_mut() = true;
    });

  let mut sm = StateMachine::new(Duration::from_millis(100))
    .root_state(IDLE_STATE, idle_state)
    .state(EDIT_STATE, edit_state)
    .state(QUIT_STATE, quit_state);

  let mut pager = Pager::default()
    .add_page_as_root(DemoPage::new("Root"))
    .add_page(1, DemoPage::new("Aaaa"))
    .add_page(2, DemoPage::new("Bbbb"))
    .add_page(3, DemoPage::new("Cccc"));

  sm.first_run(&mut pager);
  loop {
    if sm.poll_once(&mut pager) || *quit_flag.borrow() {
      return Ok(());
    }

    terminal.draw(|frame| pager.render(frame))?;
  }
}

fn main() -> Result<()> {
  color_eyre::install()?;
  let _ = ratatui::run(|terminal| run(terminal));
  Ok(())
}
