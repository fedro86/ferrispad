use fltk::dialog::{NativeFileChooser, NativeFileChooserType};

fn show_chooser(title: &str, chooser_type: NativeFileChooserType, directory: Option<&str>) -> NativeFileChooser {
    let mut chooser = NativeFileChooser::new(chooser_type);
    chooser.set_title(title);
    if let Some(dir) = directory {
        chooser.set_directory(&dir.to_string()).ok();
    }
    chooser.show();
    chooser
}

pub fn native_open_dialog(directory: Option<&str>) -> Option<String> {
    let chooser = show_chooser("Open File", NativeFileChooserType::BrowseFile, directory);
    let path = chooser.filename();
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path.to_string_lossy().to_string())
    }
}

pub fn native_open_multi_dialog(directory: Option<&str>) -> Vec<String> {
    let chooser = show_chooser("Open Files", NativeFileChooserType::BrowseMultiFile, directory);
    chooser
        .filenames()
        .into_iter()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

pub fn native_save_dialog(directory: Option<&str>) -> Option<String> {
    let chooser = show_chooser("Save As", NativeFileChooserType::BrowseSaveFile, directory);
    let path = chooser.filename();
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path.to_string_lossy().to_string())
    }
}
