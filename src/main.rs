use fltk::{
    app,
    dialog, // for alert_default
    enums::Color,
    group::Flex,
    menu::MenuBar,
    prelude::*,
    text::{TextBuffer, TextEditor, WrapMode}, // NEW: Import WrapMode
    window::Window,
};
use std::process::Command;
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

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

fn native_open_dialog(filter: &str) -> Option<String> {
    let mut nfc = NativeFileChooser::new(FileDialogType::BrowseFile);
    nfc.set_filter(filter);
    nfc.show();
    let filename = nfc.filename();
    let s = filename.to_string_lossy();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

fn native_save_dialog(filter: &str) -> Option<String> {
    let mut nfc = NativeFileChooser::new(FileDialogType::BrowseSaveFile);
    nfc.set_filter(filter);
    nfc.show();
    let filename = nfc.filename();
    let s = filename.to_string_lossy();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

fn main() {
    let app = app::App::default();
    let mut wind = Window::new(100, 100, 640, 480, "FerrisPad");

    let mut flex = Flex::new(0, 0, 640, 480, None);
    flex.set_type(fltk::group::FlexType::Column);

    let mut menu = MenuBar::new(0, 0, 0, 30, "");
    flex.fixed(&menu, 30);

    let text_buf = TextBuffer::default();
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

    // Apply initial theme
    apply_theme(&mut text_editor, &mut wind, &mut menu, settings.dark_mode_enabled);

    let mut buf_new = text_buf.clone();
    let mut wind_new = wind.clone();
    menu.add(
        "File/New",
        fltk::enums::Shortcut::Ctrl | 'n',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            buf_new.set_text("");
            wind_new.set_label("FerrisPad");
        },
    );

    // OPEN -> native dialog
    let mut buf_open = text_buf.clone();
    let mut wind_open = wind.clone();
    menu.add(
        "File/Open...",
        fltk::enums::Shortcut::Ctrl | 'o',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            if let Some(path) = native_open_dialog("*.txt") {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        buf_open.set_text(&content);
                        wind_open.set_label(&format!("FerrisPad - {}", path));
                    }
                    Err(e) => dialog::alert_default(&format!("Error opening file: {}", e)),
                }
            }
        },
    );

    // SAVE AS -> native dialog
    let buf_save = text_buf.clone();
    let mut wind_save = wind.clone();
    menu.add(
        "File/Save As...",
        fltk::enums::Shortcut::Ctrl | 's',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            if let Some(path) = native_save_dialog("*.txt") {
                match fs::write(&path, buf_save.text()) {
                    Ok(_) => wind_save.set_label(&format!("FerrisPad - {}", path)),
                    Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
                }
            }
        },
    );

    menu.add(
        "File/Quit",
        fltk::enums::Shortcut::Ctrl | 'q',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            app.quit();
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

    wind.end();
    wind.show();
    app.run().unwrap();
}