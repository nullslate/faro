use super::{AppConfig, RedactionConfig, RetentionConfig, Theme, UiConfig};
use anyhow::{Context, bail};
use ratatui::style::Color;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    #[serde(default)]
    app: RawAppConfig,
    #[serde(default)]
    retention: RawRetentionConfig,
    #[serde(default)]
    ui: RawUiConfig,
    #[serde(default)]
    redaction: RawRedactionConfig,
    #[serde(default)]
    theme: RawTheme,
}

#[derive(Debug, Default, Deserialize)]
struct RawAppConfig {
    db_path: Option<PathBuf>,
    launch_on_start: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RawRetentionConfig {
    max_requests_per_session: Option<usize>,
    max_repeated_requests_per_url: Option<usize>,
    max_console_logs_per_session: Option<usize>,
    max_websocket_frames_per_session: Option<usize>,
    prune_interval_requests: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct RawUiConfig {
    bottom_fade_rows: Option<usize>,
    max_body_tree_items: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct RawRedactionConfig {
    header_names: Option<Vec<String>>,
    json_key_patterns: Option<Vec<String>>,
    text_patterns: Option<Vec<String>>,
    mcp_body_limit_bytes: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct RawTheme {
    text: Option<String>,
    muted: Option<String>,
    accent: Option<String>,
    panel_title: Option<String>,
    panel_border: Option<String>,
    active_border: Option<String>,
    tree_edge: Option<String>,
    ok: Option<String>,
    redirect: Option<String>,
    client_error: Option<String>,
    server_error: Option<String>,
    method_get: Option<String>,
    method_post: Option<String>,
    method_write: Option<String>,
    method_delete: Option<String>,
    resource_xhr: Option<String>,
    resource_image: Option<String>,
    resource_script: Option<String>,
    resource_style: Option<String>,
    resource_sse: Option<String>,
}

pub(super) fn parse_config(raw: &str, config_dir: Option<&Path>) -> anyhow::Result<AppConfig> {
    let raw: RawConfig = toml::from_str(raw)?;
    raw.resolve(config_dir)
}

impl RawConfig {
    fn resolve(self, config_dir: Option<&Path>) -> anyhow::Result<AppConfig> {
        let defaults = AppConfig::default();
        let db_path = self.app.db_path.unwrap_or(defaults.db_path);
        Ok(AppConfig {
            db_path: resolve_db_path(db_path, config_dir),
            launch_on_start: self.app.launch_on_start.unwrap_or(defaults.launch_on_start),
            retention: self.retention.resolve(defaults.retention),
            ui: UiConfig {
                bottom_fade_rows: self
                    .ui
                    .bottom_fade_rows
                    .unwrap_or(defaults.ui.bottom_fade_rows)
                    .clamp(0, 8),
                max_body_tree_items: self
                    .ui
                    .max_body_tree_items
                    .unwrap_or(defaults.ui.max_body_tree_items)
                    .clamp(100, 100_000),
            },
            redaction: self.redaction.resolve(defaults.redaction),
            theme: self.theme.resolve(defaults.theme)?,
        })
    }
}

impl RawRetentionConfig {
    fn resolve(self, defaults: RetentionConfig) -> RetentionConfig {
        let max_requests_per_session = self
            .max_requests_per_session
            .unwrap_or(defaults.max_requests_per_session)
            .clamp(100, 100_000);
        let max_repeated_requests_per_url = self
            .max_repeated_requests_per_url
            .unwrap_or(defaults.max_repeated_requests_per_url)
            .min(max_requests_per_session);
        RetentionConfig {
            max_requests_per_session,
            max_repeated_requests_per_url,
            max_console_logs_per_session: self
                .max_console_logs_per_session
                .unwrap_or(defaults.max_console_logs_per_session)
                .clamp(100, 100_000),
            max_websocket_frames_per_session: self
                .max_websocket_frames_per_session
                .unwrap_or(defaults.max_websocket_frames_per_session)
                .clamp(100, 500_000),
            prune_interval_requests: self
                .prune_interval_requests
                .unwrap_or(defaults.prune_interval_requests)
                .clamp(25, max_requests_per_session),
        }
    }
}

impl RawRedactionConfig {
    fn resolve(self, defaults: RedactionConfig) -> RedactionConfig {
        RedactionConfig {
            header_names: normalized_patterns(self.header_names, defaults.header_names),
            json_key_patterns: normalized_patterns(
                self.json_key_patterns,
                defaults.json_key_patterns,
            ),
            text_patterns: normalized_patterns(self.text_patterns, defaults.text_patterns),
            mcp_body_limit_bytes: self
                .mcp_body_limit_bytes
                .unwrap_or(defaults.mcp_body_limit_bytes)
                .clamp(1024, 16 * 1024 * 1024),
        }
    }
}

fn normalized_patterns(value: Option<Vec<String>>, defaults: Vec<String>) -> Vec<String> {
    value
        .unwrap_or(defaults)
        .into_iter()
        .map(|entry| entry.trim().to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
        .collect()
}

fn resolve_db_path(db_path: PathBuf, config_dir: Option<&Path>) -> PathBuf {
    if db_path.is_absolute() {
        return db_path;
    }
    config_dir.map(|dir| dir.join(&db_path)).unwrap_or(db_path)
}

impl RawTheme {
    fn resolve(self, defaults: Theme) -> anyhow::Result<Theme> {
        Ok(Theme {
            text: parse_color_or(self.text, defaults.text, "theme.text")?,
            muted: parse_color_or(self.muted, defaults.muted, "theme.muted")?,
            accent: parse_color_or(self.accent, defaults.accent, "theme.accent")?,
            panel_title: parse_color_or(
                self.panel_title,
                defaults.panel_title,
                "theme.panel_title",
            )?,
            panel_border: parse_color_or(
                self.panel_border,
                defaults.panel_border,
                "theme.panel_border",
            )?,
            active_border: parse_color_or(
                self.active_border,
                defaults.active_border,
                "theme.active_border",
            )?,
            tree_edge: parse_color_or(self.tree_edge, defaults.tree_edge, "theme.tree_edge")?,
            ok: parse_color_or(self.ok, defaults.ok, "theme.ok")?,
            redirect: parse_color_or(self.redirect, defaults.redirect, "theme.redirect")?,
            client_error: parse_color_or(
                self.client_error,
                defaults.client_error,
                "theme.client_error",
            )?,
            server_error: parse_color_or(
                self.server_error,
                defaults.server_error,
                "theme.server_error",
            )?,
            method_get: parse_color_or(self.method_get, defaults.method_get, "theme.method_get")?,
            method_post: parse_color_or(
                self.method_post,
                defaults.method_post,
                "theme.method_post",
            )?,
            method_write: parse_color_or(
                self.method_write,
                defaults.method_write,
                "theme.method_write",
            )?,
            method_delete: parse_color_or(
                self.method_delete,
                defaults.method_delete,
                "theme.method_delete",
            )?,
            resource_xhr: parse_color_or(
                self.resource_xhr,
                defaults.resource_xhr,
                "theme.resource_xhr",
            )?,
            resource_image: parse_color_or(
                self.resource_image,
                defaults.resource_image,
                "theme.resource_image",
            )?,
            resource_script: parse_color_or(
                self.resource_script,
                defaults.resource_script,
                "theme.resource_script",
            )?,
            resource_style: parse_color_or(
                self.resource_style,
                defaults.resource_style,
                "theme.resource_style",
            )?,
            resource_sse: parse_color_or(
                self.resource_sse,
                defaults.resource_sse,
                "theme.resource_sse",
            )?,
        })
    }
}

fn parse_color_or(value: Option<String>, fallback: Color, field: &str) -> anyhow::Result<Color> {
    let Some(value) = value else {
        return Ok(fallback);
    };
    parse_color(&value).with_context(|| format!("parse {field} color `{value}`"))
}

fn parse_color(value: &str) -> anyhow::Result<Color> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() != 6 {
            bail!("hex colors must be #rrggbb");
        }
        let red = u8::from_str_radix(&hex[0..2], 16).context("parse red channel")?;
        let green = u8::from_str_radix(&hex[2..4], 16).context("parse green channel")?;
        let blue = u8::from_str_radix(&hex[4..6], 16).context("parse blue channel")?;
        return Ok(Color::Rgb(red, green, blue));
    }
    match value.to_lowercase().as_str() {
        "default" | "reset" => Ok(Color::Reset),
        "black" => Ok(Color::Black),
        "red" => Ok(Color::Red),
        "green" => Ok(Color::Green),
        "yellow" => Ok(Color::Yellow),
        "blue" => Ok(Color::Blue),
        "magenta" => Ok(Color::Magenta),
        "cyan" => Ok(Color::Cyan),
        "gray" | "grey" => Ok(Color::Gray),
        "dark_gray" | "dark_grey" => Ok(Color::DarkGray),
        "white" => Ok(Color::White),
        "light_red" => Ok(Color::LightRed),
        "light_green" => Ok(Color::LightGreen),
        "light_yellow" => Ok(Color::LightYellow),
        "light_blue" => Ok(Color::LightBlue),
        "light_magenta" => Ok(Color::LightMagenta),
        "light_cyan" => Ok(Color::LightCyan),
        _ => bail!("unsupported color"),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_config;

    #[test]
    fn parses_ui_body_tree_limit() -> anyhow::Result<()> {
        let config = parse_config(
            r#"
            [ui]
            bottom_fade_rows = 12
            max_body_tree_items = 42
            "#,
            None,
        )?;

        assert_eq!(config.ui.bottom_fade_rows, 8);
        assert_eq!(config.ui.max_body_tree_items, 100);
        Ok(())
    }

    #[test]
    fn defaults_ui_body_tree_limit_when_missing() -> anyhow::Result<()> {
        let config = parse_config("", None)?;

        assert_eq!(config.ui.max_body_tree_items, 2_000);
        Ok(())
    }
}
