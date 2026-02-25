//! Per-plugin configuration dialog.
//!
//! Allows users to configure plugin-specific settings like:
//! - Custom shortcut for the plugin's primary action
//! - Plugin parameters defined in plugin.toml [config] section
//!
//! Supported parameter types:
//! - "string": Text input field
//! - "number": Text input field (validated as number)
//! - "boolean": Checkbox
//! - "choice": Dropdown with predefined options
//!
//! Validation:
//! - Shortcut: Checks for conflicts with reserved and other plugin shortcuts
//! - Number: Validates that value is a valid number

use fltk::{enums::Color, prelude::*, *};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::app::domain::settings::PluginConfig;
use crate::app::plugins::ConfigParamDef;
use crate::ui::menu::{normalize_shortcut, RESERVED_SHORTCUTS};

const DIALOG_WIDTH: i32 = 480;
const MIN_DIALOG_HEIGHT: i32 = 180;
const ROW_HEIGHT: i32 = 30;
const SPACING: i32 = 10;
const LABEL_WIDTH: i32 = 150;
const FIELD_X: i32 = 170;
const FIELD_WIDTH: i32 = 280;
const ERROR_ROW_HEIGHT: i32 = 20;

/// Result from the plugin config dialog
pub struct PluginConfigResult {
    /// Custom shortcut override (None = use manifest default)
    pub shortcut: Option<String>,
    /// Plugin-specific parameters
    pub params: HashMap<String, String>,
}

/// Widget types for parameter values
enum ParamWidget {
    Input(input::Input),
    Check(button::CheckButton),
    Choice(menu::Choice),
}

/// Parse an option string into (value, display_label)
/// Supports format: "value" or "value|Display Label"
fn parse_option(opt: &str) -> (String, String) {
    if let Some((value, label)) = opt.split_once('|') {
        (value.to_string(), label.to_string())
    } else {
        (opt.to_string(), opt.to_string())
    }
}

/// Shortcut validation data (clonable for use in callbacks)
/// Maps plugin name to its effective shortcut (normalized)
#[derive(Debug, Clone, Default)]
pub struct ShortcutValidationData {
    /// Other plugins' shortcuts: plugin_name -> normalized_shortcut
    pub other_plugins: HashMap<String, String>,
}

impl ShortcutValidationData {
    /// Build validation data from settings and plugin manager
    pub fn from_plugins(
        settings: &crate::app::domain::settings::AppSettings,
        plugins: &crate::app::plugins::PluginManager,
        current_plugin: &str,
    ) -> Self {
        let mut other_plugins = HashMap::new();

        for plugin in plugins.list_plugins() {
            // Skip the current plugin
            if plugin.name == current_plugin {
                continue;
            }

            // Get effective shortcut for this plugin
            let effective = if let Some(config) = settings.plugin_configs.get(&plugin.name) {
                config
                    .shortcut
                    .as_ref()
                    .or(plugin.menu_items.first().and_then(|m| m.shortcut.as_ref()))
            } else {
                plugin.menu_items.first().and_then(|m| m.shortcut.as_ref())
            };

            if let Some(shortcut) = effective {
                other_plugins.insert(plugin.name.clone(), normalize_shortcut(shortcut));
            }
        }

        Self { other_plugins }
    }

    /// Validate a shortcut string
    /// Returns Ok(()) if valid, Err(message) if invalid
    pub fn validate(&self, shortcut: &str) -> Result<(), String> {
        // Empty is always valid (no shortcut)
        if shortcut.trim().is_empty() {
            return Ok(());
        }

        // Check if it can be parsed (basic format check)
        if !crate::ui::menu::is_valid_shortcut(shortcut) {
            return Err("Invalid shortcut format. Use e.g. Ctrl+Shift+P".to_string());
        }

        let normalized = normalize_shortcut(shortcut);

        // Check reserved shortcuts
        for reserved in RESERVED_SHORTCUTS {
            if normalize_shortcut(reserved) == normalized {
                return Err(format!("Shortcut '{}' is reserved by FerrisPad", shortcut));
            }
        }

        // Check against other plugins
        for (plugin_name, other_shortcut) in &self.other_plugins {
            if *other_shortcut == normalized {
                return Err(format!(
                    "Shortcut '{}' conflicts with {}",
                    shortcut, plugin_name
                ));
            }
        }

        Ok(())
    }
}

/// Show the per-plugin configuration dialog
///
/// # Arguments
/// * `plugin_name` - Name of the plugin
/// * `param_defs` - Configuration parameter definitions from plugin.toml
/// * `current_config` - Current config values from settings
/// * `default_shortcut` - Default shortcut from plugin manifest (may be None)
/// * `is_dark` - Whether dark mode is enabled
/// * `shortcut_validation` - Optional validation data for shortcut conflict detection
///
/// # Returns
/// Some(result) if user clicked Save and validation passed, None if cancelled
pub fn show_plugin_config_dialog(
    plugin_name: &str,
    param_defs: &[ConfigParamDef],
    current_config: &PluginConfig,
    default_shortcut: Option<&str>,
    is_dark: bool,
    shortcut_validation: Option<ShortcutValidationData>,
) -> Option<PluginConfigResult> {
    // Theme colors
    let bg_color = if is_dark {
        Color::from_rgb(45, 45, 45)
    } else {
        Color::from_rgb(250, 250, 250)
    };
    let text_color = if is_dark {
        Color::from_rgb(220, 220, 220)
    } else {
        Color::from_rgb(30, 30, 30)
    };
    let text_dim = if is_dark {
        Color::from_rgb(150, 150, 150)
    } else {
        Color::from_rgb(100, 100, 100)
    };
    let input_bg = if is_dark {
        Color::from_rgb(60, 60, 60)
    } else {
        Color::from_rgb(255, 255, 255)
    };
    let button_bg = if is_dark {
        Color::from_rgb(70, 70, 70)
    } else {
        Color::from_rgb(230, 230, 230)
    };
    let error_color = Color::from_rgb(220, 60, 60);

    // Calculate dialog height based on number of params
    // 1 row for title, 1 for shortcut, N for params, 1 for error, 1 for buttons
    let content_rows = 2 + param_defs.len();
    let dialog_height = MIN_DIALOG_HEIGHT
        .max(60 + (content_rows as i32 * (ROW_HEIGHT + SPACING)) + ERROR_ROW_HEIGHT + 50);

    let mut dialog = window::Window::default()
        .with_size(DIALOG_WIDTH, dialog_height)
        .with_label(&format!("{} Settings", plugin_name));
    dialog.make_modal(true);
    dialog.set_color(bg_color);

    let result: Rc<RefCell<Option<PluginConfigResult>>> = Rc::new(RefCell::new(None));

    let mut y = 15;

    // Title
    let mut title = frame::Frame::default()
        .with_pos(20, y)
        .with_size(DIALOG_WIDTH - 40, 25)
        .with_label(&format!("Configure {}", plugin_name));
    title.set_label_size(14);
    title.set_label_color(text_color);
    y += ROW_HEIGHT + SPACING;

    // Shortcut field (always shown)
    let mut shortcut_label = frame::Frame::default()
        .with_pos(20, y)
        .with_size(LABEL_WIDTH, 25)
        .with_label("Shortcut:");
    shortcut_label.set_label_color(text_color);
    shortcut_label.set_align(enums::Align::Left | enums::Align::Inside);

    let mut shortcut_input = input::Input::default()
        .with_pos(FIELD_X, y)
        .with_size(150, 25);

    // Use override if set, otherwise use manifest default
    let shortcut_value = current_config
        .shortcut
        .as_deref()
        .or(default_shortcut)
        .unwrap_or("");
    shortcut_input.set_value(shortcut_value);
    shortcut_input.set_color(input_bg);
    shortcut_input.set_text_color(text_color);

    let mut shortcut_hint = frame::Frame::default()
        .with_pos(FIELD_X + 155, y)
        .with_size(120, 25)
        .with_label("e.g. Ctrl+Shift+P");
    shortcut_hint.set_label_size(10);
    shortcut_hint.set_label_color(text_dim);
    shortcut_hint.set_align(enums::Align::Left | enums::Align::Inside);
    y += ROW_HEIGHT + SPACING;

    // Store references to input widgets for retrieval
    // For Choice widgets, also store the options values (not labels) for retrieval
    // For number validation, also store the param type
    let param_widgets: Rc<RefCell<Vec<(String, ParamWidget, Vec<String>, String)>>> =
        Rc::new(RefCell::new(Vec::new()));

    for def in param_defs {
        let mut label = frame::Frame::default()
            .with_pos(20, y)
            .with_size(LABEL_WIDTH, 25)
            .with_label(&format!("{}:", def.label));
        label.set_label_color(text_color);
        label.set_align(enums::Align::Left | enums::Align::Inside);

        // Get current value or default
        let current_value = current_config
            .params
            .get(&def.key)
            .cloned()
            .unwrap_or_else(|| def.default.clone());

        match def.param_type.as_str() {
            "boolean" => {
                let mut cb = button::CheckButton::default()
                    .with_pos(FIELD_X, y)
                    .with_size(FIELD_WIDTH, 25)
                    .with_label("Enabled");
                cb.set_value(current_value.eq_ignore_ascii_case("true"));
                cb.set_label_color(text_color);
                cb.set_color(bg_color);
                param_widgets.borrow_mut().push((
                    def.key.clone(),
                    ParamWidget::Check(cb),
                    vec![],
                    "boolean".to_string(),
                ));
            }
            "choice" => {
                let mut choice = menu::Choice::default()
                    .with_pos(FIELD_X, y)
                    .with_size(FIELD_WIDTH, 25);
                choice.set_color(input_bg);
                choice.set_text_color(text_color);

                // Parse options and add to choice widget
                let mut option_values: Vec<String> = Vec::new();
                let mut selected_idx: i32 = 0;

                for (idx, opt) in def.options.iter().enumerate() {
                    let (value, display_label) = parse_option(opt);
                    choice.add_choice(&display_label);
                    if value == current_value {
                        selected_idx = idx as i32;
                    }
                    option_values.push(value);
                }

                // Set current selection
                if !def.options.is_empty() {
                    choice.set_value(selected_idx);
                }

                param_widgets.borrow_mut().push((
                    def.key.clone(),
                    ParamWidget::Choice(choice),
                    option_values,
                    "choice".to_string(),
                ));
            }
            param_type => {
                // "string" or "number" - use Input widget
                let mut inp = input::Input::default()
                    .with_pos(FIELD_X, y)
                    .with_size(FIELD_WIDTH, 25);
                inp.set_value(&current_value);
                inp.set_color(input_bg);
                inp.set_text_color(text_color);

                // Show placeholder if available and value is empty
                if current_value.is_empty() {
                    if let Some(ref placeholder) = def.placeholder {
                        inp.set_value(placeholder);
                        inp.set_text_color(text_dim);
                        // Clear placeholder on focus (simple approach)
                        let placeholder_clone = placeholder.clone();
                        let text_color_clone = text_color;
                        inp.set_callback(move |i| {
                            if i.value() == placeholder_clone {
                                i.set_value("");
                                i.set_text_color(text_color_clone);
                            }
                        });
                    }
                }

                param_widgets.borrow_mut().push((
                    def.key.clone(),
                    ParamWidget::Input(inp),
                    vec![],
                    param_type.to_string(),
                ));
            }
        }

        y += ROW_HEIGHT + SPACING;
    }

    // Error label (hidden by default, shown when validation fails)
    let mut error_label = frame::Frame::default()
        .with_pos(20, y)
        .with_size(DIALOG_WIDTH - 40, ERROR_ROW_HEIGHT)
        .with_label("");
    error_label.set_label_color(error_color);
    error_label.set_label_size(11);
    error_label.set_align(enums::Align::Left | enums::Align::Inside);
    error_label.hide();

    // Buttons at the bottom
    let button_y = dialog_height - 45;

    let mut save_btn = button::Button::default()
        .with_pos(DIALOG_WIDTH - 180, button_y)
        .with_size(75, 30)
        .with_label("Save");
    save_btn.set_color(button_bg);
    save_btn.set_label_color(text_color);

    let mut cancel_btn = button::Button::default()
        .with_pos(DIALOG_WIDTH - 95, button_y)
        .with_size(75, 30)
        .with_label("Cancel");
    cancel_btn.set_color(button_bg);
    cancel_btn.set_label_color(text_color);

    dialog.end();

    // Callbacks
    let param_widgets_save = param_widgets.clone();
    let result_save = result.clone();
    let mut dialog_save = dialog.clone();
    let default_shortcut_clone = default_shortcut.map(String::from);
    let error_label = Rc::new(RefCell::new(error_label));
    let error_label_save = error_label.clone();
    let mut shortcut_input_clone = shortcut_input.clone();

    save_btn.set_callback(move |_| {
        // Get shortcut value
        let shortcut_val = shortcut_input.value();

        // Validate shortcut if validation data is available
        if let Some(ref validation) = shortcut_validation {
            if let Err(error_msg) = validation.validate(&shortcut_val) {
                let mut err = error_label_save.borrow_mut();
                err.set_label(&error_msg);
                err.show();
                err.redraw();
                return;
            }
        }

        // Validate number fields
        for (key, widget, _, param_type) in param_widgets_save.borrow().iter() {
            if param_type == "number" {
                if let ParamWidget::Input(inp) = widget {
                    let value = inp.value();
                    // Allow empty (will use default)
                    if !value.is_empty() && value.parse::<f64>().is_err() {
                        let mut err = error_label_save.borrow_mut();
                        err.set_label(&format!("'{}' must be a valid number", key));
                        err.show();
                        err.redraw();
                        return;
                    }
                }
            }
        }

        // All validation passed, build result
        let shortcut = if shortcut_val.is_empty() {
            None
        } else if Some(shortcut_val.as_str()) == default_shortcut_clone.as_deref() {
            // Same as default, don't store override
            None
        } else {
            Some(shortcut_val)
        };

        // Get all param values
        let mut params = HashMap::new();
        for (key, widget, option_values, _) in param_widgets_save.borrow().iter() {
            let value = match widget {
                ParamWidget::Input(inp) => inp.value(),
                ParamWidget::Check(cb) => {
                    if cb.value() {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
                ParamWidget::Choice(choice) => {
                    let idx = choice.value();
                    if idx >= 0 && (idx as usize) < option_values.len() {
                        option_values[idx as usize].clone()
                    } else {
                        String::new()
                    }
                }
            };
            params.insert(key.clone(), value);
        }

        *result_save.borrow_mut() = Some(PluginConfigResult { shortcut, params });
        dialog_save.hide();
    });

    // Clear error on shortcut input change
    let error_label_input = error_label.clone();
    shortcut_input_clone.handle(move |_, ev| {
        if ev == enums::Event::KeyUp || ev == enums::Event::KeyDown {
            let mut err = error_label_input.borrow_mut();
            if err.visible() {
                err.hide();
                err.redraw();
            }
        }
        false
    });

    let mut dialog_cancel = dialog.clone();
    cancel_btn.set_callback(move |_| {
        dialog_cancel.hide();
    });

    dialog.show();
    while dialog.shown() {
        fltk::app::wait();
    }

    result.borrow_mut().take()
}
