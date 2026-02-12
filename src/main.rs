#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod ui;

use fltk::{app as fltk_app, prelude::*};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::env;

use crate::app::messages::Message;
use crate::app::platform::detect_system_dark_mode;
use crate::app::state::AppState;
use crate::app::settings::{AppSettings, ThemeMode};
use crate::ui::dialogs::about::show_about_dialog;
use crate::ui::dialogs::find::{show_find_dialog, show_replace_dialog};
use crate::ui::dialogs::goto_line::show_goto_line_dialog;
use crate::ui::dialogs::update::{check_for_updates_ui, show_update_available_dialog};
use crate::ui::main_window::build_main_window;
use crate::ui::menu::build_menu;
use crate::app::updater::{check_for_updates, current_timestamp, should_check_now, UpdateCheckResult};
#[cfg(target_os = "windows")]
use crate::ui::theme::set_windows_titlebar_theme;

fn main() {
    let _ = fltk_app::lock();
    let app = fltk_app::App::default().with_scheme(fltk_app::AppScheme::Gtk);
    let (sender, receiver) = fltk_app::channel::<Message>();

    // Build UI widgets
    let mut w = build_main_window();

    // Load settings
    let settings = AppSettings::load();
    let initial_dark_mode = match settings.theme_mode {
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
        ThemeMode::SystemDefault => detect_system_dark_mode(),
    };

    // Build menu (all items are one-liner message sends)
    build_menu(&mut w.menu, &sender, &settings, initial_dark_mode);

    // Initialize state
    let app_settings = Rc::new(RefCell::new(settings.clone()));
    let has_unsaved_changes = Rc::new(Cell::new(false));

    let mut state = AppState::new(
        w.text_buf.clone(),
        w.text_editor.clone(),
        w.wind.clone(),
        w.menu.clone(),
        w.flex.clone(),
        w.update_banner_frame.clone(),
        has_unsaved_changes.clone(),
        app_settings.clone(),
        initial_dark_mode,
        settings.line_numbers_enabled,
        settings.word_wrap_enabled,
    );

    // Apply initial settings (theme, font, line numbers, word wrap)
    state.apply_settings(settings.clone());

    // Open file from CLI args if provided
    let args: Vec<String> = env::args().collect();
    if let Some(path) = args.iter().skip(1).find(|arg| !arg.starts_with("-psn")) {
        state.open_file(path.clone());
    }

    // Cursor blink timer
    let cursor_visible = Rc::new(Cell::new(true));
    let mut editor_blink = w.text_editor.clone();
    let cursor_state = cursor_visible.clone();
    fltk_app::add_timeout3(0.5, move |handle| {
        let visible = !cursor_state.get();
        cursor_state.set(visible);
        editor_blink.show_cursor(visible);
        editor_blink.redraw();
        fltk_app::repeat_timeout3(0.5, handle);
    });

    // Text change detection
    let changes_state = has_unsaved_changes.clone();
    w.text_buf.add_modify_callback(move |_, _, _, _, _| {
        changes_state.set(true);
    });

    // Window close button -> send message
    w.wind.set_callback({
        let s = sender.clone();
        move |_| s.send(Message::WindowClose)
    });

    // Banner click/dismiss handlers
    w.update_banner_frame.handle({
        let s = sender.clone();
        move |_, event| match event {
            fltk::enums::Event::Push => {
                s.send(Message::ShowBannerUpdate);
                true
            }
            fltk::enums::Event::KeyDown => {
                if fltk_app::event_key() == fltk::enums::Key::Escape {
                    s.send(Message::DismissBanner);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    });

    w.wind.end();
    w.wind.show();

    #[cfg(target_os = "windows")]
    set_windows_titlebar_theme(&w.wind, initial_dark_mode);

    // Background update check via channel
    {
        let settings_lock = app_settings.borrow();
        let auto_check = settings_lock.auto_check_updates;
        let should_check = should_check_now(settings_lock.last_update_check);
        let channel = settings_lock.update_channel;
        let skipped = settings_lock.skipped_versions.clone();
        drop(settings_lock);

        if auto_check && should_check {
            let s = sender.clone();
            std::thread::spawn(move || {
                let current_version = env!("CARGO_PKG_VERSION");
                let result = check_for_updates(current_version, channel, &skipped);
                match result {
                    UpdateCheckResult::UpdateAvailable(release) => {
                        s.send(Message::BackgroundUpdateResult(Some(release)));
                    }
                    _ => {
                        s.send(Message::BackgroundUpdateResult(None));
                    }
                }
            });
        }
    }

    // Main event loop with message dispatch
    while app.wait() {
        if let Some(msg) = receiver.recv() {
            match msg {
                // File
                Message::FileNew => state.file_new(),
                Message::FileOpen => state.file_open(),
                Message::FileSave => state.file_save(),
                Message::FileSaveAs => state.file_save_as(),
                Message::FileQuit | Message::WindowClose => {
                    if state.file_quit() {
                        fltk_app::quit();
                    }
                }

                // Edit
                Message::EditUndo => { let _ = state.buffer.undo(); }
                Message::EditRedo => { let _ = state.buffer.redo(); }
                Message::EditCut => { state.editor.cut(); }
                Message::EditCopy => { state.editor.copy(); }
                Message::EditPaste => { state.editor.paste(); }
                Message::SelectAll => { state.buffer.select(0, state.buffer.length()); }
                Message::ShowFind => show_find_dialog(&state.buffer, &mut state.editor),
                Message::ShowReplace => show_replace_dialog(&state.buffer, &mut state.editor),
                Message::ShowGoToLine => show_goto_line_dialog(&state.buffer, &mut state.editor),

                // View
                Message::ToggleLineNumbers => state.toggle_line_numbers(),
                Message::ToggleWordWrap => state.toggle_word_wrap(),
                Message::ToggleDarkMode => state.toggle_dark_mode(),

                // Format
                Message::SetFont(font) => state.set_font(font),
                Message::SetFontSize(size) => state.set_font_size(size),

                // Settings & Help
                Message::OpenSettings => state.open_settings(),
                Message::CheckForUpdates => check_for_updates_ui(&state.settings),
                Message::ShowAbout => show_about_dialog(),

                // Background updates
                Message::BackgroundUpdateResult(Some(release)) => {
                    state.show_update_banner(&release.version());
                    state.pending_update = Some(release);
                    let mut s = state.settings.borrow_mut();
                    s.last_update_check = current_timestamp();
                    let _ = s.save();
                }
                Message::BackgroundUpdateResult(None) => {
                    let mut s = state.settings.borrow_mut();
                    s.last_update_check = current_timestamp();
                    let _ = s.save();
                }
                Message::ShowBannerUpdate => {
                    if let Some(release) = state.pending_update.take() {
                        show_update_available_dialog(release, &state.settings);
                        state.hide_update_banner();
                    }
                }
                Message::DismissBanner => {
                    state.hide_update_banner();
                }
            }
        }
    }
}
