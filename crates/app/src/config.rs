use anyhow::{Context, bail};
use faro_core::config_dir;
use ratatui::style::Color;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG: &str = r##"# Faro configuration.

[app]
# Default database path used when --db is omitted. Relative paths resolve
# from this config file's directory.
db_path = "faro.db"
# Start capture immediately for URL launches instead of waiting for "o".
launch_on_start = false

[ui]
# Number of rows affected by the dark bottom overlay in the request tree.
bottom_fade_rows = 3

[theme]
text = "#d4be98"
muted = "#928374"
accent = "#89b482"
panel_title = "#d8a657"
panel_border = "#3c3836"
active_border = "#89b482"
tree_edge = "#928374"
ok = "#a9b665"
redirect = "#7daea3"
client_error = "#d8a657"
server_error = "#ea6962"
method_get = "#7daea3"
method_post = "#a9b665"
method_write = "#d8a657"
method_delete = "#ea6962"
resource_xhr = "#d3869b"
resource_image = "#7daea3"
resource_script = "#d8a657"
resource_style = "#89b482"
resource_sse = "#a9b665"
"##;

const LEGACY_NEON_DEFAULT_CONFIG: &str = r##"# Faro configuration.

[app]
# Default database path used when --db is omitted.
db_path = "faro.db"
# Start capture immediately for URL launches instead of waiting for "o".
launch_on_start = false

[ui]
# Number of rows affected by the dark bottom overlay in the request tree.
bottom_fade_rows = 3

[theme]
text = "#e5e5e5"
muted = "#969696"
accent = "#23d18b"
panel_title = "#29b8db"
panel_border = "#545454"
active_border = "#23d18b"
tree_edge = "#969696"
ok = "#23d18b"
redirect = "#11a8cd"
client_error = "#e5e510"
server_error = "#cd3131"
method_get = "#29b8db"
method_post = "#23d18b"
method_write = "#f5f543"
method_delete = "#f14c4c"
resource_xhr = "#d670d6"
resource_image = "#3b8eea"
resource_script = "#f5f543"
resource_style = "#29b8db"
resource_sse = "#23d18b"
"##;

const LEGACY_GRUVBOX_DEFAULT_CONFIG: &str = r##"# Faro configuration.

[app]
# Default database path used when --db is omitted.
db_path = "faro.db"
# Start capture immediately for URL launches instead of waiting for "o".
launch_on_start = false

[ui]
# Number of rows affected by the dark bottom overlay in the request tree.
bottom_fade_rows = 3

[theme]
text = "#ebdbb2"
muted = "#928374"
accent = "#b8bb26"
panel_title = "#fabd2f"
panel_border = "#504945"
active_border = "#b8bb26"
tree_edge = "#928374"
ok = "#b8bb26"
redirect = "#83a598"
client_error = "#fabd2f"
server_error = "#fb4934"
method_get = "#83a598"
method_post = "#b8bb26"
method_write = "#fabd2f"
method_delete = "#fb4934"
resource_xhr = "#d3869b"
resource_image = "#83a598"
resource_script = "#fabd2f"
resource_style = "#8ec07c"
resource_sse = "#b8bb26"
"##;

const LEGACY_TERMINAL_DEFAULT_CONFIG: &str = r##"# Faro configuration.

[app]
# Default database path used when --db is omitted.
db_path = "faro.db"
# Start capture immediately for URL launches instead of waiting for "o".
launch_on_start = false

[ui]
# Number of rows affected by the dark bottom overlay in the request tree.
bottom_fade_rows = 3

[theme]
text = "default"
muted = "dark_gray"
accent = "green"
panel_title = "yellow"
panel_border = "dark_gray"
active_border = "green"
tree_edge = "dark_gray"
ok = "green"
redirect = "blue"
client_error = "yellow"
server_error = "red"
method_get = "blue"
method_post = "green"
method_write = "yellow"
method_delete = "red"
resource_xhr = "magenta"
resource_image = "blue"
resource_script = "yellow"
resource_style = "cyan"
resource_sse = "green"
"##;

const LEGACY_NEUTRAL_DEFAULT_CONFIG: &str = r##"# Faro configuration.

[app]
# Default database path used when --db is omitted.
db_path = "faro.db"
# Start capture immediately for URL launches instead of waiting for "o".
launch_on_start = false

[ui]
# Number of rows affected by the dark bottom overlay in the request tree.
bottom_fade_rows = 3

[theme]
text = "white"
muted = "gray"
accent = "yellow"
panel_title = "yellow"
panel_border = "gray"
active_border = "yellow"
tree_edge = "gray"
ok = "yellow"
redirect = "white"
client_error = "yellow"
server_error = "red"
method_get = "white"
method_post = "yellow"
method_write = "yellow"
method_delete = "red"
resource_xhr = "white"
resource_image = "white"
resource_script = "yellow"
resource_style = "gray"
resource_sse = "yellow"
"##;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub db_path: PathBuf,
    pub launch_on_start: bool,
    pub ui: UiConfig,
    pub theme: Theme,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("faro.db"),
            launch_on_start: false,
            ui: UiConfig::default(),
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

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    #[serde(default)]
    app: RawAppConfig,
    #[serde(default)]
    ui: RawUiConfig,
    #[serde(default)]
    theme: RawTheme,
}

#[derive(Debug, Default, Deserialize)]
struct RawAppConfig {
    db_path: Option<PathBuf>,
    launch_on_start: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RawUiConfig {
    bottom_fade_rows: Option<usize>,
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
    let raw: RawConfig =
        toml::from_str(&raw).with_context(|| format!("parse config {}", path.display()))?;
    raw.resolve(path.parent())
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

impl RawConfig {
    fn resolve(self, config_dir: Option<&Path>) -> anyhow::Result<AppConfig> {
        let defaults = AppConfig::default();
        let db_path = self.app.db_path.unwrap_or(defaults.db_path);
        Ok(AppConfig {
            db_path: resolve_db_path(db_path, config_dir),
            launch_on_start: self.app.launch_on_start.unwrap_or(defaults.launch_on_start),
            ui: UiConfig {
                bottom_fade_rows: self
                    .ui
                    .bottom_fade_rows
                    .unwrap_or(defaults.ui.bottom_fade_rows)
                    .clamp(0, 8),
            },
            theme: self.theme.resolve(defaults.theme)?,
        })
    }
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
