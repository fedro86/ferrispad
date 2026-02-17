use fltk::dialog::{NativeFileChooser, NativeFileChooserType};

fn show_chooser(title: &str, chooser_type: NativeFileChooserType) -> NativeFileChooser {
    let mut chooser = NativeFileChooser::new(chooser_type);
    chooser.set_title(title);
    chooser.show();
    chooser
}

pub fn native_open_dialog() -> Option<String> {
    let chooser = show_chooser("Open File", NativeFileChooserType::BrowseFile);
    let path = chooser.filename();
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path.to_string_lossy().to_string())
    }
}

pub fn native_open_multi_dialog() -> Vec<String> {
    let chooser = show_chooser("Open Files", NativeFileChooserType::BrowseMultiFile);
    chooser
        .filenames()
        .into_iter()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

pub fn native_save_dialog() -> Option<String> {
    let chooser = show_chooser("Save As", NativeFileChooserType::BrowseSaveFile);
    let path = chooser.filename();
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path.to_string_lossy().to_string())
    }
}
