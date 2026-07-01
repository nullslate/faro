use crate::tui::render::{format_bytes, label_style, muted_style, warning_style};
use crate::tui::state::RequestView;
use ratatui::text::{Line, Span};

pub(crate) fn is_image_request(request: &RequestView) -> bool {
    request
        .response
        .as_ref()
        .and_then(|response| response.mime_type.as_deref())
        .map(|mime| mime.starts_with("image/"))
        .unwrap_or(false)
}

pub(crate) fn image_preview_lines(request: &RequestView) -> Vec<Line<'static>> {
    let mime = request
        .response
        .as_ref()
        .and_then(|response| response.mime_type.as_deref())
        .unwrap_or("image/*");
    let size = request
        .response
        .as_ref()
        .and_then(|response| response.body_size)
        .map(format_bytes)
        .unwrap_or_else(|| "-".to_string());
    let mut lines = vec![
        Line::styled("image preview", label_style()),
        Line::from(vec![
            Span::styled("mime ", label_style()),
            Span::raw(mime.to_string()),
            Span::styled("  size ", label_style()),
            Span::raw(size),
        ]),
        Line::raw(""),
    ];

    let Some(body) = request.response_body.as_deref() else {
        lines.push(Line::styled(
            "No image body captured yet. Refresh while capture is active.",
            warning_style(),
        ));
        return lines;
    };
    let Some((data_mime, base64_data)) = parse_image_data_url(body) else {
        lines.push(Line::styled(
            "Image body is metadata-only; no inline preview payload was stored.",
            warning_style(),
        ));
        return lines;
    };

    match terminal_image_protocol() {
        Some(ImageProtocol::Kitty) => {
            lines.push(Line::raw(kitty_image_escape(base64_data)));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Rendered with Kitty graphics protocol.",
                muted_style(),
            ));
        }
        Some(ImageProtocol::ITerm) => {
            lines.push(Line::raw(iterm_image_escape(base64_data)));
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Rendered with iTerm inline image protocol.",
                muted_style(),
            ));
        }
        None => {
            lines.push(Line::styled(
                "Inline preview requires Kitty or iTerm image protocol support.",
                warning_style(),
            ));
            lines.push(Line::from(vec![
                Span::styled("captured ", label_style()),
                Span::raw(format!(
                    "{} base64 chars for {data_mime}",
                    base64_data.len()
                )),
            ]));
        }
    }
    lines
}

pub(crate) fn parse_image_data_url(body: &str) -> Option<(&str, &str)> {
    let rest = body.strip_prefix("data:")?;
    let (metadata, data) = rest.split_once(',')?;
    let (mime, encoding) = metadata.split_once(';')?;
    (mime.starts_with("image/") && encoding == "base64").then_some((mime, data))
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ImageProtocol {
    Kitty,
    ITerm,
}

pub(crate) fn terminal_image_protocol() -> Option<ImageProtocol> {
    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    if term.contains("kitty") {
        return Some(ImageProtocol::Kitty);
    }
    let term_program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_lowercase();
    if term_program.contains("iterm") {
        return Some(ImageProtocol::ITerm);
    }
    None
}

fn kitty_image_escape(base64_data: &str) -> String {
    format!("\x1b_Ga=T,f=100;{base64_data}\x1b\\")
}

fn iterm_image_escape(base64_data: &str) -> String {
    format!("\x1b]1337;File=inline=1;width=auto;height=12;preserveAspectRatio=1:{base64_data}\x07")
}

pub(crate) fn kitty_favicon_escape(base64_data: &str) -> String {
    format!("\x1b_Ga=T,f=100,c=2,r=1;{base64_data}\x1b\\")
}

pub(crate) fn iterm_favicon_escape(base64_data: &str) -> String {
    format!("\x1b]1337;File=inline=1;width=2;height=1;preserveAspectRatio=1:{base64_data}\x07")
}
