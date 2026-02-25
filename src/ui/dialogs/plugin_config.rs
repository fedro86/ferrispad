//! Per-plugin configuration dialog.
//!
//! Allows users to configure plugin-specific settings like:
//! - Custom shortcut for the plugin's primary action
//! - Plugin parameters defined in plugin.toml [config] section

use fltk::{enums::Color, prelude::*, *};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::app::domain::settings::PluginConfig;
use crate::app::plugins::ConfigParamDef;

const DIALOG_WIDTH: i32 = 420;
const MIN_DIALOG_HEIGHT: i32 = 180;
const ROW_HEIGHT: i32 = 30;
const SPACING: i32 = 8;

/// Result from the plugin config dialog
pub struct PluginConfigResult {
    /// Custom shortcut override (None = use manifest default)
    pub shortcut: Option<String>,
    /// Plugin-specific parameters
    pub params: HashMap<String, String>,
}

/// Show the per-plugin configuration dialog
///
/// # Arguments
/// * `plugin_name` - Name of the plugin
/// * `param_defs` - Configuration parameter definitions from plugin.toml
/// * `current_config` - Current config values from settings
/// * `default_shortcut` - Default shortcut from plugin manifest (may be None)
/// * `is_dark` - Whether dark mode is enabled
///
/// # Returns
/// Some(result) if user clicked Save, None if cancelled
pub fn show_plugin_config_dialog(
    plugin_name: &str,
    param_defs: &[ConfigParamDef],
    current_config: &PluginConfig,
    default_shortcut: Option<&str>,
    is_dark: bool,
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

    // Calculate dialog height based on number of params
    // 1 row for title, 1 for shortcut, N for params, 1 for buttons
    let content_rows = 2 + param_defs.len();
    let dialog_height =
        MIN_DIALOG_HEIGHT.max(60 + (content_rows as i32 * (ROW_HEIGHT + SPACING)) + 50);

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
        .with_size(100, 25)
        .with_label("Shortcut:");
    shortcut_label.set_label_color(text_color);
    shortcut_label.set_align(enums::Align::Left | enums::Align::Inside);

    let mut shortcut_input = input::Input::default()
        .with_pos(130, y)
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
        .with_pos(285, y)
        .with_size(120, 25)
        .with_label("e.g. Ctrl+Shift+P");
    shortcut_hint.set_label_size(10);
    shortcut_hint.set_label_color(text_dim);
    shortcut_hint.set_align(enums::Align::Left | enums::Align::Inside);
    y += ROW_HEIGHT + SPACING;

    // Dynamic parameter fields
    // Store references to input widgets for retrieval
    enum ParamWidget {
        Input(input::Input),
        Check(button::CheckButton),
    }

    let param_widgets: Rc<RefCell<Vec<(String, ParamWidget)>>> = Rc::new(RefCell::new(Vec::new()));

    for def in param_defs {
        let mut label = frame::Frame::default()
            .with_pos(20, y)
            .with_size(110, 25)
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
                    .with_pos(130, y)
                    .with_size(200, 25)
                    .with_label("Enabled");
                cb.set_value(current_value.eq_ignore_ascii_case("true"));
                cb.set_label_color(text_color);
                cb.set_color(bg_color);
                param_widgets
                    .borrow_mut()
                    .push((def.key.clone(), ParamWidget::Check(cb)));
            }
            _ => {
                // "string" or "number" - use Input widget
                let mut inp = input::Input::default()
                    .with_pos(130, y)
                    .with_size(200, 25);
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

                param_widgets
                    .borrow_mut()
                    .push((def.key.clone(), ParamWidget::Input(inp)));
            }
        }

        y += ROW_HEIGHT + SPACING;
    }

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

    save_btn.set_callback(move |_| {
        // Get shortcut value
        let shortcut_val = shortcut_input.value();
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
        for (key, widget) in param_widgets_save.borrow().iter() {
            let value = match widget {
                ParamWidget::Input(inp) => inp.value(),
                ParamWidget::Check(cb) => {
                    if cb.value() {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
            };
            params.insert(key.clone(), value);
        }

        *result_save.borrow_mut() = Some(PluginConfigResult { shortcut, params });
        dialog_save.hide();
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
