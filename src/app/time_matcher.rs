use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, Timelike};
use itertools::Itertools;
use lazy_static::lazy_static;
use regex::Regex;
use std::ops::Neg;

/// 时间比较操作符
#[derive(Default, Copy, Clone, Debug)]
enum TimeCmpOp {
  /// 等于指定时间（= tp）
  #[default]
  Equal,

  /// 早于指定时间（<= tp）
  Earlier,

  /// 晚于指定时间（>= tp）
  Later,
}

impl Neg for TimeCmpOp {
  type Output = TimeCmpOp;
  fn neg(self) -> Self::Output {
    match self {
      TimeCmpOp::Equal => TimeCmpOp::Equal,
      TimeCmpOp::Earlier => TimeCmpOp::Later,
      TimeCmpOp::Later => TimeCmpOp::Earlier,
    }
  }
}

/// 时间比较条件，记录了模糊的时间点，以及比较操作
#[derive(Default, Copy, Clone)]
struct TimeCond {
  op: TimeCmpOp,
  year: i32,
  month: u32,
  day: u32,
  hour: Option<u32>,
  minute: Option<u32>,
  second: Option<u32>,
}

impl TimeCond {
  fn is_matched(&self, dt: DateTime<FixedOffset>) -> bool {
    let dt = dt.with_timezone(&Local);
    match self.op {
      TimeCmpOp::Equal => {
        self.year == dt.year()
          && self.month == dt.month()
          && self.day == dt.day()
          && (self.hour.is_none() || self.hour.unwrap() == dt.hour())
          && (self.minute.is_none() || self.minute.unwrap() == dt.minute())
          && (self.second.is_none() || self.second.unwrap() == dt.second())
      }
      TimeCmpOp::Earlier => {
        self.year >= dt.year()
          && self.month >= dt.month()
          && self.day >= dt.day()
          && (self.hour.is_none() || self.hour.unwrap() >= dt.hour())
          && (self.minute.is_none() || self.minute.unwrap() >= dt.minute())
          && (self.second.is_none() || self.second.unwrap() >= dt.second())
      }
      TimeCmpOp::Later => {
        self.year <= dt.year()
          && self.month <= dt.month()
          && self.day <= dt.day()
          && (self.hour.is_none() || self.hour.unwrap() <= dt.hour())
          && (self.minute.is_none() || self.minute.unwrap() <= dt.minute())
          && (self.second.is_none() || self.second.unwrap() <= dt.second())
      }
    }
  }
}

/// 匹配时间点的部分内容
trait TimepointPartParser {
  fn parse(&self, s: &str, con: &mut TimeCond) -> bool;
}

struct TimeParser {
  re: Regex,
}

impl Default for TimeParser {
  fn default() -> Self {
    Self {
      re: Regex::new(r"^([01]?\d|2[0-3]):([0-5]\d)(:([0-5]\d))?$").unwrap(),
    }
  }
}

impl TimepointPartParser for TimeParser {
  fn parse(&self, s: &str, con: &mut TimeCond) -> bool {
    match self.re.captures(s) {
      None => false,
      Some(cap) => {
        con.hour = cap[1].parse::<u32>().ok();
        con.minute = cap[2].parse::<u32>().ok();
        con.second = cap.get(4).and_then(|x| x.as_str().parse::<u32>().ok());
        true
      }
    }
  }
}

struct DateParser {
  re_bar: Regex,
  re_dot: Regex,
}

impl Default for DateParser {
  fn default() -> Self {
    Self {
      re_bar: Regex::new(r"^((\d{4})-)?(0?[1-9]|1[0-2])-(0?[1-9]|[12]\d|3[01])$").unwrap(),
      re_dot: Regex::new(r"^((\d{4})\.)?(0?[1-9]|1[0-2])\.(0?[1-9]|[12]\d|3[01])$").unwrap(),
    }
  }
}

impl TimepointPartParser for DateParser {
  fn parse(&self, s: &str, con: &mut TimeCond) -> bool {
    match self.re_bar.captures(s).or(self.re_dot.captures(s)) {
      None => false,
      Some(cap) => {
        if let Some(year) = cap.get(2).and_then(|x| x.as_str().parse::<i32>().ok()) {
          con.year = year;
        }
        con.month = cap[3].parse::<u32>().unwrap();
        con.day = cap[4].parse::<u32>().unwrap();
        true
      }
    }
  }
}

lazy_static! {
  static ref TIME_PARSER: TimeParser = TimeParser::default();
  static ref DATE_PARSER: DateParser = DateParser::default();
  static ref DURATION_RE: Regex =
    Regex::new(r"^((\d*)d[ \t]*)?((\d*)h[ \t]*)?((\d*)m[ \t]*)?((\d*)s)?$").unwrap();
}

/// 时间信息匹配器。分析给定的字符串，将其解析为时间判断条件。
pub struct TimeMatcher {
  now: DateTime<Local>,
  conditions: Vec<TimeCond>,
}

impl TimeMatcher {
  /// 使用当前时间点，创建本时间条件解析与匹配器，每次处理循环里都得新建
  pub fn new() -> Self {
    Self {
      now: Local::now(),
      conditions: Vec::new(),
    }
  }

  /// 检查给定的时间点是否匹配已有的规则
  pub fn is_matched(&self, dt: DateTime<FixedOffset>) -> bool {
    self.conditions.iter().all(|con| con.is_matched(dt))
  }

  /// 解析给定字符串，转换为时间判断条件。如果解析出错，返回错误信息，可供渲染。
  ///
  /// 格式支持：
  /// 1. 使用多个逗号隔开条件，这些条件是与关系；
  /// 2. 支持时间表达与间隔表达，间隔指的是距离当下时间过去多久的间隔；
  /// 3. 时间格式包括：
  ///     - 时间： 11:59, 11:59:59
  ///     - 日期： 2025.11.09, 11.09
  ///     - 任取上述部分其中之一、或者两者一起用（不分先后），使用空白字符隔开。
  /// 4. 间隔格式包括：
  ///     - 3d：天
  ///     - 4h：时
  ///     - 5m：分
  ///     - 6s：秒
  ///     - 上述各个部分任取或组合，但一定要按上述顺序排列，各部分间可以有空格，但数字和单位间不能有空格；
  ///     - 以最精确地单位作为模糊比较的单位。
  /// 5. 支持的操作符包括：
  ///     - 11:30 ~ 12:00：时间范围符，仅支持时间使用；
  ///     - \> 11:30：晚于指定时间；
  ///     - \> 1d：大于指定间隔，也即早于现在时刻 - 间隔；
  ///     - = 11:30：处于指定时间（模糊匹配，见下文）
  ///     - = 1d：处于指定间隔，也即处于现在时刻 - 间隔；
  ///     - < 11:30：早于指定时间；
  ///     - < 1d：小于指定间隔，也即晚于现在时刻 - 间隔。
  ///
  ///     Tips: \<\> 比较符对于时间和间隔而言，意义刚好相反，但符合直觉。
  /// 6. 模糊匹配规则：仅使用字符串里精度最小的单位进行时间对比，精度更小的单位则不参与比较。
  pub fn parse(&mut self, cmd: &str) -> Result<(), String> {
    // 使用逗号，分割多个部分，并 trim 每个部分的空格
    cmd
      .split(',')
      .map(|s| s.trim())
      .filter(|s| !s.is_empty())
      .try_for_each(|con| self.parse_con(con))
  }

  /// 解析其中一个条件
  fn parse_con(&mut self, con: &str) -> Result<(), String> {
    // 检查是否包含 '~' 分隔符
    let range_parts = con.split('~').map(|s| s.trim()).collect_vec();

    // 要么不包含 '-'，要么必须是两个部分
    match range_parts.len() {
      // 不包含 ~，则视作独立的条件
      1 => {
        let part = range_parts[0];
        let (op, part) = match part.chars().next() {
          None => return Err(String::from(("Wrong format: empty definition !"))),
          Some('<') => (TimeCmpOp::Earlier, &part[1..]),
          Some('>') => (TimeCmpOp::Later, &part[1..]),
          Some('=') => (TimeCmpOp::Equal, &part[1..]),
          Some(_) => (TimeCmpOp::Equal, part),
        };

        self.conditions.push(self.parse_term(part.trim(), op)?);
        Ok(())
      }
      // 如果包含 ~，且刚好能分割成两部分，则将该条件视作时间点的范围条件
      2 => {
        let start_tp = range_parts[0];
        let end_tp = range_parts[1];

        if start_tp.is_empty() || end_tp.is_empty() {
          Err(format!("Wrong format: time range '{con}' is broken !"))
        } else {
          self
            .conditions
            .push(self.parse_term_as_timepoint(start_tp, TimeCmpOp::Later)?);
          self
            .conditions
            .push(self.parse_term_as_timepoint(end_tp, TimeCmpOp::Earlier)?);
          Ok(())
        }
      }
      _ => Err(format!("Wrong format: too many '~' in '{con}' !")),
    }
  }

  /// 将字符串解析为一个时间或间隔
  fn parse_term(&self, term: &str, op: TimeCmpOp) -> Result<TimeCond, String> {
    self
      .parse_term_as_duration(term, -op)
      .or(self.parse_term_as_timepoint(term, op))
  }

  /// 将字符串解析为一个时间间隔
  fn parse_term_as_duration(&self, term: &str, op: TimeCmpOp) -> Result<TimeCond, String> {
    match DURATION_RE.captures(term) {
      None => Err(format!(
        "Wrong format: duration '{term}' cannot be parsed !"
      )),
      Some(cap) => {
        let days = cap.get(2).and_then(|x| x.as_str().parse::<u32>().ok());
        let hours = cap.get(4).and_then(|x| x.as_str().parse::<u32>().ok());
        let minutes = cap.get(6).and_then(|x| x.as_str().parse::<u32>().ok());
        let seconds = cap.get(8).and_then(|x| x.as_str().parse::<u32>().ok());

        // 按出现单位精度从高到底处理，以最高精度的单位作为模糊匹配的单位
        let result = if let Some(seconds) = seconds {
          let seconds = seconds
            + minutes.map(|n| n * 60).unwrap_or(0)
            + hours.map(|n| n * 3600).unwrap_or(0)
            + days.map(|n| n * 3600 * 24).unwrap_or(0);
          self.generate_condition_by_seconds(op, seconds)
        } else if let Some(minutes) = minutes {
          let minutes =
            minutes + hours.map(|n| n * 60).unwrap_or(0) + days.map(|n| n * 60 * 24).unwrap_or(0);
          self.generate_condition_by_minutes(op, minutes)
        } else if let Some(hours) = hours {
          let hours = hours + days.map(|n| n * 24).unwrap_or(0);
          self.generate_condition_by_hours(op, hours)
        } else if let Some(days) = days {
          self.generate_condition_by_days(op, days)
        } else {
          None
        };

        // 处理最终结果
        match result {
          None => Err(format!(
            "Wrong format: duration '{term}' cannot be parsed !"
          )),
          Some(cond) => Ok(cond),
        }
      }
    }
  }

  /// 将字符串解析为一个时间点
  fn parse_term_as_timepoint(&self, term: &str, op: TimeCmpOp) -> Result<TimeCond, String> {
    let term_parts = term.split_whitespace().collect_vec();
    match term_parts.len() {
      1 | 2 => {}
      _ => {
        return Err(format!(
          "Wrong format: timepoint '{term}' has too many parts !"
        ));
      }
    }

    let mut has_date = false;
    let mut has_time = false;
    let mut con = self.generate_condition(op);

    for term_part in term_parts {
      if TIME_PARSER.parse(term_part, &mut con) {
        if has_time {
          return Err(format!(
            "Wrong format: timepoint '{term}' has too many time !"
          ));
        }
        has_time = true;
      } else if DATE_PARSER.parse(term_part, &mut con) {
        if has_date {
          return Err(format!(
            "Wrong format: timepoint '{term}' has too many dates !"
          ));
        }
        has_date = true;
      } else {
        return Err(format!(
          "Wrong format: timepoint '{term}' cannot be parsed !"
        ));
      }
    }

    Ok(con)
  }

  fn generate_condition(&self, op: TimeCmpOp) -> TimeCond {
    TimeCond {
      op,
      year: self.now.year(),
      month: self.now.month(),
      day: self.now.day(),
      ..TimeCond::default()
    }
  }

  fn generate_condition_by_seconds(&self, op: TimeCmpOp, seconds: u32) -> Option<TimeCond> {
    let now = self.now.with_nanosecond(0)? - Duration::seconds(seconds as i64);
    Some(TimeCond {
      op,
      year: now.year(),
      month: now.month(),
      day: now.day(),
      hour: Some(now.hour()),
      minute: Some(now.minute()),
      second: Some(now.second()),
    })
  }

  fn generate_condition_by_minutes(&self, op: TimeCmpOp, minutes: u32) -> Option<TimeCond> {
    let now = self.now.with_nanosecond(0)?.with_second(0)? - Duration::minutes(minutes as i64);
    Some(TimeCond {
      op,
      year: now.year(),
      month: now.month(),
      day: now.day(),
      hour: Some(now.hour()),
      minute: Some(now.minute()),
      ..Default::default()
    })
  }

  fn generate_condition_by_hours(&self, op: TimeCmpOp, hours: u32) -> Option<TimeCond> {
    let now = self
      .now
      .with_nanosecond(0)?
      .with_second(0)?
      .with_minute(0)?
      - Duration::hours(hours as i64);
    Some(TimeCond {
      op,
      year: now.year(),
      month: now.month(),
      day: now.day(),
      hour: Some(now.hour()),
      ..Default::default()
    })
  }

  fn generate_condition_by_days(&self, op: TimeCmpOp, days: u32) -> Option<TimeCond> {
    let now = self
      .now
      .with_nanosecond(0)?
      .with_second(0)?
      .with_minute(0)?
      .with_hour(0)?
      - Duration::days(days as i64);
    Some(TimeCond {
      op,
      year: now.year(),
      month: now.month(),
      day: now.day(),
      ..Default::default()
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse() {
    let mut tm = TimeMatcher::new();
    tm.parse("< 1d").expect("should parse");
    tm.parse("= 1d 23h 33m 100s").expect("should parse");
    tm.parse("> 23h100s").expect("should parse");
    tm.parse(" 23h 100s").expect("should parse");
    tm.parse("100s 23h").err().expect("should not parse");
    tm.parse("? 23h").err().expect("should not parse");

    tm.parse("< 2025.10.11").expect("should parse");
    tm.parse("< 2025-10-11").expect("should parse");
    tm.parse("2025.10.11 ~ 1.2").expect("should parse");
    tm.parse("> 23:59").expect("should parse");
    tm.parse("> 23:59:02").expect("should parse");
    tm.parse("2025.09.10 11:22:33").expect("should parse");
    tm.parse("11:22:33 2025.09.10").expect("should parse");
    tm.parse("2025-10.11").err().expect("should not parse");
    tm.parse("2025-10").err().expect("should not parse");
    tm.parse("1.2 ~ 1.3 ~ 1.4").err().expect("should not parse");

    tm.parse("< 1d, > 23h5s   , 1d 3s  , = 1s , 1.30 ~ 11:22 , > 11:22:33 2025.09.10")
      .expect("should parse");
  }

  #[test]
  fn test_match_duration() {
    let mut tm = TimeMatcher::new();
    let now = tm.now;

    tm.parse("> 2d").expect("should parse");
    assert!(tm.is_matched(now.fixed_offset() - Duration::days(3)));
    assert!(tm.is_matched(now.fixed_offset() - Duration::days(2)));
    assert!(!tm.is_matched(now.fixed_offset() - Duration::days(1)));

    tm.parse("< 2d5h").expect("should parse");
    assert!(!tm.is_matched(now.fixed_offset() - Duration::days(3)));
    assert!(tm.is_matched(now.fixed_offset() - Duration::days(2) - Duration::hours(4)));
    assert!(!tm.is_matched(now.fixed_offset() - Duration::days(1)));
  }

  #[test]
  fn test_match_timepoint() {
    let mut tm = TimeMatcher::new();
    let now = tm.now;

    tm.parse(&format!(
      "{}-{}-{} {}:{}",
      now.year(),
      now.month(),
      now.day(),
      now.hour(),
      now.minute()
    ))
    .expect("should parse");
    assert!(tm.is_matched(now.fixed_offset()));
  }

  #[test]
  fn test_match_time_range() {
    let mut tm = TimeMatcher::new();
    let now = tm.now;

    tm.parse(&format!(
      "{}-{}-{} ~ {}-{}-{}",
      now.year(),
      now.month(),
      now.day(),
      now.year(),
      now.month(),
      now.day() + 3
    ))
    .expect("should parse");
    assert!(tm.is_matched(now.fixed_offset()));
    assert!(tm.is_matched(now.fixed_offset() + Duration::days(2)));
    assert!(tm.is_matched(now.fixed_offset() + Duration::days(3)));
    assert!(!tm.is_matched(now.fixed_offset() + Duration::days(4)));
  }
}
