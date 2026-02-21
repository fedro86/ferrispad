use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use super::buffer_utils::buffer_text_no_leak;

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

use super::document::DocumentId;
use super::highlight_controller::{HighlightController, HighlightWidgets};
use super::preview_controller::{self, PreviewController, wrap_html_for_helpview, extract_resize_tasks, ImageResizeProgress};
use super::session::{self, SessionRestore};
use super::tab_manager::{GroupColor, GroupId, TabManager};
use super::platform::detect_system_dark_mode;
use super::messages::Message;
use super::settings::{AppSettings, FontChoice, SyntaxTheme, ThemeMode};
use super::update_controller::UpdateController;
use crate::ui::dialogs::settings_dialog::show_settings_dialog;
use crate::ui::editor_container::EditorContainer;
use crate::ui::file_dialogs::{native_open_dialog, native_open_multi_dialog, native_save_dialog};
use crate::ui::tab_bar::TabBar;
use crate::ui::theme::{apply_theme, apply_syntax_theme_colors};
#[cfg(target_os = "windows")]
use crate::ui::theme::set_windows_titlebar_theme;

pub struct AppState {
    pub tab_manager: TabManager,
    pub tabs_enabled: bool,
    pub tab_bar: Option<TabBar>,
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
    /// Last directory used in a file open/save dialog.
    pub last_open_directory: Option<String>,
    /// Tracks when the session was last auto-saved.
    last_auto_save: Instant,
    /// Whether something changed since the last auto-save.
    session_dirty: bool,
}

impl AppState {
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

        let preview_enabled = settings.borrow().preview_enabled;
        let preview = PreviewController::new(preview_enabled);

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
            last_open_directory: None,
            last_auto_save: Instant::now(),
            session_dirty: false,
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
            self.editor.set_buffer(doc.buffer.clone());
            let style_buf = doc.style_buffer.clone();
            let table = self.highlight.style_table();
            self.editor.set_highlight_data(style_buf, table);
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
        if let Some(doc) = self.tab_manager.active_doc() {
            let buffer = doc.buffer.clone();
            let cursor = doc.cursor_position;
            let style_buf = doc.style_buffer.clone();
            self.editor.set_buffer(buffer);
            let table = self.highlight.style_table();
            self.editor.set_highlight_data(style_buf, table);
            self.editor.set_insert_position(cursor);
            self.editor.show_insert_position();
        }

        self.update_linenumber_width();
        self.update_window_title();
        self.update_preview();
    }

    /// Rebuild the tab bar UI from current documents
    pub fn rebuild_tab_bar(&mut self) {
        if let Some(ref mut tab_bar) = self.tab_bar {
            let active_id = self.tab_manager.active_id();
            tab_bar.rebuild(
                self.tab_manager.documents(),
                self.tab_manager.groups(),
                active_id,
                &self.sender,
                self.dark_mode,
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
        #[cfg(debug_assertions)]
        eprintln!("[debug] parent_flex children after close_tab: {}", self.editor_container.parent_flex_children());
        false
    }

    // --- File operations ---

    pub fn open_file(&mut self, path: String) {
        // Remember the parent directory for future open/save dialogs
        if let Some(parent) = std::path::Path::new(&path).parent() {
            self.last_open_directory = Some(parent.to_string_lossy().to_string());
        }
        match fs::read_to_string(&path) {
            Ok(content) => {
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
                    self.update_preview();
                }
            }
            Err(e) => dialog::alert_default(&format!("Error opening file: {}", e)),
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
            self.update_preview();
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
        let (file_path, text) = {
            if let Some(doc) = self.tab_manager.active_doc() {
                (doc.file_path.clone(), buffer_text_no_leak(&doc.buffer))
            } else {
                return;
            }
        };

        if let Some(ref path) = file_path {
            match fs::write(path, &text) {
                Ok(_) => {
                    if let Some(doc) = self.tab_manager.active_doc_mut() {
                        doc.mark_clean();
                    }
                    self.update_window_title();
                    self.rebuild_tab_bar();
                    self.update_preview();
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
                            self.editor.set_highlight_data(style_buf, table);
                        }
                    }
                    self.update_window_title();
                    self.rebuild_tab_bar();
                    self.update_preview();
                }
                Err(e) => dialog::alert_default(&format!("Error saving file: {}", e)),
            }
        }
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
        use super::tab_manager::TabGroup as TG;
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
            preview_controller::cleanup_temp_images();
            if let Err(e) = session::save_session(&self.tab_manager, session_mode, self.last_open_directory.as_deref()) {
                eprintln!("Failed to save session: {}", e);
            }
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
        apply_theme(
            &mut self.editor,
            &mut self.window,
            &mut self.menu,
            Some(&mut self.update_banner_frame),
            self.dark_mode,
        );
        if let Some(ref mut tab_bar) = self.tab_bar {
            tab_bar.apply_theme(self.dark_mode);
        }
        #[cfg(target_os = "windows")]
        set_windows_titlebar_theme(&self.window, self.dark_mode);

        let theme = self.settings.borrow().current_syntax_theme(self.dark_mode);
        self.highlight.set_theme(theme);

        // Apply syntax theme colors to editor
        let bg = self.highlight.highlighter().theme_background();
        let fg = self.highlight.highlighter().theme_foreground();
        apply_syntax_theme_colors(&mut self.editor, bg, fg);

        self.highlight.rehighlight_all_documents(&mut self.tab_manager, &self.sender);
        self.bind_active_buffer();
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

    pub fn toggle_preview(&mut self) {
        let enabled = self.preview.toggle();

        // Persist the setting
        {
            let mut s = self.settings.borrow_mut();
            s.preview_enabled = enabled;
            let _ = s.save();
        }
        self.update_menu_checkbox("View/Toggle Markdown Preview", enabled);

        if enabled {
            self.update_preview();
        } else {
            self.preview.chunked_resize = None;
            PreviewController::cleanup_preview_file();
            // hide_preview() calls set_value("") which releases HelpView refs,
            // then we release the Fl_Shared_Image cache entries.
            self.editor_container.hide_preview();
            self.preview.release_cached_images();
            // SAFETY: malloc_trim releases freed glibc memory back to the OS.
            // Safe to call at any time; helps reclaim memory after clearing preview.
            #[cfg(target_os = "linux")]
            unsafe {
                unsafe extern "C" { fn malloc_trim(pad: std::ffi::c_int) -> std::ffi::c_int; }
                malloc_trim(0);
            }
        }
    }

    pub fn update_preview(&mut self) {
        // Cancel any in-progress image resize
        self.preview.chunked_resize = None;

        if !self.preview.enabled {
            if self.editor_container.is_split() {
                // hide_preview() calls set_value("") which releases HelpView refs,
                // then we release the Fl_Shared_Image cache entries.
                self.editor_container.hide_preview();
                self.preview.release_cached_images();
            }
            return;
        }

        let is_md = self.tab_manager.active_doc()
            .and_then(|doc| doc.file_path.as_deref())
            .map(|p| PreviewController::is_markdown_file(Some(p)))
            .unwrap_or(false);

        if !is_md {
            if self.editor_container.is_split() {
                self.editor_container.hide_preview();
                self.preview.release_cached_images();
            }
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

        let raw_html = PreviewController::render_markdown(&text);

        // Phase 1: extract resize tasks and show preview immediately (images at native size)
        if let Some(ref md_path) = file_path {
            let md_dir = std::path::Path::new(md_path)
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();

            let (phase1_html, tasks) = extract_resize_tasks(&raw_html, &md_dir);

            // Show phase-1 HTML immediately
            let wrapped = wrap_html_for_helpview(&phase1_html);
            if let Some(temp_path) = PreviewController::write_preview_file(&wrapped) {
                // Release old cached images before loading new content.
                // HelpView's load() replaces content, releasing old refs first.
                self.preview.release_cached_images();
                self.preview.track_images_in_html(&phase1_html);
                self.editor_container.show_preview();
                self.editor_container.load_preview_file(&temp_path);
            }

            // Start async image resize if there are tasks.
            // Pass the original raw HTML so rewrite_img_sources can match src attrs.
            if self.preview.start_image_resize(raw_html, tasks, md_dir) {
                // Show banner
                self.update_banner_frame.set_label("  Resizing images... (0/?)");
                self.update_banner_frame.show();
                self.flex.fixed(&self.update_banner_frame, 30);
                self.window.redraw();

                // Schedule first chunk
                let s = self.sender;
                fltk::app::add_timeout3(0.0, move |_| {
                    s.send(Message::ContinueImageResize);
                });
            }
        } else {
            // Fallback: no file path, just show raw HTML
            let wrapped = wrap_html_for_helpview(&raw_html);
            self.editor_container.show_preview();
            self.editor_container.load_preview_fallback(&wrapped);
        }
    }

    pub fn continue_image_resize(&mut self) {
        match self.preview.process_next_image() {
            Some(ImageResizeProgress::InProgress(done, total)) => {
                self.update_banner_frame.set_label(
                    &format!("  Resizing images... ({}/{})", done, total)
                );
                self.window.redraw();

                let s = self.sender;
                fltk::app::add_timeout3(0.0, move |_| {
                    s.send(Message::ContinueImageResize);
                });
            }
            Some(ImageResizeProgress::Done(final_html)) => {
                let wrapped = wrap_html_for_helpview(&final_html);
                if let Some(temp_path) = PreviewController::write_preview_file(&wrapped) {
                    // Release phase-1 cached images, track phase-3 images.
                    // load_preview_file replaces content, releasing old HelpView refs.
                    self.preview.release_cached_images();
                    self.preview.track_images_in_html(&final_html);
                    self.editor_container.load_preview_file(&temp_path);
                }

                // Hide banner
                self.update_banner_frame.hide();
                self.flex.fixed(&self.update_banner_frame, 0);
                self.window.redraw();
            }
            None => {
                // No resize in progress — orphan message, ignore
            }
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
        apply_theme(
            &mut self.editor,
            &mut self.window,
            &mut self.menu,
            Some(&mut self.update_banner_frame),
            is_dark,
        );
        if let Some(ref mut tab_bar) = self.tab_bar {
            tab_bar.apply_theme(is_dark);
        }
        #[cfg(target_os = "windows")]
        set_windows_titlebar_theme(&self.window, is_dark);
        self.update_menu_checkbox("View/Toggle Dark Mode", is_dark);

        let font = match new_settings.font {
            FontChoice::ScreenBold => Font::ScreenBold,
            FontChoice::Courier => Font::Courier,
            FontChoice::HelveticaMono => Font::Screen,
        };
        self.editor.set_text_font(font);
        self.editor.set_text_size(new_settings.font_size as i32);

        let syntax_theme = new_settings.current_syntax_theme(is_dark);
        self.highlight.set_theme(syntax_theme);
        self.highlight.set_font(font, new_settings.font_size as i32);

        // Apply syntax theme colors to editor (overrides apply_theme's default colors)
        let bg = self.highlight.highlighter().theme_background();
        let fg = self.highlight.highlighter().theme_foreground();
        apply_syntax_theme_colors(&mut self.editor, bg, fg);

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

        let highlighting_changed = self.highlight.highlighting_enabled != new_settings.highlighting_enabled;
        self.highlight.highlighting_enabled = new_settings.highlighting_enabled;
        self.update_menu_checkbox("View/Toggle Syntax Highlighting", self.highlight.highlighting_enabled);

        // Preview setting
        let preview_changed = self.preview.enabled != new_settings.preview_enabled;
        self.preview.enabled = new_settings.preview_enabled;
        self.update_menu_checkbox("View/Toggle Markdown Preview", self.preview.enabled);

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

        if preview_changed {
            self.update_preview();
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

}
