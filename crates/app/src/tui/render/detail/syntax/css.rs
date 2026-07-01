use crate::tui::render::{GB_AQUA, GB_BLUE, GB_FG, GB_PURPLE};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

pub(super) fn highlight_css_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut index = 0;

    while index < line.len() {
        let rest = &line[index..];
        if rest.starts_with("/*") {
            let end = rest
                .find("*/")
                .map(|offset| index + offset + 2)
                .unwrap_or(line.len());
            spans.push(Span::styled(
                line[index..end].to_string(),
                super::javascript::js_comment_style(),
            ));
            index = end;
            continue;
        }
        let Some(ch) = rest.chars().next() else {
            break;
        };
        if ch == '"' || ch == '\'' {
            let end = super::javascript::quoted_string_end(line, index, ch);
            spans.push(Span::styled(
                line[index..end].to_string(),
                super::json::json_string_style(),
            ));
            index = end;
            continue;
        }
        if ch == '#'
            && rest
                .chars()
                .nth(1)
                .is_some_and(|next| next.is_ascii_hexdigit())
        {
            let end = consume_css_hash(line, index);
            spans.push(Span::styled(
                line[index..end].to_string(),
                Style::default().fg(GB_PURPLE),
            ));
            index = end;
            continue;
        }
        if ch == '@' {
            let end = consume_css_identifier(line, index + ch.len_utf8());
            spans.push(Span::styled(
                line[index..end].to_string(),
                Style::default().fg(GB_BLUE).add_modifier(Modifier::BOLD),
            ));
            index = end;
            continue;
        }
        if ch.is_ascii_digit() {
            let end = consume_css_number(line, index);
            spans.push(Span::styled(
                line[index..end].to_string(),
                super::json::json_number_style(),
            ));
            index = end;
            continue;
        }
        if super::javascript::is_js_ident_start(ch) || ch == '-' {
            let end = consume_css_identifier(line, index);
            let token = &line[index..end];
            let style = if line[end..].trim_start().starts_with(':') {
                Style::default().fg(GB_AQUA)
            } else {
                Style::default().fg(GB_FG)
            };
            spans.push(Span::styled(token.to_string(), style));
            index = end;
            continue;
        }
        if matches!(ch, '{' | '}' | ':' | ';' | ',' | '(' | ')' | '[' | ']') {
            spans.push(Span::styled(
                ch.to_string(),
                super::json::json_punctuation_style(),
            ));
        } else {
            spans.push(Span::raw(ch.to_string()));
        }
        index += ch.len_utf8();
    }

    Line::from(spans)
}

fn consume_css_identifier(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn consume_css_hash(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch == '#' || ch.is_ascii_hexdigit()) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn consume_css_number(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '%' | '-' | '+')) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}
