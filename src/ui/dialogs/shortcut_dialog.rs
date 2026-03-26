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
    app,
    button::Button,
    enums::{Align, Color, Event, FrameType, Key},
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
use crate::app::services::shortcut_registry::{ShortcutRegistry, normalize_shortcut};
use crate::ui::menu::{BUILTIN_SHORTCUTS, is_valid_shortcut, plugin_command_id};

use super::{DialogTheme, SCROLLBAR_SIZE};

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

    // Dynamic row width: count visible rows to decide if scrollbar will appear
    let builtin_count = BUILTIN_SHORTCUTS
        .iter()
        .filter(|&&(id, _)| {
            tabs_enabled || !matches!(id, "File/Close Tab" | "File/Next Tab" | "File/Previous Tab")
        })
        .count()
        + 1; // +1 for header
    let builtin_total_h = builtin_count as i32 * (ROW_HEIGHT + 2); // 2 = pack spacing
    let row_w_builtin = if builtin_total_h > content_h - 10 {
        scroll_w - SCROLLBAR_SIZE
    } else {
        scroll_w
    };

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
    theme.style_scroll(&mut scroll_builtin);

    let mut pack_builtin = Pack::default()
        .with_pos(PADDING + 5, content_y + 5)
        .with_size(row_w_builtin, 0);
    pack_builtin.set_type(PackType::Vertical);
    pack_builtin.set_spacing(2);

    // Add header row
    {
        let mut header = Group::default().with_size(row_w_builtin, SECTION_HEADER_HEIGHT);
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

        let actual_default = if id == "File/New" && !tabs_enabled {
            "Ctrl+N"
        } else {
            default
        };
        let effective = registry.effective_shortcut(id, actual_default);

        let row = create_shortcut_row(
            id,
            actual_default,
            effective,
            &theme,
            row_w_builtin,
            all_entries.clone(),
        );
        pack_builtin.add(&row);
    }

    pack_builtin.end();
    scroll_builtin.end();
    builtin_group.end();

    // ============ PLUGINS TAB ============
    // Dynamic row width for plugins tab
    let plugin_list = plugins.list_plugins();
    let has_plugin_shortcuts = plugin_list.iter().any(|p| !p.menu_items.is_empty());
    let plugins_row_count = if has_plugin_shortcuts {
        plugin_list
            .iter()
            .filter(|p| !p.menu_items.is_empty())
            .map(|p| 1 + p.menu_items.len()) // section header + action rows
            .sum::<usize>()
    } else {
        1 // "No plugins" label
    };
    let plugins_total_h = plugins_row_count as i32 * (ROW_HEIGHT + 2);
    let row_w_plugins = if plugins_total_h > content_h - 10 {
        scroll_w - SCROLLBAR_SIZE
    } else {
        scroll_w
    };

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
    theme.style_scroll(&mut scroll_plugins);

    let mut pack_plugins = Pack::default()
        .with_pos(PADDING + 5, content_y + 5)
        .with_size(row_w_plugins, 0);
    pack_plugins.set_type(PackType::Vertical);
    pack_plugins.set_spacing(2);

    if !has_plugin_shortcuts {
        let mut empty = Frame::default()
            .with_size(row_w_plugins, 40)
            .with_label("No plugins with shortcuts installed");
        empty.set_label_color(theme.text_dim);
    } else {
        for plugin in plugin_list {
            if plugin.menu_items.is_empty() {
                continue;
            }

            // Section header for plugin name
            let mut section = Group::default().with_size(row_w_plugins, SECTION_HEADER_HEIGHT);
            section.set_frame(FrameType::FlatBox);
            section.set_color(theme.button_bg);

            let mut section_label = Frame::default()
                .with_pos(PADDING, 4)
                .with_size(row_w_plugins - PADDING * 2, 20)
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
                    row_w_plugins,
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

/// Convert an FLTK `Key` to its display name, or `None` for modifier-only / unsupported keys.
fn key_name(key: Key) -> Option<String> {
    // Function keys F1–F12
    for i in 1..=12u32 {
        if key == Key::from_i32(0xffbe + (i as i32 - 1)) {
            return Some(format!("F{}", i));
        }
    }

    // Printable ASCII range: letters, digits, punctuation
    let bits = key.bits();
    if (0x20..=0x7e).contains(&bits) {
        let ch = bits as u8 as char;
        if ch.is_ascii_alphanumeric() {
            return Some(ch.to_ascii_uppercase().to_string());
        }
        // Map punctuation to names
        return match ch {
            ' ' => Some("Space".to_string()),
            '-' => Some("Minus".to_string()),
            '=' => Some("Equal".to_string()),
            '[' => Some("LeftBracket".to_string()),
            ']' => Some("RightBracket".to_string()),
            '\\' => Some("Backslash".to_string()),
            ';' => Some("Semicolon".to_string()),
            '\'' => Some("Quote".to_string()),
            ',' => Some("Comma".to_string()),
            '.' => Some("Period".to_string()),
            '/' => Some("Slash".to_string()),
            '`' => Some("Backtick".to_string()),
            _ => None,
        };
    }

    // Arrow keys
    if key == Key::Left {
        return Some("Left".to_string());
    }
    if key == Key::Right {
        return Some("Right".to_string());
    }
    if key == Key::Up {
        return Some("Up".to_string());
    }
    if key == Key::Down {
        return Some("Down".to_string());
    }
    if key == Key::Home {
        return Some("Home".to_string());
    }
    if key == Key::End {
        return Some("End".to_string());
    }
    if key == Key::PageUp {
        return Some("PageUp".to_string());
    }
    if key == Key::PageDown {
        return Some("PageDown".to_string());
    }
    if key == Key::Insert {
        return Some("Insert".to_string());
    }
    if key == Key::Tab {
        return Some("Tab".to_string());
    }
    if key == Key::Escape {
        return Some("Escape".to_string());
    }
    if key == Key::BackSpace {
        return Some("Backspace".to_string());
    }
    if key == Key::Delete {
        return Some("Delete".to_string());
    }

    // Modifier-only and unsupported keys return None
    None
}

/// Build a shortcut string from the current FLTK key event, e.g. "Ctrl+Shift+P".
/// Returns `None` if the key is unsupported or no modifier is held (bare keys aren't valid shortcuts,
/// except for F-keys which are allowed without modifiers).
fn shortcut_string_from_event() -> Option<String> {
    let key = app::event_key();
    let name = key_name(key)?;

    let state = app::event_state();
    let ctrl = state.contains(fltk::enums::Shortcut::Ctrl);
    let shift = state.contains(fltk::enums::Shortcut::Shift);
    let alt = state.contains(fltk::enums::Shortcut::Alt);

    // Require at least one modifier, unless it's an F-key
    let is_fkey =
        name.starts_with('F') && name.len() >= 2 && name[1..].chars().all(|c| c.is_ascii_digit());
    if !ctrl && !shift && !alt && !is_fkey {
        return None;
    }

    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if shift {
        parts.push("Shift");
    }
    if alt {
        parts.push("Alt");
    }
    parts.push(&name);

    Some(parts.join("+"))
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
    input.set_selection_color(theme.button_bg);
    input.set_text_size(12);

    // Capture key combinations directly
    input.handle(move |inp, ev| {
        if ev == Event::KeyDown {
            let key = app::event_key();
            let state = app::event_state();
            let has_modifier = state.contains(fltk::enums::Shortcut::Ctrl)
                || state.contains(fltk::enums::Shortcut::Alt);

            // Let bare Escape pass through (close dialog)
            if key == Key::Escape && !has_modifier {
                return false;
            }
            // Let bare Tab/Shift+Tab pass through (focus navigation)
            if key == Key::Tab && !has_modifier {
                return false;
            }
            // Bare Backspace/Delete clears the field (unbinds the shortcut)
            if matches!(key, Key::BackSpace | Key::Delete) && !has_modifier {
                inp.set_value("");
                return true;
            }
            if let Some(shortcut_str) = shortcut_string_from_event() {
                inp.set_value(&shortcut_str);
            }
            // Consume all other key events to prevent stray characters
            true
        } else {
            false
        }
    });

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
