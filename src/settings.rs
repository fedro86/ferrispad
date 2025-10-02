use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_line_numbers")]
    pub line_numbers_enabled: bool,

    #[serde(default = "default_word_wrap")]
    pub word_wrap_enabled: bool,

    #[serde(default = "default_theme_mode")]
    pub theme_mode: ThemeMode,

    #[serde(default = "default_font")]
    pub font: FontChoice,

    #[serde(default = "default_font_size")]
    pub font_size: u32,
}

fn default_line_numbers() -> bool {
    true
}

fn default_word_wrap() -> bool {
    true
}

fn default_theme_mode() -> ThemeMode {
    ThemeMode::SystemDefault
}

fn default_font() -> FontChoice {
    FontChoice::ScreenBold
}

fn default_font_size() -> u32 {
    16
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            line_numbers_enabled: default_line_numbers(),
            word_wrap_enabled: default_word_wrap(),
            theme_mode: default_theme_mode(),
            font: default_font(),
            font_size: default_font_size(),
        }
    }
}

impl AppSettings {
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
    pub fn save(&self) -> Result<(), String> {
        let config_path = Self::get_config_path();

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        // Serialize to pretty JSON
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;

        // Write to file
        fs::write(&config_path, json)
            .map_err(|e| format!("Failed to write settings: {}", e))?;

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
        assert_eq!(settings.font, FontChoice::ScreenBold);
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
}
