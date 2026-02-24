use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

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
use std::fs;

use super::controllers::highlight::{HighlightController, HighlightWidgets};
use super::controllers::preview::{wrap_html_for_preview, PreviewController};
use super::controllers::tabs::{GroupColor, GroupId, TabManager};
use super::controllers::update::UpdateController;
use super::domain::document::DocumentId;
use super::domain::messages::Message;
use super::domain::settings::{AppSettings, FontChoice, SyntaxTheme, ThemeMode};
use super::infrastructure::buffer::buffer_text_no_leak;
use super::infrastructure::platform::detect_system_dark_mode;
use super::services::file_size::{check_file_size, format_size, read_chunk, read_tail, FileSizeCheck, TAIL_LINE_COUNT};
use super::services::session::{self, SessionRestore};
use super::plugins::{PluginManager, PluginHook, WidgetActionData, WidgetManager, get_plugin_dir};
use crate::ui::dialogs::large_file::{show_file_too_large_dialog, show_large_file_warning, load_to_buffer_with_progress, StreamLoadResult, TooLargeAction};
use crate::ui::dialogs::plugin_manager::{show_plugin_manager_dialog, PluginManagerResult};
use crate::ui::dialogs::plugin_permissions::{show_permission_dialog, ApprovalResult, PermissionRequest};
use crate::ui::dialogs::settings_dialog::show_settings_dialog;
use crate::ui::editor_container::EditorContainer;
use crate::ui::file_dialogs::{native_open_dialog, native_open_multi_dialog, native_save_dialog};
use crate::ui::split_panel::SplitPanel;
use crate::ui::tab_bar::TabBar;
use crate::ui::theme::{apply_theme, apply_syntax_theme_colors};
use crate::ui::tree_panel::TreePanel;
#[cfg(target_os = "windows")]
use crate::ui::theme::set_windows_titlebar_theme;

pub struct AppState {
    pub tab_manager: TabManager,
    pub tabs_enabled: bool,
    pub tab_bar: Option<TabBar>,
    #[allow(dead_code)]
    pub editor_container: EditorContainer,
    pub editor: TextEditor,
    pub window: Window,
    pub menu: MenuBar,
    pub flex: Flex,
    pub update_banner_frame: Frame,
    pub sender: Sender<Message>,
    pub settings: Rc<RefCell<AppSettings>>,
    pub dark_mode: bool,
    pub show_linenumbers: bool,
    pub word_wrap: bool,
    pub update: UpdateController,
    pub highlight: HighlightController,
    pub preview: PreviewController,
    pub plugins: PluginManager,
    /// Last directory used in a file open/save dialog.
    pub last_open_directory: Option<String>,
    /// Tracks when the session was last auto-saved.
    last_auto_save: Instant,
    /// Whether something changed since the last auto-save.
    session_dirty: bool,
    /// Widget manager for plugin-created widgets
    pub widget_manager: WidgetManager,
    /// Split panel for diff/suggestion views
    pub split_panel: Option<SplitPanel>,
    /// Tree panel for file browser/YAML viewer
    pub tree_panel: Option<TreePanel>,
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
        let mut plugins = PluginManager::new(plugins_enabled);
        if plugins_enabled {
            plugins.load_plugins(&get_plugin_dir());

            // Apply previously saved permission approvals
            for plugin in plugins.plugins_mut() {
                if let Some(approvals) = plugin_approvals.get(&plugin.name) {
                    plugin.approved_commands = approvals.approved_commands.clone();
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

        let editor = editor_container.editor().clone();

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
            dark_mode,
            show_linenumbers,
            word_wrap,
            update: UpdateController::new(),
            highlight,
            preview,
            plugins,
            last_open_directory: None,
            last_auto_save: Instant::now(),
            session_dirty: false,
            widget_manager: WidgetManager::new(),
            split_panel: None,
            tree_panel: None,
        }
    }

    /// Check plugin permissions and show approval dialog for unapproved commands.
    /// Called during startup after plugins are loaded.
    fn check_plugin_permissions(
        plugins: &mut PluginManager,
        settings: &Rc<RefCell<AppSettings>>,
    ) {
        for plugin in plugins.plugins_mut() {
            // Find commands that need user approval
            let unapproved: Vec<String> = plugin
                .permissions
                .execute
                .iter()
                .filter(|cmd| !plugin.approved_commands.contains(cmd))
                .cloned()
                .collect();

            if unapproved.is_empty() {
                continue;
            }

            // Show permission dialog
            let request = PermissionRequest {
                plugin_name: plugin.name.clone(),
                description: plugin.description.clone(),
                commands: unapproved,
            };

            match show_permission_dialog(&request) {
                ApprovalResult::Approved(cmds) => {
                    // Add to plugin's approved commands
                    plugin.approved_commands.extend(cmds.clone());

                    // Save to settings
                    {
                        let mut s = settings.borrow_mut();
                        let approvals = s
                            .plugin_approvals
                            .entry(plugin.name.clone())
                            .or_default();
                        for cmd in cmds {
                            if !approvals.approved_commands.contains(&cmd) {
                                approvals.approved_commands.push(cmd);
                            }
                        }
                    }
                    if let Err(e) = settings.borrow().save() {
                        eprintln!("[plugins] Failed to save permission approvals: {}", e);
                    }
                }
                ApprovalResult::Denied => {
                    // Disable the plugin
                    plugin.enabled = false;
                    eprintln!(
                        "[plugins] {} disabled: user denied permissions",
                        plugin.name
                    );
                }
                ApprovalResult::Cancelled => {
                    // User closed without deciding - plugin runs but can't use commands
                    eprintln!(
                        "[plugins] {} permission dialog cancelled - running with limited permissions",
                        plugin.name
                    );
                }
            }
        }
    }

    /// Request permission approval for a specific plugin (called from diagnostic click)
    pub fn request_plugin_permissions(&mut self, plugin_name: &str) {
        // Find the plugin and its unapproved commands
        let plugin_info: Option<(String, String, Vec<String>)> = {
            self.plugins
                .plugins_mut()
                .iter()
                .find(|p| p.name == plugin_name)
                .map(|plugin| {
                    let unapproved: Vec<String> = plugin
                        .permissions
                        .execute
                        .iter()
                        .filter(|cmd| !plugin.approved_commands.contains(cmd))
                        .cloned()
                        .collect();
                    (plugin.name.clone(), plugin.description.clone(), unapproved)
                })
        };

        let Some((name, description, unapproved)) = plugin_info else {
            eprintln!("[plugins] Plugin '{}' not found", plugin_name);
            return;
        };

        if unapproved.is_empty() {
            eprintln!("[plugins] {} has no unapproved commands", name);
            return;
        }

        // Show permission dialog
        let request = PermissionRequest {
            plugin_name: name.clone(),
            description,
            commands: unapproved,
        };

        match show_permission_dialog(&request) {
            ApprovalResult::Approved(cmds) => {
                // Update plugin's approved commands
                if let Some(plugin) = self.plugins.plugins_mut().iter_mut().find(|p| p.name == name) {
                    plugin.approved_commands.extend(cmds.clone());
                    plugin.enabled = true; // Re-enable if it was disabled
                }

                // Save to settings
                {
                    let mut s = self.settings.borrow_mut();
                    let approvals = s.plugin_approvals.entry(name.clone()).or_default();
                    for cmd in cmds {
                        if !approvals.approved_commands.contains(&cmd) {
                            approvals.approved_commands.push(cmd);
                        }
                    }
                }
                if let Err(e) = self.settings.borrow().save() {
                    eprintln!("[plugins] Failed to save permission approvals: {}", e);
                }

                // Re-run lint on current document to pick up the new permissions
                self.request_manual_highlight();
            }
            ApprovalResult::Denied => {
                if let Some(plugin) = self.plugins.plugins_mut().iter_mut().find(|p| p.name == name) {
                    plugin.enabled = false;
                }
                eprintln!("[plugins] {} disabled: user denied permissions", name);
            }
            ApprovalResult::Cancelled => {
                eprintln!(
                    "[plugins] {} permission dialog cancelled",
                    name
                );
            }
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

        // Update plugin menus based on file type
        crate::ui::menu::update_plugin_menus_for_file(&mut self.menu, &self.plugins, file_path);
    }

    /// Update the Preview in Browser menu item based on current file type.
    /// Only enables it for markdown files (.md, .markdown).
    #[allow(dead_code)] // Keep for backward compatibility
    pub fn update_preview_menu(&mut self) {
        let file_path = self.tab_manager.active_doc()
            .and_then(|doc| doc.file_path.as_ref())
            .map(|p| p.as_str());
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
                self.dark_mode,
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
        // Ask glibc to return freed C++ (FLTK) pages to the OS.
        // jemalloc handles Rust allocations; glibc handles C++ allocations
        // and won't return pages without an explicit trim.
        #[cfg(target_os = "linux")]
        {
            // SAFETY: malloc_trim is a glibc extension that releases free memory
            // back to the OS. It's safe to call at any time - worst case it does
            // nothing if there's no memory to release. The pad=0 argument means
            // "release as much as possible". This helps prevent RSS bloat after
            // closing large documents.
            unsafe {
                unsafe extern "C" { fn malloc_trim(pad: std::ffi::c_int) -> std::ffi::c_int; }
                malloc_trim(0);
            }
        }
        false
    }

    // --- File operations ---

    pub fn open_file(&mut self, path: String) {
        // Remember the parent directory for future open/save dialogs
        let path_ref = std::path::Path::new(&path);
        if let Some(parent) = path_ref.parent() {
            self.last_open_directory = Some(parent.to_string_lossy().to_string());
        }

        // Pre-flight size check to prevent crashes on huge files
        match check_file_size(path_ref) {
            Ok(FileSizeCheck::TooLarge(size)) => {
                match show_file_too_large_dialog(path_ref, size) {
                    TooLargeAction::Cancel => return,
                    TooLargeAction::ViewReadOnly => {
                        // Open read-only viewer (memory-mapped, no editing)
                        crate::ui::dialogs::readonly_viewer::show_readonly_viewer(path_ref);
                        return;
                    }
                    TooLargeAction::OpenTail => {
                        // Read tail and open as special document
                        let filename = path_ref
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
                        match read_tail(path_ref, TAIL_LINE_COUNT) {
                            Ok(content) => {
                                self.open_tail_content(path, content, &filename);
                            }
                            Err(e) => {
                                dialog::alert_default(&format!("Failed to read file tail: {}", e));
                            }
                        }
                        return;
                    }
                    TooLargeAction::OpenChunk(start_line, end_line) => {
                        // Read specific line range and open as special document
                        let filename = path_ref
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
                        match read_chunk(path_ref, start_line, end_line) {
                            Ok(content) => {
                                self.open_chunk_content(path, content, &filename, start_line, end_line);
                            }
                            Err(e) => {
                                dialog::alert_default(&format!("Failed to read file chunk: {}", e));
                            }
                        }
                        return;
                    }
                }
            }
            Ok(FileSizeCheck::Large(size)) => {
                if !show_large_file_warning(path_ref, size) {
                    return; // User cancelled
                }
                // User chose to proceed - load with streaming progress dialog
                // This streams directly to TextBuffer, using ~1x memory instead of ~2x
                match load_to_buffer_with_progress(path_ref, size) {
                    StreamLoadResult::Success(buffer) => {
                        // For large files, skip plugins and syntax highlighting
                        self.open_large_file_buffer(path, buffer);
                    }
                    StreamLoadResult::Cancelled => {
                        // User closed progress dialog
                    }
                    StreamLoadResult::Error(e) => {
                        dialog::alert_default(&format!("Error opening file: {}", e));
                    }
                }
                return;
            }
            Ok(FileSizeCheck::Normal(_)) => {
                // Normal file, proceed with direct read
            }
            Err(e) => {
                // Can't read metadata - let the actual read fail with better error
                eprintln!("[file] Warning: could not check file size: {}", e);
            }
        }

        match fs::read_to_string(&path) {
            Ok(content) => {
                self.open_file_content(path, content);
            }
            Err(e) => dialog::alert_default(&format!("Error opening file: {}", e)),
        }
    }

    /// Open file content that has already been read.
    /// Handles both tabbed and single-document modes.
    fn open_file_content(&mut self, path: String, content: String) {
        if self.tabs_enabled {
            if let Some(existing_id) = self.tab_manager.find_by_path(&path) {
                self.switch_to_document(existing_id);
                self.rebuild_tab_bar();
                return;
            }
            // Close empty Untitled tab if it's the only one
            let empty_untitled = if self.tab_manager.count() == 1 {
                self.tab_manager.active_doc().and_then(|doc| {
                    if doc.file_path.is_none()
                        && !doc.is_dirty()
                        && doc.buffer.length() == 0
                    {
                        Some(doc.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            let id = self.tab_manager.add_from_file(path.clone(), &content);
            if let Some(untitled_id) = empty_untitled {
                self.tab_manager.remove(untitled_id);
            }
            self.detect_and_highlight(id, &path);
            self.switch_to_document(id);
            self.rebuild_tab_bar();

            // Call plugin hooks after document is loaded
            self.plugins.call_hook(PluginHook::OnDocumentOpen {
                path: Some(path.clone()),
            });

            // Run lint hook for immediate feedback on open
            let lint_result = self.plugins.call_hook(PluginHook::OnDocumentLint {
                path,
                content,
            });
            self.process_lint_result(lint_result);
        } else {
            if let Some(doc) = self.tab_manager.active_doc_mut() {
                doc.buffer.set_text(&content);
                doc.has_unsaved_changes.set(false);
                doc.file_path = Some(path.clone());
                doc.update_display_name();
            }
            if let Some(id) = self.tab_manager.active_id() {
                self.detect_and_highlight(id, &path);
            }
            self.update_window_title();
            self.update_menus_for_file_type();

            // Call plugin hooks after document is loaded
            self.plugins.call_hook(PluginHook::OnDocumentOpen {
                path: Some(path.clone()),
            });

            // Run lint hook for immediate feedback on open
            let lint_result = self.plugins.call_hook(PluginHook::OnDocumentLint {
                path,
                content,
            });
            self.process_lint_result(lint_result);
        }
    }

    /// Open large file from a pre-populated TextBuffer (memory-optimized).
    ///
    /// This is used for files > 100MB. The buffer was streamed directly
    /// to avoid keeping two copies of the file in memory.
    /// Skips plugins and syntax highlighting to avoid memory issues.
    fn open_large_file_buffer(&mut self, path: String, buffer: fltk::text::TextBuffer) {
        if self.tabs_enabled {
            if let Some(existing_id) = self.tab_manager.find_by_path(&path) {
                self.switch_to_document(existing_id);
                self.rebuild_tab_bar();
                return;
            }
            // Close empty Untitled tab if it's the only one
            let empty_untitled = if self.tab_manager.count() == 1 {
                self.tab_manager.active_doc().and_then(|doc| {
                    if doc.file_path.is_none()
                        && !doc.is_dirty()
                        && doc.buffer.length() == 0
                    {
                        Some(doc.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            };
            // Use the pre-populated buffer directly, skip style buffer init for memory savings
            let id = self.tab_manager.add_from_buffer(path.clone(), buffer, true);
            if let Some(untitled_id) = empty_untitled {
                self.tab_manager.remove(untitled_id);
            }
            // Skip syntax highlighting for large files - too slow
            // Skip plugin hooks - they may run out of memory
            self.switch_to_document(id);
            self.rebuild_tab_bar();
        } else {
            // Single document mode: replace the current buffer's content
            // Note: This path still copies the content, but single-doc mode is rare
            if let Some(doc) = self.tab_manager.active_doc_mut() {
                // Get content from the pre-populated buffer
                let content = crate::app::infrastructure::buffer::buffer_text_no_leak(&buffer);
                doc.buffer.set_text(&content);
                doc.has_unsaved_changes.set(false);
                doc.file_path = Some(path.clone());
                doc.update_display_name();
            }
            // Skip syntax highlighting and plugins for large files
            self.update_window_title();
        }
    }

    /// Open content from a file tail (last N lines) as a special document.
    /// The document is marked with "(tail)" in its display name.
    fn open_tail_content(&mut self, path: String, content: String, filename: &str) {
        if self.tabs_enabled {
            // Close empty Untitled tab if it's the only one
            let empty_untitled = if self.tab_manager.count() == 1 {
                self.tab_manager.active_doc().and_then(|doc| {
                    if doc.file_path.is_none()
                        && !doc.is_dirty()
                        && doc.buffer.length() == 0
                    {
                        Some(doc.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            };

            let id = self.tab_manager.add_from_file(path.clone(), &content);
            if let Some(untitled_id) = empty_untitled {
                self.tab_manager.remove(untitled_id);
            }

            // Mark as tail mode in display name
            if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                doc.display_name = format!("{} (tail)", filename);
                // Don't mark as dirty - this is expected state
                doc.has_unsaved_changes.set(false);
            }

            self.switch_to_document(id);
            self.rebuild_tab_bar();

            // Call plugin hooks
            self.plugins.call_hook(PluginHook::OnDocumentOpen {
                path: Some(path),
            });
        } else {
            // Single document mode - just load the tail
            if let Some(doc) = self.tab_manager.active_doc_mut() {
                doc.buffer.set_text(&content);
                doc.has_unsaved_changes.set(false);
                doc.file_path = Some(path.clone());
                doc.display_name = format!("{} (tail)", filename);
            }
            self.update_window_title();

            self.plugins.call_hook(PluginHook::OnDocumentOpen {
                path: Some(path),
            });
        }
    }

    /// Open content from a specific line range (chunk) as a special document.
    /// The document is marked with "(lines X-Y)" in its display name.
    fn open_chunk_content(
        &mut self,
        path: String,
        content: String,
        filename: &str,
        start_line: usize,
        end_line: usize,
    ) {
        let chunk_label = format!("{} (lines {}-{})", filename, start_line, end_line);

        if self.tabs_enabled {
            // Close empty Untitled tab if it's the only one
            let empty_untitled = if self.tab_manager.count() == 1 {
                self.tab_manager.active_doc().and_then(|doc| {
                    if doc.file_path.is_none()
                        && !doc.is_dirty()
                        && doc.buffer.length() == 0
                    {
                        Some(doc.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            };

            let id = self.tab_manager.add_from_file(path.clone(), &content);
            if let Some(untitled_id) = empty_untitled {
                self.tab_manager.remove(untitled_id);
            }

            // Mark as chunk mode in display name
            if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                doc.display_name = chunk_label;
                // Don't mark as dirty - this is expected state
                doc.has_unsaved_changes.set(false);
            }

            self.switch_to_document(id);
            self.rebuild_tab_bar();

            // Call plugin hooks
            self.plugins.call_hook(PluginHook::OnDocumentOpen {
                path: Some(path),
            });
        } else {
            // Single document mode - just load the chunk
            if let Some(doc) = self.tab_manager.active_doc_mut() {
                doc.buffer.set_text(&content);
                doc.has_unsaved_changes.set(false);
                doc.file_path = Some(path.clone());
                doc.display_name = chunk_label;
            }
            self.update_window_title();

            self.plugins.call_hook(PluginHook::OnDocumentOpen {
                path: Some(path),
            });
        }
    }

    pub fn file_new(&mut self) {
        if self.tabs_enabled {
            let id = self.tab_manager.add_untitled();
            self.switch_to_document(id);
            self.rebuild_tab_bar();
        } else {
            if let Some(doc) = self.tab_manager.active_doc_mut() {
                doc.buffer.set_text("");
                doc.has_unsaved_changes.set(false);
                doc.file_path = None;
                doc.display_name = "Untitled".to_string();
                doc.syntax_name = None;
                doc.checkpoints.clear();
                doc.style_buffer.set_text("");
            }
            self.update_window_title();
            self.update_menus_for_file_type();
        }
    }

    pub fn file_open(&mut self) {
        let dir = self.last_open_directory.as_deref();
        if self.tabs_enabled {
            let paths = native_open_multi_dialog(dir);
            for path in paths {
                self.open_file(path);
            }
        } else if let Some(path) = native_open_dialog(dir) {
            self.open_file(path);
        }
    }

    pub fn file_save(&mut self) {
        let (file_path, text, doc_id, is_partial) = {
            if let Some(doc) = self.tab_manager.active_doc() {
                // Check if this is a partial view (tail or chunk)
                let is_partial = doc.display_name.contains("(tail)")
                    || doc.display_name.contains("(lines ");
                (doc.file_path.clone(), buffer_text_no_leak(&doc.buffer), doc.id.0, is_partial)
            } else {
                return;
            }
        };

        // Warn if saving a partial document - user might accidentally overwrite the full file
        if is_partial {
            let msg = "Warning: This is a partial view of the file.\n\n\
                       Saving will overwrite the file with ONLY these lines.\n\
                       The rest of the original file will be lost.\n\n\
                       Continue?";
            if dialog::choice2_default(msg, "Save", "Cancel", "") != Some(0) {
                return;
            }
        }

        if let Some(ref path) = file_path {
            // Call plugin hook - plugins can modify content before save
            let hook_result = self.plugins.call_hook(PluginHook::OnDocumentSave {
                path: path.clone(),
                content: text.clone(),
            });
            let text_to_save = hook_result.modified_content.unwrap_or(text.clone());

            match fs::write(path, &text_to_save) {
                Ok(_) => {
                    if let Some(doc) = self.tab_manager.active_doc_mut() {
                        doc.mark_clean();
                    }
                    self.update_window_title();
                    self.rebuild_tab_bar();
                    self.update_preview_file(doc_id, path, &text_to_save);

                    // Call lint hook after successful save
                    let lint_result = self.plugins.call_hook(PluginHook::OnDocumentLint {
                        path: path.clone(),
                        content: text_to_save,
                    });
                    self.process_lint_result(lint_result);
                }
                Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
            }
        } else {
            self.file_save_as();
        }
    }

    pub fn file_save_as(&mut self) {
        let text = {
            if let Some(doc) = self.tab_manager.active_doc() {
                buffer_text_no_leak(&doc.buffer)
            } else {
                return;
            }
        };

        if let Some(path) = native_save_dialog(self.last_open_directory.as_deref()) {
            if let Some(parent) = std::path::Path::new(&path).parent() {
                self.last_open_directory = Some(parent.to_string_lossy().to_string());
            }
            match fs::write(&path, &text) {
                Ok(_) => {
                    let id = {
                        if let Some(doc) = self.tab_manager.active_doc_mut() {
                            doc.file_path = Some(path.clone());
                            doc.update_display_name();
                            doc.mark_clean();
                            Some(doc.id)
                        } else {
                            None
                        }
                    };
                    if let Some(id) = id {
                        self.detect_and_highlight(id, &path);
                        if let Some(doc) = self.tab_manager.doc_by_id(id) {
                            let style_buf = doc.style_buffer.clone();
                            let table = self.highlight.style_table();
                            self.editor.set_highlight_data_ext(style_buf, table);
                        }
                        self.update_preview_file(id.0, &path, &text);
                    }
                    self.update_window_title();
                    self.rebuild_tab_bar();
                    self.update_menus_for_file_type();

                    // Call lint hook after successful save
                    let lint_result = self.plugins.call_hook(PluginHook::OnDocumentLint {
                        path: path.clone(),
                        content: text,
                    });
                    self.process_lint_result(lint_result);
                }
                Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
            }
        }
    }

    /// Update the preview HTML file if the saved file is markdown.
    /// This allows the browser to refresh and show updated content.
    fn update_preview_file(&mut self, doc_id: u64, path: &str, text: &str) {
        if !PreviewController::is_markdown_file(Some(path)) {
            return;
        }

        let raw_html = PreviewController::render_markdown(text);
        let base_dir = std::path::Path::new(path).parent();
        let wrapped = wrap_html_for_preview(&raw_html, self.dark_mode, base_dir);

        // Silently update the temp file - don't open browser again
        let _ = self.preview.write_html(Some(path), doc_id, &wrapped);
    }

    /// Restore session from disk. Call after bind_active_buffer and apply_settings.
    pub fn restore_session(&mut self) {
        let mode = self.settings.borrow().session_restore;
        if mode == SessionRestore::Off || !self.tabs_enabled {
            return;
        }

        let session_data = match session::load_session(mode) {
            Some(data) => data,
            None => return,
        };

        self.last_open_directory = session_data.last_open_directory.clone();

        if let Some(id) = self.tab_manager.active_id() {
            self.tab_manager.remove(id);
        }

        // Restore groups and build index -> GroupId mapping
        use super::controllers::tabs::TabGroup as TG;
        let restored_groups: Vec<TG> = session_data.groups.iter().map(|gs| {
            TG {
                id: GroupId(0), // placeholder, restore_groups assigns real ids
                name: gs.name.clone(),
                color: GroupColor::from_str(&gs.color).unwrap_or(GroupColor::Grey),
                collapsed: gs.collapsed,
            }
        }).collect();
        let group_ids = self.tab_manager.restore_groups(restored_groups);

        let mut first_id = None;
        let target_index = session_data.active_index;

        for (i, doc_session) in session_data.documents.iter().enumerate() {
            // Resolve group assignment
            let group_id = doc_session.group_index.and_then(|idx| group_ids.get(idx).copied());

            if let Some(ref path) = doc_session.file_path {
                // Skip files that are too large (don't show dialogs at startup)
                let path_ref = std::path::Path::new(path);
                if let Ok(FileSizeCheck::TooLarge(size)) = check_file_size(path_ref) {
                    eprintln!(
                        "[session] Skipping '{}' ({}) - exceeds size limit",
                        path,
                        format_size(size)
                    );
                    continue;
                }

                if let Ok(content) = fs::read_to_string(path) {
                    let id = self.tab_manager.add_from_file(path.clone(), &content);
                    if first_id.is_none() {
                        first_id = Some(id);
                    }

                    self.detect_and_highlight(id, path);

                    if mode == SessionRestore::Full
                        && let Some(ref temp_file) = doc_session.temp_file
                            && let Some(temp_content) = session::read_temp_file(temp_file)
                                && let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                                    doc.buffer.set_text(&temp_content);
                                }

                    if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                        doc.cursor_position = doc_session.cursor_position;
                        doc.group_id = group_id;
                    }

                    if i == target_index {
                        self.tab_manager.set_active(id);
                    }
                }
            } else if mode == SessionRestore::Full
                && let Some(ref temp_file) = doc_session.temp_file
                    && let Some(temp_content) = session::read_temp_file(temp_file) {
                        let id = self.tab_manager.add_untitled();
                        if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                            doc.buffer.set_text(&temp_content);
                            doc.cursor_position = doc_session.cursor_position;
                            doc.group_id = group_id;
                        }
                        if first_id.is_none() {
                            first_id = Some(id);
                        }
                        if i == target_index {
                            self.tab_manager.set_active(id);
                        }
                    }
        }

        if self.tab_manager.count() == 0 {
            self.tab_manager.add_untitled();
        }

        self.bind_active_buffer();
        if let Some(doc) = self.tab_manager.active_doc() {
            let cursor = doc.cursor_position;
            self.editor.set_insert_position(cursor);
            self.editor.show_insert_position();
        }
        self.update_window_title();
        self.rebuild_tab_bar();

        session::clear_session();
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

            let _ = session::save_session(&self.tab_manager, session_mode, self.last_open_directory.as_deref())
                .inspect_err(|e| eprintln!("Failed to save session: {}", e));
        }

        should_quit
    }

    /// Mark that the session state has changed and should be auto-saved.
    pub fn mark_session_dirty(&mut self) {
        self.session_dirty = true;
    }

    /// Auto-save the session every 30 seconds if something changed.
    pub fn auto_save_session_if_needed(&mut self) {
        const AUTO_SAVE_INTERVAL_SECS: u64 = 30;

        if !self.session_dirty {
            return;
        }
        if self.last_auto_save.elapsed().as_secs() < AUTO_SAVE_INTERVAL_SECS {
            return;
        }

        let session_mode = self.settings.borrow().session_restore;
        if session_mode == SessionRestore::Off {
            return;
        }

        if let Err(e) = session::save_session(&self.tab_manager, session_mode, self.last_open_directory.as_deref()) {
            eprintln!("Auto-save session failed: {}", e);
        }
        self.session_dirty = false;
        self.last_auto_save = Instant::now();
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

    // --- View toggles ---

    pub fn update_linenumber_width(&mut self) {
        // Use FLTK's native line numbers
        if !self.show_linenumbers {
            self.editor.set_linenumber_width(0);
            return;
        }
        let line_count = self.active_buffer().count_lines(0, self.active_buffer().length());
        let digits = ((line_count + 1) as f64).log10().floor() as i32 + 1;
        let width = (digits * 8 + 16).max(40);
        self.editor.set_linenumber_width(width);
    }

    pub fn toggle_line_numbers(&mut self) {
        self.show_linenumbers = !self.show_linenumbers;
        self.update_linenumber_width();
        self.editor.redraw();
    }

    pub fn toggle_word_wrap(&mut self) {
        self.word_wrap = !self.word_wrap;
        if self.word_wrap {
            self.editor.wrap_mode(WrapMode::AtBounds, 0);
        } else {
            self.editor.wrap_mode(WrapMode::None, 0);
        }
        self.editor.redraw();
    }

    pub fn toggle_dark_mode(&mut self) {
        self.dark_mode = !self.dark_mode;

        // Set syntax theme first to get the background color
        let theme = self.settings.borrow().current_syntax_theme(self.dark_mode);
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
            self.dark_mode,
            bg,
        );
        #[cfg(target_os = "windows")]
        set_windows_titlebar_theme(&self.window, self.dark_mode);

        // Apply syntax theme colors to editor
        apply_syntax_theme_colors(&mut self.editor, bg, fg);

        // Apply theme to tab bar with editor background color
        if let Some(ref mut tab_bar) = self.tab_bar {
            tab_bar.apply_theme(self.dark_mode, bg);
        }

        self.highlight.rehighlight_all_documents(&mut self.tab_manager, &self.sender);
        self.bind_active_buffer();

        // Call plugin hook
        self.plugins.call_hook(PluginHook::OnThemeChanged { is_dark: self.dark_mode });
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
        // Check if current file is markdown
        let (is_md, doc_id) = self.tab_manager.active_doc()
            .map(|doc| {
                let is_md = doc.file_path.as_deref()
                    .map(|p| PreviewController::is_markdown_file(Some(p)))
                    .unwrap_or(false);
                (is_md, doc.id.0)
            })
            .unwrap_or((false, 0));

        if !is_md {
            dialog::message_default("Preview is only available for Markdown files (.md, .markdown, .mdown)");
            return;
        }

        // Get the text and file path
        let (text, file_path) = {
            if let Some(doc) = self.tab_manager.active_doc() {
                (buffer_text_no_leak(&doc.buffer), doc.file_path.clone())
            } else {
                return;
            }
        };

        // Render markdown to HTML
        let raw_html = PreviewController::render_markdown(&text);

        // Get base directory for resolving relative paths (images, links)
        let base_dir = file_path.as_ref()
            .and_then(|p| std::path::Path::new(p).parent())
            .map(|p| p.to_path_buf());

        // Wrap HTML with styling
        let wrapped = wrap_html_for_preview(&raw_html, self.dark_mode, base_dir.as_deref());

        // Open in browser
        if let Err(e) = self.preview.open_in_browser(file_path.as_deref(), doc_id, &wrapped) {
            dialog::alert_default(&e);
        }
    }

    // --- Format ---

    pub fn set_font(&mut self, font: Font) {
        self.editor.set_text_font(font);
        self.editor.redraw();
    }

    pub fn set_font_size(&mut self, size: i32) {
        self.editor.set_text_size(size);
        self.editor.redraw();
    }

    // --- Settings ---

    pub fn open_settings(&mut self) {
        let current = self.settings.borrow().clone();
        if let Some(new_settings) = show_settings_dialog(&current, &self.sender, self.dark_mode) {
            if let Err(e) = new_settings.save() {
                dialog::alert_default(&format!("Failed to save settings: {}", e));
                return;
            }
            self.apply_settings(new_settings);
        }
    }

    /// Show the plugin manager dialog
    pub fn show_plugin_manager(&mut self) {
        let result = show_plugin_manager_dialog(&self.plugins, self.dark_mode);

        match result {
            PluginManagerResult::ToggledPlugins(toggles) => {
                for (name, enabled) in toggles {
                    self.plugins.toggle_plugin(&name, enabled);
                }

                // Update settings with disabled plugins
                {
                    let mut settings = self.settings.borrow_mut();
                    settings.disabled_plugins = self.plugins.disabled_plugin_names();
                    let _ = settings.save();
                }

                // Rebuild plugins menu
                crate::ui::menu::rebuild_plugins_menu(
                    &mut self.menu,
                    &self.sender,
                    &self.settings.borrow(),
                    &self.plugins,
                );
            }
            PluginManagerResult::ReloadAll => {
                self.sender.send(Message::PluginsReloadAll);
            }
            PluginManagerResult::InstalledPlugins(names) => {
                // Reload to pick up new plugins
                self.sender.send(Message::PluginsReloadAll);
                // Show success message
                eprintln!("[plugins] Installed plugins: {}", names.join(", "));
            }
            PluginManagerResult::Cancelled => {}
        }
    }

    /// Trigger a background check for plugin updates
    pub fn check_plugin_updates(&self) {
        use crate::app::services::plugin_update_checker::check_for_plugin_updates;

        let sender = self.sender.clone();

        // Run the check in a separate thread to avoid blocking UI
        std::thread::spawn(move || {
            match check_for_plugin_updates() {
                Ok(updates) => {
                    sender.send(Message::PluginUpdatesChecked(updates));
                }
                Err(e) => {
                    eprintln!("[plugin-update-checker] Error: {}", e);
                    sender.send(Message::PluginUpdatesChecked(Vec::new()));
                }
            }
        });
    }

    /// Handle the result of a plugin update check
    pub fn handle_plugin_updates_checked(&mut self, updates: Vec<crate::app::services::plugin_update_checker::PluginUpdateInfo>) {
        // Update the last check timestamp
        {
            let mut settings = self.settings.borrow_mut();
            settings.last_plugin_update_check = crate::app::services::plugin_update_checker::current_timestamp();
            let _ = settings.save();
        }

        // Log the results
        if updates.is_empty() {
            eprintln!("[plugin-update-checker] All plugins are up to date");
        } else {
            eprintln!(
                "[plugin-update-checker] {} plugin update(s) available:",
                updates.len()
            );
            for update in &updates {
                eprintln!(
                    "  - {} ({} -> {})",
                    update.plugin_name, update.installed_version, update.available_version
                );
            }
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

    pub fn apply_settings(&mut self, new_settings: AppSettings) {
        let is_dark = match new_settings.theme_mode {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::SystemDefault => detect_system_dark_mode(),
        };
        self.dark_mode = is_dark;

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

        self.show_linenumbers = new_settings.line_numbers_enabled;
        self.update_linenumber_width();
        self.update_menu_checkbox("View/Toggle Line Numbers", self.show_linenumbers);

        self.word_wrap = new_settings.word_wrap_enabled;
        if self.word_wrap {
            self.editor.wrap_mode(WrapMode::AtBounds, 0);
        } else {
            self.editor.wrap_mode(WrapMode::None, 0);
        }
        self.update_menu_checkbox("View/Toggle Word Wrap", self.word_wrap);

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

    fn detect_and_highlight(&mut self, id: DocumentId, path: &str) {
        self.highlight.detect_and_highlight(
            id, path,
            &mut self.tab_manager,
            &self.sender,
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

    pub fn handle_group_create(&mut self, doc_id: DocumentId) {
        self.tab_manager.create_group(&[doc_id]);
        self.rebuild_tab_bar();
    }

    pub fn handle_group_delete(&mut self, group_id: GroupId) {
        self.tab_manager.delete_group(group_id);
        self.rebuild_tab_bar();
    }

    pub fn handle_group_close(&mut self, group_id: GroupId) {
        let doc_ids = self.tab_manager.group_doc_ids(group_id);
        for id in doc_ids {
            if self.close_tab(id) {
                // Last tab closed — app should exit, but we continue
                // since close_tab already handles that signal
                return;
            }
        }
        // Remove the group itself (tabs already removed from it by close_tab)
        self.tab_manager.delete_group(group_id);
        self.rebuild_tab_bar();
    }

    pub fn handle_group_rename(&mut self, group_id: GroupId) {
        let current_name = self.tab_manager.group_by_id(group_id)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        if let Some(new_name) = dialog::input_default("Group name:", &current_name) {
            self.tab_manager.rename_group(group_id, new_name);
            self.rebuild_tab_bar();
        }
    }

    pub fn handle_group_recolor(&mut self, group_id: GroupId, color: GroupColor) {
        self.tab_manager.recolor_group(group_id, color);
        self.rebuild_tab_bar();
    }

    pub fn handle_group_add_tab(&mut self, doc_id: DocumentId, group_id: GroupId) {
        self.tab_manager.set_tab_group(doc_id, Some(group_id));
        self.rebuild_tab_bar();
    }

    pub fn handle_group_remove_tab(&mut self, doc_id: DocumentId) {
        self.tab_manager.set_tab_group(doc_id, None);
        self.rebuild_tab_bar();
    }

    pub fn handle_group_toggle(&mut self, group_id: GroupId) {
        self.tab_manager.toggle_group_collapsed(group_id);
        self.rebuild_tab_bar();
    }

    pub fn handle_group_by_drag(&mut self, source_id: DocumentId, target_id: DocumentId) {
        let target_group = self.tab_manager.documents()
            .iter()
            .find(|d| d.id == target_id)
            .and_then(|d| d.group_id);

        if let Some(gid) = target_group {
            // Target is already in a group — add source to that group
            self.tab_manager.set_tab_group(source_id, Some(gid));
        } else {
            // Neither grouped — create a new group with both
            self.tab_manager.create_group(&[target_id, source_id]);
        }
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

    // --- Plugin handlers ---

    /// Check plugin permissions (deferred until after UI is ready).
    /// Called via CheckPluginPermissions message after main window is shown.
    pub fn check_plugin_permissions_deferred(&mut self) {
        Self::check_plugin_permissions(&mut self.plugins, &self.settings);
    }

    /// Toggle the global plugin system on/off
    pub fn handle_plugins_toggle_global(&mut self) {
        let currently_enabled = self.settings.borrow().plugins_enabled;
        let new_enabled = !currently_enabled;

        {
            let mut settings = self.settings.borrow_mut();
            settings.plugins_enabled = new_enabled;
            let _ = settings.save();
        }

        self.plugins.set_enabled(new_enabled);
        if new_enabled {
            // Load plugins if enabling
            self.plugins.reload_all(&get_plugin_dir());
            // Apply disabled list
            let disabled = self.settings.borrow().disabled_plugins.clone();
            for name in &disabled {
                self.plugins.toggle_plugin(name, false);
            }
        }

        // Rebuild plugins menu to reflect changes
        crate::ui::menu::rebuild_plugins_menu(
            &mut self.menu,
            &self.sender,
            &self.settings.borrow(),
            &self.plugins,
        );
    }

    /// Toggle a specific plugin on/off
    pub fn handle_plugin_toggle(&mut self, name: String) {
        // Find current state and toggle
        let was_enabled = self.plugins.list_plugins()
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.enabled)
            .unwrap_or(false);

        self.plugins.toggle_plugin(&name, !was_enabled);

        // Update settings
        {
            let mut settings = self.settings.borrow_mut();
            let disabled = self.plugins.disabled_plugin_names();
            settings.disabled_plugins = disabled;
            let _ = settings.save();
        }

        // Rebuild plugins menu
        crate::ui::menu::rebuild_plugins_menu(
            &mut self.menu,
            &self.sender,
            &self.settings.borrow(),
            &self.plugins,
        );
    }

    /// Reload all plugins from disk
    pub fn handle_plugins_reload(&mut self) {
        self.plugins.reload_all(&get_plugin_dir());

        // Apply disabled list
        let disabled = self.settings.borrow().disabled_plugins.clone();
        for name in &disabled {
            self.plugins.toggle_plugin(name, false);
        }

        // Rebuild plugins menu
        crate::ui::menu::rebuild_plugins_menu(
            &mut self.menu,
            &self.sender,
            &self.settings.borrow(),
            &self.plugins,
        );
    }

    /// Handle a plugin's custom menu action.
    /// Calls the `on_menu_action` hook on the specified plugin.
    pub fn handle_plugin_menu_action(&mut self, plugin_name: &str, action: &str) {
        // Get current document info for the hook
        let path = self.tab_manager.active_doc().and_then(|d| {
            d.file_path.as_ref().map(|p| p.clone())
        });
        let content = self.active_buffer().text();

        // Call the hook on the specific plugin
        let hook = PluginHook::OnMenuAction {
            action: action.to_string(),
            path,
            content,
        };

        let result = self.plugins.call_hook_on_plugin(plugin_name, hook);

        if let Some(result) = result {
            // Process widget requests (split view, tree view)
            self.process_widget_requests(&result, plugin_name);

            // Process the result (diagnostics, annotations, etc.)
            self.process_hook_result(result);
        } else {
            // Plugin not found or not enabled
            eprintln!(
                "[plugins] Plugin '{}' not found or not enabled for action '{}'",
                plugin_name, action
            );
        }
    }

    /// Process the result from a plugin hook (diagnostics, annotations, status message)
    fn process_hook_result(&mut self, result: crate::app::plugins::HookResult) {
        // Handle modified content (for format actions)
        if let Some(modified_content) = result.modified_content {
            let mut buf = self.active_buffer();
            buf.set_text(&modified_content);
        }

        // Update diagnostics
        if !result.diagnostics.is_empty() {
            self.sender.send(Message::DiagnosticsUpdate(result.diagnostics));
        }

        // Update line annotations
        if !result.line_annotations.is_empty() {
            self.update_annotations(result.line_annotations);
        }

        // Show status message
        if let Some(status) = result.status_message {
            self.sender.send(Message::ToastShow(status.level, status.text));
        }
    }

    /// Navigate to a specific line number (1-indexed)
    pub fn goto_line(&mut self, line: u32) {
        let buf = self.active_buffer();
        let line_count = buf.count_lines(0, buf.length());

        // Clamp line to valid range
        let target_line = (line as i32).min(line_count).max(1);

        // Find position of the line
        let mut pos = 0;
        for _ in 1..target_line {
            if let Some(next_pos) = buf.find_char_forward(pos, '\n') {
                pos = next_pos + 1;
            } else {
                break;
            }
        }

        // Set cursor position and scroll to it
        self.editor.set_insert_position(pos);
        self.editor.show_insert_position();
        self.editor.take_focus().ok();
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Line Annotations (gutter + inline highlights)
    // ─────────────────────────────────────────────────────────────────────────────

    /// Update line annotations by applying bgcolor markers to the style buffer.
    /// This creates VS Code-like line highlighting for errors, warnings, git changes, etc.
    ///
    /// Supports two modes:
    /// - **Gutter marks**: Highlight the entire line with a background color
    /// - **Inline highlights**: Highlight specific column ranges within a line
    ///
    /// When multiple annotations target the same line, the highest-priority color wins
    /// (Error > Warning > Info > Hint). Inline highlights are applied on top of gutter marks.
    ///
    /// Also supports custom RGB colors (up to ~10 unique colors).
    pub fn update_annotations(&mut self, annotations: Vec<super::plugins::LineAnnotation>) {
        use crate::app::plugins::AnnotationColor;
        use crate::app::services::syntax::style_map::StyleMap;
        use std::collections::BTreeMap;

        let Some(doc) = self.tab_manager.active_doc() else {
            return;
        };
        let mut style_buf = doc.style_buffer.clone();
        let buf = doc.buffer.clone();

        // Helper closure to get marker char, handling RGB colors
        let get_marker_char = |highlight: &mut crate::app::controllers::highlight::HighlightController, color: &AnnotationColor| -> char {
            match color {
                AnnotationColor::Rgb(r, g, b) => highlight.get_or_insert_marker_rgb(*r, *g, *b),
                _ => StyleMap::marker_style_char(color),
            }
        };

        // Merge annotations by line, keeping highest-priority gutter and all inlines
        let mut merged: BTreeMap<u32, (Option<super::plugins::GutterMark>, Vec<super::plugins::InlineHighlight>)> = BTreeMap::new();

        for ann in annotations {
            let entry = merged.entry(ann.line).or_insert((None, Vec::new()));

            // For gutter: keep the highest priority (lowest priority number)
            if let Some(new_gutter) = ann.gutter {
                match &entry.0 {
                    None => entry.0 = Some(new_gutter),
                    Some(existing) => {
                        if new_gutter.color.priority() < existing.color.priority() {
                            entry.0 = Some(new_gutter);
                        }
                    }
                }
            }

            // For inline: collect all (they apply to different column ranges)
            entry.1.extend(ann.inline);
        }

        // Now apply merged annotations
        for (line_num, (gutter, inlines)) in merged {
            let target_line = line_num.saturating_sub(1) as i32;

            // Find line start position
            let mut line_start = 0;
            for _ in 0..target_line {
                if let Some(next_pos) = buf.find_char_forward(line_start, '\n') {
                    line_start = next_pos + 1;
                } else {
                    break;
                }
            }

            // Find line end (excluding newline for column calculations)
            let line_end_with_newline = buf.find_char_forward(line_start, '\n')
                .map(|p| p + 1)
                .unwrap_or(buf.length());
            let line_end = buf.find_char_forward(line_start, '\n')
                .unwrap_or(buf.length());

            let line_len = line_end - line_start;

            // Handle gutter mark (full line highlight) - apply first as base
            if let Some(ref gutter_mark) = gutter {
                if line_len > 0 {
                    let marker_char = get_marker_char(&mut self.highlight, &gutter_mark.color);
                    let marker_str: String = std::iter::repeat_n(marker_char, (line_end_with_newline - line_start) as usize).collect();
                    style_buf.replace(line_start, line_end_with_newline, &marker_str);
                }
            }

            // Sort inlines by priority (highest priority = lowest number, applied last to win)
            let mut sorted_inlines = inlines;
            sorted_inlines.sort_by(|a, b| b.color.priority().cmp(&a.color.priority()));

            // Handle inline highlights (partial line highlight) - apply on top
            for inline in sorted_inlines {
                let marker_char = get_marker_char(&mut self.highlight, &inline.color);

                // Convert 1-indexed columns to 0-indexed buffer positions
                let start_col = (inline.start_col.saturating_sub(1) as i32).min(line_len);
                let end_col = inline.end_col
                    .map(|c| (c.saturating_sub(1) as i32).min(line_len))
                    .unwrap_or(line_len);

                if start_col >= end_col {
                    continue;
                }

                let highlight_start = line_start + start_col;
                let highlight_end = line_start + end_col;
                let highlight_len = (highlight_end - highlight_start) as usize;

                if highlight_len > 0 {
                    let marker_str: String = std::iter::repeat_n(marker_char, highlight_len).collect();
                    style_buf.replace(highlight_start, highlight_end, &marker_str);
                }
            }
        }

        // Re-apply style table to show updated highlights
        let table = self.highlight.style_table();
        self.editor.set_highlight_data_ext(style_buf, table);
        self.editor.redraw();
    }

    /// Clear line annotations by re-highlighting the active document only.
    /// This restores the original syntax highlighting without annotation overlays.
    pub fn clear_annotations(&mut self) {
        // Get active document info
        let Some(doc) = self.tab_manager.active_doc() else { return };
        let Some(syntax_name) = doc.syntax_name.clone() else { return };
        let text = buffer_text_no_leak(&doc.buffer);

        // Re-run syntax highlighting (clears annotation overlays)
        let result = self.highlight.highlight_full(&text, &syntax_name);

        // Update style buffer
        if let Some(doc) = self.tab_manager.active_doc_mut() {
            doc.style_buffer.set_text(&result.style_string);
            doc.checkpoints = result.checkpoints;
        }

        // Rebind to editor
        self.bind_active_buffer();
    }

    /// Request manual highlight from plugins (Ctrl+Shift+L)
    pub fn request_manual_highlight(&mut self) {
        // Get current document info
        let doc = self.tab_manager.active_doc();
        let path = doc.as_ref().and_then(|d| d.file_path.clone());
        let content = buffer_text_no_leak(&self.editor.buffer().unwrap_or_default());

        // Call the highlight request hook
        let result = self.plugins.call_hook(PluginHook::OnHighlightRequest {
            path,
            content,
        });

        // Process results (diagnostics, annotations, and toast)
        self.process_lint_result(result);
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

    /// Process lint result from plugin hook: send diagnostics, annotations, and toast
    fn process_lint_result(&mut self, result: super::plugins::HookResult) {
        // Always send diagnostics (even empty) to update or clear the panel
        self.sender.send(Message::DiagnosticsUpdate(result.diagnostics));

        // Update or clear annotations
        if !result.line_annotations.is_empty() {
            self.update_annotations(result.line_annotations);
        } else {
            // Clear any existing annotations when no issues found
            self.clear_annotations();
        }

        if let Some(status) = result.status_message {
            self.sender.send(Message::ToastShow(status.level, status.text));
        }
    }

    // ===== Widget API Methods =====

    /// Show a split view panel from a plugin request
    pub fn show_split_view(
        &mut self,
        session_id: u32,
        _plugin_name: &str,
        request: &super::plugins::SplitViewRequest,
    ) {
        // Create split panel if it doesn't exist
        if self.split_panel.is_none() {
            let mut panel = SplitPanel::new(self.sender);
            panel.apply_theme(self.dark_mode);
            self.split_panel = Some(panel);
        }

        if let Some(ref mut panel) = self.split_panel {
            panel.show_request(session_id, request);
            // Note: In a full implementation, we'd integrate this into the Flex layout
            // For now, the panel manages its own visibility
        }
    }

    /// Hide the split view panel
    pub fn hide_split_view(&mut self, session_id: u32) {
        if let Some(ref mut panel) = self.split_panel {
            if panel.session_id() == Some(session_id) {
                panel.hide();
            }
        }

        // Clean up session
        self.widget_manager.remove_session(session_id);
    }

    /// Handle split view accept action
    pub fn handle_split_view_accept(&mut self, session_id: u32) {
        // Get session info
        let session = match self.widget_manager.get_session(session_id) {
            Some(s) => s.clone(),
            None => return,
        };

        // Get the right pane content
        let right_content = if let Some(ref panel) = self.split_panel {
            Some(panel.right_content())
        } else {
            None
        };

        // Call plugin's on_widget_action hook
        let result = self.plugins.call_hook_on_plugin(
            &session.plugin_name,
            PluginHook::OnWidgetAction {
                widget_type: "split_view".to_string(),
                action: "accept".to_string(),
                session_id,
                data: WidgetActionData {
                    right_content,
                    node_path: None,
                },
            },
        );

        // Process result (may contain modified_content to apply to editor)
        if let Some(result) = result {
            if let Some(content) = result.modified_content {
                // Apply the accepted content to the editor
                if let Some(mut buf) = self.editor.buffer() {
                    buf.set_text(&content);
                }
            }
        }

        // Hide the panel
        self.hide_split_view(session_id);
    }

    /// Handle split view reject action
    pub fn handle_split_view_reject(&mut self, session_id: u32) {
        // Just hide the panel, no content changes
        self.hide_split_view(session_id);
    }

    /// Show a tree view panel from a plugin request
    pub fn show_tree_view(
        &mut self,
        session_id: u32,
        _plugin_name: &str,
        request: &super::plugins::TreeViewRequest,
    ) {
        // Create tree panel if it doesn't exist
        if self.tree_panel.is_none() {
            let mut panel = TreePanel::new(self.sender);
            panel.apply_theme(self.dark_mode);
            self.tree_panel = Some(panel);
        }

        // If YAML content is provided, parse it into a tree
        let final_request = if request.yaml_content.is_some() && request.root.is_none() {
            let yaml_content = request.yaml_content.as_ref().unwrap();
            let root = super::services::yaml_parser::parse_yaml_to_tree(yaml_content, &request.title);
            super::plugins::TreeViewRequest {
                title: request.title.clone(),
                root: Some(root),
                yaml_content: None,
                on_click_action: request.on_click_action.clone(),
                expand_depth: request.expand_depth,
            }
        } else {
            request.clone()
        };

        if let Some(ref mut panel) = self.tree_panel {
            panel.show_request(session_id, &final_request);
        }
    }

    /// Hide the tree view panel
    pub fn hide_tree_view(&mut self, session_id: u32) {
        if let Some(ref mut panel) = self.tree_panel {
            panel.hide();
        }

        // Clean up session
        self.widget_manager.remove_session(session_id);
    }

    /// Handle tree view node click
    pub fn handle_tree_view_node_click(&mut self, session_id: u32, node_path: Vec<String>) {
        // Get session info
        let session = match self.widget_manager.get_session(session_id) {
            Some(s) => s.clone(),
            None => return,
        };

        // Call plugin's on_widget_action hook
        let _ = self.plugins.call_hook_on_plugin(
            &session.plugin_name,
            PluginHook::OnWidgetAction {
                widget_type: "tree_view".to_string(),
                action: "node_clicked".to_string(),
                session_id,
                data: WidgetActionData {
                    right_content: None,
                    node_path: Some(node_path),
                },
            },
        );
    }

    /// Process widget requests from a hook result
    pub fn process_widget_requests(&mut self, result: &super::plugins::HookResult, plugin_name: &str) {
        // Check for split view request
        if let Some(ref split_request) = result.split_view {
            if split_request.is_valid() {
                let session_id = self.widget_manager.create_split_view_session(plugin_name);
                self.sender.send(Message::SplitViewShow {
                    session_id,
                    plugin_name: plugin_name.to_string(),
                    request: split_request.clone(),
                });
            }
        }

        // Check for tree view request
        if let Some(ref tree_request) = result.tree_view {
            if tree_request.is_valid() {
                let session_id = self.widget_manager.create_tree_view_session(plugin_name);
                self.sender.send(Message::TreeViewShow {
                    session_id,
                    plugin_name: plugin_name.to_string(),
                    request: tree_request.clone(),
                });
            }
        }
    }
}
