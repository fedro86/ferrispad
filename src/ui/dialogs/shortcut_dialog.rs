//! Key Shortcuts dialog.
//!
//! Two-tab dialog for viewing and editing all keyboard shortcuts:
//! - FerrisPad tab: built-in editor shortcuts
//! - Plugins tab: plugin action shortcuts grouped by plugin
//!
//! Features:
//! - Inline editing of shortcut strings
//! - Reset button to restore defaults
//! - Real-time conflict detection across all shortcuts

use fltk::{
    button::Button,
    enums::{Align, Color, FrameType},
    frame::Frame,
    group::{Group, Pack, PackType, Scroll, Tabs},
    input::Input,
    prelude::*,
    window::Window,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::app::plugins::PluginManager;
use crate::app::services::shortcut_registry::ShortcutRegistry;
use crate::ui::menu::{
    normalize_shortcut, is_valid_shortcut, plugin_command_id, BUILTIN_SHORTCUTS,
};

use super::DialogTheme;

// Layout constants
const DIALOG_WIDTH: i32 = 580;
const DIALOG_HEIGHT: i32 = 520;
const PADDING: i32 = 10;
const TAB_HEIGHT: i32 = 30;
const BUTTON_HEIGHT: i32 = 30;
const ROW_HEIGHT: i32 = 32;
const LABEL_WIDTH: i32 = 220;
const INPUT_WIDTH: i32 = 180;
const RESET_BTN_WIDTH: i32 = 55;
const SECTION_HEADER_HEIGHT: i32 = 28;

/// Result from the shortcut dialog
pub struct ShortcutDialogResult {
    /// All overrides (only entries that differ from defaults)
    pub overrides: HashMap<String, crate::app::domain::settings::ShortcutOverride>,
}

/// A single shortcut entry tracked during editing
struct ShortcutEntry {
    id: String,
    default: String,
    input: Input,
}

/// Show the Key Shortcuts dialog.
///
/// Returns Some(result) if user clicked Save, None if cancelled.
pub fn show_shortcut_dialog(
    registry: &ShortcutRegistry,
    plugins: &PluginManager,
    theme_bg: (u8, u8, u8),
    tabs_enabled: bool,
) -> Option<ShortcutDialogResult> {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let error_color = Color::from_rgb(220, 60, 60);

    let mut dialog = Window::default()
        .with_size(DIALOG_WIDTH, DIALOG_HEIGHT)
        .with_label("Key Shortcuts")
        .center_screen();
    dialog.make_modal(true);
    dialog.set_color(theme.bg);

    let result: Rc<RefCell<Option<ShortcutDialogResult>>> = Rc::new(RefCell::new(None));

    // Track all entries for conflict detection and value retrieval
    let all_entries: Rc<RefCell<Vec<ShortcutEntry>>> = Rc::new(RefCell::new(Vec::new()));

    // Tabs
    let tabs_y = PADDING;
    let tabs_height = DIALOG_HEIGHT - PADDING * 3 - BUTTON_HEIGHT - 10;
    let mut tabs = Tabs::default()
        .with_pos(PADDING, tabs_y)
        .with_size(DIALOG_WIDTH - PADDING * 2, tabs_height);
    tabs.set_frame(FrameType::FlatBox);
    tabs.set_color(theme.row_bg);
    tabs.set_selection_color(theme.tab_active_bg);

    let content_y = tabs_y + TAB_HEIGHT;
    let content_h = tabs_height - TAB_HEIGHT;
    let scroll_w = DIALOG_WIDTH - PADDING * 2 - 10;

    // ============ FERRISPAD TAB ============
    let mut builtin_group = Group::default()
        .with_pos(PADDING, content_y)
        .with_size(DIALOG_WIDTH - PADDING * 2, content_h)
        .with_label("FerrisPad");
    builtin_group.set_label_color(theme.text);
    builtin_group.set_color(theme.tab_active_bg);

    let mut scroll_builtin = Scroll::default()
        .with_pos(PADDING + 5, content_y + 5)
        .with_size(scroll_w, content_h - 10);
    scroll_builtin.set_color(theme.bg);

    let mut pack_builtin = Pack::default()
        .with_pos(PADDING + 5, content_y + 5)
        .with_size(scroll_w, 0);
    pack_builtin.set_type(PackType::Vertical);
    pack_builtin.set_spacing(2);

    // Add header row
    {
        let mut header = Group::default().with_size(scroll_w, SECTION_HEADER_HEIGHT);
        header.set_frame(FrameType::FlatBox);
        header.set_color(theme.button_bg);

        let mut cmd_label = Frame::default()
            .with_pos(PADDING, 4)
            .with_size(LABEL_WIDTH, 20)
            .with_label("Command");
        cmd_label.set_label_color(theme.text);
        cmd_label.set_align(Align::Left | Align::Inside);
        cmd_label.set_label_size(12);

        let mut sc_label = Frame::default()
            .with_pos(LABEL_WIDTH + PADDING, 4)
            .with_size(INPUT_WIDTH, 20)
            .with_label("Shortcut");
        sc_label.set_label_color(theme.text);
        sc_label.set_align(Align::Left | Align::Inside);
        sc_label.set_label_size(12);

        header.end();
    }

    // Add built-in shortcut rows
    for &(id, default) in BUILTIN_SHORTCUTS {
        // Skip tab-only shortcuts when tabs disabled
        if !tabs_enabled && matches!(id, "File/Close Tab" | "File/Next Tab" | "File/Previous Tab") {
            continue;
        }

        let actual_default = if id == "File/New" && !tabs_enabled { "Ctrl+N" } else { default };
        let effective = registry.effective_shortcut(id, actual_default);

        let row = create_shortcut_row(
            id,
            actual_default,
            effective,
            &theme,
            scroll_w,
            all_entries.clone(),
        );
        pack_builtin.add(&row);
    }

    pack_builtin.end();
    scroll_builtin.end();
    builtin_group.end();

    // ============ PLUGINS TAB ============
    let mut plugins_group = Group::default()
        .with_pos(PADDING, content_y)
        .with_size(DIALOG_WIDTH - PADDING * 2, content_h)
        .with_label("Plugins");
    plugins_group.set_label_color(theme.text);
    plugins_group.set_color(theme.tab_active_bg);

    let mut scroll_plugins = Scroll::default()
        .with_pos(PADDING + 5, content_y + 5)
        .with_size(scroll_w, content_h - 10);
    scroll_plugins.set_color(theme.bg);

    let mut pack_plugins = Pack::default()
        .with_pos(PADDING + 5, content_y + 5)
        .with_size(scroll_w, 0);
    pack_plugins.set_type(PackType::Vertical);
    pack_plugins.set_spacing(2);

    let plugin_list = plugins.list_plugins();
    let has_plugin_shortcuts = plugin_list.iter().any(|p| !p.menu_items.is_empty());

    if !has_plugin_shortcuts {
        let mut empty = Frame::default()
            .with_size(scroll_w, 40)
            .with_label("No plugins with shortcuts installed");
        empty.set_label_color(theme.text_dim);
    } else {
        for plugin in plugin_list {
            if plugin.menu_items.is_empty() {
                continue;
            }

            // Section header for plugin name
            let mut section = Group::default().with_size(scroll_w, SECTION_HEADER_HEIGHT);
            section.set_frame(FrameType::FlatBox);
            section.set_color(theme.button_bg);

            let mut section_label = Frame::default()
                .with_pos(PADDING, 4)
                .with_size(scroll_w - PADDING * 2, 20)
                .with_label(&plugin.name);
            section_label.set_label_color(theme.text);
            section_label.set_align(Align::Left | Align::Inside);
            section_label.set_label_size(12);

            section.end();
            pack_plugins.add(&section);

            // Add each action
            for item in &plugin.menu_items {
                let cmd_id = plugin_command_id(&plugin.name, &item.action);
                let manifest_default = item.shortcut.as_deref().unwrap_or("");
                let effective = registry.effective_shortcut(&cmd_id, manifest_default);

                let row = create_shortcut_row(
                    &cmd_id,
                    manifest_default,
                    effective,
                    &theme,
                    scroll_w,
                    all_entries.clone(),
                );
                pack_plugins.add(&row);
            }
        }
    }

    pack_plugins.end();
    scroll_plugins.end();
    plugins_group.end();

    tabs.end();
    tabs.auto_layout();

    // ============ ERROR + BUTTONS ============
    let error_y = DIALOG_HEIGHT - PADDING - BUTTON_HEIGHT - 25;
    let mut error_label = Frame::default()
        .with_pos(PADDING, error_y)
        .with_size(DIALOG_WIDTH - PADDING * 2, 18)
        .with_label("");
    error_label.set_label_color(error_color);
    error_label.set_label_size(11);
    error_label.set_align(Align::Left | Align::Inside);
    error_label.hide();

    let btn_y = DIALOG_HEIGHT - PADDING - BUTTON_HEIGHT;

    let mut save_btn = Button::default()
        .with_pos(DIALOG_WIDTH - PADDING - 165, btn_y)
        .with_size(75, BUTTON_HEIGHT)
        .with_label("Save");
    save_btn.set_frame(FrameType::RFlatBox);
    save_btn.set_color(theme.button_bg);
    save_btn.set_label_color(theme.text);

    let mut cancel_btn = Button::default()
        .with_pos(DIALOG_WIDTH - PADDING - 80, btn_y)
        .with_size(75, BUTTON_HEIGHT)
        .with_label("Cancel");
    cancel_btn.set_frame(FrameType::RFlatBox);
    cancel_btn.set_color(theme.button_bg);
    cancel_btn.set_label_color(theme.text);

    dialog.end();

    // Save callback
    let entries_save = all_entries.clone();
    let result_save = result.clone();
    let mut dialog_save = dialog.clone();
    let error_label = Rc::new(RefCell::new(error_label));
    let error_label_save = error_label.clone();

    save_btn.set_callback(move |_| {
        let entries = entries_save.borrow();

        // Validate all entries and check for conflicts
        let mut seen: HashMap<String, String> = HashMap::new(); // normalized -> id

        for entry in entries.iter() {
            let value = entry.input.value();
            let trimmed = value.trim();

            if trimmed.is_empty() {
                continue;
            }

            if !is_valid_shortcut(trimmed) {
                let mut err = error_label_save.borrow_mut();
                err.set_label(&format!(
                    "Invalid shortcut '{}' for {}",
                    trimmed,
                    display_name(&entry.id)
                ));
                err.show();
                err.redraw();
                return;
            }

            let normalized = normalize_shortcut(trimmed);
            if let Some(other_id) = seen.get(&normalized) {
                let mut err = error_label_save.borrow_mut();
                err.set_label(&format!(
                    "Conflict: '{}' used by both {} and {}",
                    trimmed,
                    display_name(other_id),
                    display_name(&entry.id)
                ));
                err.show();
                err.redraw();
                return;
            }
            seen.insert(normalized, entry.id.clone());
        }

        // Build overrides map (only entries that differ from their default)
        let mut overrides = HashMap::new();
        for entry in entries.iter() {
            let value = entry.input.value();
            let trimmed = value.trim().to_string();

            if trimmed != entry.default {
                overrides.insert(
                    entry.id.clone(),
                    crate::app::domain::settings::ShortcutOverride {
                        shortcut: trimmed,
                        enabled: true,
                    },
                );
            }
        }

        *result_save.borrow_mut() = Some(ShortcutDialogResult { overrides });
        dialog_save.hide();
    });

    let mut dialog_cancel = dialog.clone();
    cancel_btn.set_callback(move |_| {
        dialog_cancel.hide();
    });

    dialog.show();
    super::run_dialog(&dialog);

    result.borrow_mut().take()
}

/// Create a single shortcut row: [Label] [Input] [Reset]
fn create_shortcut_row(
    id: &str,
    default: &str,
    effective: &str,
    theme: &DialogTheme,
    row_width: i32,
    entries: Rc<RefCell<Vec<ShortcutEntry>>>,
) -> Group {
    let mut row = Group::default().with_size(row_width, ROW_HEIGHT);
    row.set_frame(FrameType::FlatBox);
    row.set_color(theme.row_bg);

    // Command label
    let label_text = display_name(id);
    let mut label = Frame::default()
        .with_pos(PADDING, 6)
        .with_size(LABEL_WIDTH, 20)
        .with_label(&label_text);
    label.set_label_color(theme.text);
    label.set_align(Align::Left | Align::Inside);
    label.set_label_size(12);

    // Shortcut input
    let input_x = LABEL_WIDTH + PADDING;
    let mut input = Input::default()
        .with_pos(input_x, 4)
        .with_size(INPUT_WIDTH, 24);
    input.set_value(effective);
    input.set_color(theme.input_bg);
    input.set_text_color(theme.text);
    input.set_text_size(12);

    // Reset button
    let reset_x = input_x + INPUT_WIDTH + 8;
    let mut reset_btn = Button::default()
        .with_pos(reset_x, 4)
        .with_size(RESET_BTN_WIDTH, 24)
        .with_label("Reset");
    reset_btn.set_frame(FrameType::RFlatBox);
    reset_btn.set_color(theme.button_bg);
    reset_btn.set_label_color(theme.text);
    reset_btn.set_label_size(11);

    // Reset callback
    let default_str = default.to_string();
    let mut input_reset = input.clone();
    reset_btn.set_callback(move |_| {
        input_reset.set_value(&default_str);
    });

    // Track this entry
    entries.borrow_mut().push(ShortcutEntry {
        id: id.to_string(),
        default: default.to_string(),
        input: input.clone(),
    });

    row.end();
    row
}

/// Convert a command ID to a human-readable display name.
/// "File/Save" -> "File > Save"
/// "plugin:python-lint:lint" -> "Python Lint > Lint"
fn display_name(id: &str) -> String {
    if let Some(rest) = id.strip_prefix("plugin:") {
        // plugin:name:action
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if parts.len() == 2 {
            let plugin_name = parts[0]
                .split('-')
                .map(|w| {
                    let mut chars = w.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            let action = parts[1]
                .split('_')
                .map(|w| {
                    let mut chars = w.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("{} > {}", plugin_name, action)
        } else {
            rest.to_string()
        }
    } else {
        // Built-in: "File/Save" -> "File > Save"
        id.replace('/', " > ")
    }
}
