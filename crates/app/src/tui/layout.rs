use super::state::FocusPane;
use std::env;
use std::fs;
use std::path::PathBuf;

const DEFAULT_REQUESTS_PERCENT: u16 = 48;
const DEFAULT_DETAIL_PERCENT: u16 = 38;
const MIN_SPLIT_PERCENT: u16 = 20;
const MAX_SPLIT_PERCENT: u16 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutMode {
    Normal,
    Focused,
}

impl LayoutMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Focused => "focused",
        }
    }

    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::Normal => Self::Focused,
            Self::Focused => Self::Normal,
        }
    }

    fn parse(value: &str) -> Self {
        match value.trim() {
            "focused" => Self::Focused,
            _ => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DensityMode {
    Compact,
    Comfortable,
}

impl DensityMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Comfortable => "comfortable",
        }
    }

    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::Compact => Self::Comfortable,
            Self::Comfortable => Self::Compact,
        }
    }

    fn parse(value: &str) -> Self {
        match value.trim() {
            "comfortable" => Self::Comfortable,
            _ => Self::Compact,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LayoutPreference {
    pub(crate) mode: LayoutMode,
    pub(crate) density: DensityMode,
    pub(crate) focus: FocusPane,
    pub(crate) requests_percent: u16,
    pub(crate) detail_percent: u16,
    pub(crate) filter_preset: Option<String>,
}

impl Default for LayoutPreference {
    fn default() -> Self {
        Self {
            mode: LayoutMode::Normal,
            density: DensityMode::Compact,
            focus: FocusPane::Requests,
            requests_percent: DEFAULT_REQUESTS_PERCENT,
            detail_percent: DEFAULT_DETAIL_PERCENT,
            filter_preset: None,
        }
    }
}

impl LayoutPreference {
    pub(crate) fn load() -> Self {
        let Some(path) = preference_path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };

        let mut preference = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "mode" => preference.mode = LayoutMode::parse(value),
                "density" => preference.density = DensityMode::parse(value),
                "focus" => preference.focus = FocusPane::parse(value),
                "requests_percent" => {
                    preference.requests_percent = parse_percent(value, DEFAULT_REQUESTS_PERCENT)
                }
                "detail_percent" => {
                    preference.detail_percent = parse_percent(value, DEFAULT_DETAIL_PERCENT)
                }
                "filter_preset" => {
                    let value = value.trim();
                    preference.filter_preset = (!value.is_empty()).then(|| value.to_string());
                }
                _ => {}
            }
        }
        preference
    }

    pub(crate) fn save(self) -> anyhow::Result<PathBuf> {
        let path = preference_path()
            .ok_or_else(|| anyhow::anyhow!("XDG_CONFIG_HOME and HOME are unavailable"))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            &path,
            format!(
                "mode={}\ndensity={}\nfocus={}\nrequests_percent={}\ndetail_percent={}\nfilter_preset={}\n",
                self.mode.label(),
                self.density.label(),
                self.focus.label(),
                self.requests_percent,
                self.detail_percent,
                self.filter_preset.as_deref().unwrap_or("")
            ),
        )?;
        Ok(path)
    }
}

pub(crate) fn clamp_split_percent(value: u16) -> u16 {
    value.clamp(MIN_SPLIT_PERCENT, MAX_SPLIT_PERCENT)
}

fn parse_percent(value: &str, default: u16) -> u16 {
    value
        .trim()
        .parse()
        .map(clamp_split_percent)
        .unwrap_or(default)
}

fn preference_path() -> Option<PathBuf> {
    if let Ok(config_home) = env::var("XDG_CONFIG_HOME")
        && !config_home.is_empty()
    {
        return Some(PathBuf::from(config_home).join("devbench/layout.conf"));
    }

    match env::var("HOME") {
        Ok(home) if !home.is_empty() => {
            Some(PathBuf::from(home).join(".config/devbench/layout.conf"))
        }
        _ => None,
    }
}
