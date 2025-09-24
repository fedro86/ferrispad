use fltk::{
    app, dialog,
    enums::Color, // Added for colors
    menu::MenuBar,
    prelude::*,
    text::{TextBuffer, TextEditor},
    window::Window,
};
use std::cell::RefCell; // Added for RefCell
use std::rc::Rc;        // Added for Rc
use std::fs;

fn main() {
    let app = app::App::default();
    let mut wind = Window::new(100, 100, 640, 480, "RustPad");
    let mut menu = MenuBar::new(0, 0, 640, 30, "");
    
    let text_buf = TextBuffer::default();
    let mut text_editor = TextEditor::new(5, 35, 630, 440, "");
    text_editor.set_buffer(text_buf.clone());

    // --- State for line numbers ---
    // Start with line numbers hidden.
    let show_linenumbers = Rc::new(RefCell::new(false)); 
    // Set initial colors for the line number bar.
    text_editor.set_linenumber_bgcolor(Color::from_rgb(240, 240, 240)); // Light gray
    text_editor.set_linenumber_fgcolor(Color::from_rgb(100, 100, 100)); // Dark gray text

    // --- File Menu ---
    let mut buf_new = text_buf.clone();
    let mut wind_new = wind.clone();
    menu.add("File/New", fltk::enums::Shortcut::Ctrl | 'n', fltk::menu::MenuFlag::Normal, move |_| {
        buf_new.set_text("");
        wind_new.set_label("RustPad");
    });
    // ... (other File menu items remain the same)
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
    menu.add("File/Quit", fltk::enums::Shortcut::Ctrl | 'q', fltk::menu::MenuFlag::Normal, move |_| {
        app.quit();
    });

    // --- View Menu ---
    let mut editor_clone = text_editor.clone();
    let linenumbers_state = show_linenumbers.clone();
    menu.add(
        "View/Toggle Line Numbers",
        fltk::enums::Shortcut::None,
        fltk::menu::MenuFlag::Toggle, // Makes it a checkable item
        move |_| {
            let mut state = linenumbers_state.borrow_mut(); // Get mutable access
            *state = !*state; // Flip the boolean
            if *state {
                editor_clone.set_linenumber_width(40); // Show with 40px width
            } else {
                editor_clone.set_linenumber_width(0); // Hide by setting width to 0
            }
            editor_clone.redraw(); // Tell the editor to update its appearance
        },
    );

    wind.end();
    wind.show();
    app.run().unwrap();
}