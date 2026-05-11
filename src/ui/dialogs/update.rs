use fltk::{
    app,
    button::Button,
    dialog,
    enums::{Color, Font, FrameType},
    frame::Frame,
    group::Flex,
    misc::Progress,
    prelude::*,
    text::{TextBuffer, TextEditor, WrapMode},
    window::Window,
};
use std::cell::RefCell;
use std::rc::Rc;

use super::DialogTheme;
use crate::app::AppSettings;
use crate::app::services::updater;

/// Check for updates and show UI dialog (manual check)
pub fn check_for_updates_ui(
    parent: &Window,
    settings: &Rc<RefCell<AppSettings>>,
    theme_bg: (u8, u8, u8),
) {
    use crate::app::services::updater::{UpdateCheckResult, check_for_updates, current_timestamp};

    let current_version = env!("CARGO_PKG_VERSION");
    let settings_borrowed = settings.borrow();
    let channel = settings_borrowed.update_channel;
    let skipped = settings_borrowed.skipped_versions.clone();
    drop(settings_borrowed);

    let result = check_for_updates(current_version, channel, &skipped);

    match result {
        UpdateCheckResult::UpdateAvailable(release) => {
            show_update_available_dialog(parent, release, settings, theme_bg);
        }
        UpdateCheckResult::NoUpdate => {
            super::show_themed_message(
                theme_bg,
                "Up to Date",
                &format!(
                    "\u{2705} You're up to date!\n\nFerrisPad {} is the latest version.",
                    current_version
                ),
            );
        }
        UpdateCheckResult::Error(err) => {
            super::show_themed_message(
                theme_bg,
                "Update Check Failed",
                &format!(
                    "Failed to check for updates:\n\n{}\n\nPlease try again later.",
                    err
                ),
            );
        }
    }

    let mut settings_mut = settings.borrow_mut();
    settings_mut.last_update_check = current_timestamp();
    let _ = settings_mut.save();
}

/// Show update available dialog with options.
/// `parent` is the main window used to center the dialog (reliable on Wayland).
pub fn show_update_available_dialog(
    parent: &Window,
    release: updater::ReleaseInfo,
    settings: &Rc<RefCell<AppSettings>>,
    theme_bg: (u8, u8, u8),
) {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let dialog_bg = Color::from_rgb(theme_bg.0, theme_bg.1, theme_bg.2);
    let current_version = env!("CARGO_PKG_VERSION");
    let asset_name = updater::get_platform_asset_name();
    let direct_asset = release.assets.iter().find(|a| a.name.contains(asset_name));

    const DW: i32 = 500;
    const DH: i32 = 480;
    // Regular xdg_toplevel (not modal) — on Wayland/GNOME, make_modal(true) on the
    // first invocation maps the surface before the decoration + modal-hint round-trip
    // completes, yielding an undecorated, ungrabbed window with no input (and the
    // parent window also stops receiving input). See session_picker.rs for the same fix.
    let mut dialog = Window::default()
        .with_size(DW, DH)
        .with_label("Update Available")
        .center_screen();
    dialog.set_color(dialog_bg);

    let mut flex = Flex::new(10, 10, 480, 460, None);
    flex.set_type(fltk::group::FlexType::Column);
    flex.set_spacing(10);
    flex.set_color(dialog_bg);

    // Title
    let mut title = Frame::default().with_label("\u{1f980} FerrisPad Update Available");
    title.set_label_size(18);
    title.set_label_font(Font::HelveticaBold);
    title.set_label_color(theme.text);
    title.set_frame(FrameType::FlatBox);
    title.set_color(dialog_bg);
    flex.fixed(&title, 30);

    // Version info
    let version_text = format!(
        "Current version: {}\nLatest version:  {}",
        current_version,
        release.version()
    );
    let mut version_frame = Frame::default().with_label(&version_text);
    version_frame.set_label_size(14);
    version_frame.set_label_color(theme.text);
    version_frame.set_frame(FrameType::FlatBox);
    version_frame.set_color(dialog_bg);
    flex.fixed(&version_frame, 50);

    // Release notes
    let mut notes_label = Frame::default().with_label("What's new:");
    notes_label.set_label_size(14);
    notes_label.set_label_font(Font::HelveticaBold);
    notes_label.set_label_color(theme.text);
    notes_label.set_frame(FrameType::FlatBox);
    notes_label.set_color(dialog_bg);
    flex.fixed(&notes_label, 25);

    let mut notes_editor = TextEditor::default();
    notes_editor.set_buffer(TextBuffer::default());
    notes_editor.buffer().unwrap().set_text(&release.body);
    notes_editor.wrap_mode(WrapMode::AtBounds, 0);
    notes_editor.set_color(theme.input_bg);
    notes_editor.set_text_color(theme.text);
    notes_editor.set_cursor_color(theme.text);
    notes_editor.set_selection_color(theme.button_bg);
    notes_editor.set_frame(FrameType::FlatBox);
    notes_editor.set_scrollbar_size(super::SCROLLBAR_SIZE);

    // Progress bar (initially hidden)
    let mut progress = Progress::default().with_size(0, 25);
    progress.set_minimum(0.0);
    progress.set_maximum(1.0);
    progress.set_color(theme.input_bg);
    progress.set_selection_color(theme.button_bg);
    progress.set_label_color(theme.text);
    progress.hide();
    flex.fixed(&progress, 25);

    let mut status_frame = Frame::default().with_size(0, 20);
    status_frame.set_label_size(11);
    status_frame.set_label_color(theme.text_dim);
    status_frame.set_frame(FrameType::FlatBox);
    status_frame.set_color(dialog_bg);
    status_frame.hide();
    flex.fixed(&status_frame, 20);

    // Buttons row
    let mut button_row = Flex::default();
    button_row.set_type(fltk::group::FlexType::Row);
    button_row.set_spacing(10);
    button_row.set_color(dialog_bg);

    let mut install_btn = Button::default().with_label("Install Now");
    install_btn.set_frame(FrameType::RFlatBox);
    install_btn.set_color(theme.button_bg);
    install_btn.set_label_color(theme.text);

    let mut download_btn = Button::default().with_label("View on GitHub");
    download_btn.set_frame(FrameType::RFlatBox);
    download_btn.set_color(theme.button_bg);
    download_btn.set_label_color(theme.text);

    let mut skip_btn = Button::default().with_label("Skip This Version");
    skip_btn.set_frame(FrameType::RFlatBox);
    skip_btn.set_color(theme.button_bg);
    skip_btn.set_label_color(theme.text);

    let mut later_btn = Button::default().with_label("Remind Later");
    later_btn.set_frame(FrameType::RFlatBox);
    later_btn.set_color(theme.button_bg);
    later_btn.set_label_color(theme.text);

    if direct_asset.is_none() {
        install_btn.deactivate();
        install_btn.set_label("Manual Update Only");
    }

    button_row.end();
    flex.fixed(&button_row, 35);

    flex.end();
    dialog.end();

    // Install Now button
    if direct_asset.is_some() {
        let mut progress_bar = progress.clone();
        let mut status = status_frame.clone();
        let mut btn_row = button_row.clone();
        let release_for_install = release.clone();
        install_btn.set_callback(move |_| {
            progress_bar.show();
            status.show();
            status.set_label("Starting download...");
            btn_row.deactivate();

            let release_clone = release_for_install.clone();
            let p_bar = progress_bar.clone();
            let s_frame_download = status.clone();
            let s_frame_install = status.clone();
            let btn_row_download_err = btn_row.clone();
            let btn_row_install_err = btn_row.clone();

            std::thread::spawn(move || {
                let temp_dir = std::env::temp_dir();
                let temp_file = temp_dir.join("ferrispad_update");

                // Use verified download
                let result = updater::download_and_verify(&release_clone, &temp_file, |p| {
                    let mut p_val = p_bar.clone();
                    let mut s_val = s_frame_download.clone();
                    app::awake_callback(move || {
                        p_val.set_value(p as f64);
                        if p < 0.5 {
                            s_val.set_label(&format!("Downloading: {:.0}%", p * 200.0));
                        } else if p < 0.9 {
                            s_val.set_label("Verifying signature...");
                        } else {
                            s_val.set_label("Verified! Writing to disk...");
                        }
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
                            s_err.set_label("Download/verification failed");
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
    theme.apply_titlebar(&dialog);
    // Reposition to center on the parent AFTER show().
    dialog.resize(
        parent.x() + (parent.w() - DW) / 2,
        parent.y() + (parent.h() - DH) / 2,
        DW,
        DH,
    );
    super::run_dialog(&dialog);
}
