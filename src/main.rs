#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;
mod updater;

use fltk::{
    app,
    button::{Button, CheckButton, RadioRoundButton},
    dialog, // for alert_default
    enums::{Color, Font},
    frame::Frame,
    group::{Flex, Group},
    image::PngImage,
    input::Input,
    menu::MenuBar,
    misc::Progress,
    prelude::*,
    text::{TextBuffer, TextEditor, WrapMode},
    window::Window,
};
use crate::updater::UpdateChannel;
use std::cell::RefCell;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::Command;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::{env, fs};

use fltk::dialog::{FileDialogType, NativeFileChooser};
use settings::{AppSettings, FontChoice, ThemeMode};

// AppSettings is now in settings.rs module

fn detect_system_dark_mode() -> bool {
    // Windows: Check registry for dark mode preference
    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;

        if let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        {
            // AppsUseLightTheme: 0 = dark mode, 1 = light mode
            if let Ok(value) = hkcu.get_value::<u32, _>("AppsUseLightTheme") {
                return value == 0;
            }
        }
    }

    // Linux: Try to detect system theme on GNOME
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("gsettings")
            .args(&["get", "org.gnome.desktop.interface", "gtk-theme"])
            .output()
        {
            let theme = String::from_utf8_lossy(&output.stdout).to_lowercase();
            if theme.contains("dark") {
                return true;
            }
        }

        // Try alternative method for other desktop environments
        if let Ok(output) = Command::new("gsettings")
            .args(&["get", "org.gnome.desktop.interface", "color-scheme"])
            .output()
        {
            let scheme = String::from_utf8_lossy(&output.stdout);
            if scheme.contains("prefer-dark") {
                return true;
            }
        }
    }

    // macOS: Could add detection here in the future
    // For now, macOS defaults to light mode

    // Default to light mode if detection fails
    false
}

/// Set Windows title bar theme (Windows 10 build 1809+)
/// Must be called AFTER window.show() to have a valid HWND
#[cfg(target_os = "windows")]
fn set_windows_titlebar_theme(window: &Window, is_dark: bool) {
    use std::mem::size_of;
    use std::ptr::from_ref;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};

    unsafe {
        // Construct HWND - cast to pointer then transmute to avoid type mismatch
        let hwnd = HWND(window.raw_handle() as *mut std::ffi::c_void);

        let on: i32 = if is_dark { 1 } else { 0 };

        // Try attribute 20 (Windows 11 / Windows 10 2004+)
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(20), // DWMWA_USE_IMMERSIVE_DARK_MODE
            from_ref(&on).cast(),
            size_of::<i32>() as u32,
        );

        // Also try attribute 19 (Windows 10 1809–1903)
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(19),
            from_ref(&on).cast(),
            size_of::<i32>() as u32,
        );
    }
}

fn apply_theme(
    editor: &mut TextEditor,
    window: &mut Window,
    menu: &mut MenuBar,
    banner: Option<&mut Frame>,
    is_dark: bool,
) {
    if is_dark {
        // Dark mode colors
        editor.set_color(Color::from_rgb(30, 30, 30));
        editor.set_text_color(Color::from_rgb(220, 220, 220));
        editor.set_cursor_color(Color::from_rgb(255, 255, 255));
        editor.set_selection_color(Color::from_rgb(70, 70, 100));
        editor.set_linenumber_bgcolor(Color::from_rgb(40, 40, 40));
        editor.set_linenumber_fgcolor(Color::from_rgb(150, 150, 150));
        window.set_color(Color::from_rgb(25, 25, 25));
        window.set_label_color(Color::from_rgb(220, 220, 220));
        menu.set_color(Color::from_rgb(35, 35, 35));
        menu.set_text_color(Color::from_rgb(220, 220, 220));
        menu.set_selection_color(Color::from_rgb(60, 60, 60)); // Hover color
        if let Some(b) = banner {
            b.set_color(Color::from_rgb(139, 128, 0)); // Darker yellow/olive
            b.set_label_color(Color::White);
        }
    } else {
        // Light mode colors
        editor.set_color(Color::White);
        editor.set_text_color(Color::Black);
        editor.set_cursor_color(Color::Black);
        editor.set_selection_color(Color::from_rgb(173, 216, 230));
        editor.set_linenumber_bgcolor(Color::from_rgb(240, 240, 240));
        editor.set_linenumber_fgcolor(Color::from_rgb(100, 100, 100));
        window.set_color(Color::from_rgb(240, 240, 240));
        window.set_label_color(Color::Black);
        menu.set_color(Color::from_rgb(240, 240, 240));
        menu.set_text_color(Color::Black);
        menu.set_selection_color(Color::from_rgb(200, 200, 200)); // Hover color
        if let Some(b) = banner {
            b.set_color(Color::from_rgb(255, 250, 205)); // Lemon chiffon
            b.set_label_color(Color::Black);
        }
    }

    editor.redraw();
    window.redraw();
    menu.redraw();
}

/// Get filter pattern for text file formats with multiple options
///
/// Returns a multi-line filter string where each line is a separate filter option.
/// FLTK format: "Description\tPattern\nDescription2\tPattern2"
/// Note: FLTK automatically adds "All Files (*)" option, so we don't include it
fn get_text_files_filter_multiline() -> String {
    vec![
        "Text Files\t*.txt",
        "Markdown Files\t*.{md,markdown}",
        "Rust Files\t*.rs",
        "Python Files\t*.py",
        "JavaScript Files\t*.{js,jsx,ts,tsx}",
        "Config Files\t*.{json,yaml,yml,toml,ini,cfg,conf}",
        "Web Files\t*.{html,css,scss,sass}",
    ].join("\n")
}

/// Get filter pattern for all files (used in Save dialogs)
fn get_all_files_filter() -> String {
    "*".to_string()
}

/// Extract filename from a file path
///
/// Returns the filename component of a path, or "Unknown" if it can't be extracted.
fn extract_filename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty() && *s != ".")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Find next occurrence of search string in text
///
/// Returns the byte position of the match, or None if not found.
/// Searches from start_pos onwards.
fn find_in_text(text: &str, search: &str, start_pos: usize, case_sensitive: bool) -> Option<usize> {
    if search.is_empty() || start_pos >= text.len() {
        return None;
    }

    let haystack = if case_sensitive {
        text[start_pos..].to_string()
    } else {
        text[start_pos..].to_lowercase()
    };

    let needle = if case_sensitive {
        search.to_string()
    } else {
        search.to_lowercase()
    };

    haystack.find(&needle).map(|pos| start_pos + pos)
}

/// Replace all occurrences of search string with replacement
///
/// Returns (new_text, count_of_replacements)
fn replace_all_in_text(text: &str, search: &str, replace: &str, case_sensitive: bool) -> (String, usize) {
    if search.is_empty() {
        return (text.to_string(), 0);
    }

    let mut result = text.to_string();
    let mut count = 0;
    let mut pos = 0;

    while let Some(found_pos) = find_in_text(&result, search, pos, case_sensitive) {
        // Get the actual matched text (preserves original case)
        let matched_text = &result[found_pos..found_pos + search.len()];

        // Replace this occurrence
        result.replace_range(found_pos..found_pos + matched_text.len(), replace);

        // Move position forward by replacement length
        pos = found_pos + replace.len();
        count += 1;

        // Prevent infinite loop if replace contains search
        if replace.contains(search) && pos >= result.len() {
            break;
        }
    }

    (result, count)
}

/// Show Find & Replace dialog
fn show_replace_dialog(buffer: &TextBuffer, editor: &mut TextEditor) {
    let mut dialog = Window::default()
        .with_size(400, 220)
        .with_label("Find & Replace")
        .center_screen();

    Frame::default().with_pos(20, 20).with_size(80, 30).with_label("Find what:");
    let find_input = Input::default().with_pos(110, 20).with_size(270, 30);

    Frame::default().with_pos(20, 60).with_size(80, 30).with_label("Replace:");
    let replace_input = Input::default().with_pos(110, 60).with_size(270, 30);

    let case_check = CheckButton::default()
        .with_pos(110, 100).with_size(200, 25).with_label("Match case");

    let mut find_btn = Button::default()
        .with_pos(20, 140).with_size(90, 30).with_label("Find Next");
    let mut replace_btn = Button::default()
        .with_pos(120, 140).with_size(90, 30).with_label("Replace");
    let mut replace_all_btn = Button::default()
        .with_pos(220, 140).with_size(100, 30).with_label("Replace All");
    let mut close_btn = Button::default()
        .with_pos(300, 180).with_size(90, 30).with_label("Close");

    dialog.end();
    dialog.make_resizable(false);
    dialog.show();

    let search_text = Rc::new(RefCell::new(String::new()));
    let search_pos = Rc::new(RefCell::new(0usize));
    let text_buf = buffer.clone();
    let text_ed = editor.clone();

    // Find Next button
    let st = search_text.clone();
    let sp = search_pos.clone();
    let mut tb1 = text_buf.clone();
    let mut te1 = text_ed.clone();
    let find_input1 = find_input.clone();
    let case_check1 = case_check.clone();

    find_btn.set_callback(move |_| {
        let query = find_input1.value();
        if query.is_empty() {
            dialog::message_default("Please enter text to find");
            return;
        }

        let text = tb1.text();
        let case_sensitive = case_check1.is_checked();

        // If search text changed, start from current cursor position
        let start_pos = if *st.borrow() != query {
            *st.borrow_mut() = query.clone();
            let cursor = te1.insert_position() as usize;
            *sp.borrow_mut() = cursor;
            cursor
        } else {
            *sp.borrow()
        };

        if let Some(pos) = find_in_text(&text, &query, start_pos, case_sensitive) {
            // Select the found text
            tb1.select(pos as i32, (pos + query.len()) as i32);
            te1.set_insert_position((pos + query.len()) as i32);
            te1.show_insert_position();

            // Update search position for next search
            *sp.borrow_mut() = pos + query.len();
        } else {
            // Not found, wrap around
            if start_pos > 0 {
                *sp.borrow_mut() = 0;
                dialog::message_default("No more matches. Wrapped to beginning.");
            } else {
                dialog::message_default(&format!("Cannot find '{}'", query));
            }
        }
    });

    // Replace button
    let sp2 = search_pos.clone();
    let mut tb2 = text_buf.clone();
    let mut te2 = text_ed.clone();
    let find_input2 = find_input.clone();
    let replace_input2 = replace_input.clone();
    let case_check2 = case_check.clone();
    let mut find_btn2 = find_btn.clone();

    replace_btn.set_callback(move |_| {
        let query = find_input2.value();
        let replacement = replace_input2.value();

        if query.is_empty() {
            dialog::message_default("Please enter text to find");
            return;
        }

        // Check if there's a current selection matching the search
        if let Some((start, end)) = tb2.selection_position() {
            if start != end {
                let selected = tb2.selection_text();
                let case_sensitive = case_check2.is_checked();

                let matches = if case_sensitive {
                    selected == query
                } else {
                    selected.to_lowercase() == query.to_lowercase()
                };

                if matches {
                    // Replace the selection
                    tb2.replace_selection(&replacement);
                    te2.set_insert_position(start + replacement.len() as i32);

                    // Find next occurrence
                    *sp2.borrow_mut() = (start as usize) + replacement.len();
                }
            }
        }

        // Now find next
        find_btn2.do_callback();
    });

    // Replace All button
    let mut tb3 = text_buf.clone();
    let mut te3 = text_ed.clone();
    let find_input3 = find_input.clone();
    let replace_input3 = replace_input.clone();
    let case_check3 = case_check.clone();

    replace_all_btn.set_callback(move |_| {
        let query = find_input3.value();
        let replacement = replace_input3.value();

        if query.is_empty() {
            dialog::message_default("Please enter text to find");
            return;
        }

        let text = tb3.text();
        let case_sensitive = case_check3.is_checked();

        let (new_text, count) = replace_all_in_text(&text, &query, &replacement, case_sensitive);

        if count > 0 {
            tb3.set_text(&new_text);
            te3.set_insert_position(0);
            dialog::message_default(&format!("Replaced {} occurrence(s)", count));
        } else {
            dialog::message_default(&format!("Cannot find '{}'", query));
        }
    });

    let dialog_close = dialog.clone();
    close_btn.set_callback(move |_| {
        dialog_close.clone().hide();
    });

    let dialog_x = dialog.clone();
    dialog.set_callback(move |_| {
        dialog_x.clone().hide();
    });

    while dialog.shown() {
        app::wait();
    }
}

/// Show Find dialog
fn show_find_dialog(buffer: &TextBuffer, editor: &mut TextEditor) {
    let mut dialog = Window::default()
        .with_size(400, 150)
        .with_label("Find")
        .center_screen();

    Frame::default().with_pos(20, 20).with_size(80, 30).with_label("Find what:");
    let find_input = Input::default().with_pos(110, 20).with_size(270, 30);

    let case_check = CheckButton::default()
        .with_pos(110, 60).with_size(200, 25).with_label("Match case");

    let mut find_btn = Button::default()
        .with_pos(200, 100).with_size(90, 30).with_label("Find Next");
    let mut close_btn = Button::default()
        .with_pos(300, 100).with_size(90, 30).with_label("Close");

    dialog.end();
    dialog.make_resizable(false);
    dialog.show();

    let search_text = Rc::new(RefCell::new(String::new()));
    let search_pos = Rc::new(RefCell::new(0usize));
    let mut text_buf = buffer.clone();
    let mut text_ed = editor.clone();

    let st = search_text.clone();
    let sp = search_pos.clone();

    find_btn.set_callback(move |_| {
        let query = find_input.value();
        if query.is_empty() {
            dialog::message_default("Please enter text to find");
            return;
        }

        let text = text_buf.text();
        let case_sensitive = case_check.is_checked();

        // If search text changed, start from beginning
        let start_pos = if *st.borrow() != query {
            *st.borrow_mut() = query.clone();
            *sp.borrow_mut() = 0;
            0
        } else {
            *sp.borrow()
        };

        if let Some(pos) = find_in_text(&text, &query, start_pos, case_sensitive) {
            // Select the found text
            text_buf.select(pos as i32, (pos + query.len()) as i32);
            text_ed.set_insert_position((pos + query.len()) as i32);
            text_ed.show_insert_position();

            // Update search position for next search
            *sp.borrow_mut() = pos + query.len();
        } else {
            // Not found, wrap around
            if start_pos > 0 {
                *sp.borrow_mut() = 0;
                dialog::message_default("No more matches. Wrapped to beginning.");
            } else {
                dialog::message_default(&format!("Cannot find '{}'", query));
            }
        }
    });

    let dialog_close = dialog.clone();
    close_btn.set_callback(move |_| {
        dialog_close.clone().hide();
    });

    let dialog_x = dialog.clone();
    dialog.set_callback(move |_| {
        dialog_x.clone().hide();
    });

    while dialog.shown() {
        app::wait();
    }
}

/// Show settings dialog and return updated settings if user clicked Save
fn show_settings_dialog(current_settings: &AppSettings) -> Option<AppSettings> {
    let mut dialog = Window::default()
        .with_size(350, 610)
        .with_label("Settings")
        .center_screen();
    dialog.make_modal(true);

    let vpack = Group::default()
        .with_size(320, 500)
        .with_pos(15, 15);

    // Theme section
    Frame::default().with_pos(15, 15).with_size(320, 25).with_label("Theme:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let theme_group = Group::default().with_pos(30, 45).with_size(280, 75);
    let mut theme_light = RadioRoundButton::default().with_pos(30, 45).with_size(280, 25).with_label("Light");
    let mut theme_dark = RadioRoundButton::default().with_pos(30, 70).with_size(280, 25).with_label("Dark");
    let mut theme_system = RadioRoundButton::default().with_pos(30, 95).with_size(280, 25).with_label("System Default");
    theme_group.end();

    match current_settings.theme_mode {
        ThemeMode::Light => theme_light.set_value(true),
        ThemeMode::Dark => theme_dark.set_value(true),
        ThemeMode::SystemDefault => theme_system.set_value(true),
    }

    // Font section
    Frame::default().with_pos(15, 130).with_size(320, 25).with_label("Font:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let font_group = Group::default().with_pos(30, 160).with_size(280, 75);
    let mut font_screenbold = RadioRoundButton::default().with_pos(30, 160).with_size(280, 25).with_label("Screen (Bold)");
    let mut font_courier = RadioRoundButton::default().with_pos(30, 185).with_size(280, 25).with_label("Courier");
    let mut font_helvetica = RadioRoundButton::default().with_pos(30, 210).with_size(280, 25).with_label("Helvetica Mono");
    font_group.end();

    match current_settings.font {
        FontChoice::ScreenBold => font_screenbold.set_value(true),
        FontChoice::Courier => font_courier.set_value(true),
        FontChoice::HelveticaMono => font_helvetica.set_value(true),
    }

    // Font size section
    Frame::default().with_pos(15, 245).with_size(320, 25).with_label("Font Size:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let size_group = Group::default().with_pos(30, 275).with_size(280, 75);
    let mut size_12 = RadioRoundButton::default().with_pos(30, 275).with_size(280, 25).with_label("Small (12)");
    let mut size_16 = RadioRoundButton::default().with_pos(30, 300).with_size(280, 25).with_label("Medium (16)");
    let mut size_20 = RadioRoundButton::default().with_pos(30, 325).with_size(280, 25).with_label("Large (20)");
    size_group.end();

    match current_settings.font_size {
        12 => size_12.set_value(true),
        16 => size_16.set_value(true),
        20 => size_20.set_value(true),
        _ => size_16.set_value(true),
    }

    // View options section
    Frame::default().with_pos(15, 360).with_size(320, 25).with_label("View Options:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let mut check_line_numbers = CheckButton::default().with_pos(30, 390).with_size(280, 25).with_label("Show Line Numbers");
    let mut check_word_wrap = CheckButton::default().with_pos(30, 415).with_size(280, 25).with_label("Word Wrap");

    check_line_numbers.set_value(current_settings.line_numbers_enabled);
    check_word_wrap.set_value(current_settings.word_wrap_enabled);

    // Updates section
    Frame::default().with_pos(15, 450).with_size(320, 25).with_label("Updates:").with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    let mut check_auto_update = CheckButton::default().with_pos(30, 480).with_size(280, 25).with_label("Automatically check for updates");
    check_auto_update.set_value(current_settings.auto_check_updates);

    let mut check_prerelease = CheckButton::default().with_pos(30, 505).with_size(280, 25).with_label("Include pre-releases (beta/rc)");
    check_prerelease.set_value(current_settings.update_channel == UpdateChannel::Beta);

    // Info text
    let mut info_frame = Frame::default().with_pos(30, 535).with_size(290, 35);
    info_frame.set_label("FerrisPad checks GitHub once per day.\nNo personal data is sent.");
    info_frame.set_label_size(11);
    info_frame.set_label_color(Color::from_rgb(100, 100, 100));
    info_frame.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside | fltk::enums::Align::Wrap);

    vpack.end();

    // Buttons at bottom
    let mut save_btn = Button::default().with_pos(150, 570).with_size(90, 30).with_label("Save");
    let mut cancel_btn = Button::default().with_pos(250, 570).with_size(90, 30).with_label("Cancel");

    dialog.end();
    dialog.show();

    let result = Rc::new(RefCell::new(None));
    let result_save = result.clone();
    let result_cancel = result.clone();

    let dialog_save = dialog.clone();
    let current = current_settings.clone();
    save_btn.set_callback(move |_| {
        let new_settings = AppSettings {
            theme_mode: if theme_light.value() {
                ThemeMode::Light
            } else if theme_dark.value() {
                ThemeMode::Dark
            } else {
                ThemeMode::SystemDefault
            },
            font: if font_screenbold.value() {
                FontChoice::ScreenBold
            } else if font_courier.value() {
                FontChoice::Courier
            } else {
                FontChoice::HelveticaMono
            },
            font_size: if size_12.value() {
                12
            } else if size_20.value() {
                20
            } else {
                16
            },
            line_numbers_enabled: check_line_numbers.value(),
            word_wrap_enabled: check_word_wrap.value(),
            // Update settings from UI
            auto_check_updates: check_auto_update.value(),
            update_channel: if check_prerelease.value() {
                UpdateChannel::Beta
            } else {
                UpdateChannel::Stable
            },
            last_update_check: current.last_update_check,
            skipped_versions: current.skipped_versions.clone(),
        };

        *result_save.borrow_mut() = Some(new_settings);
        dialog_save.clone().hide();
    });

    let dialog_cancel = dialog.clone();
    cancel_btn.set_callback(move |_| {
        *result_cancel.borrow_mut() = None;
        dialog_cancel.clone().hide();
    });

    // Handle window close button (X) - just hide the dialog
    let result_close = result.clone();
    dialog.set_callback(move |w| {
        // Don't propagate to parent - just hide
        *result_close.borrow_mut() = None;
        w.hide();
    });

    while dialog.shown() {
        app::wait();
    }

    result.borrow().clone()
}

/// Generate platform-specific file filter string for native dialogs
///
/// FLTK accepts these filter formats:
/// - Simple wildcard: "*.txt"
/// - Multiple wildcards: "*.{txt,md,rst}"
/// - With description (optional): "Text Files\t*.txt"
/// - Multiple filters: "Text Files\t*.txt\nMarkdown\t*.md"
///
/// For maximum compatibility, we use the simple format without description.
fn get_platform_filter(_description: &str, pattern: &str) -> String {
    // FLTK handles the platform-specific format internally
    // We just pass the pattern directly
    pattern.to_string()
}

fn native_open_dialog(description: &str, pattern: &str) -> Option<String> {
    let mut nfc = NativeFileChooser::new(FileDialogType::BrowseFile);
    let filter = get_platform_filter(description, pattern);
    nfc.set_filter(&filter);
    nfc.show();
    let filename = nfc.filename();
    let s = filename.to_string_lossy();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

fn native_save_dialog(description: &str, pattern: &str) -> Option<String> {
    let mut nfc = NativeFileChooser::new(FileDialogType::BrowseSaveFile);
    let filter = get_platform_filter(description, pattern);
    nfc.set_filter(&filter);
    nfc.show();
    let filename = nfc.filename();
    let s = filename.to_string_lossy();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

/// Show About dialog
fn show_about_dialog() {
    let version = env!("CARGO_PKG_VERSION");
    let mut dialog = Window::default()
        .with_size(450, 400)
        .with_label("About FerrisPad")
        .center_screen();
    dialog.make_modal(true);

    let mut flex = Flex::new(10, 10, 430, 380, None);
    flex.set_type(fltk::group::FlexType::Column);
    flex.set_spacing(10);

    // App icon/logo
    let mut title = Frame::default();
    title.set_label("🦀 FerrisPad");
    title.set_label_size(24);
    title.set_label_font(Font::HelveticaBold);
    flex.fixed(&title, 40);

    // Version
    let mut version_frame = Frame::default();
    version_frame.set_label(&format!("Version {}", version));
    version_frame.set_label_size(14);
    flex.fixed(&version_frame, 25);

    // Description
    let mut desc_frame = Frame::default();
    desc_frame.set_label("A blazingly fast, minimalist notepad written in Rust");
    desc_frame.set_label_size(12);
    desc_frame.set_label_color(Color::from_rgb(100, 100, 100));
    flex.fixed(&desc_frame, 25);

    // Spacing
    let mut _spacer1 = Frame::default();
    flex.fixed(&_spacer1, 10);

    // Info section
    let info_text = format!(
        "Copyright © 2025 FerrisPad Contributors\n\
         Licensed under the MIT License\n\n\
         Built with Rust 🦀 and FLTK\n\n\
         Website: www.ferrispad.com\n\
         GitHub: github.com/fedro86/ferrispad"
    );

    let mut info_frame = Frame::default();
    info_frame.set_label(&info_text);
    info_frame.set_label_size(12);
    info_frame.set_align(fltk::enums::Align::Center | fltk::enums::Align::Inside);
    flex.fixed(&info_frame, 120);

    // Spacing
    let mut _spacer2 = Frame::default();
    flex.fixed(&_spacer2, 10);

    // Credits
    let mut credits_frame = Frame::default();
    credits_frame.set_label("Made with ❤️ by developers who believe\nsoftware should be fast and simple");
    credits_frame.set_label_size(11);
    credits_frame.set_label_color(Color::from_rgb(100, 100, 100));
    credits_frame.set_align(fltk::enums::Align::Center | fltk::enums::Align::Inside);
    flex.fixed(&credits_frame, 40);

    // Close button
    let mut close_btn = Button::default().with_label("Close");
    flex.fixed(&close_btn, 35);

    flex.end();
    dialog.end();

    let mut dialog_close = dialog.clone();
    close_btn.set_callback(move |_| {
        dialog_close.hide();
    });

    dialog.show();
    while dialog.shown() {
        app::wait();
    }
}

/// Check for updates and show UI dialog (manual check)
fn check_for_updates_ui(settings: &Rc<RefCell<AppSettings>>) {
    use updater::{check_for_updates, current_timestamp, UpdateCheckResult};

    let current_version = env!("CARGO_PKG_VERSION");
    let settings_borrowed = settings.borrow();
    let channel = settings_borrowed.update_channel;
    let skipped = settings_borrowed.skipped_versions.clone();
    drop(settings_borrowed);

    // Perform the check (this may take 1-3 seconds for network call)
    let result = check_for_updates(current_version, channel, &skipped);

    match result {
        UpdateCheckResult::UpdateAvailable(release) => {
            // Show update available dialog
            show_update_available_dialog(release, settings);
        }
        UpdateCheckResult::NoUpdate => {
            dialog::message_default(&format!(
                "✅ You're up to date!\n\nFerrisPad {} is the latest version.",
                current_version
            ));
        }
        UpdateCheckResult::Error(err) => {
            dialog::alert_default(&format!(
                "Failed to check for updates:\n\n{}\n\nPlease try again later.",
                err
            ));
        }
    }

    // Update last check timestamp
    let mut settings_mut = settings.borrow_mut();
    settings_mut.last_update_check = current_timestamp();
    let _ = settings_mut.save();
}

/// Background check for updates on startup (non-blocking)
fn check_for_updates_background(
    settings: Arc<Mutex<AppSettings>>,
    update_banner: Arc<Mutex<Option<updater::ReleaseInfo>>>,
) {
    use updater::{check_for_updates, current_timestamp, should_check_now, UpdateCheckResult};

    // Check if we should run the check
    let should_check = {
        let settings_lock = settings.lock().unwrap();
        settings_lock.auto_check_updates
            && should_check_now(settings_lock.last_update_check)
    };

    if !should_check {
        return; // Don't check if disabled or checked recently
    }

    let current_version = env!("CARGO_PKG_VERSION");
    let (channel, skipped) = {
        let settings_lock = settings.lock().unwrap();
        (
            settings_lock.update_channel,
            settings_lock.skipped_versions.clone(),
        )
    };

    // Perform the check (background, non-blocking)
    let result = check_for_updates(current_version, channel, &skipped);

    match result {
        UpdateCheckResult::UpdateAvailable(release) => {
            // Store the release info to show banner
            *update_banner.lock().unwrap() = Some(release);
            // Trigger UI redraw
            app::awake();
        }
        UpdateCheckResult::NoUpdate | UpdateCheckResult::Error(_) => {
            // Silent - don't bother user on startup
        }
    }

    // Update last check timestamp
    let mut settings_mut = settings.lock().unwrap();
    settings_mut.last_update_check = current_timestamp();
    let _ = settings_mut.save();
}

/// Show update available dialog with options
fn show_update_available_dialog(release: updater::ReleaseInfo, settings: &Rc<RefCell<AppSettings>>) {
    let current_version = env!("CARGO_PKG_VERSION");
    let asset_name = updater::get_platform_asset_name();
    let direct_asset = release.assets.iter().find(|a| a.name.contains(asset_name));

    let mut dialog = Window::new(100, 100, 500, 480, "Update Available");
    dialog.make_modal(true);

    let mut flex = Flex::new(10, 10, 480, 460, None);
    flex.set_type(fltk::group::FlexType::Column);
    flex.set_spacing(10);

    // Title
    let mut title = Frame::default().with_label("🦀 FerrisPad Update Available");
    title.set_label_size(18);
    title.set_label_font(Font::HelveticaBold);
    flex.fixed(&title, 30);

    // Version info
    let version_text = format!(
        "Current version: {}\nLatest version:  {}",
        current_version, release.version()
    );
    let mut version_frame = Frame::default().with_label(&version_text);
    version_frame.set_label_size(14);
    flex.fixed(&version_frame, 50);

    // Release notes
    let mut notes_label = Frame::default().with_label("What's new:");
    notes_label.set_label_size(14);
    notes_label.set_label_font(Font::HelveticaBold);
    flex.fixed(&notes_label, 25);

    let mut notes_editor = TextEditor::default();
    notes_editor.set_buffer(TextBuffer::default());
    notes_editor.buffer().unwrap().set_text(&release.body);
    notes_editor.wrap_mode(WrapMode::AtBounds, 0);

    // Progress bar (initially hidden)
    let mut progress = Progress::default().with_size(0, 25);
    progress.set_minimum(0.0);
    progress.set_maximum(1.0);
    progress.hide();
    flex.fixed(&progress, 25);

    let mut status_frame = Frame::default().with_size(0, 20);
    status_frame.set_label_size(11);
    status_frame.hide();
    flex.fixed(&status_frame, 20);

    // Buttons row
    let mut button_row = Flex::default();
    button_row.set_type(fltk::group::FlexType::Row);
    button_row.set_spacing(10);

    let mut install_btn = Button::default().with_label("Install Now");
    let mut download_btn = Button::default().with_label("View on GitHub");
    let mut skip_btn = Button::default().with_label("Skip This Version");
    let mut later_btn = Button::default().with_label("Remind Later");

    if direct_asset.is_none() {
        install_btn.deactivate();
        install_btn.set_label("Manual Update Only");
    }

    button_row.end();
    flex.fixed(&button_row, 35);

    flex.end();
    dialog.end();

    // Install Now button
    if let Some(asset) = direct_asset.cloned() {
        let mut progress_bar = progress.clone();
        let mut status = status_frame.clone();
        let mut btn_row = button_row.clone();
        install_btn.set_callback(move |_| {
            progress_bar.show();
            status.show();
            status.set_label("Starting download...");
            btn_row.deactivate();

            let download_url = asset.browser_download_url.clone();
            let p_bar = progress_bar.clone();
            let mut s_frame = status.clone();
            let btn_row_thread = btn_row.clone();

            std::thread::spawn(move || {
                let temp_dir = std::env::temp_dir();
                let temp_file = temp_dir.join("ferrispad_update");

                let result = updater::download_file(&download_url, &temp_file, |p| {
                    let mut p_val = p_bar.clone();
                    let mut s_val = s_frame.clone();
                    app::add_timeout3(0.0, move |_| {
                        p_val.set_value(p as f64);
                        s_val.set_label(&format!("Downloading: {:.0}%", p * 100.0));
                    });
                });

                match result {
                    Ok(_) => {
                        app::add_timeout3(0.0, move |_| {
                            s_frame.set_label("Installing update...");
                        });

                        match updater::install_update(&temp_file) {
                            Ok(_) => {
                                app::add_timeout3(0.0, move |_| {
                                    dialog::message_default("Update installed successfully!\n\nFerrisPad will now restart.");
                                    // Restart the app
                                    if let Ok(current_exe) = std::env::current_exe() {
                                        let _ = std::process::Command::new(current_exe).spawn();
                                    }
                                    app::quit();
                                });
                            }
                            Err(e) => {
                                let mut br = btn_row_thread.clone();
                                app::add_timeout3(0.0, move |_| {
                                    dialog::alert_default(&format!("Failed to install update: {}", e));
                                    br.activate();
                                });
                            }
                        }
                    }
                    Err(e) => {
                        let mut br = btn_row_thread.clone();
                        app::add_timeout3(0.0, move |_| {
                            dialog::alert_default(&format!("Failed to download update: {}", e));
                            br.activate();
                        });
                    }
                }
            });
        });
    }

    // View on GitHub button - open browser
    let release_url = release.html_url.clone();
    download_btn.set_callback(move |_| {
        if let Err(e) = open::that(&release_url) {
            dialog::alert_default(&format!("Failed to open browser: {}", e));
        }
    });

    // Skip button - add to skipped versions
    let settings_skip = settings.clone();
    let version_to_skip = release.version();
    let mut dialog_skip = dialog.clone();
    skip_btn.set_callback(move |_| {
        let mut settings_mut = settings_skip.borrow_mut();
        if !settings_mut.skipped_versions.contains(&version_to_skip) {
            settings_mut.skipped_versions.push(version_to_skip.clone());
            let _ = settings_mut.save();
        }
        dialog_skip.hide();
    });

    // Later button - just close
    let mut dialog_later = dialog.clone();
    later_btn.set_callback(move |_| {
        dialog_later.hide();
    });

    dialog.show();
    while dialog.shown() {
        app::wait();
    }
}

fn open_file(path: String, buf_open: &mut TextBuffer, wind_open: &mut Window, changes_open: &Rc<RefCell<bool>>, path_open: &Rc<RefCell<Option<String>>>) {
    match fs::read_to_string(&path) {
        Ok(content) => {
            buf_open.set_text(&content);
            let filename = extract_filename(&path);
            wind_open.set_label(&format!("{} - 🦀 FerrisPad", filename));
            *changes_open.borrow_mut() = false; // Reset unsaved changes flag
            *path_open.borrow_mut() = Some(path); // Store current file path
        }
        Err(e) => dialog::alert_default(&format!("Error opening file: {}", e)),
    }
}

fn main() {
    let app = app::App::default().with_scheme(app::AppScheme::Gtk);

    let mut wind = Window::new(100, 100, 640, 480, "Untitled - 🦀 FerrisPad");

    // Set window class for proper identification by window managers
    wind.set_xclass("FerrisPad");

    // Load and set the crab emoji as window icon from embedded asset
    let icon_data = include_bytes!("../assets/crab-notepad-emoji-8bit.png");
    if let Ok(mut icon) = PngImage::from_data(icon_data) {
        icon.scale(32, 32, true, true);
        #[cfg(target_os = "linux")]
        wind.set_icon(Some(icon));
    }

    let mut flex = Flex::new(0, 0, 640, 480, None);
    flex.set_type(fltk::group::FlexType::Column);

    let mut menu = MenuBar::new(0, 0, 0, 30, "");
    flex.fixed(&menu, 30);

    // Update notification banner (initially hidden)
    let mut update_banner_frame = Frame::default().with_size(0, 0);
    update_banner_frame.set_frame(fltk::enums::FrameType::FlatBox);
    update_banner_frame.set_color(Color::from_rgb(255, 250, 205)); // Light yellow
    update_banner_frame.set_label_color(Color::Black);
    update_banner_frame.set_label_size(13);
    update_banner_frame.hide(); // Hidden by default
    flex.fixed(&update_banner_frame, 0); // 0 height when hidden

    let mut text_buf = TextBuffer::default();
    let mut text_editor = TextEditor::new(0, 0, 0, 0, "");
    text_editor.set_buffer(text_buf.clone());

    flex.end();
    wind.resizable(&flex);

    // Load settings from disk (or create defaults)
    let settings = AppSettings::load();

    // Determine initial dark mode based on settings
    let initial_dark_mode = match settings.theme_mode {
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
        ThemeMode::SystemDefault => detect_system_dark_mode(),
    };

    // Initialize state variables from settings (wrapped in Rc<RefCell> for sharing)
    let _app_settings = Rc::new(RefCell::new(settings.clone()));  // TODO: Use in settings dialog
    let dark_mode = Rc::new(RefCell::new(initial_dark_mode));
    let show_linenumbers = Rc::new(RefCell::new(settings.line_numbers_enabled));
    let word_wrap = Rc::new(RefCell::new(settings.word_wrap_enabled));
    let has_unsaved_changes = Rc::new(RefCell::new(false));
    let current_file_path = Rc::new(RefCell::new(Option::<String>::None));

    // Check if a file should be opened
    let args: Vec<String> = env::args().collect();
    // Skip the first argument (app path) and find the first argument that doesn't start with -
    if let Some(path) = args.iter().skip(1).find(|arg| !arg.starts_with("-psn")) {
        let mut buf_open = text_buf.clone();
        let mut wind_open = wind.clone();
        let changes_open = has_unsaved_changes.clone();
        let path_open = current_file_path.clone();
        open_file(path.clone(), &mut buf_open, &mut wind_open, &changes_open, &path_open)
    }

    // Apply settings to editor
    if settings.line_numbers_enabled {
        text_editor.set_linenumber_width(40);
    } else {
        text_editor.set_linenumber_width(0);
    }
    text_editor.set_linenumber_bgcolor(Color::from_rgb(240, 240, 240));
    text_editor.set_linenumber_fgcolor(Color::from_rgb(100, 100, 100));

    if settings.word_wrap_enabled {
        text_editor.wrap_mode(WrapMode::AtBounds, 0);
    } else {
        text_editor.wrap_mode(WrapMode::None, 0);
    }

    // Apply font settings from config
    let font_to_use = match settings.font {
        FontChoice::ScreenBold => Font::ScreenBold,
        FontChoice::Courier => Font::Courier,
        FontChoice::HelveticaMono => Font::Screen,
    };
    text_editor.set_text_font(font_to_use);
    text_editor.set_text_size(settings.font_size as i32);

    // Apply initial theme
    apply_theme(&mut text_editor, &mut wind, &mut menu, Some(&mut update_banner_frame), initial_dark_mode);

    // Set up cursor blinking
    let cursor_visible = Rc::new(RefCell::new(true));
    let mut editor_blink = text_editor.clone();
    let cursor_state = cursor_visible.clone();

    app::add_timeout3(0.5, move |handle| {
        let mut visible = cursor_state.borrow_mut();
        *visible = !*visible;
        if *visible {
            editor_blink.show_cursor(true);
        } else {
            editor_blink.show_cursor(false);
        }
        editor_blink.redraw();
        app::repeat_timeout3(0.5, handle);
    });

    // Set up text change detection
    let changes_state = has_unsaved_changes.clone();
    text_buf.add_modify_callback(move |_, _, _, _, _| {
        *changes_state.borrow_mut() = true;
    });

    let mut buf_new = text_buf.clone();
    let mut wind_new = wind.clone();
    let changes_new = has_unsaved_changes.clone();
    let path_new = current_file_path.clone();
    menu.add(
        "File/New",
        fltk::enums::Shortcut::Ctrl | 'n',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            buf_new.set_text("");
            wind_new.set_label("Untitled - 🦀 FerrisPad");
            *changes_new.borrow_mut() = false; // Reset unsaved changes flag
            *path_new.borrow_mut() = None; // Clear current file path
        },
    );

    // OPEN -> native dialog
    let mut buf_open = text_buf.clone();
    let mut wind_open = wind.clone();
    let changes_open = has_unsaved_changes.clone();
    let path_open = current_file_path.clone();
    menu.add(
        "File/Open...",
        fltk::enums::Shortcut::Ctrl | 'o',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            // Use empty description since we're providing multi-line filter with descriptions
            if let Some(path) = native_open_dialog("", &get_text_files_filter_multiline()) {
                open_file(path, &mut buf_open, &mut wind_open, &changes_open, &path_open)
            }
        },
    );

    // SAVE -> quick save to existing file, or Save As dialog if new file
    let buf_save_quick = text_buf.clone();
    let mut wind_save_quick = wind.clone();
    let changes_save_quick = has_unsaved_changes.clone();
    let path_save_quick = current_file_path.clone();
    menu.add(
        "File/Save",
        fltk::enums::Shortcut::Ctrl | 's',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            let current_path = path_save_quick.borrow().clone();

            if let Some(path) = current_path {
                // File has been saved before, quick save without dialog
                match fs::write(&path, buf_save_quick.text()) {
                    Ok(_) => {
                        *changes_save_quick.borrow_mut() = false;
                        // Title already has correct filename, no need to update
                    },
                    Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
                }
            } else {
                // New file, show Save As dialog
                if let Some(path) = native_save_dialog("All Files", &get_all_files_filter()) {
                    match fs::write(&path, buf_save_quick.text()) {
                        Ok(_) => {
                            let filename = extract_filename(&path);
                            wind_save_quick.set_label(&format!("{} - 🦀 FerrisPad", filename));
                            *changes_save_quick.borrow_mut() = false;
                            *path_save_quick.borrow_mut() = Some(path);
                        },
                        Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
                    }
                }
            }
        },
    );

    // SAVE AS -> always show dialog for new location
    let buf_save_as = text_buf.clone();
    let mut wind_save_as = wind.clone();
    let changes_save_as = has_unsaved_changes.clone();
    let path_save_as = current_file_path.clone();
    menu.add(
        "File/Save As...",
        fltk::enums::Shortcut::Ctrl | fltk::enums::Shortcut::Shift | 's',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            if let Some(path) = native_save_dialog("All Files", &get_all_files_filter()) {
                match fs::write(&path, buf_save_as.text()) {
                    Ok(_) => {
                        let filename = extract_filename(&path);
                        wind_save_as.set_label(&format!("{} - 🦀 FerrisPad", filename));
                        *changes_save_as.borrow_mut() = false;
                        *path_save_as.borrow_mut() = Some(path);
                    },
                    Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
                }
            }
        },
    );

    // Settings menu item
    let app_settings_menu = _app_settings.clone();
    let mut editor_settings = text_editor.clone();
    let mut wind_settings = wind.clone();
    let mut menu_settings = menu.clone();
    let menu_update = menu.clone();
    let dark_mode_settings = dark_mode.clone();
    let linenumbers_settings = show_linenumbers.clone();
    let wordwrap_settings = word_wrap.clone();
    let mut banner_settings = update_banner_frame.clone();

    menu.add(
        "File/Settings...",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Normal,
        move |_| {
            let current = app_settings_menu.borrow().clone();
            if let Some(new_settings) = show_settings_dialog(&current) {
                // Save to disk
                if let Err(e) = new_settings.save() {
                    dialog::alert_default(&format!("Failed to save settings: {}", e));
                    return;
                }

                // Apply new settings immediately
                *app_settings_menu.borrow_mut() = new_settings.clone();

                // Apply theme
                let is_dark = match new_settings.theme_mode {
                    ThemeMode::Light => false,
                    ThemeMode::Dark => true,
                    ThemeMode::SystemDefault => detect_system_dark_mode(),
                };
                *dark_mode_settings.borrow_mut() = is_dark;
                apply_theme(&mut editor_settings, &mut wind_settings, &mut menu_settings, Some(&mut banner_settings), is_dark);
                #[cfg(target_os = "windows")]
                set_windows_titlebar_theme(&wind_settings, is_dark);

                // Update Dark Mode menu checkbox
                let idx = menu_update.find_index("View/Toggle Dark Mode");
                if idx >= 0 {
                    if let Some(mut item) = menu_update.at(idx) {
                        if is_dark {
                            item.set();
                        } else {
                            item.clear();
                        }
                    }
                }

                // Apply font
                let font = match new_settings.font {
                    FontChoice::ScreenBold => Font::ScreenBold,
                    FontChoice::Courier => Font::Courier,
                    FontChoice::HelveticaMono => Font::Screen,
                };
                editor_settings.set_text_font(font);
                editor_settings.set_text_size(new_settings.font_size as i32);

                // Apply line numbers
                *linenumbers_settings.borrow_mut() = new_settings.line_numbers_enabled;
                if new_settings.line_numbers_enabled {
                    editor_settings.set_linenumber_width(40);
                } else {
                    editor_settings.set_linenumber_width(0);
                }

                // Update Line Numbers menu checkbox
                let idx = menu_update.find_index("View/Toggle Line Numbers");
                if idx >= 0 {
                    if let Some(mut item) = menu_update.at(idx) {
                        if new_settings.line_numbers_enabled {
                            item.set();
                        } else {
                            item.clear();
                        }
                    }
                }

                // Apply word wrap
                *wordwrap_settings.borrow_mut() = new_settings.word_wrap_enabled;
                if new_settings.word_wrap_enabled {
                    editor_settings.wrap_mode(WrapMode::AtBounds, 0);
                } else {
                    editor_settings.wrap_mode(WrapMode::None, 0);
                }

                // Update Word Wrap menu checkbox
                let idx = menu_update.find_index("View/Toggle Word Wrap");
                if idx >= 0 {
                    if let Some(mut item) = menu_update.at(idx) {
                        if new_settings.word_wrap_enabled {
                            item.set();
                        } else {
                            item.clear();
                        }
                    }
                }

                editor_settings.redraw();
            }
        },
    );

    let changes_quit = has_unsaved_changes.clone();
    let path_quit = current_file_path.clone();
    let buf_quit = text_buf.clone();
    let mut wind_quit = wind.clone();
    menu.add(
        "File/Quit",
        fltk::enums::Shortcut::Ctrl | 'q',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            if *changes_quit.borrow() {
                // There are unsaved changes, ask user for confirmation with 3 options
                let choice = dialog::choice2_default(
                    "You have unsaved changes.",
                    "Save",
                    "Quit Without Saving",
                    "Cancel"
                );

                match choice {
                    Some(0) => { // User chose "Save"
                        let saved = if let Some(ref current_path) = *path_quit.borrow() {
                            // File has been saved before, save to existing path
                            match fs::write(current_path, buf_quit.text()) {
                                Ok(_) => {
                                    *changes_quit.borrow_mut() = false;
                                    true
                                }
                                Err(e) => {
                                    dialog::alert_default(&format!("Error saving file: {}", e));
                                    false
                                }
                            }
                        } else {
                            // New file, open save dialog
                            if let Some(path) = native_save_dialog("All Files", &get_all_files_filter()) {
                                match fs::write(&path, buf_quit.text()) {
                                    Ok(_) => {
                                        let filename = extract_filename(&path);
                                        wind_quit.set_label(&format!("{} - 🦀 FerrisPad", filename));
                                        *changes_quit.borrow_mut() = false;
                                        *path_quit.borrow_mut() = Some(path);
                                        true
                                    }
                                    Err(e) => {
                                        dialog::alert_default(&format!("Error saving file: {}", e));
                                        false
                                    }
                                }
                            } else {
                                false // User canceled save dialog
                            }
                        };

                        if saved {
                            app.quit();
                        }
                    }
                    Some(1) => { // User chose "Quit Without Saving"
                        app.quit();
                    }
                    _ => { // User chose "Cancel" or closed dialog
                        // Do nothing (don't quit)
                    }
                }
            } else {
                // No unsaved changes, quit immediately
                app.quit();
            }
        },
    );

    // Edit menu
    let mut buf_edit = text_buf.clone();

    menu.add(
        "Edit/Undo",
        fltk::enums::Shortcut::Ctrl | 'z',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            let _ = buf_edit.undo();
        },
    );

    let mut buf_redo = text_buf.clone();
    menu.add(
        "Edit/Redo",
        fltk::enums::Shortcut::Ctrl | fltk::enums::Shortcut::Shift | 'z',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            let _ = buf_redo.redo();
        },
    );

    let editor_cut = text_editor.clone();
    menu.add(
        "Edit/Cut",
        fltk::enums::Shortcut::Ctrl | 'x',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            editor_cut.cut();
        },
    );

    let editor_copy = text_editor.clone();
    menu.add(
        "Edit/Copy",
        fltk::enums::Shortcut::Ctrl | 'c',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            editor_copy.copy();
        },
    );

    let editor_paste = text_editor.clone();
    menu.add(
        "Edit/Paste",
        fltk::enums::Shortcut::Ctrl | 'v',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            editor_paste.paste();
        },
    );


    let buf_find = text_buf.clone();
    let mut editor_find = text_editor.clone();
    menu.add(
        "Edit/Find...",
        fltk::enums::Shortcut::Ctrl | 'f',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            show_find_dialog(&buf_find, &mut editor_find);
        },
    );

    let buf_replace = text_buf.clone();
    let mut editor_replace = text_editor.clone();
    menu.add(
        "Edit/Replace...",
        fltk::enums::Shortcut::Ctrl | 'h',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            show_replace_dialog(&buf_replace, &mut editor_replace);
        },
    );

    let mut editor_clone_ln = text_editor.clone();
    let linenumbers_state = show_linenumbers.clone();
    let _menu_item_ln = menu.add(
        "View/Toggle Line Numbers",
        fltk::enums::Shortcut::None,
        if settings.line_numbers_enabled {
            fltk::menu::MenuFlag::Toggle | fltk::menu::MenuFlag::Value
        } else {
            fltk::menu::MenuFlag::Toggle
        },
        move |_| {
            let mut state = linenumbers_state.borrow_mut();
            *state = !*state;
            if *state {
                editor_clone_ln.set_linenumber_width(40);
            } else {
                editor_clone_ln.set_linenumber_width(0);
            }
            editor_clone_ln.redraw();
        },
    );

    // NEW: Add "Toggle Word Wrap" menu item
    let mut editor_clone_ww = text_editor.clone();
    let word_wrap_state = word_wrap.clone();
    let _menu_item_ww = menu.add(
        "View/Toggle Word Wrap",
        fltk::enums::Shortcut::None,
        if settings.word_wrap_enabled {
            fltk::menu::MenuFlag::Toggle | fltk::menu::MenuFlag::Value
        } else {
            fltk::menu::MenuFlag::Toggle
        },
        move |_| {
            let mut state = word_wrap_state.borrow_mut();
            *state = !*state;
            if *state {
                // Wrap text at the widget's bounds
                editor_clone_ww.wrap_mode(WrapMode::AtBounds, 0);
            } else {
                // Disable wrapping
                editor_clone_ww.wrap_mode(WrapMode::None, 0);
            }
            editor_clone_ww.redraw();
        },
    );

    // NEW: Add "Toggle Dark Mode" menu item
    let mut editor_clone_dm = text_editor.clone();
    let mut wind_clone_dm = wind.clone();
    let mut menu_clone_dm = menu.clone();
    let mut banner_clone_dm = update_banner_frame.clone();
    let dark_mode_state = dark_mode.clone();
    let _menu_item_dm = menu.add(
        "View/Toggle Dark Mode",
        fltk::enums::Shortcut::None,
        if initial_dark_mode {
            fltk::menu::MenuFlag::Toggle | fltk::menu::MenuFlag::Value
        } else {
            fltk::menu::MenuFlag::Toggle
        },
        move |_| {
            let mut state = dark_mode_state.borrow_mut();
            *state = !*state;
            apply_theme(&mut editor_clone_dm, &mut wind_clone_dm, &mut menu_clone_dm, Some(&mut banner_clone_dm), *state);
            #[cfg(target_os = "windows")]
            set_windows_titlebar_theme(&wind_clone_dm, *state);
        },
    );

    // TODO: Add Settings dialog window (modal with radio buttons and toggles)
    // For now, keep Format menu as temporary way to change settings without saving

    // Add font selection submenu under Format (temporary, no saving)
    let mut editor_font1 = text_editor.clone();
    menu.add(
        "Format/Font/Screen (Bold)",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Normal,
        move |_| {
            editor_font1.set_text_font(Font::ScreenBold);
            editor_font1.redraw();
        },
    );

    let mut editor_font2 = text_editor.clone();
    menu.add(
        "Format/Font/Courier",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Normal,
        move |_| {
            editor_font2.set_text_font(Font::Courier);
            editor_font2.redraw();
        },
    );

    let mut editor_font3 = text_editor.clone();
    menu.add(
        "Format/Font/Helvetica Mono",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Normal,
        move |_| {
            editor_font3.set_text_font(Font::Screen);
            editor_font3.redraw();
        },
    );

    // Add font size options under Format (temporary, no saving)
    let mut editor_size1 = text_editor.clone();
    menu.add(
        "Format/Font Size/Small (12)",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Normal,
        move |_| {
            editor_size1.set_text_size(12);
            editor_size1.redraw();
        },
    );

    let mut editor_size2 = text_editor.clone();
    menu.add(
        "Format/Font Size/Medium (16)",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Normal,
        move |_| {
            editor_size2.set_text_size(16);
            editor_size2.redraw();
        },
    );

    let mut editor_size3 = text_editor.clone();
    menu.add(
        "Format/Font Size/Large (20)",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Normal,
        move |_| {
            editor_size3.set_text_size(20);
            editor_size3.redraw();
        },
    );

    // Help Menu
    menu.add(
        "Help/About FerrisPad",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Normal,
        move |_| {
            show_about_dialog();
        },
    );

    menu.add(
        "Help/Check for Updates...",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Normal,
        {
            let settings_clone = _app_settings.clone();
            move |_| {
                check_for_updates_ui(&settings_clone);
            }
        },
    );

    // Handle window close button (X)
    let changes_close = has_unsaved_changes.clone();
    let path_close = current_file_path.clone();
    let buf_close = text_buf.clone();
    let mut wind_close = wind.clone();
    wind.set_callback(move |_| {
        if *changes_close.borrow() {
            // There are unsaved changes, ask user for confirmation with 3 options
            let choice = dialog::choice2_default(
                "You have unsaved changes.",
                "Save",
                "Quit Without Saving",
                "Cancel"
            );

            match choice {
                Some(0) => { // User chose "Save"
                    let saved = if let Some(ref current_path) = *path_close.borrow() {
                        // File has been saved before, save to existing path
                        match fs::write(current_path, buf_close.text()) {
                            Ok(_) => {
                                *changes_close.borrow_mut() = false;
                                true
                            }
                            Err(e) => {
                                dialog::alert_default(&format!("Error saving file: {}", e));
                                false
                            }
                        }
                    } else {
                        // New file, open save dialog
                        if let Some(path) = native_save_dialog("All Files", &get_all_files_filter()) {
                            match fs::write(&path, buf_close.text()) {
                                Ok(_) => {
                                    let filename = extract_filename(&path);
                                    wind_close.set_label(&format!("{} - 🦀 FerrisPad", filename));
                                    *changes_close.borrow_mut() = false;
                                    *path_close.borrow_mut() = Some(path);
                                    true
                                }
                                Err(e) => {
                                    dialog::alert_default(&format!("Error saving file: {}", e));
                                    false
                                }
                            }
                        } else {
                            false // User canceled save dialog
                        }
                    };

                    if saved {
                        app.quit();
                    }
                }
                Some(1) => { // User chose "Quit Without Saving"
                    app.quit();
                }
                _ => { // User chose "Cancel" or closed dialog
                    // Do nothing (don't close)
                }
            }
        } else {
            // No unsaved changes, quit immediately
            app.quit();
        }
    });

    wind.end();
    wind.show();

    // Apply Windows title bar theme AFTER window is shown
    #[cfg(target_os = "windows")]
    set_windows_titlebar_theme(&wind, initial_dark_mode);

    // Background update check on startup
    let update_banner_data = Arc::new(Mutex::new(None));
    let settings_arc = Arc::new(Mutex::new(settings.clone()));
    let settings_for_check = settings_arc.clone();
    let banner_data_clone = update_banner_data.clone();

    // Spawn background thread for update check
    std::thread::spawn(move || {
        check_for_updates_background(settings_for_check, banner_data_clone);
    });

    // Set up periodic check to show banner if update is available
    let mut banner_frame_check = update_banner_frame.clone();
    let mut flex_check = flex.clone();
    let banner_data_check = update_banner_data.clone();
    let settings_banner = _app_settings.clone();
    let mut wind_check = wind.clone();

    app::add_timeout3(0.5, move |handle| {
        if let Some(release) = banner_data_check.lock().unwrap().take() {
            // Update found! Show the banner
            let version = release.version();
            banner_frame_check.set_label(&format!(
                "  🦀 FerrisPad {} is available - Click to view details or press ESC to dismiss",
                version
            ));
            banner_frame_check.show();
            flex_check.fixed(&banner_frame_check, 30); // Set height when visible
            wind_check.redraw();

            // Make banner clickable
            let release_clone = release.clone();
            let settings_clone = settings_banner.clone();
            banner_frame_check.handle(move |frame, event| {
                match event {
                    fltk::enums::Event::Push => {
                        // Clicked - show update dialog
                        show_update_available_dialog(release_clone.clone(), &settings_clone);
                        frame.hide();
                        true
                    }
                    fltk::enums::Event::KeyDown => {
                        if app::event_key() == fltk::enums::Key::Escape {
                            // ESC pressed - dismiss banner
                            frame.hide();
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            });
        }
        app::repeat_timeout3(0.5, handle);
    });

    app.run().unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_filter_simple() {
        let filter = get_platform_filter("Text Files", "*.txt");
        assert_eq!(filter, "*.txt");
    }

    #[test]
    fn test_platform_filter_multiple_extensions() {
        let filter = get_platform_filter("Text Files", "*.{txt,md,rst}");
        assert_eq!(filter, "*.{txt,md,rst}");
    }

    #[test]
    fn test_platform_filter_all_files() {
        let filter = get_platform_filter("All Files", "*");
        assert_eq!(filter, "*");
    }

    #[test]
    fn test_platform_filter_ignores_description() {
        // Description parameter is kept for API consistency but not used
        let filter1 = get_platform_filter("Text Files", "*.txt");
        let filter2 = get_platform_filter("Different Description", "*.txt");
        assert_eq!(filter1, filter2);
        assert_eq!(filter1, "*.txt");
    }

    #[test]
    fn test_all_files_filter() {
        let filter = get_all_files_filter();
        assert_eq!(filter, "*");
    }

    #[test]
    fn test_multiline_filter_format() {
        let filter = get_text_files_filter_multiline();
        // Should contain newline separators
        assert!(filter.contains("\n"));
        // Should contain tab separators between description and pattern
        assert!(filter.contains("\t"));
        // Should contain various file type options
        assert!(filter.contains("Text Files"));
        assert!(filter.contains("Markdown Files"));
        assert!(filter.contains("Rust Files"));
        assert!(filter.contains("Python Files"));
        assert!(filter.contains("Config Files"));
        // Note: "All Files" is automatically added by FLTK, not in our filter string
    }

    #[test]
    fn test_extract_filename_from_path() {
        assert_eq!(extract_filename("/home/user/test.txt"), "test.txt");
        assert_eq!(extract_filename("/home/user/document.md"), "document.md");
        assert_eq!(extract_filename("test.txt"), "test.txt");
        assert_eq!(extract_filename("/path/with/many/levels/file.rs"), "file.rs");
    }

    #[test]
    fn test_extract_filename_edge_cases() {
        // Path ending with directory extracts directory name (reasonable behavior)
        assert_eq!(extract_filename("/home/user/"), "user");
        // Empty path
        assert_eq!(extract_filename(""), "Unknown");
        // Just a dot (current directory marker)
        assert_eq!(extract_filename("."), "Unknown");
        // Root path
        assert_eq!(extract_filename("/"), "Unknown");
    }

    #[test]
    fn test_find_next_simple() {
        let text = "Hello world, hello Rust, hello FerrisPad";
        let search = "hello";
        let result = find_in_text(text, search, 0, false);
        assert_eq!(result, Some(0)); // First occurrence (case-insensitive matches "Hello")
    }

    #[test]
    fn test_find_case_sensitive() {
        let text = "Hello world, hello Rust, hello FerrisPad";
        let search = "Hello";
        let result = find_in_text(text, search, 0, true);
        assert_eq!(result, Some(0)); // First occurrence (exact case)
    }

    #[test]
    fn test_find_no_match() {
        let text = "Hello world";
        let search = "rust";
        let result = find_in_text(text, search, 0, false);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_from_position() {
        let text = "cat dog cat mouse cat";
        let search = "cat";
        let result = find_in_text(text, search, 10, false);
        assert_eq!(result, Some(18)); // Third occurrence
    }

    #[test]
    fn test_replace_all_simple() {
        let text = "cat cat cat";
        let result = replace_all_in_text(text, "cat", "dog", false);
        assert_eq!(result.0, "dog dog dog");
        assert_eq!(result.1, 3); // 3 replacements
    }

    #[test]
    fn test_replace_all_case_sensitive() {
        let text = "Cat cat CAT";
        let result = replace_all_in_text(text, "cat", "dog", true);
        assert_eq!(result.0, "Cat dog CAT");
        assert_eq!(result.1, 1); // Only middle one matches
    }

    #[test]
    fn test_replace_all_case_insensitive() {
        let text = "Cat cat CAT";
        let result = replace_all_in_text(text, "cat", "dog", false);
        assert_eq!(result.0, "dog dog dog");
        assert_eq!(result.1, 3);
    }

    #[test]
    fn test_replace_all_no_matches() {
        let text = "hello world";
        let result = replace_all_in_text(text, "rust", "ferris", false);
        assert_eq!(result.0, "hello world");
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_replace_all_empty_replacement() {
        let text = "hello world hello";
        let result = replace_all_in_text(text, "hello", "", false);
        assert_eq!(result.0, " world ");
        assert_eq!(result.1, 2); // Both "hello" occurrences replaced
    }
}