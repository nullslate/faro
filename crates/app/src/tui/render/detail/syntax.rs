use super::*;

mod css;
mod html;
mod javascript;
mod json;

use css::highlight_css_line;
use html::highlight_html_line;
pub(crate) use javascript::highlight_javascript_line;
use json::highlight_json_line;

enum BodySyntax {
    Json,
    Html,
    Css,
    JavaScript,
    Xml,
    Text,
}

pub(crate) fn syntax_body_lines(body: String) -> Vec<Line<'static>> {
    syntax_body_lines_with(body, BodySyntax::Json)
}

pub(crate) fn syntax_body_lines_for_request(
    request: &RequestView,
    body: String,
) -> Vec<Line<'static>> {
    syntax_body_lines_with(body, body_syntax_for_request(request))
}

fn syntax_body_lines_with(body: String, syntax: BodySyntax) -> Vec<Line<'static>> {
    let body = strip_terminal_controls(&body);
    match syntax {
        BodySyntax::Json if serde_json::from_str::<serde_json::Value>(&body).is_ok() => {
            body.lines().map(highlight_json_line).collect()
        }
        BodySyntax::Html => body.lines().map(highlight_html_line).collect(),
        BodySyntax::Css => body.lines().map(highlight_css_line).collect(),
        BodySyntax::JavaScript => body.lines().map(highlight_javascript_line).collect(),
        BodySyntax::Xml => body.lines().map(highlight_html_line).collect(),
        BodySyntax::Json | BodySyntax::Text => body
            .lines()
            .map(|line| Line::styled(line.to_string(), Style::default().fg(GB_FG)))
            .collect(),
    }
}

fn strip_terminal_controls(value: &str) -> String {
    let mut cleaned = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            strip_escape_sequence(&mut chars);
            continue;
        }
        if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t') {
            continue;
        }
        cleaned.push(ch);
    }
    cleaned
}

fn strip_escape_sequence<I>(chars: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = char>,
{
    let Some(next) = chars.peek().copied() else {
        return;
    };
    if next == '[' {
        let _ = chars.next();
        for ch in chars.by_ref() {
            if ('@'..='~').contains(&ch) {
                break;
            }
        }
        return;
    }
    if next == ']' {
        let _ = chars.next();
        let mut previous_escape = false;
        for ch in chars.by_ref() {
            if ch == '\u{7}' || (previous_escape && ch == '\\') {
                break;
            }
            previous_escape = ch == '\u{1b}';
        }
        return;
    }
    let _ = chars.next();
}

fn body_syntax_for_request(request: &RequestView) -> BodySyntax {
    let mime = request
        .response
        .as_ref()
        .and_then(|response| response.mime_type.as_deref())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let resource = request
        .request
        .resource_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path = path_for_url(&request.request.url).to_ascii_lowercase();

    if mime.contains("json") || path.ends_with(".json") {
        BodySyntax::Json
    } else if mime.contains("html") || resource == "document" || path.ends_with(".html") {
        BodySyntax::Html
    } else if mime.contains("css") || resource == "stylesheet" || path.ends_with(".css") {
        BodySyntax::Css
    } else if mime.contains("javascript")
        || mime.contains("ecmascript")
        || resource == "script"
        || path.ends_with(".js")
        || path.ends_with(".mjs")
    {
        BodySyntax::JavaScript
    } else if mime.contains("xml") || path.ends_with(".xml") || path.ends_with(".svg") {
        BodySyntax::Xml
    } else {
        BodySyntax::Text
    }
}
