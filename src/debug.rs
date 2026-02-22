use chrono::{DateTime, Local};
use std::{collections::VecDeque, sync::Mutex};

/// 展示区里维护的数据条目
#[derive(Clone)]
pub struct Item {
  pub date: DateTime<Local>,
  pub content: String,
  pub is_error: bool,
}

pub struct Buffer {
  data: VecDeque<Item>,
  limit: usize,
}

impl Buffer {
  fn new(limit: usize) -> Self {
    Self {
      data: VecDeque::with_capacity(limit),
      limit,
    }
  }

  fn push(&mut self, item: Item) {
    if self.data.len() == self.limit {
      self.data.pop_front();
    }
    self.data.push_back(item);
  }

  pub fn data(&self) -> &VecDeque<Item> {
    &self.data
  }
}

pub static BUFFER: Mutex<Option<Buffer>> = Mutex::new(None);

pub fn enable_debug(buffer_size: usize) {
  BUFFER.lock().unwrap().replace(Buffer::new(buffer_size));
}

pub fn log_message(content: String, is_error: bool) {
  match BUFFER.lock().unwrap().as_mut() {
    None => match is_error {
      true => eprintln!("{}", content),
      false => println!("{}", content),
    },
    Some(buffer) => buffer.push(Item {
      date: Local::now(),
      content,
      is_error,
    }),
  }
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {
      crate::debug::log_message(format!($($arg)*), false)
    };
}

#[macro_export]
macro_rules! eprintln {
  ($($arg:tt)*) => {
    crate::debug::log_message(format!($($arg)*), true)
  }
}
