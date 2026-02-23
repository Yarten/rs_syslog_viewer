use crate::{
  app::{Controller, Index, LogHubRef},
  log::{LogDirection, LogLine},
  ui::CursorEx,
};
use std::{path::PathBuf, sync::Arc};

/// 展示区里维护的数据条目
type Item = (Index, LogLine);

impl CursorEx for Option<&Item> {
  type Key = Index;
  type Value = LogLine;

  fn key(self) -> Option<Self::Key> {
    self.map(|x| x.0.clone())
  }
}

// 定义日志展示区的可视化数据
crate::view_port!(ViewPort, Item);

impl ViewPort {
  /// 根据已经配置好的光标位置，从指定索引处的日志开始填充数据区
  fn fill(&mut self, data: &mut LogHubRef, index: Index) {
    // 从指定索引位置处，取出正向与逆向的迭代器
    let (mut iter_down, mut iter_up) = data.iter_at(index);
    iter_up.next(); // 光标位置默认用的 iter_down 迭代器插入，因此 iter_up 需要先跳过这一行。

    // 使用 view port ui 的能力，逐一填充数据
    self.do_fill(|dir| match dir {
      LogDirection::Forward => iter_down.next().map(|(index, log)| (index, log.clone())),
      LogDirection::Backward => iter_up.next().map(|(index, log)| (index, log.clone())),
    })
  }
}

/// 时间戳展示风格
#[derive(Default, PartialEq, Copy, Clone)]
pub enum TimestampStyle {
  /// 完整时间戳信息
  Full,

  /// 不展示日期，仅展示时间，精确到毫秒
  Time,

  /// 展示月日以及时间，精确到毫秒
  #[default]
  MonthDayTime,

  /// 仅展示时分秒
  RoughTime,
}

impl TimestampStyle {
  pub fn next(&mut self) {
    *self = match self {
      TimestampStyle::Full => TimestampStyle::Time,
      TimestampStyle::Time => TimestampStyle::MonthDayTime,
      TimestampStyle::MonthDayTime => TimestampStyle::RoughTime,
      TimestampStyle::RoughTime => TimestampStyle::Full,
    }
  }
}

/// 标签展示风格
#[derive(Default, PartialEq, Copy, Clone)]
pub enum TagStyle {
  /// 完整展示
  #[default]
  Full,

  /// 过长在左边省略
  OmitLeft,

  /// 过长在右边省略
  OmitRight,

  /// 过长在中间省略
  OmitMiddle,

  /// 不展示
  Hidden,
}

impl TagStyle {
  pub fn next(&mut self) {
    *self = match self {
      TagStyle::Full => TagStyle::OmitLeft,
      TagStyle::OmitLeft => TagStyle::OmitRight,
      TagStyle::OmitRight => TagStyle::OmitMiddle,
      TagStyle::OmitMiddle => TagStyle::Hidden,
      TagStyle::Hidden => TagStyle::Full,
    }
  }
}

/// PID 展示风格
#[derive(Default, PartialEq, Copy, Clone)]
pub enum PidStyle {
  /// 展示
  Shown,

  /// 不展示
  #[default]
  Hidden,
}

impl PidStyle {
  pub fn next(&mut self) {
    *self = match self {
      PidStyle::Shown => PidStyle::Hidden,
      PidStyle::Hidden => PidStyle::Shown,
    }
  }
}

/// 日志各项内容展示风格配置
#[derive(Default, PartialEq, Copy, Clone)]
pub struct Style {
  pub timestamp_style: TimestampStyle,
  pub tag_style: TagStyle,
  pub pid_style: PidStyle,
  type_index: usize,
}

impl Style {
  pub fn next(&mut self) {
    let style = match self.type_index {
      0 => Style {
        timestamp_style: TimestampStyle::MonthDayTime,
        tag_style: TagStyle::Full,
        pid_style: PidStyle::Hidden,
        type_index: 0,
      },
      1 => Style {
        timestamp_style: TimestampStyle::Time,
        tag_style: TagStyle::OmitLeft,
        pid_style: PidStyle::Hidden,
        type_index: 1,
      },
      2 => Style {
        timestamp_style: TimestampStyle::RoughTime,
        tag_style: TagStyle::Hidden,
        pid_style: PidStyle::Hidden,
        type_index: 2,
      },
      3 => Style {
        timestamp_style: TimestampStyle::Full,
        tag_style: TagStyle::Full,
        pid_style: PidStyle::Shown,
        type_index: 3,
      },
      _ => {
        todo!()
      }
    };

    // 若当前选择的索引对应的风格与预设相同，则应用下一个风格，
    // 否则先回归本索引应有的风格。
    if style == *self {
      self.type_index += 1;
      if self.type_index > 3 {
        self.type_index = 0;
      }
      self.next();
    } else {
      *self = style;
    }
  }
}

/// 日志展示区的控制器
pub struct LogController {
  ///展示区的数据
  view_port: ViewPort,

  /// 日志的根目录
  log_files_root: Option<Arc<PathBuf>>,

  /// 日志展示风格
  style: Style,
}

impl Default for LogController {
  fn default() -> Self {
    let mut res = Self {
      view_port: Default::default(),
      log_files_root: Default::default(),
      style: Default::default(),
    };

    // 默认跟踪最新日志
    res.view_port.ui.want_follow();
    res
  }
}

impl LogController {
  pub fn view_mut(&mut self) -> &mut ViewPort {
    &mut self.view_port
  }
  pub fn view(&self) -> &ViewPort {
    &self.view_port
  }

  pub fn style_mut(&mut self) -> &mut Style {
    &mut self.style
  }
  pub fn style(&self) -> &Style {
    &self.style
  }

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

  /// 定位光标指向的数据索引。因为可能标签过滤规则的变化，会导致原来光标指向的数据不可见了
  fn relocate_cursor_index(data: &mut LogHubRef, index: Index) -> Index {
    let (mut iter_down, mut iter_up) = data.iter_at(index.clone());
    match iter_down.next() {
      None => {}
      Some((index, _)) => return index,
    }
    match iter_up.next() {
      None => {}
      Some((index, _)) => return index,
    }
    index
  }
}

impl Controller for LogController {
  fn run_once(&mut self, data: &mut LogHubRef) {
    // 记录日志根目录
    self.log_files_root = Some(data.data_board().get_root_path().clone());

    // TODO: 刷新上一帧 index 在这一帧的值，根据各个 log file 的增删情况来近似更新
    // 取出变更历史，进行 fix(index)

    // 取出当前光标应指向的数据索引，同时，对光标的位置完成配置
    let cursor_index: Index = self.view_port.apply().key_or(|| data.last_index());

    // 重定位索引，确保它光标总是指向可见的数据
    let cursor_index = Self::relocate_cursor_index(data, cursor_index);

    // 基于当前的光标位置，及其指向的数据索引，填充整个展示区
    self.view_port.fill(data, cursor_index);

    // 如果存在数据顶到头，触发更老的日志加载
    data.try_load_older_logs(
      self
        .view()
        .data
        .front()
        .map(|(first_index, _)| first_index)
        .unwrap_or(&data.first_index()),
    );
  }

  fn view_port(&mut self) -> Option<&mut ViewPortBase> {
    Some(&mut self.view_port.ui)
  }
}
