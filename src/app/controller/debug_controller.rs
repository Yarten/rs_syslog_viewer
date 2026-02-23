use crate::ui::CursorExpectation;
use crate::{
  app::{Controller, LogHubRef},
  debug,
  debug::Item as LogItem,
  log::LogDirection,
};

/// 展示区里维护的数据条目
type Item = (usize, LogItem);

// 展示区数据维护器
crate::view_port!(ViewPort, Item);

impl ViewPort {
  /// 加锁调试数据缓冲区，取出展示区里需要的那些
  fn fill(&mut self, mut index: usize) {
    if let Some(buffer) = debug::BUFFER.lock().unwrap().as_ref() {
      let buffer = buffer.data();
      index = index.clamp(0, buffer.len().saturating_sub(1));

      let mut iter_down = buffer.iter().enumerate().skip(index);
      let mut iter_up = buffer.iter().enumerate().take(index).rev();

      self.do_fill(|dir| match dir {
        LogDirection::Forward => iter_down.next().map(|(a, b)| (a, b.clone())),
        LogDirection::Backward => iter_up.next().map(|(a, b)| (a, b.clone())),
      })
    }
  }
}

/// 调试打印展示区的控制器
pub struct DebugController {
  /// 展示区里的数据
  view_port: ViewPort,
}

impl Default for DebugController {
  fn default() -> Self {
    let mut res = Self {
      view_port: Default::default(),
    };

    res.view_port.ui.want_follow();
    res
  }
}

impl DebugController {
  /// 获得 view port 控制器
  pub fn view_mut(&mut self) -> &mut ViewPort {
    &mut self.view_port
  }
}

impl Controller for DebugController {
  fn run_once(&mut self, _: &mut LogHubRef) {
    // 响应调试区的控制，取出其中最新
    let (cursor_index, cursor_expectation) = self
      .view_port
      .apply()
      .map(|((i, _), e)| (*i, e))
      .unwrap_or((usize::MAX, CursorExpectation::None));

    // 处理光标越界加载期望
    let cursor_index = match cursor_expectation {
      CursorExpectation::None => cursor_index,
      CursorExpectation::MoreUp => (cursor_index as isize - 1).max(0) as usize,
      CursorExpectation::MoreDown => cursor_index.saturating_add(1),
    };

    // 取出数据，填充展示区
    self.view_port.fill(cursor_index);
  }

  fn view_port(&mut self) -> Option<&mut ViewPortBase> {
    Some(self.view_port.ui_mut())
  }
}
