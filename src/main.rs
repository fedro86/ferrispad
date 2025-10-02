#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use fltk::{
    app,
    dialog, // for alert_default
    enums::{Color, Font},
    group::Flex,
    image::PngImage,
    menu::MenuBar,
    prelude::*,
    text::{TextBuffer, TextEditor, WrapMode}, // NEW: Import WrapMode
    window::Window,
};
use std::process::Command;
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use std::path::Path;

use fltk::dialog::{FileDialogType, NativeFileChooser};

#[derive(Clone)]
struct AppSettings {
    line_numbers_enabled: bool,
    word_wrap_enabled: bool,
    dark_mode_enabled: bool,
}

impl AppSettings {
    fn new() -> Self {
        let system_dark_mode = detect_system_dark_mode();
        Self {
            line_numbers_enabled: true,  // Favorite setting: enabled by default
            word_wrap_enabled: true,     // Favorite setting: enabled by default
            dark_mode_enabled: system_dark_mode,  // Based on system detection
        }
    }
}

fn detect_system_dark_mode() -> bool {
    // Try to detect system theme on Linux
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

    // Default to light mode if detection fails
    false
}

fn apply_theme(
    editor: &mut TextEditor,
    window: &mut Window,
    menu: &mut MenuBar,
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

fn main() {
    let app = app::App::default().with_scheme(app::AppScheme::Gtk);

    let mut wind = Window::new(100, 100, 640, 480, "Untitled - 🦀 FerrisPad");

    // Set window class for proper identification by window managers
    wind.set_xclass("FerrisPad");

    // Load and set the crab emoji as window icon from embedded asset
    let icon_data = include_bytes!("../assets/crab-notepad-emoji-8bit.png");
    if let Ok(mut icon) = PngImage::from_data(icon_data) {
        icon.scale(32, 32, true, true);
        wind.set_icon(Some(icon));
    }

    let mut flex = Flex::new(0, 0, 640, 480, None);
    flex.set_type(fltk::group::FlexType::Column);

    let mut menu = MenuBar::new(0, 0, 0, 30, "");
    flex.fixed(&menu, 30);

    let mut text_buf = TextBuffer::default();
    let mut text_editor = TextEditor::new(0, 0, 0, 0, "");
    text_editor.set_buffer(text_buf.clone());

    flex.end();
    wind.resizable(&flex);

    // Initialize settings with favorite defaults
    let settings = AppSettings::new();

    // Initialize state variables from settings
    let dark_mode = Rc::new(RefCell::new(settings.dark_mode_enabled));
    let show_linenumbers = Rc::new(RefCell::new(settings.line_numbers_enabled));
    let word_wrap = Rc::new(RefCell::new(settings.word_wrap_enabled));
    let has_unsaved_changes = Rc::new(RefCell::new(false));
    let current_file_path = Rc::new(RefCell::new(Option::<String>::None));

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

    // Set up better font for the editor
    text_editor.set_text_font(Font::ScreenBold); // Nice monospace font
    text_editor.set_text_size(16); // Slightly larger for better readability

    // Apply initial theme
    apply_theme(&mut text_editor, &mut wind, &mut menu, settings.dark_mode_enabled);

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
    let dark_mode_state = dark_mode.clone();
    let _menu_item_dm = menu.add(
        "View/Toggle Dark Mode",
        fltk::enums::Shortcut::None,
        if settings.dark_mode_enabled {
            fltk::menu::MenuFlag::Toggle | fltk::menu::MenuFlag::Value
        } else {
            fltk::menu::MenuFlag::Toggle
        },
        move |_| {
            let mut state = dark_mode_state.borrow_mut();
            *state = !*state;
            apply_theme(&mut editor_clone_dm, &mut wind_clone_dm, &mut menu_clone_dm, *state);
        },
    );

    // Add font selection submenu under Format
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

    // Add font size options under Format
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
}