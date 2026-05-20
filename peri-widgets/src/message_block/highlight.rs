use ratatui::style::Color;
use ratatui::text::Span;

use crate::theme::Theme;

pub fn highlight_diff_line(line: &str, theme: &dyn Theme) -> Vec<Span<'static>> {
    if line.starts_with("@@ ") {
        vec![Span::styled(
            line.to_string(),
            ratatui::style::Style::default().fg(theme.diff_hunk()),
        )]
    } else if line.starts_with('+') {
        vec![Span::styled(
            line.to_string(),
            ratatui::style::Style::default().fg(theme.diff_add()),
        )]
    } else if line.starts_with('-') {
        vec![Span::styled(
            line.to_string(),
            ratatui::style::Style::default().fg(theme.diff_remove()),
        )]
    } else {
        vec![Span::raw(line.to_string())]
    }
}

pub fn is_diff_content(text: &str) -> bool {
    for line in text.lines().take(5) {
        if line.starts_with("@@ ") || line.starts_with("+++") {
            return true;
        }
    }
    false
}

pub fn highlight_code_line(line: &str, _lang: &str) -> Vec<Span<'static>> {
    let keyword_re = regex::Regex::new(
        r"\b(fn|let|mut|pub|use|struct|enum|impl|if|else|match|return|for|while|async|await)\b",
    )
    .unwrap();
    let string_re = regex::Regex::new(r#""[^"]*""#).unwrap();
    let comment_re = regex::Regex::new(r"//.*$").unwrap();

    // Simple approach: check if line has comment first
    if let Some(mat) = comment_re.find(line) {
        let before = &line[..mat.start()];
        let comment = mat.as_str();
        let mut spans = highlight_spans(before, &keyword_re, &string_re);
        spans.push(Span::styled(
            comment.to_string(),
            ratatui::style::Style::default().fg(Color::DarkGray),
        ));
        return spans;
    }

    highlight_spans(line, &keyword_re, &string_re)
}

fn highlight_spans(
    text: &str,
    keyword_re: &regex::Regex,
    string_re: &regex::Regex,
) -> Vec<Span<'static>> {
    if text.is_empty() {
        return vec![Span::raw(String::new())];
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut last_end = 0;

    // Collect all matches sorted by position
    let mut matches: Vec<(usize, usize, &str, Color)> = Vec::new();

    for cap in keyword_re.captures_iter(text) {
        let m = cap.get(0).unwrap();
        matches.push((m.start(), m.end(), m.as_str(), Color::Yellow));
    }
    for cap in string_re.captures_iter(text) {
        let m = cap.get(0).unwrap();
        matches.push((m.start(), m.end(), m.as_str(), Color::Green));
    }

    matches.sort_by_key(|m| m.0);

    for (start, end, matched, color) in &matches {
        if *start > last_end {
            spans.push(Span::raw(text[last_end..*start].to_string()));
        }
        if *start >= last_end {
            spans.push(Span::styled(
                matched.to_string(),
                ratatui::style::Style::default().fg(*color),
            ));
            last_end = *end;
        }
    }

    if last_end < text.len() {
        spans.push(Span::raw(text[last_end..].to_string()));
    }

    if spans.is_empty() {
        spans.push(Span::raw(text.to_string()));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("highlight_test.rs");
}
