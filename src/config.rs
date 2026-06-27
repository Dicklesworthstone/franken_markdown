//! Native `fmd` configuration.
//!
//! This module is compiled only with the CLI feature. The render core and WASM
//! build do not read files, environment variables, or platform paths.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::{DarkModePolicy, FontFamily, PageMargins, Theme};

/// Supported config keys.
pub const CONFIG_KEYS: &[&str] = &[
    "font",
    "dark_mode",
    "custom_css",
    "page_size",
    "margin_top_pt",
    "margin_right_pt",
    "margin_bottom_pt",
    "margin_left_pt",
];

/// Native CLI configuration. Every field is optional; unresolved values come
/// from the built-in [`Theme`] default so renders stay deterministic.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FmdConfig {
    pub font: Option<FontFamily>,
    pub dark_mode: Option<DarkModePolicy>,
    pub custom_css: Option<PathBuf>,
    pub margins: Option<PageMargins>,
}

/// Config read/parse/write error.
#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Parse(msg) => f.write_str(msg),
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type ConfigResult<T> = std::result::Result<T, ConfigError>;

impl FmdConfig {
    /// Load the default native config path. Missing config means defaults.
    pub fn load_default() -> ConfigResult<Self> {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(src) => Self::parse(&src),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(ConfigError::Io(err)),
        }
    }

    /// Save to the default native config path.
    pub fn save_default(&self) -> ConfigResult<PathBuf> {
        let path = config_path();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, self.to_file_string())?;
        Ok(path)
    }

    /// Parse `key=value` config text.
    pub fn parse(src: &str) -> ConfigResult<Self> {
        let mut cfg = Self::default();
        for (idx, raw) in src.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(ConfigError::Parse(format!(
                    "line {}: expected key=value",
                    idx + 1
                )));
            };
            cfg.set_key_value(key.trim(), unquote(value.trim()))
                .map_err(|msg| ConfigError::Parse(format!("line {}: {msg}", idx + 1)))?;
        }
        Ok(cfg)
    }

    /// Set one supported key.
    pub fn set_key_value(&mut self, key: &str, value: &str) -> std::result::Result<(), String> {
        match normalize_key(key).as_str() {
            "font" => {
                self.font = Some(
                    FontFamily::parse(value)
                        .ok_or_else(|| "font must be `sans` or `serif`".to_string())?,
                );
            }
            "dark_mode" => {
                self.dark_mode = Some(parse_dark_mode(value)?);
            }
            "custom_css" => {
                let trimmed = value.trim();
                self.custom_css = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                };
            }
            "page_size" => {
                if !value.trim().eq_ignore_ascii_case("letter") {
                    return Err("page_size currently supports only `letter`".to_string());
                }
            }
            "margin_top_pt" => self.set_margin(|m, v| m.top_pt = v, value)?,
            "margin_right_pt" => self.set_margin(|m, v| m.right_pt = v, value)?,
            "margin_bottom_pt" => self.set_margin(|m, v| m.bottom_pt = v, value)?,
            "margin_left_pt" => self.set_margin(|m, v| m.left_pt = v, value)?,
            _ => {
                return Err(format!(
                    "unknown config key `{key}`; supported keys: {}",
                    CONFIG_KEYS.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// Resolve a key to the effective value used by rendering.
    #[must_use]
    pub fn get_resolved(&self, key: &str) -> Option<String> {
        let theme = self.to_theme();
        match normalize_key(key).as_str() {
            "font" => Some(theme.font.as_str().to_string()),
            "dark_mode" => Some(theme.dark_mode.as_str().to_string()),
            "custom_css" => Some(
                self.custom_css
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            ),
            "page_size" => Some(theme.page.size.name.to_string()),
            "margin_top_pt" => Some(json_num(theme.page.margins.top_pt)),
            "margin_right_pt" => Some(json_num(theme.page.margins.right_pt)),
            "margin_bottom_pt" => Some(json_num(theme.page.margins.bottom_pt)),
            "margin_left_pt" => Some(json_num(theme.page.margins.left_pt)),
            _ => None,
        }
    }

    /// Resolve this config into a full theme.
    #[must_use]
    pub fn to_theme(&self) -> Theme {
        let mut theme = Theme::default();
        if let Some(font) = self.font {
            theme = theme.with_font(font);
        }
        if let Some(dark_mode) = self.dark_mode {
            theme = theme.with_dark_mode(dark_mode);
        }
        if let Some(margins) = self.margins {
            theme.page.margins = margins;
        }
        theme
    }

    /// Stable JSON object for CLI surfaces.
    #[must_use]
    pub fn to_json(&self) -> String {
        let theme = self.to_theme();
        let custom_css = self
            .custom_css
            .as_ref()
            .map(|p| format!("\"{}\"", json_escape(&p.display().to_string())))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"font\":\"{}\",\"dark_mode\":\"{}\",\"custom_css\":{},\"page_size\":\"{}\",\
             \"margins\":{{\"top_pt\":{},\"right_pt\":{},\"bottom_pt\":{},\"left_pt\":{}}}}}",
            theme.font.as_str(),
            theme.dark_mode.as_str(),
            custom_css,
            theme.page.size.name,
            json_num(theme.page.margins.top_pt),
            json_num(theme.page.margins.right_pt),
            json_num(theme.page.margins.bottom_pt),
            json_num(theme.page.margins.left_pt),
        )
    }

    /// Deterministic file representation.
    #[must_use]
    pub fn to_file_string(&self) -> String {
        let mut out = String::new();
        if let Some(font) = self.font {
            out.push_str("font=");
            out.push_str(font.as_str());
            out.push('\n');
        }
        if let Some(dark_mode) = self.dark_mode {
            out.push_str("dark_mode=");
            out.push_str(dark_mode.as_str());
            out.push('\n');
        }
        if let Some(path) = &self.custom_css {
            out.push_str("custom_css=");
            out.push_str(&path.display().to_string());
            out.push('\n');
        }
        if let Some(margins) = self.margins {
            out.push_str("margin_top_pt=");
            out.push_str(&json_num(margins.top_pt));
            out.push('\n');
            out.push_str("margin_right_pt=");
            out.push_str(&json_num(margins.right_pt));
            out.push('\n');
            out.push_str("margin_bottom_pt=");
            out.push_str(&json_num(margins.bottom_pt));
            out.push('\n');
            out.push_str("margin_left_pt=");
            out.push_str(&json_num(margins.left_pt));
            out.push('\n');
        }
        out
    }

    fn set_margin(
        &mut self,
        update: impl FnOnce(&mut PageMargins, f32),
        value: &str,
    ) -> std::result::Result<(), String> {
        let parsed = value
            .trim()
            .parse::<f32>()
            .map_err(|_| "margin value must be a number of points".to_string())?;
        if !parsed.is_finite() || parsed < 0.0 {
            return Err("margin value must be finite and non-negative".to_string());
        }
        let mut margins = self.margins.unwrap_or_default();
        update(&mut margins, parsed);
        self.margins = Some(margins);
        Ok(())
    }
}

/// Default config path using explicit override, XDG, platform env, then HOME.
#[must_use]
pub fn config_path() -> PathBuf {
    if let Some(path) = non_empty_env("FMD_CONFIG") {
        return PathBuf::from(path);
    }
    if let Some(path) = non_empty_env("XDG_CONFIG_HOME") {
        return Path::new(&path).join("fmd").join("config");
    }
    if cfg!(windows)
        && let Some(path) = non_empty_env("APPDATA")
    {
        return Path::new(&path).join("fmd").join("config");
    }
    if let Some(home) = non_empty_env("HOME") {
        return Path::new(&home).join(".config").join("fmd").join("config");
    }
    PathBuf::from("fmd.config")
}

fn parse_dark_mode(value: &str) -> std::result::Result<DarkModePolicy, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" | "on" | "true" => Ok(DarkModePolicy::Auto),
        "disabled" | "off" | "false" | "none" => Ok(DarkModePolicy::Disabled),
        _ => Err("dark_mode must be `auto` or `disabled`".to_string()),
    }
}

fn normalize_key(key: &str) -> String {
    key.trim().replace('-', "_").to_ascii_lowercase()
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value)
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn json_num(value: f32) -> String {
    let mut s = format!("{value:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() { "0".to_string() } else { s }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}
