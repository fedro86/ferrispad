use fltk::{
    app,
    button::Button,
    dialog,
    enums::Font,
    frame::Frame,
    group::Flex,
    misc::Progress,
    prelude::*,
    text::{TextBuffer, TextEditor, WrapMode},
    window::Window,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::settings::AppSettings;
use crate::app::updater;

/// Check for updates and show UI dialog (manual check)
pub fn check_for_updates_ui(settings: &Rc<RefCell<AppSettings>>) {
    use crate::app::updater::{check_for_updates, current_timestamp, UpdateCheckResult};

    let current_version = env!("CARGO_PKG_VERSION");
    let settings_borrowed = settings.borrow();
    let channel = settings_borrowed.update_channel;
    let skipped = settings_borrowed.skipped_versions.clone();
    drop(settings_borrowed);

    let result = check_for_updates(current_version, channel, &skipped);

    match result {
        UpdateCheckResult::UpdateAvailable(release) => {
            show_update_available_dialog(release, settings);
        }
        UpdateCheckResult::NoUpdate => {
            dialog::message_default(&format!(
                "\u{2705} You're up to date!\n\nFerrisPad {} is the latest version.",
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

    let mut settings_mut = settings.borrow_mut();
    settings_mut.last_update_check = current_timestamp();
    let _ = settings_mut.save();
}

/// Show update available dialog with options
pub fn show_update_available_dialog(release: updater::ReleaseInfo, settings: &Rc<RefCell<AppSettings>>) {
    let current_version = env!("CARGO_PKG_VERSION");
    let asset_name = updater::get_platform_asset_name();
    let direct_asset = release.assets.iter().find(|a| a.name.contains(asset_name));

    let mut dialog = Window::new(100, 100, 500, 480, "Update Available");
    dialog.make_modal(true);

    let mut flex = Flex::new(10, 10, 480, 460, None);
    flex.set_type(fltk::group::FlexType::Column);
    flex.set_spacing(10);

    // Title
    let mut title = Frame::default().with_label("\u{1f980} FerrisPad Update Available");
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
            let s_frame_download = status.clone();
            let s_frame_install = status.clone();
            let btn_row_download_err = btn_row.clone();
            let btn_row_install_err = btn_row.clone();

            std::thread::spawn(move || {
                let temp_dir = std::env::temp_dir();
                let temp_file = temp_dir.join("ferrispad_update");

                let result = updater::download_file(&download_url, &temp_file, |p| {
                    let mut p_val = p_bar.clone();
                    let mut s_val = s_frame_download.clone();
                    app::awake_callback(move || {
                        p_val.set_value(p as f64);
                        s_val.set_label(&format!("Downloading: {:.0}%", p * 100.0));
                    });
                });

                match result {
                    Ok(_) => {
                        let mut s_install = s_frame_install.clone();
                        app::awake_callback(move || {
                            s_install.set_label("Installing update...");
                        });

                        match updater::install_update(&temp_file) {
                            Ok(_) => {
                                app::awake_callback(move || {
                                    dialog::message_default("Update installed successfully!\n\nFerrisPad will now restart.");
                                    if let Ok(current_exe) = std::env::current_exe() {
                                        let _ = std::process::Command::new(current_exe).spawn();
                                    }
                                    app::quit();
                                });
                            }
                            Err(e) => {
                                let mut br = btn_row_install_err.clone();
                                let mut s_err = s_frame_install.clone();
                                app::awake_callback(move || {
                                    s_err.set_label("Installation failed");
                                    dialog::alert_default(&format!("Failed to install update: {}", e));
                                    br.activate();
                                });
                            }
                        }
                    }
                    Err(e) => {
                        let mut br = btn_row_download_err.clone();
                        let mut s_err = s_frame_download.clone();
                        app::awake_callback(move || {
                            s_err.set_label("Download failed");
                            dialog::alert_default(&format!("Failed to download update: {}", e));
                            br.activate();
                        });
                    }
                }
            });
        });
    }

    // View on GitHub button
    let release_url = release.html_url.clone();
    download_btn.set_callback(move |_| {
        if let Err(e) = open::that(&release_url) {
            dialog::alert_default(&format!("Failed to open browser: {}", e));
        }
    });

    // Skip button
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

    // Later button
    let mut dialog_later = dialog.clone();
    later_btn.set_callback(move |_| {
        dialog_later.hide();
    });

    dialog.show();
    super::run_dialog(&dialog);
}
