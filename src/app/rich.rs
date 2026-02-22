use lazy_static::lazy_static;
use ratatui::{
  prelude::Modifier,
  style::{Color, Style, Styled},
  text::{self, Span},
  widgets::ListItem,
};
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::{Range, RangeFrom};

/// 在文本中查找所有匹配的子字符串区间（不考虑重叠）
///
/// # 参数
/// - `content`: 要搜索的原始文本
/// - `search`: 要查找的子字符串
///
/// # 返回值
/// - 返回 `Vec<(usize, usize)>`，每个元素是一个匹配的(start, end)区间
/// - 区间是左闭右开的：[start, end)
pub fn find_all_matches(content: &str, search: &str) -> Vec<(usize, usize)> {
  // 如果搜索字符串为空，返回空结果
  if search.is_empty() {
    return Vec::new();
  }

  let mut matches = Vec::new();
  let search_len = search.len();

  // 使用字符串的 find 方法进行搜索
  let mut start = 0;

  while let Some(pos) = content[start..].find(search) {
    let match_start = start + pos;
    let match_end = match_start + search_len;

    matches.push((match_start, match_end));

    // 移动到下一个位置继续搜索（考虑重叠）
    // 如果想要非重叠搜索，可以用：start = match_end;
    start = match_start + 1;

    // 防止无限循环（当 search_len == 0 时）
    if search_len == 0 {
      break;
    }
  }

  matches
}

fn substring(cow: Cow<str>, range: Range<usize>) -> Cow<str> {
  match cow {
    Cow::Borrowed(s) => Cow::Borrowed(&s[range]),
    Cow::Owned(s) => {
      let substring = s[range.clone()].to_string();
      Cow::Owned(substring)
    }
  }
}

/// 将匹配区间应用到已有的 Span 列表上
pub fn apply_matches_on_spans<'a>(
  spans: Vec<(Span<'a>, (usize, usize))>,
  matches: Vec<(usize, usize)>,
) -> Vec<Span<'a>> {
  // 交叉投影：将匹配区间映射到各个 Span 上
  let mut result_spans: Vec<Span<'a>> = Vec::new();
  let mut match_idx = 0;

  for (span, (span_start, span_end)) in spans.into_iter() {
    // 如果没有匹配区间了，直接添加剩余的 Span
    if match_idx >= matches.len() {
      result_spans.push(span);
      continue;
    }

    let span_text = span.content;
    let span_style = span.style;
    let span_len = span_text.len();

    let mut span_result = Vec::new();
    let mut current_pos = 0; // 在当前 Span 文本内的位置

    // 处理当前 Span 范围内的所有匹配
    while current_pos < span_len && match_idx < matches.len() {
      let (match_start, match_end) = matches[match_idx];

      // 计算匹配在当前 Span 中的相对位置
      let match_in_span_start = match_start.saturating_sub(span_start);
      let match_in_span_end = match_end.saturating_sub(span_start);

      // 如果匹配完全在当前 Span 之前，跳过这个匹配
      if match_end <= span_start {
        match_idx += 1;
        continue;
      }

      // 如果匹配完全在当前 Span 之后，结束当前 Span 的处理
      if match_start >= span_end {
        break;
      }

      // 匹配与当前 Span 有重叠部分
      let overlap_start = match_in_span_start.max(current_pos);
      let overlap_end = match_in_span_end.min(span_len);

      // 添加匹配前的部分（如果有）
      if overlap_start > current_pos {
        let before_text = substring(span_text.clone(), current_pos..overlap_start);
        span_result.push(Span::styled(before_text, span_style));
      }

      // 添加匹配的部分（应用加粗）
      if overlap_start < overlap_end {
        let match_text = substring(span_text.clone(), overlap_start..overlap_end);
        let bold_style = span_style.add_modifier(Modifier::REVERSED);
        span_result.push(Span::styled(match_text, bold_style));
      }

      // 移动当前位置
      current_pos = overlap_end;

      // 如果匹配已经处理完，移动到下一个匹配
      if match_end <= span_end {
        match_idx += 1;
      } else {
        // 匹配跨越了多个 Span，在当前 Span 中只处理了部分
        break;
      }
    }

    // 添加 Span 中剩余的部分（如果有）
    if current_pos < span_len {
      let remaining_text = substring(span_text, current_pos..span_len);
      span_result.push(Span::styled(remaining_text, span_style));
    }

    // 将当前 Span 的处理结果添加到最终结果
    result_spans.extend(span_result);
  }

  result_spans
}

#[derive(Default)]
struct Highlighter {
  patterns: Vec<(usize, Regex)>,
  styles: HashMap<usize, Style>,
}

impl Highlighter {
  fn new() -> Self {
    let url_style = Style::default().blue().underlined();

    Self::build(vec![
      // URL
      (
        Regex::new(r#"(?i)\b(?:https?|ftp|ftps|file|mailto|tel)://[^\s<>"']+"#).unwrap(),
        url_style,
      ),
      (
        Regex::new(r#"(?i)\b(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,}\b"#).unwrap(),
        url_style,
      ),
      (
        Regex::new(r#"\b(?:\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}|\[[0-9a-fA-F:]+])\b"#).unwrap(),
        url_style,
      ),
      // 时间
      (
        Regex::new(r"\b\d{2}:\d{2}(:\d{2}(\.\d+)?)?\b").unwrap(),
        Style::default().magenta(),
      ),
      // 日期
      (
        Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").unwrap(),
        Style::default().magenta(),
      ),
      // 数字
      (
        Regex::new(r"\b\d+(\.\d+)?\b").unwrap(),
        Style::default().cyan(),
      ),
    ])
  }

  fn build(styled_patterns: Vec<(Regex, Style)>) -> Self {
    let mut res = Self::default();
    res.patterns.reserve(styled_patterns.len());
    res.styles.reserve(styled_patterns.len());

    for (index, (regex, style)) in styled_patterns.into_iter().enumerate() {
      res.patterns.push((index, regex));
      res.styles.insert(index, style);
    }

    res
  }

  fn highlight<'a>(&self, text: &'a str) -> Vec<(Span<'a>, (usize, usize))> {
    if text.is_empty() {
      return vec![];
    }

    let mut spans = Vec::new();
    let mut last_end = 0;
    let mut matches = Vec::new();

    // 收集所有匹配
    for (text_type, pattern) in &self.patterns {
      for mat in pattern.find_iter(text) {
        matches.push((mat.start(), mat.end(), *text_type));
      }
    }

    // 按起始位置排序
    matches.sort_by_key(|&(start, _, _)| start);

    // 合并重叠的匹配（简单处理：取第一个匹配）
    let mut filtered_matches = Vec::new();
    let mut i = 0;
    while i < matches.len() {
      let (start, end, text_type) = matches[i];
      filtered_matches.push((start, end, text_type));

      // 跳过所有重叠的匹配
      let mut j = i + 1;
      while j < matches.len() && matches[j].0 < end {
        j += 1;
      }
      i = j;
    }

    // 构建 Spans
    for (start, end, text_type) in filtered_matches {
      // 添加前面的普通文本
      if start > last_end {
        spans.push((Span::raw(&text[last_end..start]), (last_end, start)));
      }

      // 添加高亮文本
      let style = self.styles.get(&text_type).unwrap();
      spans.push((Span::styled(&text[start..end], *style), (start, end)));

      last_end = end;
    }

    // 添加剩余的普通文本
    if last_end < text.len() {
      spans.push((Span::raw(&text[last_end..]), (last_end, text.len())));
    }

    spans
  }
}

lazy_static! {
  static ref HIGHLIGHTER: Highlighter = Highlighter::new();
}

/// 将给定字符串，转换为有丰富颜色呈现的
pub fn rich<'a>(line: &mut text::Line<'a>, content: &'a str, search: &str) {
  // 为内容加上高亮样式
  let spans = HIGHLIGHTER.highlight(content);

  // 根据内容找到符合搜索内容的区间
  let matches = find_all_matches(content, search);

  // 将搜素匹配的内容加粗
  let spans = apply_matches_on_spans(spans, matches);

  // 添加所有内容到目标行里
  for span in spans {
    line.push_span(span);
  }
}
