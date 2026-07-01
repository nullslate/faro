use crate::tui::render::{GB_AQUA, GB_BLUE, GB_FG};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

pub(super) fn highlight_html_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut index = 0;

    while index < line.len() {
        let rest = &line[index..];
        if rest.starts_with("<!--") {
            let end = rest
                .find("-->")
                .map(|offset| index + offset + 3)
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
        if ch == '<' {
            spans.push(Span::styled(
                "<".to_string(),
                super::json::json_punctuation_style(),
            ));
            index += ch.len_utf8();
            if line[index..].starts_with('/') {
                spans.push(Span::styled(
                    "/".to_string(),
                    super::json::json_punctuation_style(),
                ));
                index += 1;
            }
            let name_start = index;
            while index < line.len() {
                let Some(next) = line[index..].chars().next() else {
                    break;
                };
                if !(next.is_ascii_alphanumeric() || matches!(next, '-' | ':' | '_' | '!')) {
                    break;
                }
                index += next.len_utf8();
            }
            if index > name_start {
                spans.push(Span::styled(
                    line[name_start..index].to_string(),
                    Style::default().fg(GB_BLUE).add_modifier(Modifier::BOLD),
                ));
            }
            continue;
        }
        if ch == '>' || ch == '/' || ch == '=' {
            spans.push(Span::styled(
                ch.to_string(),
                super::json::json_punctuation_style(),
            ));
            index += ch.len_utf8();
            continue;
        }
        if ch == '"' || ch == '\'' {
            let end = super::javascript::quoted_string_end(line, index, ch);
            spans.push(Span::styled(
                line[index..end].to_string(),
                super::json::json_string_style(),
            ));
            index = end;
            continue;
        }
        if super::javascript::is_js_ident_start(ch) {
            let end = consume_html_identifier(line, index);
            let token = &line[index..end];
            let style = if line[end..].trim_start().starts_with('=') {
                Style::default().fg(GB_AQUA)
            } else {
                Style::default().fg(GB_FG)
            };
            spans.push(Span::styled(token.to_string(), style));
            index = end;
            continue;
        }
        spans.push(Span::raw(ch.to_string()));
        index += ch.len_utf8();
    }

    Line::from(spans)
}

fn consume_html_identifier(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | ':' | '_' | '.')) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}
