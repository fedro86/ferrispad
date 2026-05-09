use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::app::infrastructure::error::AppError;
use crate::app::services::session::SessionRestore;
use crate::app::services::updater::UpdateChannel;

/// Per-plugin permission approvals stored in settings.
/// Tracks which commands the user has approved or denied for each plugin.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginApprovals {
    /// Commands the user has approved for this plugin
    #[serde(default)]
    pub approved_commands: Vec<String>,
    /// Commands the user has explicitly denied
    #[serde(default)]
    pub denied_commands: Vec<String>,
}

/// A shortcut override entry stored in settings.
/// Used by the centralized ShortcutRegistry to override default shortcuts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShortcutOverride {
    /// Normalized shortcut string (e.g., "Ctrl+Shift+S"), empty = unbound
    pub shortcut: String,
    /// Whether this shortcut is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Per-plugin configuration (stored in settings.json)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Plugin-specific parameters as key-value pairs
    /// e.g., {"max_line_length": "120", "ignore_rules": "E501,W503"}
    #[serde(default)]
    pub params: HashMap<String, String>,
}

/// Position of a tree panel (used by plugins like file-explorer)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TreePanelPosition {
    #[default]
    Left,
    Right,
    Bottom,
}

impl TreePanelPosition {
    /// Parse from a plugin config string value
    pub fn from_config_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "right" => Self::Right,
            "bottom" => Self::Bottom,
            _ => Self::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
    SystemDefault,
}

/// Returns true for the three built-in font names that FerrisPad has shipped historically.
/// Used to decide whether the system font catalog must be eagerly loaded on Windows
/// before the saved font name can be resolved at startup.
pub fn is_legacy_font_name(name: &str) -> bool {
    matches!(name, "Courier" | "ScreenBold" | "HelveticaMono")
}

/// Resolve a font name string to an FLTK `Font` handle. Legacy enum tags
/// (`Courier`, `ScreenBold`, `HelveticaMono`) map to their original FLTK constants;
/// any other string is looked up via `Font::by_name`, which falls back to Helvetica
/// when the name is not present in the FLTK fonts table.
pub fn resolve_font(name: &str) -> fltk::enums::Font {
    use fltk::enums::Font;
    match name {
        "Courier" => Font::Courier,
        "ScreenBold" => Font::ScreenBold,
        "HelveticaMono" => Font::Screen,
        other => Font::by_name(other),
    }
}

/// Available syntax highlighting themes from syntect
/// Each theme has a display name and the internal syntect theme key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SyntaxTheme {
    #[default]
    Base16OceanDark,
    Base16OceanLight,
    Base16EightiesDark,
    Base16MochaDark,
    SolarizedDark,
    SolarizedLight,
    InspiredGitHub,
}

impl SyntaxTheme {
    /// Get the syntect theme key for this theme
    pub fn theme_key(&self) -> &'static str {
        match self {
            Self::Base16OceanDark => "base16-ocean.dark",
            Self::Base16OceanLight => "base16-ocean.light",
            Self::Base16EightiesDark => "base16-eighties.dark",
            Self::Base16MochaDark => "base16-mocha.dark",
            Self::SolarizedDark => "Solarized (dark)",
            Self::SolarizedLight => "Solarized (light)",
            Self::InspiredGitHub => "InspiredGitHub",
        }
    }

    /// Get the display name for this theme
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Base16OceanDark => "Base16 Ocean Dark",
            Self::Base16OceanLight => "Base16 Ocean Light",
            Self::Base16EightiesDark => "Base16 Eighties Dark",
            Self::Base16MochaDark => "Base16 Mocha Dark",
            Self::SolarizedDark => "Solarized Dark",
            Self::SolarizedLight => "Solarized Light",
            Self::InspiredGitHub => "Inspired GitHub",
        }
    }

    /// Get the background color RGB for this theme (hardcoded from syntect defaults).
    pub fn background(&self) -> (u8, u8, u8) {
        match self {
            Self::Base16OceanDark => (43, 48, 59),
            Self::Base16OceanLight => (239, 241, 245),
            Self::Base16EightiesDark => (45, 45, 45),
            Self::Base16MochaDark => (59, 50, 40),
            Self::SolarizedDark => (0, 43, 54),
            Self::SolarizedLight => (253, 246, 227),
            Self::InspiredGitHub => (255, 255, 255),
        }
    }

    /// Get the foreground color RGB for this theme (hardcoded from syntect defaults).
    pub fn foreground(&self) -> (u8, u8, u8) {
        match self {
            Self::Base16OceanDark => (192, 197, 206),
            Self::Base16OceanLight => (79, 91, 102),
            Self::Base16EightiesDark => (211, 208, 200),
            Self::Base16MochaDark => (208, 200, 198),
            Self::SolarizedDark => (131, 148, 150),
            Self::SolarizedLight => (101, 123, 131),
            Self::InspiredGitHub => (50, 50, 50),
        }
    }

    /// Get all available themes
    pub fn all() -> &'static [SyntaxTheme] {
        &[
            Self::Base16OceanDark,
            Self::Base16OceanLight,
            Self::Base16EightiesDark,
            Self::Base16MochaDark,
            Self::SolarizedDark,
            Self::SolarizedLight,
            Self::InspiredGitHub,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_line_numbers")]
    pub line_numbers_enabled: bool,

    #[serde(default = "default_word_wrap")]
    pub word_wrap_enabled: bool,

    #[serde(default = "default_highlighting")]
    pub highlighting_enabled: bool,

    #[serde(default = "default_theme_mode")]
    pub theme_mode: ThemeMode,

    #[serde(default = "default_font")]
    pub font: String,

    #[serde(default = "default_font_size")]
    pub font_size: u32,

    #[serde(default = "default_auto_check_updates")]
    pub auto_check_updates: bool,

    #[serde(default = "default_update_channel")]
    pub update_channel: UpdateChannel,

    #[serde(default)]
    pub last_update_check: i64,

    #[serde(default)]
    pub skipped_versions: Vec<String>,

    #[serde(default)]
    pub tabs_enabled: bool,

    #[serde(default)]
    pub session_restore: SessionRestore,

    #[serde(default)]
    pub preview_enabled: bool,

    /// Syntax theme for light mode
    #[serde(default = "default_syntax_theme_light")]
    pub syntax_theme_light: SyntaxTheme,

    /// Syntax theme for dark mode
    #[serde(default = "default_syntax_theme_dark")]
    pub syntax_theme_dark: SyntaxTheme,

    /// Tab size in spaces (default 4)
    #[serde(default = "default_tab_size")]
    pub tab_size: u32,

    /// Insert spaces instead of tab characters (default false)
    #[serde(default)]
    pub use_spaces: bool,

    /// Whether the plugin system is enabled
    #[serde(default = "default_plugins_enabled")]
    pub plugins_enabled: bool,

    /// Names of explicitly disabled plugins
    #[serde(default)]
    pub disabled_plugins: Vec<String>,

    /// Per-plugin permission approvals (plugin_name -> approvals)
    #[serde(default)]
    pub plugin_approvals: HashMap<String, PluginApprovals>,

    /// Whether to automatically check for plugin updates
    #[serde(default = "default_auto_check_plugin_updates")]
    pub auto_check_plugin_updates: bool,

    /// Timestamp of last plugin update check (UNIX timestamp)
    #[serde(default)]
    pub last_plugin_update_check: i64,

    /// Names of plugins to include in "Run All Checks" (empty = all enabled plugins)
    #[serde(default)]
    pub run_all_checks_plugins: Vec<String>,

    /// Shortcut for "Run All Checks" command (default: "Ctrl+Shift+L")
    #[serde(default = "default_run_all_checks_shortcut")]
    pub run_all_checks_shortcut: String,

    /// Per-plugin configuration (plugin_name -> config)
    #[serde(default)]
    pub plugin_configs: HashMap<String, PluginConfig>,

    /// Centralized shortcut overrides (command_id -> override)
    /// Keys: "File/Save" for built-ins, "plugin:name:action" for plugins
    #[serde(default)]
    pub shortcut_overrides: HashMap<String, ShortcutOverride>,

    /// File size (MB) above which a warning is shown before loading (default 50)
    #[serde(default = "default_large_file_warning_mb")]
    pub large_file_warning_mb: u32,

    /// File size (MB) above which editing is blocked — read-only/tail only (default 150)
    #[serde(default = "default_max_editable_size_mb")]
    pub max_editable_size_mb: u32,
}

fn default_line_numbers() -> bool {
    true
}

fn default_word_wrap() -> bool {
    true
}

fn default_highlighting() -> bool {
    true
}

fn default_theme_mode() -> ThemeMode {
    ThemeMode::SystemDefault
}

fn default_font() -> String {
    "Courier".to_string()
}

fn default_font_size() -> u32 {
    16 // Medium size
}

fn default_auto_check_updates() -> bool {
    true
}

fn default_update_channel() -> UpdateChannel {
    UpdateChannel::Stable
}

fn default_syntax_theme_light() -> SyntaxTheme {
    SyntaxTheme::Base16OceanLight
}

fn default_syntax_theme_dark() -> SyntaxTheme {
    SyntaxTheme::Base16OceanDark
}

fn default_tab_size() -> u32 {
    4
}

fn default_plugins_enabled() -> bool {
    true
}

fn default_auto_check_plugin_updates() -> bool {
    true
}

fn default_run_all_checks_shortcut() -> String {
    "Ctrl+Shift+L".to_string()
}

fn default_large_file_warning_mb() -> u32 {
    50
}

fn default_max_editable_size_mb() -> u32 {
    150
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            line_numbers_enabled: default_line_numbers(),
            word_wrap_enabled: default_word_wrap(),
            highlighting_enabled: default_highlighting(),
            theme_mode: default_theme_mode(),
            font: default_font(),
            font_size: default_font_size(),
            auto_check_updates: default_auto_check_updates(),
            update_channel: default_update_channel(),
            last_update_check: 0,
            skipped_versions: Vec::new(),
            tabs_enabled: true,
            session_restore: SessionRestore::Off,
            preview_enabled: false,
            syntax_theme_light: default_syntax_theme_light(),
            syntax_theme_dark: default_syntax_theme_dark(),
            tab_size: default_tab_size(),
            use_spaces: false,
            plugins_enabled: default_plugins_enabled(),
            disabled_plugins: Vec::new(),
            plugin_approvals: HashMap::new(),
            auto_check_plugin_updates: default_auto_check_plugin_updates(),
            last_plugin_update_check: 0,
            run_all_checks_plugins: Vec::new(),
            run_all_checks_shortcut: default_run_all_checks_shortcut(),
            plugin_configs: HashMap::new(),
            shortcut_overrides: HashMap::new(),
            large_file_warning_mb: default_large_file_warning_mb(),
            max_editable_size_mb: default_max_editable_size_mb(),
        }
    }
}

impl AppSettings {
    /// Resolve the saved font name to an FLTK `Font` handle.
    /// Legacy tags resolve to built-in fonts; arbitrary system font names go through
    /// `Font::by_name`, which falls back to Helvetica when missing.
    pub fn current_font(&self) -> fltk::enums::Font {
        resolve_font(&self.font)
    }

    /// Font size clamped to the renderable range `[6, 96]`. Defends against
    /// hand-edited `settings.json` (or stale settings from older builds with
    /// different limits) that would otherwise leave the editor unreadable
    /// until the user explicitly picks a new size.
    pub fn font_size_clamped(&self) -> i32 {
        (self.font_size as i32).clamp(6, 96)
    }

    /// Get the syntax theme for the current mode
    pub fn current_syntax_theme(&self, is_dark: bool) -> SyntaxTheme {
        if is_dark {
            self.syntax_theme_dark
        } else {
            self.syntax_theme_light
        }
    }

    /// Load settings from disk, or create default if not exists
    pub fn load() -> Self {
        let config_path = Self::get_config_path();

        match fs::read_to_string(&config_path) {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(settings) => settings,
                Err(e) => {
                    eprintln!("Failed to parse settings: {}. Using defaults.", e);
                    Self::default()
                }
            },
            Err(_) => {
                // File doesn't exist, use defaults
                let default = Self::default();
                // Try to save defaults for next time
                let _ = default.save();
                default
            }
        }
    }

    /// Save settings to disk
    pub fn save(&self) -> Result<(), AppError> {
        let config_path = Self::get_config_path();

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(&config_path, json)?;

        Ok(())
    }

    /// Get config file path (cross-platform)
    pub fn get_config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("ferrispad");
        path.push("settings.json");
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = AppSettings::default();
        assert_eq!(settings.font_size, 16);
        assert!(settings.line_numbers_enabled);
        assert!(settings.word_wrap_enabled);
        assert_eq!(settings.theme_mode, ThemeMode::SystemDefault);
        assert_eq!(settings.font, "Courier");
        assert!(settings.auto_check_updates);
        assert_eq!(settings.update_channel, UpdateChannel::Stable);
        assert_eq!(settings.last_update_check, 0);
        assert!(settings.skipped_versions.is_empty());
        assert_eq!(settings.tab_size, 4);
    }

    #[test]
    fn test_serialize_deserialize() {
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let loaded: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings, loaded);
    }

    #[test]
    fn test_partial_config() {
        // Simulate old config missing new fields
        let json = r#"{"line_numbers_enabled": false}"#;
        let settings: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.font_size, 16); // Should use default
        assert!(!settings.line_numbers_enabled); // Should use file value
    }

    #[test]
    fn test_theme_mode_serialization() {
        let settings = AppSettings {
            theme_mode: ThemeMode::Dark,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"Dark\""));
    }

    #[test]
    fn test_font_serialization() {
        let settings = AppSettings {
            font: "Courier".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"Courier\""));
    }

    #[test]
    fn test_font_custom_name_round_trip() {
        let settings = AppSettings {
            font: "Consolas".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let loaded: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.font, "Consolas");
    }

    #[test]
    fn test_resolve_font_legacy_courier() {
        assert_eq!(resolve_font("Courier"), fltk::enums::Font::Courier);
    }

    #[test]
    fn test_resolve_font_legacy_screenbold() {
        assert_eq!(resolve_font("ScreenBold"), fltk::enums::Font::ScreenBold);
    }

    #[test]
    fn test_resolve_font_legacy_helveticamono() {
        assert_eq!(resolve_font("HelveticaMono"), fltk::enums::Font::Screen);
    }

    #[test]
    fn test_resolve_font_unknown_does_not_panic() {
        // by_name falls back to Helvetica for unknown names — must not panic.
        let _ = resolve_font("NopeFontThatDoesNotExist");
    }

    #[test]
    fn test_is_legacy_font_name() {
        assert!(is_legacy_font_name("Courier"));
        assert!(is_legacy_font_name("ScreenBold"));
        assert!(is_legacy_font_name("HelveticaMono"));
        assert!(!is_legacy_font_name("Consolas"));
        assert!(!is_legacy_font_name(""));
    }

    #[test]
    fn test_load_legacy_settings_with_font_screenbold() {
        // Simulate a settings.json from a previous version that used the closed
        // FontChoice enum. The string tag should round-trip and resolve correctly.
        let json = r#"{"font": "ScreenBold"}"#;
        let settings: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.font, "ScreenBold");
        assert_eq!(settings.current_font(), fltk::enums::Font::ScreenBold);
    }

    #[test]
    fn test_update_settings_serialization() {
        let settings = AppSettings {
            auto_check_updates: false,
            update_channel: UpdateChannel::Beta,
            last_update_check: 1234567890,
            skipped_versions: vec!["0.1.5".to_string()],
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let loaded: AppSettings = serde_json::from_str(&json).unwrap();

        assert!(!loaded.auto_check_updates);
        assert_eq!(loaded.update_channel, UpdateChannel::Beta);
        assert_eq!(loaded.last_update_check, 1234567890);
        assert_eq!(loaded.skipped_versions, vec!["0.1.5".to_string()]);
    }

    #[test]
    fn test_backward_compatibility() {
        // Old config without update fields should use defaults
        let json = r#"{
            "line_numbers_enabled": false,
            "word_wrap_enabled": true,
            "theme_mode": "Dark",
            "font": "Courier",
            "font_size": 14
        }"#;
        let settings: AppSettings = serde_json::from_str(json).unwrap();

        // Old fields preserved
        assert!(!settings.line_numbers_enabled);
        assert_eq!(settings.font_size, 14);

        // New fields use defaults
        assert!(settings.auto_check_updates);
        assert_eq!(settings.update_channel, UpdateChannel::Stable);
        assert_eq!(settings.last_update_check, 0);
        assert!(settings.skipped_versions.is_empty());
    }
}
