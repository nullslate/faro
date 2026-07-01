use super::*;

pub(super) const GB_FG: Color = Color::Rgb(235, 219, 178);
pub(super) const GB_MUTED: Color = Color::Rgb(146, 131, 116);
pub(super) const GB_BG2: Color = Color::Rgb(58, 58, 54);
pub(super) const GB_RED: Color = Color::Rgb(234, 105, 98);
pub(super) const GB_GREEN: Color = Color::Rgb(169, 182, 101);
pub(super) const GB_YELLOW: Color = Color::Rgb(216, 166, 87);
pub(super) const GB_BLUE: Color = Color::Rgb(125, 174, 163);
pub(super) const GB_PURPLE: Color = Color::Rgb(211, 134, 155);
pub(super) const GB_AQUA: Color = Color::Rgb(137, 180, 130);
pub(super) const GB_ORANGE: Color = Color::Rgb(231, 138, 78);
pub(super) const GB_FAINT: Color = Color::Rgb(102, 92, 84);

pub(super) fn latency_bar(ms: i64, theme: &Theme) -> Vec<Span<'static>> {
    const WIDTH: usize = 3;
    let (filled, color) = match ms {
        ..=50 => (1, theme.ok),
        51..=100 => (1, theme.ok),
        101..=200 => (1, theme.redirect),
        201..=400 => (2, theme.client_error),
        401..=800 => (2, theme.client_error),
        801..=1_500 => (3, GB_ORANGE),
        _ => (3, theme.server_error),
    };
    vec![
        Span::styled(
            "━".repeat(filled),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled("─".repeat(WIDTH - filled), Style::default().fg(GB_FAINT)),
    ]
}

pub(super) fn label_style() -> Style {
    Style::default().fg(GB_YELLOW)
}

pub(super) fn key_style() -> Style {
    Style::default().fg(GB_GREEN).add_modifier(Modifier::BOLD)
}

pub(super) fn modal_section_style() -> Style {
    Style::default().fg(GB_YELLOW).add_modifier(Modifier::BOLD)
}

pub(super) fn modal_selection_style(selected: bool) -> Style {
    if selected {
        Style::default().fg(GB_GREEN).add_modifier(Modifier::BOLD)
    } else {
        muted_style()
    }
}

pub(super) fn muted_style() -> Style {
    Style::default().fg(GB_MUTED)
}

fn active_border(active: bool) -> Style {
    if active {
        Style::default().fg(GB_GREEN)
    } else {
        Style::default().fg(GB_BG2)
    }
}

pub(super) fn panel_block(title: impl Into<String>, active: bool) -> Block<'static> {
    let title = title.into();
    Block::default()
        .title(Line::from(Span::styled(
            format!(" {title} "),
            panel_title_style(active),
        )))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(active_border(active))
}

pub(super) fn themed_panel_block(
    title: impl Into<String>,
    hotkey: Option<char>,
    active: bool,
    theme: &Theme,
) -> Block<'static> {
    let title = title.into();
    let title_color = if active {
        theme.accent
    } else {
        theme.panel_title
    };
    let border_color = if active {
        theme.active_border
    } else {
        theme.panel_border
    };
    Block::default()
        .title(themed_title_line(&title, hotkey, title_color, theme))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
}

fn themed_title_line(
    title: &str,
    hotkey: Option<char>,
    title_color: Color,
    theme: &Theme,
) -> Line<'static> {
    let base_style = Style::default()
        .fg(title_color)
        .add_modifier(Modifier::BOLD);
    let Some(hotkey) = hotkey.map(|value| value.to_ascii_lowercase()) else {
        return Line::from(Span::styled(title.to_string(), base_style));
    };
    let Some((start, character)) = title
        .char_indices()
        .find(|(_, character)| character.to_ascii_lowercase() == hotkey)
    else {
        return Line::from(Span::styled(title.to_string(), base_style));
    };
    let end = start + character.len_utf8();
    let key_color = if title_color == theme.accent {
        theme.text
    } else {
        theme.accent
    };
    let key_style = Style::default().fg(key_color).add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled(title[..start].to_string(), base_style),
        Span::styled(title[start..end].to_string(), key_style),
        Span::styled(title[end..].to_string(), base_style),
    ])
}

pub(super) fn panel_title_style(active: bool) -> Style {
    if active {
        Style::default().fg(GB_GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(GB_YELLOW).add_modifier(Modifier::BOLD)
    }
}

pub(super) fn format_bytes(bytes: i64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}mb", bytes as f64 / 1024.0 / 1024.0)
    } else if bytes >= 1024 {
        format!("{:.1}kb", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}b")
    }
}

pub(super) fn compact_value(value: &str, max_chars: usize) -> String {
    let normalized = value.replace(['\n', '\r', '\t'], " ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }

    if max_chars <= 3 {
        return normalized.chars().take(max_chars).collect();
    }

    let mut compact = normalized
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    compact.push_str("...");
    compact
}

pub(super) fn warning_style() -> Style {
    Style::default().fg(GB_YELLOW).add_modifier(Modifier::BOLD)
}
