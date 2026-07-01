use crate::tui::render::{GB_AQUA, GB_BLUE, GB_FG, muted_style};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

pub(crate) fn highlight_javascript_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut index = 0;

    while index < line.len() {
        let rest = &line[index..];
        let Some(ch) = rest.chars().next() else {
            break;
        };

        if rest.starts_with("//") {
            spans.push(Span::styled(rest.to_string(), js_comment_style()));
            break;
        }

        if ch == '"' || ch == '\'' || ch == '`' {
            let end = quoted_string_end(line, index, ch);
            spans.push(Span::styled(
                line[index..end].to_string(),
                super::json::json_string_style(),
            ));
            index = end;
            continue;
        }

        if ch.is_ascii_digit() {
            let end = consume_js_number(line, index);
            spans.push(Span::styled(
                line[index..end].to_string(),
                super::json::json_number_style(),
            ));
            index = end;
            continue;
        }

        if is_js_ident_start(ch) {
            let end = consume_js_identifier(line, index);
            let token = &line[index..end];
            spans.push(Span::styled(token.to_string(), js_identifier_style(token)));
            index = end;
            continue;
        }

        if matches!(
            ch,
            '{' | '}'
                | '['
                | ']'
                | '('
                | ')'
                | ':'
                | ','
                | '.'
                | ';'
                | '='
                | '+'
                | '-'
                | '*'
                | '/'
                | '!'
                | '?'
                | '<'
                | '>'
                | '|'
                | '&'
        ) {
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

pub(super) fn quoted_string_end(line: &str, start: usize, quote: char) -> usize {
    let mut escaped = false;
    for (offset, ch) in line[start + quote.len_utf8()..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return start + quote.len_utf8() + offset + ch.len_utf8();
        }
    }
    line.len()
}

fn consume_js_number(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !(ch.is_ascii_digit() || matches!(ch, '.' | '_' | 'x' | 'X' | 'a'..='f' | 'A'..='F')) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn consume_js_identifier(line: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in line[start..].char_indices() {
        if !is_js_ident_continue(ch) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

pub(super) fn is_js_ident_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_js_ident_continue(ch: char) -> bool {
    is_js_ident_start(ch) || ch.is_ascii_digit()
}

fn js_identifier_style(token: &str) -> Style {
    if js_keyword(token) {
        Style::default().fg(GB_BLUE).add_modifier(Modifier::BOLD)
    } else if js_global(token) {
        Style::default().fg(GB_AQUA)
    } else {
        Style::default().fg(GB_FG)
    }
}

fn js_keyword(token: &str) -> bool {
    matches!(
        token,
        "async"
            | "await"
            | "break"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "else"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "let"
            | "new"
            | "null"
            | "return"
            | "throw"
            | "true"
            | "try"
            | "undefined"
            | "var"
            | "while"
    )
}

fn js_global(token: &str) -> bool {
    matches!(
        token,
        "console"
            | "document"
            | "fetch"
            | "history"
            | "localStorage"
            | "location"
            | "navigator"
            | "sessionStorage"
            | "window"
    )
}

pub(super) fn js_comment_style() -> Style {
    muted_style()
}
