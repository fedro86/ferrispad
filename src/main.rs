#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_os = "windows"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod app;
mod ui;

use fltk::{app as fltk_app, prelude::*};
use std::cell::RefCell;
use std::rc::Rc;
use std::env;

use crate::app::messages::Message;
use crate::app::platform::detect_system_dark_mode;
use crate::app::state::AppState;
use crate::app::settings::{AppSettings, ThemeMode};
use crate::ui::dialogs::about::show_about_dialog;
use crate::ui::dialogs::find::{show_find_dialog, show_replace_dialog};
use crate::ui::dialogs::goto_line::show_goto_line_dialog;
use crate::app::update_controller::BannerWidgets;
use crate::ui::dialogs::update::check_for_updates_ui;
use crate::ui::main_window::build_main_window;
use crate::ui::menu::build_menu;
use crate::app::updater::{check_for_updates, current_timestamp, should_check_now, UpdateCheckResult};
use crate::app::preview_controller;
#[cfg(target_os = "windows")]
use crate::ui::theme::set_windows_titlebar_theme;

fn main() {
    // Strip snap library paths from LD_LIBRARY_PATH before GTK loads.
    // Snap's broken libpthread causes crashes when GTK is initialized.
    #[cfg(target_os = "linux")]
    {
        if let Ok(val) = env::var("LD_LIBRARY_PATH") {
            let cleaned: String = val
                .split(':')
                .filter(|p| !p.contains("/snap/"))
                .collect::<Vec<_>>()
                .join(":");
            // SAFETY: must happen before any threads or GTK init
            unsafe { env::set_var("LD_LIBRARY_PATH", &cleaned) };
        }
    }

    // Configure jemalloc to immediately return freed pages to the OS.
    // Without this, jemalloc keeps dirty/muzzy pages mapped, inflating RSS.
    #[cfg(not(target_os = "windows"))]
    {
        let decay: isize = 0;
        // SAFETY: jemalloc is initialized before main() by the global_allocator.
        // These mallctl writes configure decay timing; invalid keys or values
        // are silently ignored by jemalloc rather than causing UB.
        // Must be called early before significant allocations occur.
        unsafe {
            let _ = tikv_jemalloc_ctl::raw::write(b"arenas.dirty_decay_ms\0", decay);
            let _ = tikv_jemalloc_ctl::raw::write(b"arenas.muzzy_decay_ms\0", decay);
        }
    }

    // Clean up stale temp images from previous runs (crash recovery)
    preview_controller::cleanup_temp_images();

    let _ = fltk_app::lock();
    let app = fltk_app::App::default().with_scheme(fltk_app::AppScheme::Gtk);

    // Register PNG/JPEG/GIF handlers so HelpView can display images
    {
        unsafe extern "C" { fn Fl_register_images(); }
        // SAFETY: Fl_register_images() is a standard FLTK initialization function
        // that registers image format handlers. Must be called once after FLTK
        // is initialized (App::default() above) and before loading any images.
        // It's idempotent - multiple calls are safe but unnecessary.
        unsafe { Fl_register_images(); }
    }
    let (sender, receiver) = fltk_app::channel::<Message>();

    // Load settings
    let settings = AppSettings::load();
    let tabs_enabled = settings.tabs_enabled;
    let initial_dark_mode = match settings.theme_mode {
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
        ThemeMode::SystemDefault => detect_system_dark_mode(),
    };

    // Build UI widgets (tab bar included only when tabs enabled)
    let mut w = build_main_window(tabs_enabled, &sender);

    // Build menu (all items are one-liner message sends)
    build_menu(&mut w.menu, &sender, &settings, initial_dark_mode, tabs_enabled);

    // Initialize state
    let app_settings = Rc::new(RefCell::new(settings.clone()));

    let mut state = AppState::new(
        w.editor_container,
        w.wind.clone(),
        w.menu.clone(),
        w.flex.clone(),
        w.update_banner_frame.clone(),
        sender,
        app_settings.clone(),
        initial_dark_mode,
        settings.line_numbers_enabled,
        settings.word_wrap_enabled,
        tabs_enabled,
        w.tab_bar,
    );

    // Bind the initial document's buffer to the editor
    state.bind_active_buffer();

    // Apply initial settings (theme, font, line numbers, word wrap)
    state.apply_settings(settings.clone());

    // Restore session if enabled (before CLI args so args can override)
    state.restore_session();

    // Open file from CLI args if provided
    let args: Vec<String> = env::args().collect();
    if let Some(path) = args.iter().skip(1).find(|arg| !arg.starts_with("-psn")) {
        state.open_file(path.clone());
    }

    // Window close button -> signal quit so nested dialog loops break out,
    // then send the message for the main event loop to handle.
    w.wind.set_callback({
        let s = sender;
        move |_| {
            fltk::app::program_should_quit(true);
            s.send(Message::WindowClose);
        }
    });

    // Banner click/dismiss handlers
    w.update_banner_frame.handle({
        let s = sender;
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

    // Build initial tab bar after window is shown (so Flex layout is resolved)
    if tabs_enabled {
        state.rebuild_tab_bar();
    }

    // Start deferred highlighting for session-restored documents
    state.start_queued_highlights();

    // Background update check via channel
    {
        let settings_lock = app_settings.borrow();
        let auto_check = settings_lock.auto_check_updates;
        let should_check = should_check_now(settings_lock.last_update_check);
        let channel = settings_lock.update_channel;
        let skipped = settings_lock.skipped_versions.clone();
        drop(settings_lock);

        if auto_check && should_check {
            let s = sender;
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
                Message::FileNew => { state.file_new(); state.mark_session_dirty(); }
                Message::FileOpen => { state.file_open(); state.mark_session_dirty(); }
                Message::FileSave => { state.file_save(); state.mark_session_dirty(); }
                Message::FileSaveAs => { state.file_save_as(); state.mark_session_dirty(); }
                Message::FileQuit | Message::WindowClose => {
                    if state.file_quit() {
                        fltk_app::quit();
                    } else {
                        // User cancelled the quit — reset the flag
                        fltk::app::program_should_quit(false);
                    }
                }

                // Tabs
                Message::TabSwitch(id) => {
                    state.switch_to_document(id);
                    state.rebuild_tab_bar();
                }
                Message::TabClose(id) => {
                    if state.close_tab(id) {
                        fltk_app::quit();
                    }
                    state.mark_session_dirty();
                }
                Message::TabCloseActive => {
                    if let Some(id) = state.tab_manager.active_id()
                        && state.close_tab(id) {
                            fltk_app::quit();
                        }
                    state.mark_session_dirty();
                }
                Message::TabMove(from, to) => {
                    state.tab_manager.move_tab(from, to);
                    state.rebuild_tab_bar();
                    state.mark_session_dirty();
                }
                Message::TabNext => state.switch_to_next_tab(),
                Message::TabPrevious => state.switch_to_previous_tab(),

                // Tab Groups
                Message::TabGroupCreate(doc_id) => { state.handle_group_create(doc_id); state.mark_session_dirty(); }
                Message::TabGroupDelete(group_id) => { state.handle_group_delete(group_id); state.mark_session_dirty(); }
                Message::TabGroupClose(group_id) => { state.handle_group_close(group_id); state.mark_session_dirty(); }
                Message::TabGroupRename(group_id) => { state.handle_group_rename(group_id); state.mark_session_dirty(); }
                Message::TabGroupRecolor(group_id, color) => { state.handle_group_recolor(group_id, color); state.mark_session_dirty(); }
                Message::TabGroupAddTab(doc_id, group_id) => { state.handle_group_add_tab(doc_id, group_id); state.mark_session_dirty(); }
                Message::TabGroupRemoveTab(doc_id) => { state.handle_group_remove_tab(doc_id); state.mark_session_dirty(); }
                Message::TabGroupToggle(group_id) => { state.handle_group_toggle(group_id); state.mark_session_dirty(); }
                Message::TabGroupByDrag(source_id, target_id) => { state.handle_group_by_drag(source_id, target_id); state.mark_session_dirty(); }
                Message::TabGroupMove(group_id, to) => {
                    state.tab_manager.move_group(group_id, to);
                    state.rebuild_tab_bar();
                    state.mark_session_dirty();
                }

                // Edit
                Message::EditUndo => { let _ = state.active_buffer().undo(); }
                Message::EditRedo => { let _ = state.active_buffer().redo(); }
                Message::EditCut => { state.editor.cut(); }
                Message::EditCopy => { state.editor.copy(); }
                Message::EditPaste => { state.editor.paste(); }
                Message::SelectAll => {
                    let mut buf = state.active_buffer();
                    buf.select(0, buf.length());
                }
                Message::ShowFind => show_find_dialog(&state.active_buffer(), &mut state.editor),
                Message::ShowReplace => show_replace_dialog(&state.active_buffer(), &mut state.editor),
                Message::ShowGoToLine => show_goto_line_dialog(&state.active_buffer(), &mut state.editor),

                // View
                Message::ToggleLineNumbers => state.toggle_line_numbers(),
                Message::ToggleWordWrap => state.toggle_word_wrap(),
                Message::ToggleDarkMode => state.toggle_dark_mode(),
                Message::ToggleHighlighting => state.toggle_highlighting(),
                Message::TogglePreview => state.toggle_preview(),

                // Format
                Message::SetFont(font) => state.set_font(font),
                Message::SetFontSize(size) => state.set_font_size(size),

                // Settings & Help
                Message::OpenSettings => state.open_settings(),
                Message::CheckForUpdates => check_for_updates_ui(&state.settings),
                Message::ShowAbout => show_about_dialog(),

                // Syntax highlighting (debounced)
                Message::BufferModified(id, pos) => {
                    state.schedule_rehighlight(id, pos);
                    state.mark_session_dirty();
                }
                Message::DoRehighlight => {
                    state.do_pending_rehighlight();
                }
                Message::ContinueHighlight => {
                    state.continue_chunked_highlight();
                }
                Message::ContinueImageResize => {
                    state.continue_image_resize();
                }

                // Background updates
                Message::BackgroundUpdateResult(Some(release)) => {
                    state.update.receive_update(release, &mut BannerWidgets {
                        banner_frame: &mut state.update_banner_frame,
                        flex: &mut state.flex,
                        window: &mut state.window,
                    });
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
                    state.update.show_update_dialog(&state.settings, &mut BannerWidgets {
                        banner_frame: &mut state.update_banner_frame,
                        flex: &mut state.flex,
                        window: &mut state.window,
                    });
                }
                Message::DismissBanner => {
                    state.update.dismiss_banner(&mut BannerWidgets {
                        banner_frame: &mut state.update_banner_frame,
                        flex: &mut state.flex,
                        window: &mut state.window,
                    });
                }

                Message::PreviewSyntaxTheme(theme) => state.preview_syntax_theme(theme),
            }
        }
        state.auto_save_session_if_needed();
    }
}
