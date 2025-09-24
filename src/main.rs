use fltk::{
    app, dialog,
    menu::MenuBar,
    prelude::*,
    text::{TextBuffer, TextEditor},
    window::Window,
};
use std::fs;

fn main() {
    let app = app::App::default();
    let mut wind = Window::new(100, 100, 640, 480, "RustPad");
    let mut menu = MenuBar::new(0, 0, 640, 30, "");
    
    let text_buf = TextBuffer::default();
    let mut text_editor = TextEditor::new(5, 35, 630, 440, "");
    text_editor.set_buffer(text_buf.clone());

    // --- Menu Actions ---

    // File -> New
    let mut buf_new = text_buf.clone();
    let mut wind_new = wind.clone(); // <-- THIS IS THE FIX
    menu.add("File/New", fltk::enums::Shortcut::Ctrl | 'n', fltk::menu::MenuFlag::Normal, move |_| {
        buf_new.set_text("");
        wind_new.set_label("RustPad"); // Use the clone here
    });

    // File -> Open
    let mut buf_open = text_buf.clone();
    let mut wind_open = wind.clone();
    menu.add("File/Open...", fltk::enums::Shortcut::Ctrl | 'o', fltk::menu::MenuFlag::Normal, move |_| {
        if let Some(path) = dialog::file_chooser("Open File", "*.txt", ".", false) {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    buf_open.set_text(&content);
                    wind_open.set_label(&format!("RustPad - {}", path));
                },
                Err(e) => dialog::alert_default(&format!("Error opening file: {}", e)),
            }
        }
    });

    // File -> Save As
    let buf_save = text_buf.clone();
    let mut wind_save = wind.clone();
    menu.add("File/Save As...", fltk::enums::Shortcut::Ctrl | 's', fltk::menu::MenuFlag::Normal, move |_| {
        if let Some(path) = dialog::file_chooser("Save File As", "*.txt", ".", true) {
            match fs::write(&path, buf_save.text()) {
                Ok(_) => wind_save.set_label(&format!("RustPad - {}", path)),
                Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
            }
        }
    });

    // File -> Exit
    menu.add("File/Quit", fltk::enums::Shortcut::Ctrl | 'q', fltk::menu::MenuFlag::Normal, move |_| {
        app.quit();
    });

    wind.end();
    wind.show();
    app.run().unwrap();
}