//! Plugin settings dialog for configuring Run All Checks behavior.

use fltk::{prelude::*, *};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::app::domain::settings::AppSettings;

use super::DialogTheme;

const DIALOG_WIDTH: i32 = 400;
const DIALOG_HEIGHT: i32 = 350;

/// Result from the plugin settings dialog
pub struct PluginSettingsResult {
    /// Plugins selected for Run All Checks (empty = all enabled)
    pub run_all_checks_plugins: Vec<String>,
    /// Custom shortcut (e.g., "Ctrl+Shift+L")
    pub run_all_checks_shortcut: String,
}

/// Show the plugin settings dialog
///
/// # Arguments
/// * `settings` - Current app settings
/// * `available_plugins` - List of (plugin_name, is_enabled) pairs
/// * `theme_bg` - Syntax theme background color for consistent styling
///
/// # Returns
/// Some(result) if user clicked Save, None if cancelled
pub fn show_plugin_settings_dialog(
    settings: &AppSettings,
    available_plugins: &[(String, bool)],
    theme_bg: (u8, u8, u8),
) -> Option<PluginSettingsResult> {
    // Theme colors from DialogTheme
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let bg_color = theme.bg;
    let text_color = theme.text;
    let text_dim = theme.text_dim;
    let input_bg = theme.input_bg;
    let button_bg = theme.button_bg;

    let mut dialog = window::Window::default()
        .with_size(DIALOG_WIDTH, DIALOG_HEIGHT)
        .with_label("Plugin Settings");
    dialog.make_modal(true);
    dialog.set_color(bg_color);

    let result: Rc<RefCell<Option<PluginSettingsResult>>> = Rc::new(RefCell::new(None));

    // Title
    let mut title = frame::Frame::default()
        .with_pos(20, 15)
        .with_size(DIALOG_WIDTH - 40, 25)
        .with_label("Run All Checks Configuration");
    title.set_label_size(14);
    title.set_label_color(text_color);

    // Description
    let mut desc = frame::Frame::default()
        .with_pos(20, 40)
        .with_size(DIALOG_WIDTH - 40, 40)
        .with_label("Select which plugins run when you press the\nRun All Checks shortcut:");
    desc.set_label_size(12);
    desc.set_label_color(text_dim);
    desc.set_align(enums::Align::Left | enums::Align::Inside | enums::Align::Wrap);

    // Plugin checkboxes scroll area
    let mut scroll = group::Scroll::default()
        .with_pos(20, 90)
        .with_size(DIALOG_WIDTH - 40, 150);
    scroll.set_color(bg_color);
    theme.style_scroll(&mut scroll);

    let mut pack = group::Pack::default()
        .with_pos(20, 90)
        .with_size(DIALOG_WIDTH - 60, 150);
    pack.set_spacing(5);

    // Determine which plugins are currently selected
    let selected: HashSet<_> = settings.run_all_checks_plugins.iter().collect();
    let all_selected = selected.is_empty(); // Empty means "all enabled"

    // Create checkboxes for each plugin
    let checkboxes: Rc<RefCell<Vec<(button::CheckButton, String)>>> =
        Rc::new(RefCell::new(Vec::new()));

    if available_plugins.is_empty() {
        let mut empty_label = frame::Frame::default()
            .with_size(DIALOG_WIDTH - 80, 30)
            .with_label("No plugins installed");
        empty_label.set_label_color(text_dim);
    } else {
        for (name, enabled) in available_plugins {
            let mut cb = button::CheckButton::default()
                .with_size(DIALOG_WIDTH - 80, 22)
                .with_label(&format!(
                    "{}{}",
                    name,
                    if *enabled { "" } else { " (disabled)" }
                ));
            cb.set_label_color(if *enabled { text_color } else { text_dim });
            cb.set_color(bg_color);
            cb.set_selection_color(button_bg);

            // Check if this plugin is selected (all_selected means check all enabled ones)
            let should_check = if all_selected {
                *enabled
            } else {
                selected.contains(name)
            };
            cb.set_value(should_check);

            checkboxes.borrow_mut().push((cb, name.clone()));
        }
    }

    pack.end();
    scroll.end();

    // Shortcut configuration
    let mut shortcut_label = frame::Frame::default()
        .with_pos(20, 250)
        .with_size(100, 25)
        .with_label("Shortcut:");
    shortcut_label.set_label_color(text_color);
    shortcut_label.set_align(enums::Align::Left | enums::Align::Inside);

    let mut shortcut_input = input::Input::default()
        .with_pos(120, 250)
        .with_size(150, 25);
    shortcut_input.set_value(&settings.run_all_checks_shortcut);
    shortcut_input.set_color(input_bg);
    shortcut_input.set_text_color(text_color);
    shortcut_input.set_selection_color(button_bg);

    let mut shortcut_hint = frame::Frame::default()
        .with_pos(275, 250)
        .with_size(110, 25)
        .with_label("e.g. Ctrl+Shift+L");
    shortcut_hint.set_label_size(10);
    shortcut_hint.set_label_color(text_dim);
    shortcut_hint.set_align(enums::Align::Left | enums::Align::Inside);

    // Buttons
    let mut save_btn = button::Button::default()
        .with_pos(DIALOG_WIDTH - 180, DIALOG_HEIGHT - 45)
        .with_size(75, 30)
        .with_label("Save");
    save_btn.set_color(button_bg);
    save_btn.set_label_color(text_color);

    let mut cancel_btn = button::Button::default()
        .with_pos(DIALOG_WIDTH - 95, DIALOG_HEIGHT - 45)
        .with_size(75, 30)
        .with_label("Cancel");
    cancel_btn.set_color(button_bg);
    cancel_btn.set_label_color(text_color);

    dialog.end();

    // Callbacks
    let checkboxes_save = checkboxes.clone();
    let result_save = result.clone();
    let mut dialog_save = dialog.clone();
    save_btn.set_callback(move |_| {
        let selected_plugins: Vec<String> = checkboxes_save
            .borrow()
            .iter()
            .filter(|(cb, _)| cb.value())
            .map(|(_, name)| name.clone())
            .collect();

        let shortcut = shortcut_input.value();

        *result_save.borrow_mut() = Some(PluginSettingsResult {
            run_all_checks_plugins: selected_plugins,
            run_all_checks_shortcut: shortcut,
        });
        dialog_save.hide();
    });

    let mut dialog_cancel = dialog.clone();
    cancel_btn.set_callback(move |_| {
        dialog_cancel.hide();
    });

    dialog.show();
    theme.apply_titlebar(&dialog);
    while dialog.shown() {
        fltk::app::wait();
    }

    result.borrow_mut().take()
}
