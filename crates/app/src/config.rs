use anyhow::Context;
use faro_core::config_dir;
use ratatui::style::Color;
use std::fs;
use std::path::{Path, PathBuf};

mod defaults;
mod raw;

use defaults::{
    DEFAULT_CONFIG, LEGACY_GRUVBOX_DEFAULT_CONFIG, LEGACY_NEON_DEFAULT_CONFIG,
    LEGACY_NEUTRAL_DEFAULT_CONFIG, LEGACY_TERMINAL_DEFAULT_CONFIG,
};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub db_path: PathBuf,
    pub launch_on_start: bool,
    pub ui: UiConfig,
    pub redaction: RedactionConfig,
    pub theme: Theme,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("faro.db"),
            launch_on_start: false,
            ui: UiConfig::default(),
            redaction: RedactionConfig::default(),
            theme: Theme::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UiConfig {
    pub bottom_fade_rows: usize,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            bottom_fade_rows: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RedactionConfig {
    pub header_names: Vec<String>,
    pub json_key_patterns: Vec<String>,
    pub text_patterns: Vec<String>,
    pub mcp_body_limit_bytes: usize,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            header_names: [
                "authorization",
                "proxy-authorization",
                "cookie",
                "set-cookie",
                "x-api-key",
                "x-auth-token",
                "x-csrf-token",
                "x-xsrf-token",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            json_key_patterns: [
                "authorization",
                "auth",
                "cookie",
                "email",
                "jwt",
                "key",
                "password",
                "secret",
                "session",
                "token",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            text_patterns: ["bearer ", "token=", "password=", "secret="]
                .into_iter()
                .map(str::to_string)
                .collect(),
            mcp_body_limit_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub panel_title: Color,
    pub panel_border: Color,
    pub active_border: Color,
    pub tree_edge: Color,
    pub ok: Color,
    pub redirect: Color,
    pub client_error: Color,
    pub server_error: Color,
    pub method_get: Color,
    pub method_post: Color,
    pub method_write: Color,
    pub method_delete: Color,
    pub resource_xhr: Color,
    pub resource_image: Color,
    pub resource_script: Color,
    pub resource_style: Color,
    pub resource_sse: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            text: Color::Rgb(212, 190, 152),
            muted: Color::Rgb(146, 131, 116),
            accent: Color::Rgb(137, 180, 130),
            panel_title: Color::Rgb(216, 166, 87),
            panel_border: Color::Rgb(60, 56, 54),
            active_border: Color::Rgb(137, 180, 130),
            tree_edge: Color::Rgb(146, 131, 116),
            ok: Color::Rgb(169, 182, 101),
            redirect: Color::Rgb(125, 174, 163),
            client_error: Color::Rgb(216, 166, 87),
            server_error: Color::Rgb(234, 105, 98),
            method_get: Color::Rgb(125, 174, 163),
            method_post: Color::Rgb(169, 182, 101),
            method_write: Color::Rgb(216, 166, 87),
            method_delete: Color::Rgb(234, 105, 98),
            resource_xhr: Color::Rgb(211, 134, 155),
            resource_image: Color::Rgb(125, 174, 163),
            resource_script: Color::Rgb(216, 166, 87),
            resource_style: Color::Rgb(137, 180, 130),
            resource_sse: Color::Rgb(169, 182, 101),
        }
    }
}

pub fn load_or_create() -> anyhow::Result<AppConfig> {
    let Some(path) = config_path() else {
        return Ok(AppConfig::default());
    };
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create config directory {}", parent.display()))?;
        }
        fs::write(&path, DEFAULT_CONFIG)
            .with_context(|| format!("write default config {}", path.display()))?;
    }
    load_from_path(&path)
}

fn load_from_path(path: &Path) -> anyhow::Result<AppConfig> {
    let mut raw =
        fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
    if raw.trim() == LEGACY_NEON_DEFAULT_CONFIG.trim()
        || raw.trim() == LEGACY_GRUVBOX_DEFAULT_CONFIG.trim()
        || raw.trim() == LEGACY_TERMINAL_DEFAULT_CONFIG.trim()
        || raw.trim() == LEGACY_NEUTRAL_DEFAULT_CONFIG.trim()
    {
        fs::write(path, DEFAULT_CONFIG)
            .with_context(|| format!("migrate default config {}", path.display()))?;
        raw = DEFAULT_CONFIG.to_string();
    }
    raw::parse_config(&raw, path.parent())
        .with_context(|| format!("parse config {}", path.display()))
}

fn config_path() -> Option<PathBuf> {
    let primary = config_path_for("faro");
    let legacy = config_path_for("devbench");
    match (primary, legacy) {
        (Some(primary), Some(legacy)) if !primary.exists() && legacy.exists() => Some(legacy),
        (Some(primary), _) => Some(primary),
        (None, legacy) => legacy,
    }
}

fn config_path_for(app_dir: &str) -> Option<PathBuf> {
    config_dir(app_dir).map(|path| path.join("config.toml"))
}
