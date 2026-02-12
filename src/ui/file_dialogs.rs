use fltk::dialog::{FileDialogType, NativeFileChooser};

use crate::app::file_filters::get_platform_filter;

pub fn native_open_dialog(description: &str, pattern: &str) -> Option<String> {
    let mut nfc = NativeFileChooser::new(FileDialogType::BrowseFile);
    let filter = get_platform_filter(description, pattern);
    nfc.set_filter(&filter);
    nfc.show();
    let filename = nfc.filename();
    let s = filename.to_string_lossy();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

pub fn native_save_dialog(description: &str, pattern: &str) -> Option<String> {
    let mut nfc = NativeFileChooser::new(FileDialogType::BrowseSaveFile);
    let filter = get_platform_filter(description, pattern);
    nfc.set_filter(&filter);
    nfc.show();
    let filename = nfc.filename();
    let s = filename.to_string_lossy();
    if s.is_empty() { None } else { Some(s.to_string()) }
}
