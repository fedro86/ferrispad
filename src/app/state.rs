use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    app::Sender,
    dialog,
    enums::Font,
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
use super::controllers::update::UpdateController;
use super::controllers::view::ViewController;
use super::controllers::widget::WidgetController;
use super::domain::document::DocumentId;
use super::domain::messages::Message;
use super::domain::settings::{AppSettings, FontChoice, SyntaxTheme, ThemeMode};
use super::infrastructure::buffer::buffer_text_no_leak;
use super::infrastructure::defer::defer_send;
use super::infrastructure::platform::detect_system_dark_mode;
use super::services::session;
use super::plugins::{PluginManager, PluginHook, get_plugin_dir};
use super::services::shortcut_registry::ShortcutRegistry;
use crate::ui::dialogs::settings_dialog::show_settings_dialog;
use crate::ui::editor_container::EditorContainer;
use crate::ui::tab_bar::TabBar;
use crate::ui::theme::{apply_theme, apply_syntax_theme_colors};
#[cfg(target_os = "windows")]
use crate::ui::theme::set_windows_titlebar_theme;

pub struct AppState {
    pub tab_manager: TabManager,
    pub tabs_enabled: bool,
    pub tab_bar: Option<TabBar>,
    #[allow(dead_code)]  // Holds ownership; editor clone is extracted in constructor
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
}

impl AppState {
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
    ) -> Self {
        let mut tab_manager = TabManager::new(sender);
        tab_manager.add_untitled();

        let font = {
            let s = settings.borrow();
            match s.font {
                FontChoice::ScreenBold => Font::ScreenBold,
                FontChoice::Courier => Font::Courier,
                FontChoice::HelveticaMono => Font::Screen,
            }
        };
        let font_size = settings.borrow().font_size as i32;
        let highlighting_enabled = settings.borrow().highlighting_enabled;
        let syntax_theme = settings.borrow().current_syntax_theme(dark_mode);
        let highlight = HighlightController::new(syntax_theme, font, font_size, highlighting_enabled);

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
            plugins.call_hook(PluginHook::Init);
        }

        let shortcut_registry = ShortcutRegistry::from_settings(&settings.borrow().shortcut_overrides);

        let editor = editor_container.editor().clone();

        let view = ViewController::new(editor.clone(), dark_mode, show_linenumbers, word_wrap);
        let session = SessionController::default();
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
    }

    /// Update the window title based on active document
    pub fn update_window_title(&mut self) {
        if let Some(doc) = self.tab_manager.active_doc() {
            let prefix = if doc.is_dirty() { "*" } else { "" };
            self.window.set_label(&format!(
                "{}{} - \u{1f980} FerrisPad",
                prefix, doc.display_name
            ));
        } else {
            self.window
                .set_label("Untitled - \u{1f980} FerrisPad");
        }
    }

    /// Switch the editor to display a different document
    pub fn switch_to_document(&mut self, id: DocumentId) {
        // Save current doc's cursor/scroll state
        if let Some(current) = self.tab_manager.active_doc_mut() {
            current.cursor_position = self.editor.insert_position();
        }

        // Set new active
        self.tab_manager.set_active(id);

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
        self.widget.refresh_tree_view_for_active_doc(&self.tab_manager);
    }

    /// Execute the tree view refresh (called directly or from deferred message).
    /// If the plugin does not return a tree view (e.g., non-YAML file), hides
    /// the tree panel so it doesn't stay open with stale/loading content.
    pub fn run_tree_refresh(&mut self, path: Option<String>, content: String) {
        let open_result = self.plugins.call_hook(PluginHook::OnDocumentOpen {
            path,
            content: Some(content),
        });
        self.widget.process_widget_requests(&open_result, "");
        let mut ctx = HookContext {
            tab_manager: &mut self.tab_manager,
            view: &mut self.view,
            widget_manager: &mut self.widget.widget_manager,
            sender: self.sender,
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
        let file_path = self.tab_manager.active_doc()
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
            && doc.is_dirty() {
                let name = doc.display_name.clone();
                let choice = dialog::choice2_default(
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
                            && doc.is_dirty() {
                                if let Some(prev) = was_active
                                    && prev != id {
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
        let close_path = self.tab_manager.doc_by_id(id).and_then(|d| d.file_path.clone());
        self.plugins.call_hook(PluginHook::OnDocumentClose { path: close_path });

        self.tab_manager.remove(id);

        if self.tab_manager.count() == 0 {
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
        let actions = self.file.file_save(
            &mut self.tab_manager,
            &self.plugins,
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
                        id, &path, &mut self.tab_manager, &self.sender,
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
                    defer_send(self.sender, 0.0, Message::DeferredPluginHooks { path, content });
                }
                FileAction::RunPluginOpenHook { path } => {
                    let result = self.plugins.call_hook(PluginHook::OnDocumentOpen {
                        path: Some(path),
                        content: None,
                    });
                    self.widget.process_widget_requests(&result, "");
                    let mut ctx = HookContext {
                        tab_manager: &mut self.tab_manager,
                        view: &mut self.view,
                        widget_manager: &mut self.widget.widget_manager,
                        sender: self.sender,
                    };
                    hook_dispatch::dispatch_hook_result(result, "", &mut ctx);
                }
                FileAction::ProcessLintResult(result) => {
                    let mut ctx = HookContext {
                        tab_manager: &mut self.tab_manager,
                        view: &mut self.view,
                        widget_manager: &mut self.widget.widget_manager,
                        sender: self.sender,
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
            }
        }
    }

    /// Restore session from disk. Call after bind_active_buffer and apply_settings.
    pub fn restore_session(&mut self) {
        let result = match SessionController::restore(
            &mut self.tab_manager,
            &self.settings,
            self.tabs_enabled,
        ) {
            Some(r) => r,
            None => return,
        };

        self.file.last_open_directory = result.last_open_directory;

        // Apply syntax highlighting for each restored document
        for (id, path) in &result.highlight_docs {
            self.highlight.detect_and_highlight(
                *id, path, &mut self.tab_manager, &self.sender,
            );
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
            self.widget.process_widget_requests(&open_result, "");
            let mut ctx = HookContext {
                tab_manager: &mut self.tab_manager,
                view: &mut self.view,
                widget_manager: &mut self.widget.widget_manager,
                sender: self.sender,
            };
            hook_dispatch::dispatch_hook_result(open_result, "", &mut ctx);
        }

        // Re-save immediately so on-disk state matches restored state.
        let session_mode = self.settings.borrow().session_restore;
        let _ = session::save_session(
            &self.tab_manager,
            session_mode,
            self.file.last_open_directory.as_deref(),
        );
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
                let choice = dialog::choice2_default(
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
                                && doc.is_dirty() {
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
            let is_dirty = self
                .tab_manager
                .active_doc()
                .is_some_and(|d| d.is_dirty());

            if is_dirty {
                let choice = dialog::choice2_default(
                    "You have unsaved changes.",
                    "Save",
                    "Quit Without Saving",
                    "Cancel",
                );

                match choice {
                    Some(0) => {
                        self.file_save();
                        !self
                            .tab_manager
                            .active_doc()
                            .is_some_and(|d| d.is_dirty())
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
            self.plugins.call_hook(PluginHook::Shutdown);

            let _ = session::save_session(&self.tab_manager, session_mode, self.file.last_open_directory.as_deref())
                .inspect_err(|e| eprintln!("Failed to save session: {}", e));
        }

        should_quit
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

    pub fn update_linenumber_width(&mut self) {
        self.view.update_linenumber_width(&self.tab_manager);
    }

    pub fn toggle_dark_mode(&mut self) {
        self.view.dark_mode = !self.view.dark_mode;

        // Set syntax theme first to get the background color
        let theme = self.settings.borrow().current_syntax_theme(self.view.dark_mode);
        self.highlight.set_theme(theme);

        // Get syntax theme colors
        let bg = self.highlight.highlighter().theme_background();
        let fg = self.highlight.highlighter().theme_foreground();

        // Apply theme with syntax background for menu bar color
        apply_theme(
            &mut self.editor,
            &mut self.window,
            &mut self.menu,
            Some(&mut self.update_banner_frame),
            self.view.dark_mode,
            bg,
        );
        #[cfg(target_os = "windows")]
        if self.window.shown() {
            set_windows_titlebar_theme(&self.window, is_dark);
        }

        // Apply syntax theme colors to editor
        apply_syntax_theme_colors(&mut self.editor, bg, fg);

        // Apply theme to tab bar with editor background color
        if let Some(ref mut tab_bar) = self.tab_bar {
            tab_bar.apply_theme(self.view.dark_mode, bg);
        }

        // Split panel theme is applied via Message::SplitViewShow (panel lives in MainWidgets)

        self.highlight.rehighlight_all_documents(&mut self.tab_manager, &self.sender);
        self.bind_active_buffer();

        // Call plugin hook
        self.plugins.call_hook(PluginHook::OnThemeChanged { is_dark: self.view.dark_mode });
    }

    pub fn toggle_highlighting(&mut self) {
        self.highlight.highlighting_enabled = !self.highlight.highlighting_enabled;
        if self.highlight.highlighting_enabled {
            self.highlight.rehighlight_all_documents(&mut self.tab_manager, &self.sender);
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
            self.preview.preview_in_browser(
                file_path.as_deref(),
                doc_id,
                &text,
                self.view.dark_mode,
            );
        }
    }

    // --- Settings ---

    pub fn open_settings(&mut self) {
        let current = self.settings.borrow().clone();
        let theme_bg = self.highlight.highlighter().theme_background();
        if let Some(new_settings) = show_settings_dialog(&current, &self.sender, theme_bg) {
            if let Err(e) = new_settings.save() {
                dialog::alert_default(&format!("Failed to save settings: {}", e));
                return;
            }
            self.apply_settings(new_settings);
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

    /// Preview a syntax theme (called from settings dialog for live preview)
    pub fn preview_syntax_theme(&mut self, theme: SyntaxTheme) {
        self.highlight.set_theme(theme);

        // Apply theme background/foreground colors to editor
        let bg = self.highlight.highlighter().theme_background();
        let fg = self.highlight.highlighter().theme_foreground();
        apply_syntax_theme_colors(&mut self.editor, bg, fg);

        self.highlight.rehighlight_all_documents(&mut self.tab_manager, &self.sender);
        self.bind_active_buffer();
    }

    pub fn set_font(&mut self, font: Font) {
        self.editor.set_text_font(font);
        self.highlight.set_font(font, self.highlight.font_size());
        self.bind_active_buffer();
        let font_choice = match font {
            Font::ScreenBold => FontChoice::ScreenBold,
            Font::Courier => FontChoice::Courier,
            _ => FontChoice::HelveticaMono,
        };
        self.settings.borrow_mut().font = font_choice;
        self.editor.redraw();
    }

    pub fn set_font_size(&mut self, size: i32) {
        self.editor.set_text_size(size);
        self.highlight.set_font(self.highlight.font(), size);
        self.bind_active_buffer();
        self.settings.borrow_mut().font_size = size as u32;
        self.editor.redraw();
    }

    pub fn apply_settings(&mut self, new_settings: AppSettings) {
        let is_dark = match new_settings.theme_mode {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::SystemDefault => detect_system_dark_mode(),
        };
        self.view.dark_mode = is_dark;

        let font = match new_settings.font {
            FontChoice::ScreenBold => Font::ScreenBold,
            FontChoice::Courier => Font::Courier,
            FontChoice::HelveticaMono => Font::Screen,
        };
        self.editor.set_text_font(font);
        self.editor.set_text_size(new_settings.font_size as i32);

        // Set syntax theme first to get background color
        let syntax_theme = new_settings.current_syntax_theme(is_dark);
        self.highlight.set_theme(syntax_theme);
        self.highlight.set_font(font, new_settings.font_size as i32);

        // Get syntax theme colors
        let bg = self.highlight.highlighter().theme_background();
        let fg = self.highlight.highlighter().theme_foreground();

        // Apply theme with syntax background for menu bar color
        apply_theme(
            &mut self.editor,
            &mut self.window,
            &mut self.menu,
            Some(&mut self.update_banner_frame),
            is_dark,
            bg,
        );
        #[cfg(target_os = "windows")]
        set_windows_titlebar_theme(&self.window, is_dark);
        self.update_menu_checkbox("View/Toggle Dark Mode", is_dark);

        // Apply syntax theme colors to editor
        apply_syntax_theme_colors(&mut self.editor, bg, fg);

        // Apply theme to tab bar with editor background color
        if let Some(ref mut tab_bar) = self.tab_bar {
            tab_bar.apply_theme(is_dark, bg);
        }

        // Split panel theme is applied via show_split_view (panel lives in MainWidgets)

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

        // Apply tab size to all document buffers
        self.tab_manager.set_all_tab_distance(new_settings.tab_size as i32);

        let highlighting_changed = self.highlight.highlighting_enabled != new_settings.highlighting_enabled;
        self.highlight.highlighting_enabled = new_settings.highlighting_enabled;
        self.update_menu_checkbox("View/Toggle Syntax Highlighting", self.highlight.highlighting_enabled);

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
            self.highlight.rehighlight_all_documents(&mut self.tab_manager, &self.sender);
            self.bind_active_buffer();
        }
    }

    fn update_menu_checkbox(&self, path: &str, checked: bool) {
        let idx = self.menu.find_index(path);
        if idx >= 0
            && let Some(mut item) = self.menu.at(idx) {
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
            id, pos,
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
    pub fn schedule_text_change_hook(&mut self, id: DocumentId, pos: i32, inserted: i32, deleted: i32) {
        // Coalesce: keep earliest position, sum inserted/deleted
        match self.pending_text_change {
            Some((existing_id, existing_pos, existing_ins, existing_del)) if existing_id == id => {
                self.pending_text_change = Some((id, pos.min(existing_pos), existing_ins + inserted, existing_del + deleted));
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
        let path = self.tab_manager.doc_by_id(id)
            .and_then(|d| d.file_path.clone());

        let result = self.plugins.call_hook(PluginHook::OnTextChanged {
            position,
            inserted_len,
            deleted_len,
        });

        // Process results (diagnostics, annotations, status, widgets)
        let mut ctx = HookContext {
            tab_manager: &mut self.tab_manager,
            view: &mut self.view,
            widget_manager: &mut self.widget.widget_manager,
            sender: self.sender,
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
            self.plugins.call_hook(PluginHook::OnHighlightRequest {
                path,
                content,
            })
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
                    combined.line_annotations.extend(plugin_result.line_annotations);
                    if plugin_result.status_message.is_some() {
                        combined.status_message = plugin_result.status_message;
                    }
                }
            }
            combined
        };

        // Process results (diagnostics, annotations, and toast)
        let mut ctx = HookContext {
            tab_manager: &mut self.tab_manager,
            view: &mut self.view,
            widget_manager: &mut self.widget.widget_manager,
            sender: self.sender,
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
