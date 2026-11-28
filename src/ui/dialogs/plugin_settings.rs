//! Plugin settings dialog for configuring Run All Checks behavior.

use fltk::{prelude::*, *};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::app::domain::settings::AppSettings;

use super::DialogTheme;

/// Smallest width of the dialog; it widens when the measured shortcut hint
/// needs more room.
const DIALOG_WIDTH: i32 = 400;
const DIALOG_HEIGHT: i32 = 350;
/// The shortcut hint: its x, label size and old fixed width, and the right
/// margin it keeps (T0110).
const HINT_X: i32 = 275;
const HINT_LABEL_SIZE: i32 = 10;
const HINT_MIN_W: i32 = 110;
const HINT_RIGHT_MARGIN: i32 = 15;
const HINT_TEXT: &str = "e.g. Ctrl+Shift+L";

/// Result from the plugin settings dialog
pub struct PluginSettingsResult {
    /// Plugins selected for Run All Checks (empty = all enabled)
    pub run_all_checks_plugins: Vec<String>,
    /// Custom shortcut (e.g., "Ctrl+Shift+L")
    pub run_all_checks_shortcut: String,
}

/// The plugin settings dialog's window and widgets, built but not shown (a
/// seam for the layout tests).
pub struct PluginSettingsDialogUi {
    pub dialog: window::Window,
    /// One checkbox per available plugin, with the plugin's name.
    pub checkboxes: Vec<(button::CheckButton, String)>,
    pub shortcut_input: input::Input,
    pub save_btn: button::Button,
    pub cancel_btn: button::Button,
}

/// Build the plugin settings dialog.
///
/// `run_all_checks_plugins` and `run_all_checks_shortcut` are the current
/// values of the same-named `AppSettings` fields; `available_plugins` is a
/// list of (plugin_name, is_enabled) pairs.
pub fn build_plugin_settings_dialog(
    theme: &DialogTheme,
    run_all_checks_plugins: &[String],
    run_all_checks_shortcut: &str,
    available_plugins: &[(String, bool)],
) -> PluginSettingsDialogUi {
    // Theme colors from DialogTheme
    let bg_color = theme.bg;
    let text_color = theme.text;
    let text_dim = theme.text_dim;

    // The hint measured with the installed fonts (T0110); the dialog widens
    // to keep its old right margin, never narrower than DIALOG_WIDTH.
    let hint_w = super::label_width(HINT_TEXT, HINT_LABEL_SIZE).max(HINT_MIN_W);
    let dialog_w = (HINT_X + hint_w + HINT_RIGHT_MARGIN).max(DIALOG_WIDTH);

    // Built top-level even if another group is current (T0058).
    let top_level = super::TopLevelScope::new();
    let mut dialog = window::Window::default()
        .with_size(dialog_w, DIALOG_HEIGHT)
        .with_label("Plugin Settings");
    dialog.set_color(bg_color);

    // Title
    let mut title = frame::Frame::default()
        .with_pos(20, 15)
        .with_size(dialog_w - 40, 25)
        .with_label("Run All Checks Configuration");
    title.set_label_size(14);
    title.set_label_color(text_color);

    // Description
    let mut desc = frame::Frame::default()
        .with_pos(20, 40)
        .with_size(dialog_w - 40, 40)
        .with_label("Select which plugins run when you press the\nRun All Checks shortcut:");
    desc.set_label_size(12);
    desc.set_label_color(text_dim);
    desc.set_align(enums::Align::Left | enums::Align::Inside | enums::Align::Wrap);

    // Plugin checkboxes scroll area
    let mut scroll = group::Scroll::default()
        .with_pos(20, 90)
        .with_size(dialog_w - 40, 150);
    scroll.set_color(bg_color);
    theme.style_scroll(&mut scroll);

    let mut pack = group::Pack::default()
        .with_pos(20, 90)
        .with_size(dialog_w - 60, 150);
    pack.set_spacing(5);

    // Determine which plugins are currently selected
    let selected: HashSet<_> = run_all_checks_plugins.iter().collect();
    let all_selected = selected.is_empty(); // Empty means "all enabled"

    // Create checkboxes for each plugin
    let mut checkboxes: Vec<(button::CheckButton, String)> = Vec::new();

    if available_plugins.is_empty() {
        let mut empty_label = frame::Frame::default()
            .with_size(dialog_w - 80, 30)
            .with_label("No plugins installed");
        empty_label.set_label_color(text_dim);
    } else {
        for (name, enabled) in available_plugins {
            let mut cb = theme
                .check_button()
                .with_size(dialog_w - 80, 22)
                .with_label(&format!(
                    "{}{}",
                    name,
                    if *enabled { "" } else { " (disabled)" }
                ));
            cb.set_label_color(if *enabled { text_color } else { text_dim });

            // Check if this plugin is selected (all_selected means check all enabled ones)
            let should_check = if all_selected {
                *enabled
            } else {
                selected.contains(name)
            };
            cb.set_value(should_check);

            checkboxes.push((cb, name.clone()));
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

    let mut shortcut_input = theme.input().with_pos(120, 250).with_size(150, 25);
    shortcut_input.set_value(run_all_checks_shortcut);

    let mut shortcut_hint = frame::Frame::default()
        .with_pos(HINT_X, 250)
        .with_size(hint_w, 25)
        .with_label(HINT_TEXT);
    shortcut_hint.set_label_size(HINT_LABEL_SIZE);
    shortcut_hint.set_label_color(text_dim);
    shortcut_hint.set_align(enums::Align::Left | enums::Align::Inside);

    // Buttons
    let save_btn = theme
        .button()
        .with_pos(dialog_w - 180, DIALOG_HEIGHT - 45)
        .with_size(75, 30)
        .with_label("Save");

    let cancel_btn = theme
        .button()
        .with_pos(dialog_w - 95, DIALOG_HEIGHT - 45)
        .with_size(75, 30)
        .with_label("Cancel");

    dialog.end();
    drop(top_level);
    PluginSettingsDialogUi {
        dialog,
        checkboxes,
        shortcut_input,
        save_btn,
        cancel_btn,
    }
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
    let theme = DialogTheme::from_theme_bg(theme_bg);

    let PluginSettingsDialogUi {
        mut dialog,
        checkboxes,
        shortcut_input,
        mut save_btn,
        mut cancel_btn,
    } = build_plugin_settings_dialog(
        &theme,
        &settings.run_all_checks_plugins,
        &settings.run_all_checks_shortcut,
        available_plugins,
    );
    dialog.make_modal(true);

    let result: Rc<RefCell<Option<PluginSettingsResult>>> = Rc::new(RefCell::new(None));
    let checkboxes: Rc<RefCell<Vec<(button::CheckButton, String)>>> =
        Rc::new(RefCell::new(checkboxes));

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
    // Shared modal loop: exits when the dialog closes OR the app is quitting,
    // so closing the main window while this dialog is open no longer hangs
    // (T0017).
    super::run_dialog(&dialog);

    result.borrow_mut().take()
}
