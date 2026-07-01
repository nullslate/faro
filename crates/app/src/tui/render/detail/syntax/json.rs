use crate::tui::render::{GB_BLUE, GB_GREEN, GB_PURPLE, GB_YELLOW, muted_style};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

pub(super) fn highlight_json_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut index = 0;

    while index < line.len() {
        let rest = &line[index..];
        let Some(ch) = rest.chars().next() else {
            break;
        };

        if ch == '"' {
            let end = json_string_end(line, index);
            let token = &line[index..end];
            let is_key = line[end..].trim_start().starts_with(':');
            spans.push(Span::styled(
                token.to_string(),
                if is_key {
                    json_key_style()
                } else {
                    json_string_style()
                },
            ));
            index = end;
            continue;
        }

        if ch == '-' || ch.is_ascii_digit() {
            let end = consume_json_number(line, index);
            spans.push(Span::styled(
                line[index..end].to_string(),
                json_number_style(),
            ));
            index = end;
            continue;
        }

        if let Some((literal, style)) = json_literal(rest) {
            spans.push(Span::styled(literal.to_string(), style));
            index += literal.len();
            continue;
        }

        if matches!(ch, '{' | '}' | '[' | ']' | ':' | ',') {
            spans.push(Span::styled(ch.to_string(), json_punctuation_style()));
        } else {
            spans.push(Span::raw(ch.to_string()));
        }
        index += ch.len_utf8();
    }

    Line::from(spans)
}

fn json_string_end(line: &str, start: usize) -> usize {
    let mut escaped = false;
    for (offset, ch) in line[start + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return start + 1 + offset + ch.len_utf8();
        }
    }
    line.len()
}

fn consume_json_number(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E')) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn json_literal(input: &str) -> Option<(&'static str, Style)> {
    if input.starts_with("true") {
        Some(("true", json_literal_style()))
    } else if input.starts_with("false") {
        Some(("false", json_literal_style()))
    } else if input.starts_with("null") {
        Some(("null", json_null_style()))
    } else {
        None
    }
}

fn json_key_style() -> Style {
    Style::default().fg(GB_BLUE).add_modifier(Modifier::BOLD)
}

pub(super) fn json_string_style() -> Style {
    Style::default().fg(GB_GREEN)
}

pub(super) fn json_number_style() -> Style {
    Style::default().fg(GB_PURPLE)
}

fn json_literal_style() -> Style {
    Style::default().fg(GB_YELLOW)
}

fn json_null_style() -> Style {
    muted_style()
}

pub(super) fn json_punctuation_style() -> Style {
    muted_style()
}
