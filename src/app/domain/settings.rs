use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::app::infrastructure::error::AppError;
use crate::app::services::session::SessionRestore;
use crate::app::services::updater::UpdateChannel;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
    SystemDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FontChoice {
    ScreenBold,
    Courier,
    HelveticaMono,
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
    pub font: FontChoice,

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

fn default_font() -> FontChoice {
    FontChoice::Courier
}

fn default_font_size() -> u32 {
    16  // Medium size
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
        }
    }
}

impl AppSettings {
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
            Ok(contents) => {
                match serde_json::from_str(&contents) {
                    Ok(settings) => settings,
                    Err(e) => {
                        eprintln!("Failed to parse settings: {}. Using defaults.", e);
                        Self::default()
                    }
                }
            }
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
        let mut path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."));
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
        assert_eq!(settings.font, FontChoice::Courier);
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
        assert_eq!(settings.font_size, 16);  // Should use default
        assert!(!settings.line_numbers_enabled);  // Should use file value
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
    fn test_font_choice_serialization() {
        let settings = AppSettings {
            font: FontChoice::Courier,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"Courier\""));
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
