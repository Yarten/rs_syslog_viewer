use crate::ui::{KeyEventEx, Pager};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::{collections::HashMap, time::Duration};

/// 在某个状态下，识别到指定按键事件后，执行的动作。不会引起状态切换
type Action = Box<dyn FnMut(&mut Pager)>;

///在某个状态下，识别到指定按键事件后，执行的操作，
/// 返回值决定了是否要跳转到下一个状态（这个状态在定义状态机时已经完成定义）
type GotoAction = Box<dyn FnMut(&mut Pager) -> bool>;

/// 输入栏内容变更时的回调函数
type InputHandler = Box<dyn FnMut(&str)>;

/// 定义状态响应某个键盘事件后，发生的动作、或下一个转移的目标状态
struct Transition {
  /// 响应的事件
  event: KeyEvent,

  /// 转移前执行的动作，返回 true 则进行转移
  act: GotoAction,

  /// 转移的目标状态
  next_state: usize,
}

/// 输入模式下的数据
struct InputMode {
  /// 状态栏提示的内容
  prompt: String,

  /// 内容发生变更时调用的回调函数
  handler: InputHandler,
}

/// 状态机中的一个状态
pub struct State {
  /// 状态的名称，仅用于调试。在状态机中索引状态，使用的是整数
  name: String,

  /// 标识本状态是否处理 status bar 的输入，和输入相关的操作，
  /// 包括一般字符、大写字符、左右方向键、退格键都会被优先处理，
  input_mode: Option<InputMode>,

  /// 有序的转移条件及其动作定义
  transitions: Vec<Transition>,

  /// 进入该状态时，执行的动作
  enter_action: Vec<Action>,

  /// 离开该状态时，执行的动作
  leave_action: Vec<Action>,
}

impl State {
  pub fn new<T>(name: T) -> Self
  where
    T: Into<String>,
  {
    Self {
      name: name.into(),
      input_mode: None,
      transitions: Vec::new(),
      enter_action: Vec::new(),
      leave_action: Vec::new(),
    }
  }

  /// 将本状态配置为一个内容输入状态，将会优先处理和输入相关的事件，
  /// 并设置输入内容到 status bar 中。
  pub fn input<T, F>(mut self, prompt: T, handler: F) -> Self
  where
    T: Into<String>,
    F: FnMut(&str) + 'static,
  {
    self.input_mode = Some(InputMode {
      prompt: prompt.into(),
      handler: Box::new(handler),
    });
    self
  }

  /// 设置一个简单的事件响应动作
  pub fn action<F>(self, event: KeyEvent, mut act: F) -> Self
  where
    F: FnMut(&mut Pager) + 'static,
  {
    self.goto_action(event, 0, move |pager| {
      act(pager);
      false
    })
  }

  /// 设置一个状态跳转动作
  pub fn goto(self, event: KeyEvent, next_state: usize) -> Self {
    self.goto_action(event, next_state, |_| true)
  }

  /// 设置一个状态跳转动作，但跳转前先执行一个处理流程，返回 true 时进行跳转
  pub fn goto_action<F>(mut self, event: KeyEvent, next_state: usize, act: F) -> Self
  where
    F: FnMut(&mut Pager) -> bool + 'static,
  {
    self.transitions.push(Transition {
      event,
      act: Box::new(act),
      next_state,
    });
    self
  }

  /// 设置进入状态时执行的动作
  pub fn enter_action<F>(mut self, act: F) -> Self
  where
    F: FnMut(&mut Pager) + 'static,
  {
    self.enter_action.push(Box::new(act));
    self
  }

  /// 设置离开状态时执行的动作
  pub fn leave_action<F>(mut self, act: F) -> Self
  where
    F: FnMut(&mut Pager) + 'static,
  {
    self.leave_action.push(Box::new(act));
    self
  }
}

impl State {
  /// 获取状态的名称
  pub fn name(&self) -> &str {
    &self.name
  }

  /// 进入状态时，执行的处理
  fn enter(&mut self, pager: &mut Pager) {
    if let Some(state) = &self.input_mode {
      pager.status().set_input(state.prompt.clone());
    }

    for act in self.enter_action.iter_mut() {
      act(pager);
    }
  }

  /// 离开状态时，执行的处理
  fn leave(&mut self, pager: &mut Pager) {
    for act in self.leave_action.iter_mut() {
      act(pager);
    }
  }

  /// 响应处理键入的事件，返回是否进行状态跳转
  fn react(&mut self, pager: &mut Pager, event: KeyEvent) -> Option<usize> {
    // 处理 repeat 的情况，防止触发过快（一般也不会默认使能这个特性）
    if event.is_repeat() {
      return None;
    }

    // 优先处理输入的相关的事件
    if self.handle_input(pager, event).is_some() {
      return None;
    }

    // 从前往后逐一对比事件响应条件，命中第一个时进行处理
    for t in self.transitions.iter_mut() {
      if t.event.same_as(&event) {
        return if (t.act)(pager) {
          Some(t.next_state)
        } else {
          None
        };
      }
    }

    // 没有找到任何预设的事件
    None
  }

  /// 处理输入事件。如果事件被消耗，返回 true
  fn handle_input(&mut self, pager: &mut Pager, event: KeyEvent) -> Option<()> {
    if !event.is_press() {
      return None;
    }

    // 取出输入模式下的回调处理函数。如果没有配置输入模式，则终止处理
    let handler = &mut self.input_mode.as_mut()?.handler;

    // 响应输入相关的按键输入，记录内容是否变更
    let changed = match event.code {
      KeyCode::Char(to_insert) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
        pager.status().enter_char(to_insert);
        true
      }
      KeyCode::Backspace => pager.status().delete_char(),
      KeyCode::Left => {
        pager.status().move_cursor_left();
        false
      }
      KeyCode::Right => {
        pager.status().move_cursor_right();
        false
      }
      _ => {
        return None;
      }
    };

    if changed && let Some(input) = pager.status().get_input() {
      (handler)(input);
    }

    Some(())
  }
}

/// 状态机的配置
pub struct Config {
  poll_interval: Duration,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      poll_interval: Duration::from_millis(100),
    }
  }
}

/// 处理 UI 的键盘事件，管理多个状态，并执行它们的转移与响应
pub struct StateMachine {
  /// 使用整数索引的所有状态量
  states: HashMap<usize, State>,

  /// 根状态的索引
  root_state_index: usize,

  /// 等待事件到来的时间
  poll_interval: Duration,

  /// 当前正在活跃的状态
  curr_state_index: usize,
}

impl Default for StateMachine {
  fn default() -> Self {
    Self::new(Default::default())
  }
}

impl StateMachine {
  pub fn new(config: Config) -> Self {
    let res = Self {
      states: HashMap::new(),
      root_state_index: 0,
      poll_interval: config.poll_interval,
      curr_state_index: 0,
    };
    res.root_state(0, State::new("default".to_owned()))
  }

  /// 添加一个状态
  pub fn state(mut self, index: usize, state: State) -> Self {
    self.states.insert(index, state);
    self
  }

  /// 添加一个状态，并将其设置为根状态
  pub fn root_state(mut self, index: usize, state: State) -> Self {
    self.root_state_index = index;
    self.curr_state_index = index;
    self.state(index, state)
  }

  /// 状态机的第一次运行，主要的作用是执行了根状态的进入流程
  pub fn first_run(&mut self, page: &mut Pager) {
    self.enter(page, self.root_state_index);
  }

  /// 等待事件，并进行处理，返回是否结束程序
  pub fn poll_once(&mut self, pager: &mut Pager) -> bool {
    match event::poll(self.poll_interval) {
      // 有键入事件，分析是否是 ctrl+c，是则结束程序，
      // 否则响应处理
      Ok(true) => match event::read() {
        // 处理键盘事件
        Ok(Event::Key(event)) => match event {
          // ctrl+c/C 退出进程
          KeyEvent {
            code: KeyCode::Char('c') | KeyCode::Char('C'),
            modifiers: KeyModifiers::CONTROL,
            ..
          } => return true,

          // 处理状态流转，程序继续运行
          event => self.manage_once(pager, event),
        },

        // 非键盘事件，全部忽略，程序继续运行
        Ok(_) => {}

        // 读取事件出错，记录，程序继续运行
        Err(e) => crate::eprintln!("event::read() error: {}", e),
      },

      // 没有事件发生，程序继续运行
      Ok(false) => {}

      // 报错，记录错误，程序继续运行
      Err(e) => crate::eprintln!("event::poll() error: {}", e),
    }

    // 程序继续运行
    false
  }

  fn manage_once(&mut self, pager: &mut Pager, event: KeyEvent) {
    let event = KeyEvent::platform_consistent(event);
    if let Some(next_state_index) = self.get_current_state().react(pager, event) {
      self.leave_current(pager);
      self.enter(pager, next_state_index);
    }
  }

  fn enter(&mut self, pager: &mut Pager, index: usize) {
    self.curr_state_index = index;
    self.get_current_state().enter(pager);
  }

  fn leave_current(&mut self, pager: &mut Pager) {
    self.get_current_state().leave(pager);
  }

  fn get_current_state(&mut self) -> &mut State {
    self
      .states
      .get_mut(&self.curr_state_index)
      .expect(format!("cannot enter state {}", self.curr_state_index).as_str())
  }
}
