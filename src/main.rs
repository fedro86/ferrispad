use fltk::{
    app,
    dialog, // for alert_default
    enums::Color,
    group::Flex,
    menu::MenuBar,
    prelude::*,
    text::{TextBuffer, TextEditor},
    window::Window,
};
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use fltk::dialog::{FileDialogType, NativeFileChooser};

fn native_open_dialog(filter: &str) -> Option<String> {
    let mut nfc = NativeFileChooser::new(FileDialogType::BrowseFile);
    nfc.set_filter(filter);
    nfc.show(); // returns (), blocks until close
    let filename = nfc.filename();
    let s = filename.to_string_lossy();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

fn native_save_dialog(filter: &str) -> Option<String> {
    let mut nfc = NativeFileChooser::new(FileDialogType::BrowseSaveFile);
    nfc.set_filter(filter);
    nfc.show(); // returns (), blocks until close
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

    let show_linenumbers = Rc::new(RefCell::new(false));
    text_editor.set_linenumber_bgcolor(Color::from_rgb(240, 240, 240));
    text_editor.set_linenumber_fgcolor(Color::from_rgb(100, 100, 100));

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

    let mut editor_clone = text_editor.clone();
    let linenumbers_state = show_linenumbers.clone();
    menu.add(
        "View/Toggle Line Numbers",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Toggle,
        move |_| {
            let mut state = linenumbers_state.borrow_mut();
            *state = !*state;
            if *state {
                editor_clone.set_linenumber_width(40);
            } else {
                editor_clone.set_linenumber_width(0);
            }
            editor_clone.redraw();
        },
    );

    wind.end();
    wind.show();
    app.run().unwrap();
}
