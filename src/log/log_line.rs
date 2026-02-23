//! 描述一条、也即一行的系统日志，并维护相关操作状态

use crate::log::LogLine::{Bad, Good};
use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDateTime};
use lazy_static::lazy_static;
use std::cmp::Ordering;

/// 日志内容标签
#[derive(PartialEq, Debug, Clone, Default)]
pub enum Label {
  #[default]
  Unknown,
  Debug,
  Info,
  Warn,
  Error,
}

/// 日志遍历的方向，主要用于描述 LogLink 的方向
#[derive(Clone, Copy)]
pub enum LogDirection {
  /// 正向遍历，也即从旧到新
  Forward,

  /// 逆向遍历，也即从新到旧
  Backward,
}

/// 在有序的遍历序列中，用于快速跳转的“软链接”，使用相对差距进行定义。
///
/// 在迭代过程中，我们希望跳过某些不符合 tag 过滤规则的日志，为了防止每次
/// 遍历都得进行大量无效迭代和条件判断，遂设计这么一个链接数据结构体，
/// 用于缓存之前的分析结果。
#[derive(Default, Debug, PartialEq, Copy, Clone)]
pub struct LogLink {
  /// 版本号，用于标识该链接是否有效
  pub ver: usize,

  /// 从本日志行跳转到下一条日志行，还需要跳过多少步长
  pub skip: usize,
}

/// 来自 syslog 的日志行
#[derive(Debug, Clone, Default)]
pub struct NormalLogLine {
  /// 日志的产生时间
  pub timestamp: DateTime<FixedOffset>,

  /// 日志的标签
  pub tag: String,

  /// 产生的进程 PID，如果是 rsyslog 自己的日志，这个值为 0
  pub pid: i32,

  /// 内容
  pub message: String,

  /// 内容里若包含特定的字样，会被贴上相关标签，只能贴第一个相关的
  pub label: Label,

  /// 标记该日志是否被 marked，用于 viewer 快速定位
  pub marked: bool,

  /// 正向迭代的跳转链接
  pub forward_link: LogLink,

  /// 逆向迭代的跳转链接
  pub backward_link: LogLink,
}

impl PartialEq for NormalLogLine {
  fn eq(&self, other: &Self) -> bool {
    self.timestamp == other.timestamp
      && self.tag == other.tag
      && self.pid == other.pid
      && self.message == other.message
      && self.label == other.label
      && self.marked == other.marked
  }
}

/// 无法解析的日志行
#[derive(PartialEq, Debug, Clone, Default)]
pub struct BrokenLogLine {
  /// 内容
  pub content: String,

  /// 标记该日志是否被 marked，用于 viewer 快速定位
  pub marked: bool,
}

/// 记录当前的时间
struct NowDate {
  now: DateTime<Local>,
  year: i32,
}

impl NowDate {
  pub fn new() -> NowDate {
    let now = Local::now();
    let year = now.year();
    NowDate { now, year }
  }
}

lazy_static! {
  static ref NOW_DATE: NowDate = NowDate::new();
}

/// 日志行
#[derive(PartialEq, Debug, Clone)]
pub enum LogLine {
  Good(NormalLogLine),
  Bad(BrokenLogLine),
}

impl LogLine {
  pub fn new(line: String) -> LogLine {
    let bytes = line.as_bytes();

    // 尝试解析不同时间戳格式的系统日志行
    if let Some((timestamp, seeker)) = Self::try_parse_any_timestamp(bytes)
      && let Some(log) = Self::try_parse_rest(timestamp, seeker)
    {
      LogLine::Good(log)
    } else {
      LogLine::Bad(BrokenLogLine {
        content: line,
        ..Default::default()
      })
    }
  }

  fn try_parse_any_timestamp(bytes: &'_ [u8]) -> Option<(DateTime<FixedOffset>, BytesSeeker<'_>)> {
    Self::try_parse_modern_timestamp(&bytes).or(Self::try_parse_traditional_timestamp(&bytes))
  }

  fn try_parse_modern_timestamp(
    bytes: &'_ [u8],
  ) -> Option<(DateTime<FixedOffset>, BytesSeeker<'_>)> {
    const RFC3339_STR_LEN: usize = 32;

    // 内容搜索器
    let mut seeker = BytesSeeker::new(&bytes);

    // 可以通过长度和第 10 位的固定字符来快速判断
    if let Some(timestamp) = seeker.take(RFC3339_STR_LEN)
      && timestamp[10] == b'T'
    {
      let timestamp = String::from_utf8_lossy(timestamp);
      let timestamp = DateTime::parse_from_rfc3339(&timestamp).ok()?;
      Some((timestamp, seeker))
    } else {
      None
    }
  }

  fn try_parse_traditional_timestamp(
    bytes: &'_ [u8],
  ) -> Option<(DateTime<FixedOffset>, BytesSeeker<'_>)> {
    const TRADITIONAL_TIME_STR_LEN: usize = 15;

    // 内容搜索器
    let mut seeker = BytesSeeker::new(&bytes);

    // 取出前缀的时间戳字节
    let timestamp = seeker.take(TRADITIONAL_TIME_STR_LEN)?;
    let timestamp =
      String::from_utf8_lossy(timestamp).to_string() + NOW_DATE.year.to_string().as_str();

    // 传统的时间戳字符串没有年份信息，只能认为日志在今年，补充上再解析。
    // 另外，时区信息也缺失，我们也只能拿当地时间时区进行假设与补充
    let dt = NaiveDateTime::parse_from_str(&timestamp, "%b %d %T%Y").ok()?;
    let dt = dt.and_local_timezone(Local).single()?;

    // 由于没有准确的年份信息，当日志卡在年份跨越时，时间可能会出现错误，分不清是上一年还是下一年，
    // 选择那个不晚于“现在”的最近日期
    let dt_prev_year = dt.with_year(NOW_DATE.year - 1)?;
    let final_dt = if dt <= NOW_DATE.now { dt } else { dt_prev_year };

    Some((final_dt.fixed_offset(), seeker))
  }

  fn try_parse_rest(
    timestamp: DateTime<FixedOffset>,
    mut seeker: BytesSeeker,
  ) -> Option<NormalLogLine> {
    // 按照这样的格式解析：
    // {timestamp} {hostname} {tag}[{pid}]: {message..}
    // 其中，timestamp 已经被解析，另外，rsyslog 自己的日志，没有 pid 的部分。
    // 跳过 hostname
    seeker.next_is(b' ')?;
    seeker.find_next(b' ')?;

    // 找到 tag 与 pid 部分。由于 pid 不一定存在，因此我们只能直接找到 : 前的所有
    let bytes_tag_and_ip = seeker.find_next(b':')?;
    seeker.next_is(b' ')?;

    // 剩余的全部是日志内容
    let message = seeker.rest_of_all();
    let message = String::from_utf8_lossy(message).to_string();

    // 切割 tag 和 pid
    let (tag, pid) = {
      let mut seeker = BytesSeeker::new(&bytes_tag_and_ip);
      if let Some(tag) = seeker.find_next(b'[') {
        let pid = seeker.find_next(b']')?;
        (tag, String::from_utf8_lossy(pid).parse::<i32>().ok()?)
      } else {
        (bytes_tag_and_ip, 0)
      }
    };

    let tag = String::from_utf8_lossy(&tag).to_string();

    // 返回结果
    Some(NormalLogLine {
      timestamp,
      tag,
      pid,
      message,
      ..Default::default()
    })
  }
}

/// 将字符串解析为日志行数据的字节串分析器
struct BytesSeeker<'a> {
  bytes: &'a [u8],
}

impl<'a> BytesSeeker<'a> {
  fn new(bytes: &'a [u8]) -> BytesSeeker<'a> {
    Self { bytes }
  }

  fn take(&mut self, count: usize) -> Option<&'a [u8]> {
    if self.bytes.len() >= count {
      let res = &self.bytes[..count];
      self.bytes = &self.bytes[count..];
      Some(res)
    } else {
      None
    }
  }

  fn next_is(&mut self, byte: u8) -> Option<()> {
    if let Some(&b) = self.bytes.first()
      && b == byte
    {
      self.bytes = &self.bytes[1..];
      Some(())
    } else {
      None
    }
  }

  fn find_next(&mut self, byte: u8) -> Option<&'a [u8]> {
    if let Some(pos) = self.bytes.iter().position(|&b| b == byte)
      && pos != 0
      && self.bytes[pos - 1] != b'\\'
    {
      let res = &self.bytes[..pos];
      self.bytes = &self.bytes[pos + 1..];
      Some(res)
    } else {
      None
    }
  }

  fn rest_of_all(self) -> &'a [u8] {
    self.bytes
  }
}

impl LogLine {
  /// 比较两个日志，如果左边日志旧于右边日志，返回 Less，
  /// 如果日志中有坏行，总是认为该坏行是更旧的（早点让它出现，否则它可能会饿死下一条正常日志）
  pub fn is_older(lhs: &LogLine, rhs: &LogLine) -> Ordering {
    match (lhs, rhs) {
      (Good(lhs), Good(rhs)) => lhs.timestamp.cmp(&rhs.timestamp),
      (Good(_), Bad(_)) => Ordering::Greater,
      (Bad(_), Good(_)) => Ordering::Less,
      (Bad(_), Bad(_)) => Ordering::Less,
    }
  }

  /// 比较两个日志，如果左边日志新于右边日志，返回 Less，
  /// 如果日志中有坏行，总是认为该坏行是更新的（早点让它出现，否则它可能会饿死下一条正常日志）
  pub fn is_newer(lhs: &LogLine, rhs: &LogLine) -> Ordering {
    match (lhs, rhs) {
      (Good(lhs), Good(rhs)) => lhs.timestamp.cmp(&rhs.timestamp).reverse(),
      (Good(_), Bad(_)) => Ordering::Greater,
      (Bad(_), Good(_)) => Ordering::Less,
      (Bad(_), Bad(_)) => Ordering::Less,
    }
  }

  /// 切换本条日志的标记状态
  pub fn toggle_mark(&mut self) {
    match self {
      Good(log) => log.marked = !log.marked,
      Bad(log) => log.marked = !log.marked,
    }
  }

  /// 获取本日志是否被标记
  pub fn is_marked(&self) -> bool {
    match self {
      Good(log) => log.marked,
      Bad(log) => log.marked,
    }
  }

  /// 获取本行日志目标遍历方向的下一跳信息
  pub fn get_link(&self, direction: LogDirection) -> LogLink {
    match self {
      Good(log) => match direction {
        LogDirection::Forward => log.forward_link,
        LogDirection::Backward => log.backward_link,
      },
      Bad(_) => LogLink::default(),
    }
  }

  /// 设置本日志行新的下一跳信息
  pub fn set_link(&mut self, direction: LogDirection, new_link: LogLink) {
    match self {
      Good(log) => match direction {
        LogDirection::Forward => log.forward_link = new_link,
        LogDirection::Backward => log.backward_link = new_link,
      },
      Bad(_) => {}
    }
  }

  /// 获取日志的标签
  pub fn get_tag(&self) -> Option<&str> {
    match self {
      Good(log) => Some(&log.tag),
      Bad(_) => None,
    }
  }

  /// 获取日志内容
  pub fn get_content(&self) -> &str {
    match self {
      Good(log) => &log.message,
      Bad(log) => &log.content,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_modern() {
    let timestamp = "2026-01-17T10:22:55.642782+08:00";
    let tag = "gnome-shell";
    let pid = 3208;
    let content = "Can't update stage views actor <unnamed>[<MetaSurfaceActorX11>:0x572a8221c360] is on because it needs an allocation.";
    let log = LogLine::new(format!(
      "{timestamp} yarten-Dell-G16-7630 {tag}[{pid}]: {content}"
    ));

    let log = match log {
      LogLine::Good(log) => log,
      LogLine::Bad(_) => {
        panic!("bad log line")
      }
    };

    assert_eq!(
      log.timestamp,
      DateTime::parse_from_rfc3339(timestamp).unwrap()
    );
    assert_eq!(log.tag, tag);
    assert_eq!(log.pid, pid);
    assert_eq!(log.message, content);
  }

  #[test]
  fn test_parse_traditional() {
    let timestamp = "Jan 15 22:41:02";
    let tag = "gnome-shell";
    let pid = 3203;
    let content = "Can't update stage views actor <unnamed>[<MetaSurfaceActorX11>:0x6178166954e0] is on because it needs an allocation.";
    let log = LogLine::new(format!(
      "{timestamp} yarten-Dell-G16-7630 {tag}[{pid}]: {content}"
    ));

    let log = match log {
      LogLine::Good(log) => log,
      LogLine::Bad(_) => {
        panic!("bad log line")
      }
    };

    assert_eq!(log.tag, tag);
    assert_eq!(log.pid, pid);
    assert_eq!(log.message, content);
  }
}
