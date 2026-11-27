use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use fltk::{
    app::Sender,
    frame::Frame,
    group::Flex,
    menu::MenuBar,
    prelude::*,
    text::{TextBuffer, TextEditor, WrapMode},
    window::Window,
};

use super::controllers::file::{FileAction, FileController};
use super::controllers::highlight::{HighlightController, HighlightWidgets};
use super::controllers::hook_dispatch::{self, HookContext};
use super::controllers::plugin::PluginController;
use super::controllers::preview::PreviewController;
use super::controllers::session::SessionController;
use super::controllers::tabs::{GroupId, TabManager};
use super::controllers::theme_manager::{ThemeManagerAction, ThemeManagerModel};
use super::controllers::update::UpdateController;
use super::controllers::view::ViewController;
use super::controllers::widget::WidgetController;
use super::domain::document::DocumentId;
use super::domain::messages::Message;
use super::domain::settings::{self, AppSettings, ThemeRef, default_syntax_theme};
use super::infrastructure::buffer::{
    buffer_text_no_leak, text_range_bytes_no_leak, text_range_no_leak,
};
use super::infrastructure::defer::defer_send;
use super::infrastructure::platform::detect_system_dark_mode;
use super::mcp::McpResponses;
use super::plugins::HookResult;
use super::plugins::{PluginHook, PluginManager, get_plugin_dir};
use super::services::current_line::{cursor_line, twin_bytes, untwin_bytes};
use super::services::custom_themes::{ThemeStore, themes_dir};
use super::services::session;
use super::services::shortcut_registry::ShortcutRegistry;
use super::services::window_geometry::WindowGeometryTracker;
use crate::ui::dialogs::settings_dialog::{SettingsHook, show_settings_dialog};
use crate::ui::dialogs::theme_manager::show_theme_manager;
use crate::ui::dialogs::{show_themed_choice, show_themed_message};
use crate::ui::editor_container::EditorContainer;
use crate::ui::tab_bar::TabBar;
#[cfg(target_os = "windows")]
use crate::ui::theme::set_windows_titlebar_theme;
use crate::ui::theme::{PanelTheme, Themeable, apply_syntax_theme_colors, apply_theme};
#[cfg(target_os = "macos")]
use crate::ui::theme::{set_macos_titlebar_color, update_macos_title_label};

/// The main window title: `[*]<tab label> - 🦀 FerrisPad [session]`, "*"
/// when the active document has unsaved changes (T0093). `active` is the
/// active document's tab label and dirty state; with no document open (the
/// start page) the title is just the app name.
pub fn window_title(active: Option<(&str, bool)>, session_name: &str) -> String {
    let base_suffix = if cfg!(target_os = "windows") {
        "FerrisPad"
    } else {
        "\u{1f980} FerrisPad"
    };
    let suffix = if session_name == session::DEFAULT_SESSION_NAME {
        base_suffix.to_string()
    } else {
        format!("{} [{}]", base_suffix, session_name)
    };
    match active {
        Some((label, dirty)) => {
            let prefix = if dirty { "*" } else { "" };
            format!("{prefix}{label} - {suffix}")
        }
        None => suffix,
    }
}

pub struct AppState {
    pub tab_manager: TabManager,
    pub tabs_enabled: bool,
    pub tab_bar: Option<TabBar>,
    #[allow(dead_code)] // Holds ownership; editor clone is extracted in constructor
    pub editor_container: EditorContainer,
    pub editor: TextEditor,
    pub window: Window,
    pub menu: MenuBar,
    pub flex: Flex,
    pub update_banner_frame: Frame,
    pub sender: Sender<Message>,
    pub settings: Rc<RefCell<AppSettings>>,
    pub update: UpdateController,
    pub highlight: HighlightController,
    pub preview: PreviewController,
    pub plugins: PluginManager,
    pub shortcut_registry: ShortcutRegistry,
    pub view: ViewController,
    pub session: SessionController,
    pub plugin_coord: PluginController,
    pub file: FileController,
    pub widget: WidgetController,
    /// Pending text change for debounced OnTextChanged hook: (doc_id, pos, inserted, deleted)
    pending_text_change: Option<(DocumentId, i32, i32, i32)>,
    /// Whether a DoTextChangeHook timer is active
    text_change_timer_active: bool,
    /// MCP response channels keyed by request_id
    pub mcp_responses: McpResponses,
    /// Pending diff reviews from MCP tools (session_id → (file_path, decision_fifo))
    /// If decision_fifo is Some, write "accept"/"reject" to it on user action.
    pub pending_diff_reviews: HashMap<u32, (String, Option<String>)>,
    /// Main-window geometry tracked for the close-time save (T0077). Shared
    /// with the window's resize callback and close handler in `main.rs`.
    pub window_geometry: Rc<RefCell<WindowGeometryTracker>>,
}

impl AppState {
    /// Look up approved_commands for the source plugin of a hook result.
    fn approved_commands_for_source(&self, result: &HookResult) -> Vec<String> {
        let source = result.source_plugin.as_deref().unwrap_or("");
        self.plugins
            .list_plugins()
            .iter()
            .find(|p| p.name == source)
            .map(|p| p.approved_commands.clone())
            .unwrap_or_default()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        editor_container: EditorContainer,
        window: Window,
        menu: MenuBar,
        flex: Flex,
        update_banner_frame: Frame,
        sender: Sender<Message>,
        settings: Rc<RefCell<AppSettings>>,
        dark_mode: bool,
        show_linenumbers: bool,
        word_wrap: bool,
        tabs_enabled: bool,
        tab_bar: Option<TabBar>,
        session_name: String,
    ) -> Self {
        let mut tab_manager = TabManager::new(sender);
        tab_manager.add_untitled();

        let font = settings.borrow().current_font();
        let font_size = settings.borrow().font_size_clamped();
        let highlighting_enabled = settings.borrow().highlighting_enabled;
        let syntax_theme = settings.borrow().current_syntax_theme(dark_mode);
        let mut highlight = HighlightController::new(
            default_syntax_theme(dark_mode),
            font,
            font_size,
            highlighting_enabled,
        );
        // Custom themes are read once, here (no directory watching).
        let (theme_store, theme_warnings) = ThemeStore::load(themes_dir());
        for warning in theme_warnings {
            eprintln!("{warning}");
        }
        highlight.set_custom_themes(theme_store.into_themes());
        highlight.set_theme(&syntax_theme, dark_mode);

        let preview = PreviewController::new();

        // Initialize plugin system
        let plugins_enabled = settings.borrow().plugins_enabled;
        let disabled_plugins = settings.borrow().disabled_plugins.clone();
        let plugin_approvals = settings.borrow().plugin_approvals.clone();
        let plugin_configs = settings.borrow().plugin_configs.clone();
        let mut plugins = PluginManager::new(plugins_enabled);
        if plugins_enabled {
            plugins.load_plugins(&get_plugin_dir());

            // Apply previously saved permission approvals and config params
            for plugin in plugins.plugins_mut() {
                if let Some(approvals) = plugin_approvals.get(&plugin.name) {
                    plugin.approved_commands = approvals.approved_commands.clone();
                }
                if let Some(config) = plugin_configs.get(&plugin.name) {
                    plugin.config_params = config.params.clone();
                }
            }

            // NOTE: Permission check is deferred until after UI is ready.
            // The main.rs sends CheckPluginPermissions message after window.show().

            // Apply disabled list from settings
            for name in &disabled_plugins {
                plugins.toggle_plugin(name, false);
            }
            plugins.call_hook(PluginHook::Init {
                project_root: super::mcp::cwd_as_string(),
            });
        }

        let shortcut_registry =
            ShortcutRegistry::from_settings(&settings.borrow().shortcut_overrides);

        let editor = editor_container.editor().clone();

        let view = ViewController::new(editor.clone(), dark_mode, show_linenumbers, word_wrap);
        let session = SessionController::with_session_name(session_name);
        let plugin_coord = PluginController::new(menu.clone(), sender);

        Self {
            tab_manager,
            tabs_enabled,
            tab_bar,
            editor_container,
            editor,
            window,
            menu,
            flex,
            update_banner_frame,
            sender,
            settings,
            update: UpdateController::default(),
            highlight,
            preview,
            plugins,
            shortcut_registry,
            view,
            session,
            plugin_coord,
            file: FileController::default(),
            widget: WidgetController::new(sender),
            pending_text_change: None,
            text_change_timer_active: false,
            mcp_responses: Default::default(),
            pending_diff_reviews: HashMap::new(),
            window_geometry: Rc::new(RefCell::new(WindowGeometryTracker::default())),
        }
    }

    /// Get the active document's buffer
    pub fn active_buffer(&self) -> TextBuffer {
        self.tab_manager
            .active_buffer()
            .expect("No active document")
    }

    /// Bind the active document's buffer to the editor
    pub fn bind_active_buffer(&mut self) {
        if let Some(doc) = self.tab_manager.active_doc() {
            // Save dirty state before rebinding (set_buffer may trigger modify callback)
            let was_dirty = doc.is_dirty();

            self.editor.set_buffer(doc.buffer.clone());
            let style_buf = doc.style_buffer.clone();
            let table = self.highlight.style_table();
            self.editor.set_highlight_data_ext(style_buf, table);

            // Restore dirty state (binding shouldn't mark document dirty)
            if !was_dirty {
                doc.mark_clean();
            }
        }
        self.update_linenumber_width();
        // The active document may have changed (session restore, new
        // session, ...): the file-type menus follow it here, once, instead
        // of in every caller (T0103).
        self.update_menus_for_file_type();
    }

    /// Update the window title based on active document. The label is only
    /// set when the text changes.
    pub fn update_window_title(&mut self) {
        let active = self
            .tab_manager
            .active_doc()
            .map(|doc| (doc.tab_label(), doc.is_dirty()));
        let title = window_title(active, self.session.current_session_name());
        if self.window.label() != title {
            self.window.set_label(&title);
        }
        #[cfg(target_os = "macos")]
        update_macos_title_label(&self.window);
    }

    /// Switch the editor to display a different document
    pub fn switch_to_document(&mut self, id: DocumentId) {
        // Save current doc's cursor/scroll state
        if let Some(current) = self.tab_manager.active_doc_mut() {
            current.cursor_position = self.editor.insert_position();
        }

        // Set new active and expand collapsed group if needed
        self.tab_manager.set_active(id);
        self.tab_manager.expand_group_of(id);

        // Bind new buffer and restore state
        if let Some(doc) = self.tab_manager.active_doc_mut() {
            // IMPORTANT: Save dirty state FIRST, before any operations that might trigger
            // the modify callback (set_tab_distance, set_buffer, etc.)
            let was_dirty = doc.is_dirty();

            // Ensure tab distance is set (for newly created docs)
            let tab_size = self.settings.borrow().tab_size as i32;
            doc.buffer.set_tab_distance(tab_size);

            let buffer = doc.buffer.clone();
            let cursor = doc.cursor_position;
            let style_buf = doc.style_buffer.clone();
            self.editor.set_buffer(buffer);
            let table = self.highlight.style_table();
            self.editor.set_highlight_data_ext(style_buf, table);
            self.editor.set_insert_position(cursor);
            self.editor.show_insert_position();

            // Restore dirty state (binding shouldn't mark document dirty)
            if !was_dirty {
                doc.mark_clean();
            }
        }

        self.update_linenumber_width();
        self.update_window_title();

        // Restore diagnostics for the new active document (or hide panel if never linted)
        if let Some(diagnostics) = self.get_active_diagnostics() {
            self.sender.send(Message::DiagnosticsUpdate(diagnostics));
        } else {
            // Document has never been linted - hide the panel entirely
            self.sender.send(Message::DiagnosticsClear);
        }

        // Update menus based on file type (preview, plugin items)
        self.update_menus_for_file_type();

        // If a tree view is currently visible, refresh it for the new document.
        // YAML/JSON files get a new tree; other files close the stale tree.
        self.widget
            .refresh_tree_view_for_active_doc(&self.tab_manager);
    }

    /// Execute the tree view refresh (called directly or from deferred message).
    /// If the plugin does not return a tree view (e.g., non-YAML file), hides
    /// the tree panel so it doesn't stay open with stale/loading content.
    pub fn run_tree_refresh(&mut self, path: Option<String>, content: String) {
        let open_result = self.plugins.call_hook(PluginHook::OnDocumentOpen {
            path,
            content: Some(content),
        });
        let approved = self.approved_commands_for_source(&open_result);
        self.widget
            .process_widget_requests(&open_result, "", &approved);
        let mut ctx = HookContext {
            tab_manager: &mut self.tab_manager,
            view: &mut self.view,
            widget_manager: &mut self.widget.widget_manager,
            sender: self.sender,
            approved_commands: approved,
        };
        hook_dispatch::dispatch_hook_result(open_result, "", &mut ctx);

        // If the plugin didn't produce a new tree view, hide the panel.
        if self.widget.widget_manager.any_tree_view_session().is_none() {
            self.sender.send(Message::TreeViewHide(0));
        }
    }

    /// Update all file-type-dependent menu items based on the current file.
    /// This includes:
    /// - Preview in Browser (only for markdown)
    /// - Plugin menu items (based on their supported file extensions)
    pub fn update_menus_for_file_type(&mut self) {
        let file_path = self
            .tab_manager
            .active_doc()
            .and_then(|doc| doc.file_path.as_ref())
            .map(|p| p.as_str());

        // Update built-in menus
        crate::ui::menu::update_preview_menu(&mut self.menu, file_path);
    }

    /// Rebuild the tab bar UI from current documents
    pub fn rebuild_tab_bar(&mut self) {
        if let Some(ref mut tab_bar) = self.tab_bar {
            let active_id = self.tab_manager.active_id();
            let theme_bg = self.highlight.highlighter().theme_background();
            tab_bar.rebuild(
                self.tab_manager.documents(),
                self.tab_manager.groups(),
                active_id,
                &self.sender,
                self.view.dark_mode,
                theme_bg,
            );
        }
    }

    /// Close a tab by id. Returns true if the app should exit (no tabs remaining).
    pub fn close_tab(&mut self, id: DocumentId) -> bool {
        // Check if document is dirty
        if let Some(doc) = self.tab_manager.doc_by_id(id)
            && doc.is_dirty()
        {
            let name = doc.display_name.clone();
            let choice = show_themed_choice(
                self.highlight.highlighter().theme_background(),
                "Unsaved Changes",
                &format!("\"{}\" has unsaved changes.", name),
                "Save",
                "Discard",
                "Cancel",
            );

            match choice {
                Some(0) => {
                    let was_active = self.tab_manager.active_id();
                    if was_active != Some(id) {
                        self.switch_to_document(id);
                    }
                    self.file_save();
                    if let Some(doc) = self.tab_manager.doc_by_id(id)
                        && doc.is_dirty()
                    {
                        if let Some(prev) = was_active
                            && prev != id
                        {
                            self.switch_to_document(prev);
                        }
                        return false;
                    }
                }
                Some(1) => {}
                _ => return false,
            }
        }

        // Call plugin hook before closing
        let close_path = self
            .tab_manager
            .doc_by_id(id)
            .and_then(|d| d.file_path.clone());
        self.plugins
            .call_hook(PluginHook::OnDocumentClose { path: close_path });

        self.tab_manager.remove(id);

        if self.tab_manager.count() == 0 {
            // No document left: nothing to preview (T0103), and the title
            // drops "Untitled" for the start page (T0093).
            self.update_menus_for_file_type();
            self.update_window_title();
            return true;
        }

        if let Some(active_id) = self.tab_manager.active_id() {
            self.switch_to_document(active_id);
        }
        self.rebuild_tab_bar();
        // Defer malloc_trim so FLTK can free widgets first, and rapid closes batch naturally.
        defer_send(self.sender, 0.1, Message::MallocTrim);
        false
    }

    // --- File operations (delegated to FileController) ---

    /// Run OnDocumentOpen plugin hooks for a file.
    /// Called directly (small files) or deferred via `DeferredPluginHooks` message.
    pub fn run_open_hooks(&mut self, path: String, content: String) {
        self.run_tree_refresh(Some(path), content);
    }

    pub fn file_save(&mut self) {
        let theme_bg = self.highlight.highlighter().theme_background();
        let actions = self.file.file_save(
            &mut self.tab_manager,
            &self.plugins,
            theme_bg,
            self.tabs_enabled,
        );
        self.dispatch_file_actions(actions);
    }

    /// Dispatch file actions returned by FileController methods.
    pub fn dispatch_file_actions(&mut self, actions: Vec<FileAction>) {
        for action in actions {
            match action {
                FileAction::SwitchToDocument(id) => self.switch_to_document(id),
                FileAction::RebuildTabBar => self.rebuild_tab_bar(),
                FileAction::DetectAndHighlight(id, path) => {
                    self.highlight.detect_and_highlight(
                        id,
                        &path,
                        &mut self.tab_manager,
                        &self.sender,
                    );
                }
                FileAction::BindHighlightData(id) => {
                    if let Some(doc) = self.tab_manager.doc_by_id(id) {
                        let style_buf = doc.style_buffer.clone();
                        let table = self.highlight.style_table();
                        self.editor.set_highlight_data_ext(style_buf, table);
                    }
                }
                FileAction::UpdateWindowTitle => self.update_window_title(),
                FileAction::UpdateMenusForFileType => self.update_menus_for_file_type(),
                FileAction::RunOpenHooks { path, content } => {
                    self.run_open_hooks(path, content);
                }
                FileAction::DeferOpenHooks { path, content } => {
                    defer_send(
                        self.sender,
                        0.0,
                        Message::DeferredPluginHooks { path, content },
                    );
                }
                FileAction::ProcessLintResult(result) => {
                    let approved = self.approved_commands_for_source(&result);
                    let mut ctx = HookContext {
                        tab_manager: &mut self.tab_manager,
                        view: &mut self.view,
                        widget_manager: &mut self.widget.widget_manager,
                        sender: self.sender,
                        approved_commands: approved,
                    };
                    hook_dispatch::dispatch_lint_result(*result, &mut ctx);
                }
                FileAction::UpdatePreviewFile { doc_id, path, text } => {
                    FileController::update_preview_file(
                        &mut self.preview,
                        self.view.dark_mode,
                        doc_id,
                        &path,
                        &text,
                    );
                }
                FileAction::ShowError(message) => {
                    let theme_bg = self.highlight.highlighter().theme_background();
                    show_themed_message(theme_bg, "Error", &message);
                }
            }
        }
    }

    /// Reload the active document from disk.
    pub fn file_reload_active(&mut self) {
        if let Some(doc) = self.tab_manager.active_doc() {
            let id = doc.id;
            let actions = self.file.reload_file(id, &mut self.tab_manager);
            self.dispatch_file_actions(actions);
        }
    }

    /// Reload all file-backed documents from disk.
    pub fn file_reload_all(&mut self) {
        let actions = self.file.reload_all_files(&mut self.tab_manager);
        self.dispatch_file_actions(actions);
    }

    /// Check for externally modified files and reload or prompt as needed.
    pub fn check_and_reload_external_changes(&mut self) {
        let modified = FileController::check_external_modifications(&self.tab_manager);
        if modified.is_empty() {
            return;
        }

        use super::controllers::file::ExternalChange;

        let mut all_actions = Vec::new();
        for (doc_id, path, has_unsaved, change) in modified {
            match change {
                ExternalChange::Deleted => {
                    // Keep tab open, mark dirty so user can save to recreate
                    if let Some(doc) = self.tab_manager.doc_by_id_mut(doc_id) {
                        doc.has_unsaved_changes.set(true);
                        doc.disk_mtime = None;
                    }
                    all_actions.push(FileAction::RebuildTabBar);
                }
                ExternalChange::Modified if !has_unsaved => {
                    // Auto-reload silently
                    all_actions.extend(self.file.reload_file(doc_id, &mut self.tab_manager));
                }
                ExternalChange::Modified => {
                    // Prompt user
                    let filename = std::path::Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.clone());
                    let msg = format!(
                        "'{}' has been modified externally.\n\n\
                         Reload from disk? (Your unsaved changes will be lost.)",
                        filename
                    );
                    let theme_bg = self.highlight.highlighter().theme_background();
                    let choice = show_themed_choice(
                        theme_bg,
                        "File Changed on Disk",
                        &msg,
                        "Reload",
                        "Keep Mine",
                        "",
                    );
                    if choice == Some(0) {
                        all_actions.extend(self.file.reload_file(doc_id, &mut self.tab_manager));
                    } else {
                        // User chose to keep their version — update mtime to avoid re-prompting
                        if let Some(doc) = self.tab_manager.doc_by_id_mut(doc_id) {
                            doc.disk_mtime = std::fs::metadata(&path)
                                .ok()
                                .and_then(|m| m.modified().ok());
                        }
                    }
                }
            }
        }
        self.dispatch_file_actions(all_actions);
    }

    /// Refresh the active tree view panel (e.g. file explorer) if visible.
    pub fn refresh_tree_if_visible(&mut self) {
        if !self.widget.tree_view_active {
            return;
        }
        let session_id = match self.widget.widget_manager.any_tree_view_session() {
            Some(id) => id,
            None => return,
        };
        if let Some(doc) = self.tab_manager.active_doc_mut() {
            doc.cached_tree = None;
        }
        self.sender.send(Message::TreeViewContextAction {
            session_id,
            action: "refresh".to_string(),
            node_path: vec![],
            input_text: None,
            target_path: None,
        });
    }

    /// Restore session from disk. Call after bind_active_buffer and apply_settings.
    pub fn restore_session(&mut self) {
        let session_name = self.session.current_session_name().to_string();
        let result = match SessionController::restore(
            &mut self.tab_manager,
            &self.settings,
            self.tabs_enabled,
            &session_name,
        ) {
            Some(r) => r,
            None => return,
        };

        self.file.last_open_directory = result.last_open_directory;

        // Apply syntax highlighting for each restored document
        for (id, path) in &result.highlight_docs {
            self.highlight
                .detect_and_highlight(*id, path, &mut self.tab_manager, &self.sender);
        }

        self.bind_active_buffer();
        if let Some(doc) = self.tab_manager.active_doc() {
            let cursor = doc.cursor_position;
            self.editor.set_insert_position(cursor);
            self.editor.show_insert_position();
        }
        self.update_window_title();
        self.rebuild_tab_bar();

        // Call on_document_open hook for the active document so plugins
        // can initialize state (e.g. register shortcuts).
        if let Some(doc) = self.tab_manager.active_doc()
            && let Some(ref path) = doc.file_path
        {
            let path = path.clone();
            let open_result = self.plugins.call_hook(PluginHook::OnDocumentOpen {
                path: Some(path),
                content: None,
            });
            let approved = self.approved_commands_for_source(&open_result);
            self.widget
                .process_widget_requests(&open_result, "", &approved);
            let mut ctx = HookContext {
                tab_manager: &mut self.tab_manager,
                view: &mut self.view,
                widget_manager: &mut self.widget.widget_manager,
                sender: self.sender,
                approved_commands: approved,
            };
            hook_dispatch::dispatch_hook_result(open_result, "", &mut ctx);
        }

        // Re-save immediately so on-disk state matches restored state.
        let session_mode = self.settings.borrow().session_restore;
        let _ = session::save_session(
            &self.tab_manager,
            session_mode,
            self.file.last_open_directory.as_deref(),
            self.session.current_session_name(),
        );
    }

    /// Switch to a different named session.
    /// Saves current session, closes all tabs, loads the target session.
    pub fn switch_session(&mut self, name: &str) {
        // 1. Save current cursor position
        if let Some(current) = self.tab_manager.active_doc_mut() {
            current.cursor_position = self.editor.insert_position();
        }

        // 2. Force-save current session
        self.session.force_save(
            &self.tab_manager,
            &self.settings,
            self.file.last_open_directory.as_deref(),
        );

        // 3. Close all documents (content is saved in temp files, no prompts needed)
        self.tab_manager.clear();

        // 4. Unbind editor buffer (no active doc)
        let empty_buf = fltk::text::TextBuffer::default();
        let empty_style = fltk::text::TextBuffer::default();
        self.editor.set_buffer(empty_buf.clone());
        self.editor.set_highlight_data(
            empty_style,
            vec![fltk::text::StyleTableEntry {
                color: fltk::enums::Color::Foreground,
                font: fltk::enums::Font::Courier,
                size: 14,
            }],
        );

        // 5. Switch session name and add initial untitled doc
        self.session.set_session_name(name.to_string());
        self.tab_manager.add_untitled();

        // 6. Restore new session (will remove the untitled doc if session has content)
        self.restore_session();

        // 7. Rebuild UI
        self.bind_active_buffer();
        if let Some(doc) = self.tab_manager.active_doc() {
            let cursor = doc.cursor_position;
            self.editor.set_insert_position(cursor);
            self.editor.show_insert_position();
        }
        self.update_window_title();
        self.rebuild_tab_bar();
    }

    /// Handle quit request. Returns `true` if the app should exit.
    pub fn file_quit(&mut self) -> bool {
        let session_mode = self.settings.borrow().session_restore;

        if let Some(current) = self.tab_manager.active_doc_mut() {
            current.cursor_position = self.editor.insert_position();
        }

        let should_quit = if self.tabs_enabled {
            let dirty_docs: Vec<DocumentId> = self
                .tab_manager
                .documents()
                .iter()
                .filter(|d| d.is_dirty())
                .map(|d| d.id)
                .collect();

            if dirty_docs.is_empty() {
                true
            } else {
                let choice = show_themed_choice(
                    self.highlight.highlighter().theme_background(),
                    "Unsaved Changes",
                    "You have unsaved changes in one or more tabs.",
                    "Save All",
                    "Quit Without Saving",
                    "Cancel",
                );

                match choice {
                    Some(0) => {
                        for id in dirty_docs {
                            self.switch_to_document(id);
                            self.file_save();
                            if let Some(doc) = self.tab_manager.doc_by_id(id)
                                && doc.is_dirty()
                            {
                                return false;
                            }
                        }
                        true
                    }
                    Some(1) => true,
                    _ => false,
                }
            }
        } else {
            let is_dirty = self.tab_manager.active_doc().is_some_and(|d| d.is_dirty());

            if is_dirty {
                let choice = show_themed_choice(
                    self.highlight.highlighter().theme_background(),
                    "Unsaved Changes",
                    "You have unsaved changes.",
                    "Save",
                    "Quit Without Saving",
                    "Cancel",
                );

                match choice {
                    Some(0) => {
                        self.file_save();
                        !self.tab_manager.active_doc().is_some_and(|d| d.is_dirty())
                    }
                    Some(1) => true,
                    _ => false,
                }
            } else {
                true
            }
        };

        if should_quit {
            // Call plugin shutdown hook
            self.plugins.call_hook(PluginHook::Shutdown {
                project_root: super::mcp::cwd_as_string(),
            });

            let _ = session::save_session(
                &self.tab_manager,
                session_mode,
                self.file.last_open_directory.as_deref(),
                self.session.current_session_name(),
            )
            .inspect_err(|e| eprintln!("Failed to save session: {}", e));
        }

        should_quit
    }

    /// Close-time geometry save, after a confirmed quit: patches only the
    /// `window` object of the settings file on disk (see
    /// `AppSettings::save_window_geometry`). Minimized at close → no write.
    pub fn save_window_geometry(&self) {
        let geometry = match self.window_geometry.try_borrow_mut() {
            Ok(mut tracker) => tracker.take_geometry_to_save(),
            Err(_) => None,
        };
        let Some(geometry) = geometry else {
            return;
        };
        let Ok(settings) = self.settings.try_borrow() else {
            return;
        };
        let _ = settings
            .save_window_geometry(geometry)
            .inspect_err(|e| eprintln!("Failed to save window geometry: {}", e));
    }

    pub fn switch_to_next_tab(&mut self) {
        if let Some(next_id) = self.tab_manager.next_doc_id() {
            self.switch_to_document(next_id);
            self.rebuild_tab_bar();
        }
    }

    pub fn switch_to_previous_tab(&mut self) {
        if let Some(prev_id) = self.tab_manager.prev_doc_id() {
            self.switch_to_document(prev_id);
            self.rebuild_tab_bar();
        }
    }

    /// The main loop took `msg` off the queue, before dispatch (T0096,
    /// T0093). Every message that owns a "pending" flag clears it here, the
    /// one hook for all of them, so the reset does not depend on its handler
    /// running (the main loop skips some handlers when no tab is open).
    pub fn note_dequeued(&mut self, msg: &Message) {
        self.highlight.note_dequeued(msg);
        crate::app::domain::document::note_dequeued(msg);
        if matches!(msg, Message::DoTextChangeHook) {
            self.text_change_timer_active = false;
        }
    }

    pub fn update_linenumber_width(&mut self) {
        self.view.update_linenumber_width(&self.tab_manager);
    }

    pub fn toggle_dark_mode(&mut self) {
        self.view.dark_mode = !self.view.dark_mode;

        // Set syntax theme first to get the background color
        let theme = self
            .settings
            .borrow()
            .current_syntax_theme(self.view.dark_mode);
        self.highlight.set_theme(&theme, self.view.dark_mode);
        self.apply_theme_to_owned_widgets();

        self.highlight
            .rehighlight_all_documents(&mut self.tab_manager, &self.sender);
        self.bind_active_buffer();

        // Call plugin hook
        self.plugins.call_hook(PluginHook::OnThemeChanged {
            is_dark: self.view.dark_mode,
        });
    }

    /// The theme inputs panels derive their colors from: the dark-mode flag
    /// and the current syntax theme's background.
    pub fn panel_theme(&self) -> PanelTheme {
        PanelTheme {
            is_dark: self.view.dark_mode,
            bg: self.highlight.highlighter().theme_background(),
        }
    }

    /// Re-theme the widgets `AppState` owns (editor, window, menu bar, title
    /// bar, tab bar) from the current dark-mode flag and syntax theme. The
    /// panels in `LayoutWidgets` are re-themed by `dispatch::retheme_panels`.
    fn apply_theme_to_owned_widgets(&mut self) {
        let theme = self.panel_theme();
        let bg = theme.bg;
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let fg = self.highlight.highlighter().theme_foreground();

        // Apply theme with syntax background for menu bar color
        apply_theme(
            &mut self.window,
            &mut self.menu,
            Some(&mut self.update_banner_frame),
            theme.is_dark,
            bg,
        );
        #[cfg(target_os = "windows")]
        set_windows_titlebar_theme(&self.window, bg, fg);
        #[cfg(target_os = "macos")]
        set_macos_titlebar_color(&self.window, bg, fg);

        // Apply the active theme's editor colors
        apply_syntax_theme_colors(&mut self.editor, &self.highlight.editor_colors());

        // Apply theme to tab bar with editor background color
        if let Some(ref mut tab_bar) = self.tab_bar {
            tab_bar.apply_theme(theme);
        }
    }

    pub fn toggle_highlighting(&mut self) {
        self.highlight.highlighting_enabled = !self.highlight.highlighting_enabled;
        if self.highlight.highlighting_enabled {
            self.highlight
                .rehighlight_all_documents(&mut self.tab_manager, &self.sender);
            self.bind_active_buffer();
        } else {
            self.highlight.disable_highlighting(
                &mut self.tab_manager,
                &mut HighlightWidgets {
                    editor: &mut self.editor,
                    banner_frame: &mut self.update_banner_frame,
                    flex: &mut self.flex,
                    window: &mut self.window,
                },
            );
            self.bind_active_buffer();
            self.editor.redraw();
        }
    }

    // --- Preview ---

    /// Open the current markdown file in the default browser for preview.
    pub fn preview_in_browser(&mut self) {
        if let Some(doc) = self.tab_manager.active_doc() {
            let text = buffer_text_no_leak(&doc.buffer);
            let file_path = doc.file_path.clone();
            let doc_id = doc.id.0;
            if let Err(notice) = self.preview.preview_in_browser(
                file_path.as_deref(),
                doc_id,
                &text,
                self.view.dark_mode,
            ) {
                let theme_bg = self.highlight.highlighter().theme_background();
                show_themed_message(theme_bg, notice.title(), &notice.message());
            }
        }
    }

    // --- Settings ---

    /// Show the Settings dialog and apply the result on Save. `on_preview`
    /// runs while the dialog is open, for each live syntax-theme preview and
    /// for the revert to the original theme on Cancel/close.
    pub fn open_settings(&mut self, mut on_preview: impl FnMut(&mut AppState, ThemeRef)) {
        let current = self.settings.borrow().clone();
        let theme_bg = self.highlight.highlighter().theme_background();
        let custom_themes = self.custom_theme_choices();
        let mut managed = false;
        let result = show_settings_dialog(&current, theme_bg, &custom_themes, |hook| match hook {
            SettingsHook::Preview(theme) => {
                on_preview(self, theme);
                None
            }
            SettingsHook::ManageThemes => {
                managed = true;
                Some(self.manage_themes(&mut on_preview))
            }
        });
        if result.is_none() && managed {
            // The theme manager kept the dialogs' colors in FLTK's globals
            // while it previewed; give them back to the main window.
            self.apply_theme_to_owned_widgets();
        }
        if let Some(new_settings) = result {
            if let Err(e) = new_settings.save() {
                show_themed_message(
                    theme_bg,
                    "Settings",
                    &format!("Failed to save settings: {}", e),
                );
                return;
            }
            let is_dark = new_settings
                .theme_mode
                .resolve_dark(detect_system_dark_mode);
            self.apply_settings(new_settings, is_dark);
        }
    }

    /// The loaded custom themes as (reference, name), sorted by name.
    pub fn custom_theme_choices(&self) -> Vec<(ThemeRef, String)> {
        let mut themes: Vec<(ThemeRef, String)> = self
            .highlight
            .custom_themes()
            .values()
            .map(|t| (t.theme_ref(), t.name.clone()))
            .collect();
        themes.sort_by_key(|(_, name)| name.to_lowercase());
        themes
    }

    /// Run the theme manager (T0080) from the Settings dialog. `retheme`
    /// shows a theme with its panels, like a Settings preview. Returns the
    /// custom themes afterwards, for the Settings choices.
    fn manage_themes(
        &mut self,
        retheme: &mut impl FnMut(&mut AppState, ThemeRef),
    ) -> Vec<(ThemeRef, String)> {
        // What the editor shows now: a Settings preview included.
        let shown = self.highlight.active_theme().clone();
        let (store, warnings) = ThemeStore::load(themes_dir());
        for warning in warnings {
            eprintln!("{warning}");
        }
        let (light, dark) = {
            let settings = self.settings.borrow();
            (
                settings.syntax_theme_light.clone(),
                settings.syntax_theme_dark.clone(),
            )
        };
        let model = ThemeManagerModel::new(store, light, dark, self.view.dark_mode, shown);
        let theme_bg = self.highlight.highlighter().theme_background();
        show_theme_manager(theme_bg, model, |model, actions| {
            let mut show_again = false;
            for action in actions {
                match action {
                    ThemeManagerAction::ThemesChanged => {
                        self.highlight.set_custom_themes(model.custom_themes());
                    }
                    ThemeManagerAction::SetSlot { is_dark, theme } => {
                        let mut settings = self.settings.borrow_mut();
                        if is_dark {
                            settings.syntax_theme_dark = theme;
                        } else {
                            settings.syntax_theme_light = theme;
                        }
                    }
                    ThemeManagerAction::SaveSettings => {
                        let saved = self.settings.borrow().save();
                        if let Err(e) = saved {
                            eprintln!("[theme] could not save settings: {e}");
                        }
                    }
                    ThemeManagerAction::Retheme => show_again = true,
                    ThemeManagerAction::Error(_) | ThemeManagerAction::Select(_) => {}
                }
            }
            if show_again {
                retheme(self, model.shown().clone());
            }
        });
        self.custom_theme_choices()
    }

    /// Run the menu command bound to `shortcut` (a normalized shortcut
    /// string), resolved like the menu binds it: built-in table, the user's
    /// overrides, then plugin menu items. Does nothing when no active item is
    /// bound to it.
    pub fn run_shortcut(&mut self, shortcut: &str) {
        let plugins = self
            .plugins
            .list_plugins()
            .iter()
            .map(|p| (p.name.as_str(), p.menu_items.as_slice()));
        let Some(path) = crate::ui::menu::command_for_shortcut(
            shortcut,
            &self.shortcut_registry,
            self.tabs_enabled,
            plugins,
        ) else {
            return;
        };
        if let Some(mut item) = self.menu.find_item(&path)
            && item.active()
        {
            item.do_callback(&self.menu);
        }
    }

    /// Show the Key Shortcuts dialog and apply changes.
    pub fn show_key_shortcuts(&mut self) {
        use crate::ui::dialogs::shortcut_dialog::show_shortcut_dialog;

        let theme_bg = self.highlight.highlighter().theme_background();
        if let Some(result) = show_shortcut_dialog(
            &self.shortcut_registry,
            &self.plugins,
            theme_bg,
            self.tabs_enabled,
        ) {
            // Update registry
            self.shortcut_registry.replace_all(result.overrides.clone());

            // Persist to settings
            {
                let mut settings = self.settings.borrow_mut();
                settings.shortcut_overrides = result.overrides;

                // Sync run_all_checks_shortcut for backward compat
                let effective_run_all = self
                    .shortcut_registry
                    .effective_shortcut("Plugins/General/Run All Checks", "Ctrl+Shift+L");
                settings.run_all_checks_shortcut = effective_run_all.to_string();

                let _ = settings.save();
            }

            // Apply built-in shortcut overrides to menu in-place
            crate::ui::menu::apply_shortcut_overrides(
                &mut self.menu,
                &self.shortcut_registry,
                self.tabs_enabled,
            );

            // Rebuild plugin menu for plugin shortcuts
            crate::ui::menu::rebuild_plugins_menu(
                &mut self.menu,
                &self.sender,
                &self.settings.borrow(),
                &self.plugins,
                &self.shortcut_registry,
            );
        }
    }

    /// Preview a syntax theme (called from settings dialog for live preview).
    /// Re-themes the widgets `AppState` owns; the caller re-themes the panels
    /// in `LayoutWidgets` through `dispatch::retheme_panels`.
    pub fn preview_syntax_theme(&mut self, theme: &ThemeRef) {
        self.highlight.set_theme(theme, self.view.dark_mode);
        self.apply_theme_to_owned_widgets();

        self.highlight
            .rehighlight_all_documents(&mut self.tab_manager, &self.sender);
        self.bind_active_buffer();
    }

    pub fn set_font(&mut self, name: &str) {
        let font = settings::resolve_font(name);
        self.editor.set_text_font(font);
        self.highlight.set_font(font, self.highlight.font_size());
        self.bind_active_buffer();
        self.settings.borrow_mut().font = name.to_string();
        let _ = self.settings.borrow().save();
        self.editor.redraw();
    }

    pub fn set_font_size(&mut self, size: i32) {
        let size = size.clamp(6, 96);
        let prev = self.settings.borrow().font_size_clamped();
        self.editor.set_text_size(size);
        self.highlight.set_font(self.highlight.font(), size);
        self.bind_active_buffer();
        self.settings.borrow_mut().font_size = size as u32;
        let _ = self.settings.borrow().save();
        self.editor.redraw();
        crate::ui::menu::apply_font_size_active(&mut self.menu, prev, size);
    }

    /// Apply `new_settings`; `is_dark` is the already-resolved theme mode
    /// (callers resolve it once, so the desktop query never runs twice).
    pub fn apply_settings(&mut self, new_settings: AppSettings, is_dark: bool) {
        let prev_font_size = self.settings.borrow().font_size_clamped();
        let new_font_size = new_settings.font_size_clamped();
        self.view.dark_mode = is_dark;

        let font = new_settings.current_font();
        self.editor.set_text_font(font);
        self.editor.set_text_size(new_font_size);
        crate::ui::menu::apply_font_size_active(&mut self.menu, prev_font_size, new_font_size);

        // Set syntax theme first to get background color
        let syntax_theme = new_settings.current_syntax_theme(is_dark);
        self.highlight.set_theme(&syntax_theme, is_dark);
        self.highlight.set_font(font, new_font_size);
        self.apply_theme_to_owned_widgets();
        self.update_menu_checkbox("View/Toggle Dark Mode", is_dark);

        self.view.show_linenumbers = new_settings.line_numbers_enabled;
        self.update_linenumber_width();
        self.update_menu_checkbox("View/Toggle Line Numbers", self.view.show_linenumbers);

        self.view.word_wrap = new_settings.word_wrap_enabled;
        if self.view.word_wrap {
            self.editor.wrap_mode(WrapMode::AtBounds, 0);
        } else {
            self.editor.wrap_mode(WrapMode::None, 0);
        }
        self.update_menu_checkbox("View/Toggle Word Wrap", self.view.word_wrap);
        self.update_menu_checkbox(
            "View/Toggle Current Line Highlight",
            new_settings.highlight_current_line,
        );
        // Re-apply (or clear) the current-line twin everywhere.
        self.mark_all_styles_stale();

        // Apply tab size to all document buffers
        self.tab_manager
            .set_all_tab_distance(new_settings.tab_size as i32);

        let highlighting_changed =
            self.highlight.highlighting_enabled != new_settings.highlighting_enabled;
        self.highlight.highlighting_enabled = new_settings.highlighting_enabled;
        self.update_menu_checkbox(
            "View/Toggle Syntax Highlighting",
            self.highlight.highlighting_enabled,
        );

        self.editor.redraw();

        *self.settings.borrow_mut() = new_settings;

        if highlighting_changed && !self.highlight.highlighting_enabled {
            self.highlight.disable_highlighting(
                &mut self.tab_manager,
                &mut HighlightWidgets {
                    editor: &mut self.editor,
                    banner_frame: &mut self.update_banner_frame,
                    flex: &mut self.flex,
                    window: &mut self.window,
                },
            );
            self.bind_active_buffer();
        } else if self.highlight.highlighting_enabled {
            self.highlight
                .rehighlight_all_documents(&mut self.tab_manager, &self.sender);
            self.bind_active_buffer();
        }
    }

    /// View > Toggle Current Line Highlight (T0081): flip and persist the
    /// setting. The next main-loop update applies or clears the band.
    pub fn toggle_current_line_highlight(&mut self) {
        let enabled = {
            let mut settings = self.settings.borrow_mut();
            settings.highlight_current_line = !settings.highlight_current_line;
            settings.highlight_current_line
        };
        if let Err(e) = self.settings.borrow().save() {
            eprintln!("Failed to save settings: {}", e);
        }
        self.update_menu_checkbox("View/Toggle Current Line Highlight", enabled);
        self.mark_all_styles_stale();
    }

    /// Every document's current-line twin must be re-applied or cleared on
    /// its next update (setting toggled, theme or table changed).
    fn mark_all_styles_stale(&self) {
        for doc in self.tab_manager.documents() {
            doc.mark_style_stale();
        }
    }

    /// Current-line highlight (T0081). Runs on every main-loop iteration,
    /// like the status bar, so there is no timer. O(1) while the cursor stays
    /// put and nothing was edited or restyled; otherwise it rewrites the style
    /// bytes of at most two lines (the old one and the new one, each capped at
    /// `MAX_LINE_BYTES`).
    pub fn update_current_line_highlight(&mut self) {
        let enabled = self
            .settings
            .try_borrow()
            .is_ok_and(|s| s.highlight_current_line);
        let Some(doc) = self.tab_manager.active_doc() else {
            return;
        };
        let cursor = self.editor.insert_position();
        let mut twin = doc.current_line.get();
        if (enabled && twin.is_current(cursor)) || (!enabled && twin.twinned().is_none()) {
            return;
        }
        // No style buffer bound (huge read-only / tail views): silent no-op.
        let len = doc.buffer.length();
        if doc.style_buffer.length() != len {
            return;
        }

        // Bounded window around the cursor, never FLTK's unbounded
        // line_start / findchar_forward (O(line) on a one-line minified file).
        let line = if enabled {
            cursor_line(len, cursor, |start, end| {
                text_range_bytes_no_leak(&doc.buffer, start, end)
            })
        } else {
            None
        };
        let Some(plan) = twin.plan(line) else {
            twin.applied(twin.twinned(), Some(cursor));
            doc.current_line.set(twin);
            return;
        };

        let mut style_buf = doc.style_buffer.clone();
        let mut changed = false;
        if let Some(range) = plan.untwin {
            changed |= rewrite_styles(&mut style_buf, range.start, range.end, untwin_bytes);
        }
        if let Some(range) = plan.twin {
            let twinnable = self.highlight.highlighter().twinnable_styles();
            changed |= rewrite_styles(&mut style_buf, range.start, range.end, |bytes| {
                twin_bytes(bytes, &twinnable)
            });
        }
        twin.applied(plan.twin, Some(cursor));
        doc.current_line.set(twin);
        if changed {
            self.editor.redraw();
        }
    }

    fn update_menu_checkbox(&self, path: &str, checked: bool) {
        let idx = self.menu.find_index(path);
        if idx >= 0
            && let Some(mut item) = self.menu.at(idx)
        {
            if checked {
                item.set();
            } else {
                item.clear();
            }
        }
    }

    // --- Syntax highlighting (delegates to HighlightController) ---

    pub fn schedule_rehighlight(&mut self, id: DocumentId, pos: i32) {
        self.highlight.schedule_rehighlight(
            id,
            pos,
            &mut self.tab_manager,
            &self.sender,
            &mut HighlightWidgets {
                editor: &mut self.editor,
                banner_frame: &mut self.update_banner_frame,
                flex: &mut self.flex,
                window: &mut self.window,
            },
        );
    }

    /// Schedule a debounced OnTextChanged plugin hook (300ms after last edit).
    pub fn schedule_text_change_hook(
        &mut self,
        id: DocumentId,
        pos: i32,
        inserted: i32,
        deleted: i32,
    ) {
        // Coalesce: keep earliest position, sum inserted/deleted
        match self.pending_text_change {
            Some((existing_id, existing_pos, existing_ins, existing_del)) if existing_id == id => {
                self.pending_text_change = Some((
                    id,
                    pos.min(existing_pos),
                    existing_ins + inserted,
                    existing_del + deleted,
                ));
            }
            _ => {
                self.pending_text_change = Some((id, pos, inserted, deleted));
            }
        }

        if !self.text_change_timer_active {
            self.text_change_timer_active = true;
            defer_send(self.sender, 0.3, Message::DoTextChangeHook);
        }
    }

    /// Fire the debounced OnTextChanged hook to plugins.
    pub fn do_pending_text_change_hook(&mut self) {
        self.text_change_timer_active = false;

        let (id, position, inserted_len, deleted_len) = match self.pending_text_change.take() {
            Some(p) => p,
            None => return,
        };

        // Only fire for the active document
        let path = self
            .tab_manager
            .doc_by_id(id)
            .and_then(|d| d.file_path.clone());

        let result = self.plugins.call_hook(PluginHook::OnTextChanged {
            position,
            inserted_len,
            deleted_len,
        });

        // Process results (diagnostics, annotations, status, widgets)
        let approved = self.approved_commands_for_source(&result);
        let mut ctx = HookContext {
            tab_manager: &mut self.tab_manager,
            view: &mut self.view,
            widget_manager: &mut self.widget.widget_manager,
            sender: self.sender,
            approved_commands: approved,
        };
        let plugin_name = path.as_deref().unwrap_or("");
        hook_dispatch::dispatch_hook_result(result, plugin_name, &mut ctx);
    }

    pub fn do_pending_rehighlight(&mut self) {
        self.highlight.do_pending_rehighlight(
            &mut self.tab_manager,
            &self.sender,
            &mut HighlightWidgets {
                editor: &mut self.editor,
                banner_frame: &mut self.update_banner_frame,
                flex: &mut self.flex,
                window: &mut self.window,
            },
        );
    }

    pub fn continue_chunked_highlight(&mut self) {
        self.highlight.continue_chunked_highlight(
            &mut self.tab_manager,
            &self.sender,
            &mut HighlightWidgets {
                editor: &mut self.editor,
                banner_frame: &mut self.update_banner_frame,
                flex: &mut self.flex,
                window: &mut self.window,
            },
        );
    }

    // --- Tab Group handlers ---

    pub fn handle_group_close(&mut self, group_id: GroupId) {
        let doc_ids = self.tab_manager.group_doc_ids(group_id);
        for id in doc_ids {
            if self.close_tab(id) {
                return;
            }
        }
        self.tab_manager.delete_group(group_id);
        self.rebuild_tab_bar();
    }

    pub fn start_queued_highlights(&mut self) {
        self.highlight.start_queued_highlights(
            &self.sender,
            &mut HighlightWidgets {
                editor: &mut self.editor,
                banner_frame: &mut self.update_banner_frame,
                flex: &mut self.flex,
                window: &mut self.window,
            },
        );
    }

    /// Navigate to a specific line number (1-indexed)
    pub fn goto_line(&mut self, line: u32) {
        let buf = self.active_buffer();
        self.view.goto_line(&buf, line);
    }

    // --- Annotations (delegates to HighlightController) ---

    /// Request manual highlight from plugins (Ctrl+Shift+L / Run All Checks)
    pub fn request_manual_highlight(&mut self) {
        // Get current document info
        let doc = self.tab_manager.active_doc();
        let path = doc.as_ref().and_then(|d| d.file_path.clone());
        let content = buffer_text_no_leak(&self.editor.buffer().unwrap_or_default());

        // Check if specific plugins are configured for Run All Checks
        let selected_plugins = self.settings.borrow().run_all_checks_plugins.clone();

        let result = if selected_plugins.is_empty() {
            // Call all enabled plugins (default behavior)
            self.plugins
                .call_hook(PluginHook::OnHighlightRequest { path, content })
        } else {
            // Call only selected plugins
            let mut combined = super::plugins::HookResult::default();
            for plugin_name in &selected_plugins {
                if let Some(plugin_result) = self.plugins.call_hook_on_plugin(
                    plugin_name,
                    PluginHook::OnHighlightRequest {
                        path: path.clone(),
                        content: content.clone(),
                    },
                ) {
                    combined.diagnostics.extend(plugin_result.diagnostics);
                    combined
                        .line_annotations
                        .extend(plugin_result.line_annotations);
                    combined.had_lint_results |= plugin_result.had_lint_results;
                    if plugin_result.status_message.is_some() {
                        combined.status_message = plugin_result.status_message;
                    }
                }
            }
            combined
        };

        // Process results (diagnostics, annotations, and toast)
        let approved = self.approved_commands_for_source(&result);
        let mut ctx = HookContext {
            tab_manager: &mut self.tab_manager,
            view: &mut self.view,
            widget_manager: &mut self.widget.widget_manager,
            sender: self.sender,
            approved_commands: approved,
        };
        hook_dispatch::dispatch_lint_result(result, &mut ctx);
    }

    /// Store diagnostics in the active document for persistence across tab switches
    pub fn store_diagnostics(&mut self, diagnostics: Vec<super::plugins::Diagnostic>) {
        if let Some(doc) = self.tab_manager.active_doc_mut() {
            doc.diagnostics = diagnostics;
            doc.has_been_linted = true;
        }
    }

    /// Get stored diagnostics for the active document (None if never linted)
    pub fn get_active_diagnostics(&self) -> Option<Vec<super::plugins::Diagnostic>> {
        self.tab_manager.active_doc().and_then(|d| {
            if d.has_been_linted {
                Some(d.diagnostics.clone())
            } else {
                None
            }
        })
    }
}

/// Rewrite the style bytes `[start, end)` (clamped to the buffer) with `f`.
/// Returns whether anything changed. Style chars are ASCII; a non-ASCII run
/// (never written by FerrisPad) is left alone.
fn rewrite_styles(
    style_buf: &mut TextBuffer,
    start: i32,
    end: i32,
    f: impl FnOnce(&mut [u8]),
) -> bool {
    let end = end.clamp(0, style_buf.length());
    let start = start.clamp(0, end);
    if start == end {
        return false;
    }
    let old = text_range_no_leak(style_buf, start, end);
    if !old.is_ascii() || old.len() != (end - start) as usize {
        return false;
    }
    let mut bytes = old.clone().into_bytes();
    f(&mut bytes);
    match String::from_utf8(bytes) {
        Ok(new) if new != old => {
            style_buf.replace(start, end, &new);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T0093: the title string (pinned; the bug was the missing refresh).
    #[test]
    fn dirty_window_title() {
        let app = if cfg!(target_os = "windows") {
            "FerrisPad"
        } else {
            "\u{1f980} FerrisPad"
        };
        let default = session::DEFAULT_SESSION_NAME;
        assert_eq!(
            window_title(Some(("notes.txt", true)), default),
            format!("*notes.txt - {app}")
        );
        assert_eq!(
            window_title(Some(("notes.txt", false)), default),
            format!("notes.txt - {app}")
        );
        assert_eq!(
            window_title(Some(("Untitled", true)), "work"),
            format!("*Untitled - {app} [work]")
        );
        // No document open (the start page): just the app name, not a
        // phantom "Untitled".
        assert_eq!(window_title(None, default), app);
        assert_eq!(window_title(None, "work"), format!("{app} [work]"));
    }
}
