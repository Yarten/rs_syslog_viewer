use crate::{
  app::{Controller, Index, LogHubRef, then::Then},
  log::LogLine, ui::{ViewPort as ViewPortBase, ViewPortEx},
};
use std::sync::Arc;
use std::{collections::VecDeque, path::PathBuf};
use crate::log::LogDirection;

/// 展示区里维护的数据条目
type Item = (Index, LogLine);

/// 为 cursor 返回的数据类型做一些能力扩展，方便取用数据
trait CursorEx {
  fn index(self) -> Option<Index>;

  fn index_or(self, fallback: impl Fn() -> Index) -> Index
  where
    Self: Sized,
  {
    self.index().unwrap_or((fallback)())
  }
}

impl CursorEx for Option<&Item> {
  fn index(self) -> Option<Index> {
    self.map(|x| x.0.clone())
  }
}

/// 维护日志展示区的数据
#[derive(Default)]
struct ViewPort {
  /// 展示区 UI 相关的数据
  ui: ViewPortBase,

  /// 日志行，从前往后对应展示区的日志从上往下
  logs: VecDeque<Item>,
}

impl Then for ViewPortBase {}
impl Then for ViewPort {}

impl ViewPort {
  /// 顶部的数据
  fn top(&self) -> Option<&Item> {
    self.logs.front()
  }

  /// 底部的数据
  fn bottom(&self) -> Option<&Item> {
    self.logs.back()
  }

  /// 获取光标指向的数据
  fn cursor(&self) -> Option<&Item> {
    self.logs.get(self.ui.cursor())
  }

  /// 移动光标指定步长，并返回指向的数据（旧的，上一帧的内容）
  fn move_cursor(&mut self, steps: isize) -> Option<&Item> {
    self.ui.set_cursor((self.ui.cursor() as isize + steps).max(0) as usize);
    self.logs.get(self.ui.cursor())
  }

  /// 根据已经配置好的光标位置，从指定索引处的日志开始填充数据区
  fn fill(&mut self, data: &mut LogHubRef, index: Index) {
    // 清除已有的数据
    self.logs.clear();

    // 从指定索引位置处，取出正向与逆向的迭代器
    let (mut iter_down, mut iter_up) = data.iter_at(index);
    iter_up.next(); // 光标位置默认用的 iter_down 迭代器插入，因此 iter_up 需要先跳过这一行。

    // 使用 view port ui 的能力，逐一填充数据
    self.ui.fill(
      |dir| match dir {
        LogDirection::Forward => match iter_down.next() {
          None => false,
          Some((index, log)) => {
            self.logs.push_back((index, log.clone()));
            true
          }
        }
        LogDirection::Backward => match iter_up.next() {
          None => false,
          Some((index, log)) => {
            self.logs.push_front((index, log.clone()));
            true
          }
        }
      }
    )
  }
}

impl ViewPortEx for ViewPort {
  fn ui(&mut self) -> &mut ViewPortBase {
    &mut self.ui
  }
}

/// 描述本帧内的控制
enum Control {
  /// 没有动作，光标将停在上一帧的位置
  Idle,

  /// 跟随最新日志
  Follow,

  /// 逐步移动日志
  MoveBySteps(isize),

  /// 往上翻页
  PageUp,

  /// 往下翻页
  PageDown,
}

pub struct LogController {
  /// 当帧需要处理的控制
  control: Control,

  ///展示区的数据
  view_port: ViewPort,

  /// 日志的根目录
  log_files_root: Option<Arc<PathBuf>>,
}

impl LogController {
  pub fn new() -> Self {
    Self {
      control: Control::Follow,
      view_port: Default::default(),
      log_files_root: Default::default(),
    }
  }

  /// 总是跟踪到最新的日志（退出导航模式）
  pub fn follow(&mut self) {
    self.control = Control::Follow;
  }

  /// 更新日志区高度
  pub fn set_height(&mut self, height: usize) {
    self.view_port.set_height(height);
  }

  /// 按步移动光标
  pub fn move_by_steps(&mut self, steps: isize) {
    self.control = Control::MoveBySteps(steps);
  }

  /// 往上翻页
  pub fn page_up(&mut self) {
    self.control = Control::PageUp;
  }

  /// 往下翻页
  pub fn page_down(&mut self) {
    self.control = Control::PageDown;
  }
}

impl LogController {
  /// 日志所处根目录
  pub fn logs_root(&self) -> &str {
    if let Some(root) = &self.log_files_root
      && let Some(root) = root.to_str()
    {
      root
    } else {
      "logs"
    }
  }

  /// 展示的日志条目
  pub fn logs(&self) -> impl Iterator<Item = &LogLine> {
    self.view_port.logs.iter().map(|x| &x.1)
  }

  /// 光标所在的行索引
  pub fn cursor(&self) -> usize {
    self.view_port.ui.cursor()
  }
}

impl Controller for LogController {
  fn run_once(&mut self, data: &mut LogHubRef) {
    // 记录日志根目录
    self.log_files_root = Some(data.data_board().get_root_path().clone());

    // TODO: 刷新上一帧 index 在这一帧的值，根据各个 log file 的增删情况来近似更新
    // 取出变更历史，进行 fix(index)

    // 取出当前光标应指向的数据索引，同时，对光标的位置完成配置
    let cursor_index: Index = match self.control {
      Control::Idle => self.view_port.cursor().index_or(|| data.last_index()),

      Control::Follow => self
        .view_port
        .ui
        .set_cursor_at_bottom()
        .then(|| data.last_index()),

      Control::MoveBySteps(n) => self.view_port.move_cursor(n).index_or(|| data.last_index()),

      Control::PageUp => self
        .view_port
        .set_cursor_at_bottom()
        .top()
        .index_or(|| data.last_index()),

      Control::PageDown => self
        .view_port
        .set_cursor_at_top()
        .bottom()
        .index_or(|| data.last_index()),
    };

    // 基于当前的光标位置，及其指向的数据索引，填充整个展示区
    self.view_port.fill(data, cursor_index.clone());

    // 如果存在数据顶到头，触发更老的日志加载
    data.try_load_older_logs(cursor_index);

    // 重置控制量
    match self.control {
      Control::Follow => {}
      _ => self.control = Control::Idle,
    }
  }
}
