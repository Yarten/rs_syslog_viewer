use crate::ui::ViewPort;

pub trait Then {
  fn then<F, T>(&self, f: F) -> T
  where
    F: FnOnce() -> T,
  {
    (f)()
  }
}

impl Then for ViewPort {}
