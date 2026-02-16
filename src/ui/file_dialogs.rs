use fltk::dialog;

use crate::app::file_filters::get_platform_filter;

pub fn native_open_dialog(description: &str, pattern: &str) -> Option<String> {
    let filter = get_platform_filter(description, pattern);
    dialog::file_chooser("Open File", &filter, ".", false)
}

pub fn native_open_multi_dialog(description: &str, pattern: &str) -> Vec<String> {
    // FLTK's built-in file_chooser doesn't support multi-select directly.
    // Open one file at a time — the user can Ctrl+O again for more.
    match native_open_dialog(description, pattern) {
        Some(path) => vec![path],
        None => vec![],
    }
}

pub fn native_save_dialog(description: &str, pattern: &str) -> Option<String> {
    let filter = get_platform_filter(description, pattern);
    dialog::file_chooser("Save As", &filter, ".", false)
}
