use crate::app::Controller;
use crate::app::LogHubRef;

#[derive(Default)]
pub struct AppController {
  quit: bool,
}

impl AppController {
  pub fn quit(&mut self) {
    self.quit = true;
  }
}

impl Controller for AppController {
  fn run_once(&mut self, _: &mut LogHubRef) {}

  fn should_quit(&self) -> bool {
    self.quit
  }
}
