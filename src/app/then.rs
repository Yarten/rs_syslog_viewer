pub trait Then {
  fn then<F, T>(&self, f: F) -> T
  where
    F: FnOnce() -> T
  {
    (f)()
  }
}
