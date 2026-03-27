#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_os = "windows"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

pub use ferris_pad::app;
pub use ferris_pad::ui;
mod dispatch;

use fltk::{app as fltk_app, prelude::*};
use std::cell::RefCell;
use std::env;
use std::rc::Rc;

use crate::app::domain::settings::TreePanelPosition;
use crate::app::infrastructure::defer::defer_send;
use crate::app::services::editor_context::EditorContextWriter;
use crate::app::services::session;
use crate::app::services::shortcut_registry::ShortcutRegistry;
use crate::app::services::updater::{UpdateCheckResult, check_for_updates, should_check_now};
use crate::app::state::AppState;
use crate::app::{AppSettings, Message, ThemeMode, detect_system_dark_mode};
use crate::ui::main_window::{LayoutWidgets, build_main_window};
use crate::ui::menu::build_menu;
#[cfg(target_os = "windows")]
use crate::ui::theme::set_windows_titlebar_theme;

/// Parsed CLI arguments.
struct CliArgs {
    mcp_server: bool,
    files: Vec<String>,
    goto_line: Option<usize>,
    new_tab: bool,
}

fn parse_cli_args() -> CliArgs {
    let mut args = CliArgs {
        mcp_server: false,
        files: Vec::new(),
        goto_line: None,
        new_tab: false,
    };

    let raw: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    let mut flags_done = false;

    while i < raw.len() {
        let arg = &raw[i];

        // macOS Process Serial Number — skip silently
        if arg.starts_with("-psn") {
            i += 1;
            continue;
        }

        // Stop flag parsing after --
        if !flags_done && arg == "--" {
            flags_done = true;
            i += 1;
            continue;
        }

        if !flags_done && arg.starts_with('-') {
            match arg.as_str() {
                "--mcp-server" => args.mcp_server = true,
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                "--version" | "-v" => {
                    println!("FerrisPad {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                "--line" | "-l" => {
                    i += 1;
                    if let Some(n) = raw.get(i).and_then(|s| s.parse::<usize>().ok()) {
                        args.goto_line = Some(n);
                    } else {
                        eprintln!("Error: --line requires a number");
                        std::process::exit(1);
                    }
                }
                "--new" | "-n" => args.new_tab = true,
                other => {
                    eprintln!("Unknown option: {other}");
                    eprintln!("Try 'FerrisPad --help' for usage information.");
                    std::process::exit(1);
                }
            }
        } else {
            args.files.push(arg.clone());
        }

        i += 1;
    }

    args
}

fn print_help() {
    println!(
        "FerrisPad {} — A blazingly fast, minimalist text editor\n\n\
         USAGE:\n    \
             FerrisPad [OPTIONS] [FILE...]\n    \
             fpad [OPTIONS] [FILE...]\n\n\
         OPTIONS:\n    \
             -l, --line <N>       Go to line N after opening (applies to last file)\n    \
             -n, --new            Open a new empty tab\n    \
             -v, --version        Print version and exit\n    \
             -h, --help           Print this help and exit\n    \
             --mcp-server         Run as MCP bridge (internal)\n\n\
         EXAMPLES:\n    \
             fpad                         Open FerrisPad\n    \
             fpad file.txt                Open a file\n    \
             fpad file1.txt file2.txt     Open multiple files as tabs\n    \
             fpad --line 42 main.rs       Open file and jump to line 42\n    \
             fpad .                       Open with current directory as project",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() {
    // Parse CLI arguments before anything else (--help/--version exit immediately)
    let cli_args = parse_cli_args();

    if cli_args.mcp_server {
        app::mcp::run_bridge();
    }

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

    let _ = fltk_app::lock();
    let app = fltk_app::App::default().with_scheme(fltk_app::AppScheme::Gtk);
    fltk_app::set_scrollbar_size(12);
    fltk_app::set_visible_focus(false);
    // Lower FLTK's CIELAB contrast threshold from default 39 to 10.
    // The default overrides our custom tree label colors (git status amber/green/red)
    // when selected in light mode. Level 10 preserves them while still protecting
    // against truly unreadable combinations (e.g., white-on-white).
    fltk::app::set_contrast_level(10);

    // Set subtle rounded corners for RFlatBox widgets globally (min is 5, default is 15)
    fltk_app::set_frame_border_radius_max(5);

    let (sender, receiver) = fltk_app::channel::<Message>();

    // Load settings
    let settings = AppSettings::load();
    let tabs_enabled = settings.tabs_enabled;
    let initial_dark_mode = match settings.theme_mode {
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
        ThemeMode::SystemDefault => detect_system_dark_mode(),
    };

    // Read tree panel position from file-explorer plugin config
    let tree_position = settings
        .plugin_configs
        .get("file-explorer")
        .and_then(|c| c.params.get("position"))
        .map(|s| TreePanelPosition::from_config_str(s))
        .unwrap_or_default();

    // Build UI widgets (tab bar included only when tabs enabled)
    let mut w = build_main_window(tabs_enabled, &sender, tree_position);

    // Build shortcut registry from settings
    let shortcut_registry = ShortcutRegistry::from_settings(&settings.shortcut_overrides);

    // Build menu (all items are one-liner message sends)
    build_menu(
        &mut w.menu,
        &sender,
        &settings,
        initial_dark_mode,
        tabs_enabled,
        &shortcut_registry,
    );

    // Start MCP TCP server before AppState (plugins need the port file during init)
    let mcp_responses = app::mcp::start_tcp_server(sender).map(|(_port, responses)| responses);

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
    if let Some(responses) = mcp_responses {
        state.mcp_responses = responses;
    }

    // Bind the initial document's buffer to the editor
    state.bind_active_buffer();

    // Set up Tab key handler for "use spaces" feature.
    // Replace FLTK's built-in Tab binding with a no-op so our handle() is the sole handler.
    state.editor.add_key_binding(
        fltk::enums::Key::Tab,
        fltk::enums::Shortcut::None,
        |_key, _editor| 1, // 1 = handled (do nothing, our handle() already inserted text)
    );
    {
        let settings_ref = app_settings.clone();
        state.editor.handle(move |editor, event| {
            if event == fltk::enums::Event::KeyDown
                && fltk::app::event_key() == fltk::enums::Key::Tab
            {
                let s = settings_ref.borrow();
                if let Some(mut buf) = editor.buffer() {
                    let pos = editor.insert_position();
                    if s.use_spaces {
                        let spaces = " ".repeat(s.tab_size as usize);
                        buf.insert(pos, &spaces);
                        editor.set_insert_position(pos + s.tab_size as i32);
                    } else {
                        buf.insert(pos, "\t");
                        editor.set_insert_position(pos + 1);
                    }
                }
                return true;
            }
            false
        });
    }

    // Apply initial settings (theme, font, line numbers, word wrap)
    state.apply_settings(settings.clone());

    // Populate plugins menu with loaded plugins (before wind.end()/show()
    // so FLTK registers shortcuts the same way as built-in menu items)
    ui::menu::rebuild_plugins_menu(
        &mut state.menu,
        &state.sender,
        &state.settings.borrow(),
        &state.plugins,
        &state.shortcut_registry,
    );

    // Update menus based on active file type (preview)
    state.update_menus_for_file_type();

    // Defer session restore and CLI file open until after the window is shown,
    // so the UI appears immediately instead of blocking on large file reads.
    defer_send(sender, 0.0, Message::DeferredSessionRestore);

    // CLI: open files after session restore (staggered delays for ordering)
    let mut file_count = 0;
    for path in &cli_args.files {
        let abs_path = std::path::Path::new(path);
        if abs_path.is_dir() {
            // Directory arg: not yet supported (needs Start Page feature)
            continue;
        }
        // Resolve to absolute path (user may pass relative paths)
        let resolved = if abs_path.is_absolute() {
            path.clone()
        } else {
            env::current_dir()
                .map(|cwd| cwd.join(path).to_string_lossy().to_string())
                .unwrap_or_else(|_| path.clone())
        };
        let delay = 0.01 + (file_count as f64 * 0.01);
        defer_send(sender, delay, Message::DeferredOpenFile(resolved));
        file_count += 1;
    }

    // CLI: --line N — jump to line after files are loaded
    if let Some(line) = cli_args.goto_line {
        let delay = 0.01 + (file_count as f64 * 0.01) + 0.02;
        defer_send(sender, delay, Message::DeferredGotoLine(line));
    }

    // CLI: --new — open a new empty tab
    if cli_args.new_tab {
        let delay = 0.01 + (file_count as f64 * 0.01) + 0.01;
        defer_send(sender, delay, Message::FileNew);
    }

    // Window event handler for close and resize.
    // Using handle() with Event::Close to catch close even when menu is open.
    w.wind.handle({
        let s = sender;
        move |wind, event| {
            match event {
                fltk::enums::Event::Close => {
                    fltk::app::program_should_quit(true);
                    s.send(Message::WindowClose);
                    // Hide the window to break out of any modal loops (like open menus)
                    wind.hide();
                    true
                }
                fltk::enums::Event::Resize => {
                    s.send(Message::WindowResize);
                    false // Let FLTK handle the resize too
                }
                fltk::enums::Event::Focus => {
                    s.send(Message::WindowFocusGained);
                    false // Let FLTK handle focus too
                }
                _ => false,
            }
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

    // Set up diagnostic panel click and hover handlers
    w.diagnostic_panel.setup_click_handler();
    w.diagnostic_panel.setup_hover_handler();

    // Extract layout widgets from partially-moved MainWidgets for use in dispatch loop.
    // w.editor_container and w.tab_bar were already moved into AppState above.
    let mut lw = LayoutWidgets {
        wind: w.wind,
        flex: w.flex,
        split_panel: w.split_panel,
        diagnostic_panel: w.diagnostic_panel,
        tree_panel: w.tree_panel,
        terminal_panel: w.terminal_panel,
        toast: w.toast,
        status_bar: w.status_bar,
        content_row: w.content_row,
        right_col: w.right_col,
        tree_position: w.tree_position,
    };

    // Apply initial theme to status bar
    lw.status_bar
        .apply_theme(state.highlight.highlighter().theme_background());

    // Check plugin permissions now that UI is ready (dialog needs event loop)
    sender.send(Message::CheckPluginPermissions);

    // Background plugin update check
    {
        use crate::app::services::plugin_update_checker::should_check_plugin_updates;

        let settings_lock = app_settings.borrow();
        let plugins_enabled = settings_lock.plugins_enabled;
        let auto_check_plugin_updates = settings_lock.auto_check_plugin_updates;
        let should_check = should_check_plugin_updates(settings_lock.last_plugin_update_check);
        drop(settings_lock);

        if plugins_enabled && auto_check_plugin_updates && should_check {
            state.sender.send(Message::CheckPluginUpdates);
        }
    }

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

    // Editor context file writer (for AI agent selection context)
    let mut editor_context = EditorContextWriter::new();

    // Track whether file_quit() completed successfully
    let mut quit_clean = false;

    // Main event loop with message dispatch
    while app.wait() {
        if let Some(msg) = receiver.recv() {
            let result = match msg {
                // File
                Message::FileNew
                | Message::FileOpen
                | Message::FileReload
                | Message::FileReloadAll
                | Message::FileSave
                | Message::FileSaveAs
                | Message::FileQuit
                | Message::WindowClose => dispatch::handle_file(msg, &mut state),

                // Tabs
                Message::TabSwitch(_)
                | Message::TabClose(_)
                | Message::TabCloseActive
                | Message::TabMove(..)
                | Message::TabNext
                | Message::TabPrevious
                | Message::TabGroupCreate(_)
                | Message::TabGroupDelete(_)
                | Message::TabGroupClose(_)
                | Message::TabGroupRename(_)
                | Message::TabGroupRecolor(..)
                | Message::TabGroupAddTab(..)
                | Message::TabGroupRemoveTab(_)
                | Message::TabGroupToggle(_)
                | Message::TabGroupByDrag(..)
                | Message::TabGroupMove(..)
                | Message::TabMoveToGroup(..) => dispatch::handle_tab(msg, &mut state, &mut lw),

                // Edit
                Message::EditUndo
                | Message::EditRedo
                | Message::EditCut
                | Message::EditCopy
                | Message::EditPaste
                | Message::SelectAll
                | Message::ShowFind
                | Message::ShowReplace
                | Message::ShowGoToLine => {
                    dispatch::handle_edit(msg, &mut state);
                    dispatch::DispatchResult::Continue
                }

                // View & Format
                Message::ToggleLineNumbers
                | Message::ToggleWordWrap
                | Message::ToggleDarkMode
                | Message::ToggleHighlighting
                | Message::TogglePreview
                | Message::SetFont(_)
                | Message::SetFontSize(_) => {
                    dispatch::handle_view(msg, &mut state, &mut lw);
                    dispatch::DispatchResult::Continue
                }

                // Settings & Help
                Message::OpenSettings
                | Message::CheckForUpdates
                | Message::ShowAbout
                | Message::ShowKeyShortcuts => {
                    dispatch::handle_settings(msg, &mut state, &mut lw);
                    dispatch::DispatchResult::Continue
                }

                // Syntax highlighting
                Message::BufferModified { .. }
                | Message::DoRehighlight
                | Message::ContinueHighlight
                | Message::DoTextChangeHook => {
                    dispatch::handle_highlight(msg, &mut state);
                    dispatch::DispatchResult::Continue
                }

                // Background updates
                Message::BackgroundUpdateResult(_)
                | Message::ShowBannerUpdate
                | Message::DismissBanner
                | Message::PreviewSyntaxTheme(_) => {
                    dispatch::handle_update(msg, &mut state);
                    dispatch::DispatchResult::Continue
                }

                // Plugins
                Message::PluginsToggleGlobal
                | Message::PluginToggle(_)
                | Message::PluginsReloadAll
                | Message::CheckPluginPermissions
                | Message::PluginMenuAction { .. }
                | Message::ShowPluginManager
                | Message::ShowPluginSettings
                | Message::ShowPluginConfig(_)
                | Message::CheckPluginUpdates
                | Message::PluginUpdatesChecked(_) => {
                    dispatch::handle_plugin(msg, &mut state);
                    dispatch::DispatchResult::Continue
                }

                // Diagnostics
                Message::DiagnosticsUpdate(_)
                | Message::DiagnosticsClear
                | Message::DiagnosticsAutoDismiss
                | Message::DiagnosticGoto(_)
                | Message::DiagnosticOpenDocs(_)
                | Message::ToggleDiagnosticsPanel => {
                    dispatch::handle_diagnostic(msg, &mut state, &mut lw);
                    dispatch::DispatchResult::Continue
                }

                // Annotations
                Message::AnnotationsUpdate(_)
                | Message::AnnotationsClear
                | Message::ManualHighlight => {
                    dispatch::handle_annotation(msg, &mut state);
                    dispatch::DispatchResult::Continue
                }

                // Deferred actions
                Message::DeferredPluginHooks { .. }
                | Message::DeferredTreeRefresh { .. }
                | Message::DeferredSessionRestore
                | Message::DeferredOpenFile(_)
                | Message::DeferredGotoLine(_) => {
                    dispatch::handle_deferred(msg, &mut state, &mut lw, tabs_enabled);
                    dispatch::DispatchResult::Continue
                }

                // Toast
                Message::ToastShow(..) | Message::ToastHide => {
                    dispatch::handle_toast(msg, &mut lw);
                    dispatch::DispatchResult::Continue
                }

                // Split view
                Message::SplitViewShow { .. }
                | Message::SplitViewAccept(_)
                | Message::SplitViewReject(_)
                | Message::SplitViewResize(_)
                | Message::DiffTabActivate(_)
                | Message::SplitViewToggleMode(_) => {
                    dispatch::handle_split_view(msg, &mut state, &mut lw);
                    dispatch::DispatchResult::Continue
                }

                // Tree view
                Message::TreeViewShow { .. }
                | Message::TreeViewHide(_)
                | Message::TreeViewLoading
                | Message::TreeViewNodeClicked { .. }
                | Message::TreeViewNodeExpanded { .. }
                | Message::TreeViewContextAction { .. }
                | Message::TreeViewSearch { .. }
                | Message::TreeViewResize(_) => {
                    dispatch::handle_tree_view(msg, &mut state, &mut lw);
                    dispatch::DispatchResult::Continue
                }

                // Terminal view
                Message::TerminalViewShow { .. }
                | Message::TerminalViewHide(_)
                | Message::TerminalOutput(_)
                | Message::TerminalExited
                | Message::TerminalViewResize(_) => {
                    dispatch::handle_terminal_view(msg, &mut state, &mut lw);
                    dispatch::DispatchResult::Continue
                }

                // MCP
                Message::McpRequest { .. } => {
                    dispatch::handle_mcp(msg, &mut state);
                    dispatch::DispatchResult::Continue
                }

                // Window events
                Message::WindowResize | Message::MallocTrim => {
                    dispatch::handle_window(msg, &mut state, &mut lw);
                    dispatch::DispatchResult::Continue
                }

                Message::WindowFocusGained => {
                    state.check_and_reload_external_changes();
                    state.refresh_tree_if_visible();
                    dispatch::DispatchResult::Continue
                }
            };
            if matches!(result, dispatch::DispatchResult::Quit) {
                quit_clean = true;
                fltk_app::quit();
            }
        }

        // Update status bar and editor context on every event loop iteration,
        // not just message dispatch — mouse selection doesn't generate messages.
        lw.status_bar.update(&state.editor);
        let file_path = state
            .tab_manager
            .active_doc()
            .and_then(|d| d.file_path.as_deref());
        editor_context.update(&state.editor, file_path);

        state.session.auto_save_if_needed(
            &state.tab_manager,
            &state.settings,
            state.file.last_open_directory.as_deref(),
        );
    }

    // Clean up MCP port file and editor context file
    app::mcp::cleanup_port_file();
    editor_context.cleanup();

    // Safety-net: save session if file_quit() was never called or didn't complete
    if !quit_clean {
        let session_mode = state.settings.borrow().session_restore;
        let _ = session::save_session(
            &state.tab_manager,
            session_mode,
            state.file.last_open_directory.as_deref(),
        )
        .inspect_err(|e| eprintln!("Post-loop session save failed: {}", e));
    }
}
