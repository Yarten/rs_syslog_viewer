use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// 创造专门用于处理控制类事件的键盘事件对象，保证不会被连续触发（其实我们只能收到 Press 事件）
fn control_event(code: char, modifiers: KeyModifiers) -> KeyEvent {
  KeyEvent::new_with_kind(KeyCode::Char(code), modifiers, KeyEventKind::Press)
}

pub trait KeyEventEx {
  /// 创建一个 shift + <char> 的键盘事件，用于某些控制场景，不会被连续触发
  fn shift(code: char) -> KeyEvent {
    Self::platform_consistent(control_event(code, KeyModifiers::SHIFT))
  }

  /// 创建一个 alt + <char> 的键盘事件，用于某些控制场景，不会连续触发
  fn alt(code: char) -> KeyEvent {
    control_event(code, KeyModifiers::ALT)
  }

  /// 创建一个 ctrl + <char> 的键盘事件，用于某些控制场景，不会连续触发
  fn ctrl(code: char) -> KeyEvent {
    control_event(code, KeyModifiers::CONTROL)
  }

  /// 简单无修饰的键盘事件
  fn simple(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
  }

  /// The shift key modifier is not consistent across platforms.
  ///
  /// For upper case alphabets, e.g. 'A'
  ///
  /// Unix: Char("A") + SHIFT
  /// Windows: Char("A") + SHIFT
  ///
  /// For non-alphabets, e.g. '>'
  ///
  /// Unix: Char(">") + NULL
  /// Windows: Char(">") + SHIFT
  ///
  /// But the key event handling below assumes that the shift key modifier is only added for
  /// alphabets. To satisfy the assumption, the following ensures that the presence or absence
  /// of shift modifier is consistent across platforms.
  ///
  /// Idea borrowed from: https://github.com/sxyazi/yazi/pull/174
  fn platform_consistent(mut key: KeyEvent) -> KeyEvent {
    let platform_consistent_shift = match (key.code, key.modifiers) {
      (KeyCode::Char(c), _) => c.is_ascii_uppercase(),
      (_, m) => m.contains(KeyModifiers::SHIFT),
    };

    if platform_consistent_shift {
      key.modifiers.insert(KeyModifiers::SHIFT);
    } else {
      key.modifiers.remove(KeyModifiers::SHIFT);
    }

    key
  }

  fn same_as(&self, other: &KeyEvent) -> bool;
}

impl KeyEventEx for KeyEvent {
  fn same_as(&self, other: &KeyEvent) -> bool {
    // 首先，按键的工作状态需要相同
    if self.kind != other.kind {
      return false;
    }

    // 接着，分析具体的按键是否相同
    match (self.code, other.code) {
      // 对于字母键来说，我们希望忽略大小写进行比较。另外，为了避免歧义，对比字母键时需要去掉 shift
      (KeyCode::Char(c1), KeyCode::Char(c2))
        if c1.is_ascii_alphabetic() && c2.is_ascii_alphabetic() && c1.eq_ignore_ascii_case(&c2) =>
      {
        // 如果字母键相同，那么在去掉了 shift 的基础上，对比额外的修饰键是否相同
        let mut m1 = self.modifiers;
        m1.remove(KeyModifiers::SHIFT);

        let mut m2 = other.modifiers;
        m2.remove(KeyModifiers::SHIFT);

        // 到这里字母键的对比已经完全结束
        return m1 == m2;
      }

      // 其余情况，简单对比具体键是否相同
      (x1, x2) => {
        if x1 != x2 {
          return false;
        }
      }
    }

    // 在上述条件都相同的情况下，对比修饰键是否相同
    self.modifiers == other.modifiers
  }
}
