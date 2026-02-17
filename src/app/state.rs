use std::cell::RefCell;
use std::rc::Rc;

/// Read text from an FLTK TextBuffer without leaking the C-allocated copy.
/// fltk-rs's `TextBuffer::text()` leaks the C `char*` returned by
/// `Fl_Text_Buffer_text`. This helper calls the FFI directly and frees it.
pub fn buffer_text_no_leak(buf: &fltk::text::TextBuffer) -> String {
    unsafe extern "C" {
        fn Fl_Text_Buffer_text(buf: *mut std::ffi::c_void) -> *mut std::ffi::c_char;
        fn free(ptr: *mut std::ffi::c_void);
    }

    unsafe {
        let inner = buf.as_ptr() as *mut std::ffi::c_void;
        let ptr = Fl_Text_Buffer_text(inner);
        if ptr.is_null() {
            return String::new();
        }
        let cstr = std::ffi::CStr::from_ptr(ptr);
        let result = cstr.to_string_lossy().into_owned();
        free(ptr as *mut std::ffi::c_void);
        result
    }
}

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
use super::session::{self, SessionRestore};
use super::syntax::SyntaxHighlighter;
use super::tab_manager::TabManager;
use super::platform::detect_system_dark_mode;
use super::messages::Message;
use super::settings::{AppSettings, FontChoice, ThemeMode};
use super::updater::ReleaseInfo;
use crate::ui::dialogs::settings_dialog::show_settings_dialog;
use crate::ui::file_dialogs::{native_open_dialog, native_open_multi_dialog, native_save_dialog};
use crate::ui::tab_bar::TabBar;
use crate::ui::theme::apply_theme;
#[cfg(target_os = "windows")]
use crate::ui::theme::set_windows_titlebar_theme;

pub struct AppState {
    pub tab_manager: TabManager,
    pub tabs_enabled: bool,
    pub tab_bar: Option<TabBar>,
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
    pub pending_update: Option<ReleaseInfo>,
    pub highlighter: SyntaxHighlighter,
    /// Pending rehighlight: (doc_id, earliest_edit_position)
    pub pending_rehighlight: Option<(DocumentId, i32)>,
    pub rehighlight_timer_active: bool,
    /// Queue of documents awaiting deferred chunked highlighting (session restore).
    pub highlight_queue: Vec<DocumentId>,
    /// Whether syntax highlighting is active (toggle via View menu).
    pub highlighting_enabled: bool,
}

impl AppState {
    pub fn new(
        editor: TextEditor,
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
        let mut tab_manager = TabManager::new(sender.clone());
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
        let highlighter = SyntaxHighlighter::new(dark_mode, font, font_size);

        Self {
            tab_manager,
            tabs_enabled,
            tab_bar,
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
            pending_update: None,
            highlighter,
            pending_rehighlight: None,
            rehighlight_timer_active: false,
            highlight_queue: Vec::new(),
            highlighting_enabled,
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
            let table = self.highlighter.style_table();
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
            let table = self.highlighter.style_table();
            self.editor.set_highlight_data(style_buf, table);
            self.editor.set_insert_position(cursor);
            self.editor.show_insert_position();
        }

        self.update_linenumber_width();
        self.update_window_title();
    }

    /// Rebuild the tab bar UI from current documents
    pub fn rebuild_tab_bar(&mut self) {
        if let Some(ref mut tab_bar) = self.tab_bar {
            let active_id = self.tab_manager.active_id();
            tab_bar.rebuild(
                self.tab_manager.documents(),
                active_id,
                &self.sender,
                self.dark_mode,
            );
        }
    }

    /// Close a tab by id. Returns true if the app should exit (no tabs remaining).
    pub fn close_tab(&mut self, id: DocumentId) -> bool {
        // Check if document is dirty
        if let Some(doc) = self.tab_manager.doc_by_id(id) {
            if doc.is_dirty() {
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
                        if let Some(doc) = self.tab_manager.doc_by_id(id) {
                            if doc.is_dirty() {
                                if let Some(prev) = was_active {
                                    if prev != id {
                                        self.switch_to_document(prev);
                                    }
                                }
                                return false;
                            }
                        }
                    }
                    Some(1) => {}
                    _ => return false,
                }
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
        false
    }

    // --- File operations ---

    pub fn open_file(&mut self, path: String) {
        match fs::read_to_string(&path) {
            Ok(content) => {
                if self.tabs_enabled {
                    if let Some(existing_id) = self.tab_manager.find_by_path(&path) {
                        self.switch_to_document(existing_id);
                        self.rebuild_tab_bar();
                        return;
                    }
                    let id = self.tab_manager.add_from_file(path.clone(), &content);
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
        }
    }

    pub fn file_open(&mut self) {
        if self.tabs_enabled {
            let paths = native_open_multi_dialog();
            for path in paths {
                self.open_file(path);
            }
        } else if let Some(path) = native_open_dialog() {
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

        if let Some(path) = native_save_dialog() {
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
                            let table = self.highlighter.style_table();
                            self.editor.set_highlight_data(style_buf, table);
                        }
                    }
                    self.update_window_title();
                    self.rebuild_tab_bar();
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

        if let Some(id) = self.tab_manager.active_id() {
            self.tab_manager.remove(id);
        }

        let mut first_id = None;
        let target_index = session_data.active_index;

        for (i, doc_session) in session_data.documents.iter().enumerate() {
            if let Some(ref path) = doc_session.file_path {
                if let Ok(content) = fs::read_to_string(path) {
                    let id = self.tab_manager.add_from_file(path.clone(), &content);
                    if first_id.is_none() {
                        first_id = Some(id);
                    }

                    self.detect_and_highlight(id, path);

                    if mode == SessionRestore::Full {
                        if let Some(ref temp_file) = doc_session.temp_file {
                            if let Some(temp_content) = session::read_temp_file(temp_file) {
                                if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                                    doc.buffer.set_text(&temp_content);
                                }
                            }
                        }
                    }

                    if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                        doc.cursor_position = doc_session.cursor_position;
                    }

                    if i == target_index {
                        self.tab_manager.set_active(id);
                    }
                }
            } else if mode == SessionRestore::Full {
                if let Some(ref temp_file) = doc_session.temp_file {
                    if let Some(temp_content) = session::read_temp_file(temp_file) {
                        let id = self.tab_manager.add_untitled();
                        if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                            doc.buffer.set_text(&temp_content);
                            doc.cursor_position = doc_session.cursor_position;
                        }
                        if first_id.is_none() {
                            first_id = Some(id);
                        }
                        if i == target_index {
                            self.tab_manager.set_active(id);
                        }
                    }
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
                            if let Some(doc) = self.tab_manager.doc_by_id(id) {
                                if doc.is_dirty() {
                                    return false;
                                }
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
                .map_or(false, |d| d.is_dirty());

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
                            .map_or(false, |d| d.is_dirty())
                    }
                    Some(1) => true,
                    _ => false,
                }
            } else {
                true
            }
        };

        if should_quit {
            if let Err(e) = session::save_session(&self.tab_manager, session_mode) {
                eprintln!("Failed to save session: {}", e);
            }
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

        self.highlighter.set_dark_mode(self.dark_mode);
        self.rehighlight_all_documents();
        self.bind_active_buffer();
    }

    pub fn toggle_highlighting(&mut self) {
        self.highlighting_enabled = !self.highlighting_enabled;
        {
            let mut s = self.settings.borrow_mut();
            s.highlighting_enabled = self.highlighting_enabled;
            let _ = s.save();
        }
        if self.highlighting_enabled {
            self.rehighlight_all_documents();
            self.bind_active_buffer();
        } else {
            // Cancel any in-progress chunked highlight
            self.highlighter.cancel_chunked();
            self.highlight_queue.clear();
            self.hide_highlight_banner();
            self.pending_rehighlight = None;

            // Clear all checkpoints and reset style buffers to plain
            let doc_ids: Vec<DocumentId> = self.tab_manager.documents().iter().map(|d| d.id).collect();
            for id in doc_ids {
                if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                    doc.checkpoints.clear();
                    let len = doc.buffer.length() as usize;
                    let plain: String = std::iter::repeat('A').take(len).collect();
                    doc.style_buffer.set_text(&plain);
                }
            }
            self.bind_active_buffer();
            self.editor.redraw();
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
        if let Some(new_settings) = show_settings_dialog(&current) {
            if let Err(e) = new_settings.save() {
                dialog::alert_default(&format!("Failed to save settings: {}", e));
                return;
            }
            self.apply_settings(new_settings);
        }
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

        self.highlighter.set_dark_mode(is_dark);
        self.highlighter.set_font(font, new_settings.font_size as i32);

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

        let highlighting_changed = self.highlighting_enabled != new_settings.highlighting_enabled;
        self.highlighting_enabled = new_settings.highlighting_enabled;
        self.update_menu_checkbox("View/Toggle Syntax Highlighting", self.highlighting_enabled);

        self.editor.redraw();

        *self.settings.borrow_mut() = new_settings;

        if highlighting_changed && !self.highlighting_enabled {
            // Switched off via settings — clear everything
            self.highlighter.cancel_chunked();
            self.highlight_queue.clear();
            self.hide_highlight_banner();
            self.pending_rehighlight = None;
            let doc_ids: Vec<DocumentId> = self.tab_manager.documents().iter().map(|d| d.id).collect();
            for id in doc_ids {
                if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                    doc.checkpoints.clear();
                    let len = doc.buffer.length() as usize;
                    let plain: String = std::iter::repeat('A').take(len).collect();
                    doc.style_buffer.set_text(&plain);
                }
            }
            self.bind_active_buffer();
        } else if self.highlighting_enabled {
            self.rehighlight_all_documents();
            self.bind_active_buffer();
        }
    }

    fn update_menu_checkbox(&self, path: &str, checked: bool) {
        let idx = self.menu.find_index(path);
        if idx >= 0 {
            if let Some(mut item) = self.menu.at(idx) {
                if checked {
                    item.set();
                } else {
                    item.clear();
                }
            }
        }
    }

    // --- Syntax highlighting ---

    /// Schedule a debounced rehighlight for a document.
    pub fn schedule_rehighlight(&mut self, id: DocumentId, pos: i32) {
        if !self.highlighting_enabled {
            return;
        }
        // If this doc is waiting in the highlight queue, the queue will
        // handle it — skip the debounced rehighlight to avoid a redundant
        // synchronous full-highlight on a large file.
        if self.highlight_queue.contains(&id) {
            return;
        }

        // Cancel any active chunked highlight for this document
        if let Some(chunked_id) = self.highlighter.chunked_doc_id() {
            if chunked_id == id {
                if let Some(cp) = self.highlighter.cancel_chunked() {
                    // Save partial checkpoints so incremental can use them
                    if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                        doc.checkpoints = cp;
                    }
                }
                self.hide_highlight_banner();
            }
        }

        // Track the earliest edit position across buffered edits
        match self.pending_rehighlight {
            Some((existing_id, existing_pos)) if existing_id == id => {
                self.pending_rehighlight = Some((id, pos.min(existing_pos)));
            }
            _ => {
                self.pending_rehighlight = Some((id, pos));
            }
        }

        if !self.rehighlight_timer_active {
            self.rehighlight_timer_active = true;
            let s = self.sender.clone();
            fltk::app::add_timeout3(0.05, move |_| {
                s.send(Message::DoRehighlight);
            });
        }
    }

    /// Execute the pending rehighlight (called when debounce timer fires).
    pub fn do_pending_rehighlight(&mut self) {
        self.rehighlight_timer_active = false;
        if let Some((id, pos)) = self.pending_rehighlight.take() {
            self.rehighlight_document(id, pos);
        }
    }

    /// Detect syntax for a document by path and highlight it.
    /// Small files (<= 5000 lines) are highlighted synchronously.
    /// Large files are always queued for chunked highlighting.
    fn detect_and_highlight(&mut self, id: DocumentId, path: &str) {
        const LARGE_FILE_THRESHOLD: usize = 5000;

        if !self.highlighting_enabled {
            // Still detect syntax name so re-enabling works later
            let syntax_name = self.highlighter.detect_syntax(path);
            if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                doc.syntax_name = syntax_name;
            }
            return;
        }

        let syntax_name = self.highlighter.detect_syntax(path);
        if let Some(ref name) = syntax_name {
            // Set syntax name on doc before highlighting
            if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                doc.syntax_name = syntax_name.clone();
            }

            let (text, line_count) = {
                if let Some(doc) = self.tab_manager.doc_by_id(id) {
                    let text = buffer_text_no_leak(&doc.buffer);
                    let line_count = text.lines().count();
                    (text, line_count)
                } else {
                    return;
                }
            };

            if line_count <= LARGE_FILE_THRESHOLD {
                let result = self.highlighter.highlight_full(&text, name);
                if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                    doc.style_buffer.set_text(&result.style_string);
                    doc.checkpoints = result.checkpoints;
                }
            } else {
                let was_empty = self.highlight_queue.is_empty()
                    && self.highlighter.chunked_doc_id().is_none();
                self.highlight_queue.push(id);
                if was_empty {
                    let s = self.sender.clone();
                    fltk::app::add_timeout3(0.0, move |_| {
                        s.send(Message::ContinueHighlight);
                    });
                }
            }
        } else {
            if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                doc.syntax_name = None;
                doc.checkpoints.clear();
            }
        }
    }

    /// Begin chunked highlighting for a large file.
    fn start_chunked_highlight(&mut self, id: DocumentId, text: String, syntax_name: &str) {
        // Show banner
        self.update_banner_frame.set_label("  Highlighting large file...");
        self.update_banner_frame.show();
        self.flex.fixed(&self.update_banner_frame, 30);
        self.window.redraw();

        self.highlighter.start_chunked(id, text, syntax_name);

        // Schedule first chunk via event loop yield
        let s = self.sender.clone();
        fltk::app::add_timeout3(0.0, move |_| {
            s.send(Message::ContinueHighlight);
        });
    }

    /// Process the next chunk of the active chunked highlight,
    /// or start the next queued document if no chunk is active.
    pub fn continue_chunked_highlight(&mut self) {
        // If no chunked op is active, try to start the next queued doc
        if self.highlighter.chunked_doc_id().is_none() {
            self.start_next_queued_highlight();
            return;
        }

        let doc_id = self.highlighter.chunked_doc_id().unwrap();

        // Check the document still exists
        if self.tab_manager.doc_by_id(doc_id).is_none() {
            self.highlighter.cancel_chunked();
            self.start_next_queued_highlight();
            return;
        }

        if let Some(output) = self.highlighter.process_chunk() {
            let is_active = self.tab_manager.active_id() == Some(doc_id);

            // Apply the chunk's style chars to the document's style buffer
            if let Some(doc) = self.tab_manager.doc_by_id_mut(doc_id) {
                let start = output.byte_start as i32;
                let end = start + output.style_chars.len() as i32;
                doc.style_buffer.replace(start, end, &output.style_chars);
            }

            // Refresh the editor's highlight data so new style table entries
            // are picked up and the visible portion redraws with colors.
            if is_active {
                if let Some(doc) = self.tab_manager.doc_by_id(doc_id) {
                    let style_buf = doc.style_buffer.clone();
                    let table = self.highlighter.style_table();
                    self.editor.set_highlight_data(style_buf, table);
                }
                self.editor.redraw();
            }

            if output.done {
                // Save final checkpoints
                if let Some(doc) = self.tab_manager.doc_by_id_mut(doc_id) {
                    if let Some(cp) = output.final_checkpoints {
                        doc.checkpoints = cp;
                    }
                }
                // Start next queued doc (or hide banner if queue is empty)
                self.start_next_queued_highlight();
            } else {
                // Schedule next chunk
                let s = self.sender.clone();
                fltk::app::add_timeout3(0.0, move |_| {
                    s.send(Message::ContinueHighlight);
                });
            }
        }
    }

    /// Pop the next document from the highlight queue and start highlighting it.
    /// Small files are highlighted synchronously; large files start chunked.
    /// Hides the banner when the queue is empty.
    fn start_next_queued_highlight(&mut self) {
        const LARGE_FILE_THRESHOLD: usize = 5000;

        while let Some(id) = self.highlight_queue.first().copied() {
            self.highlight_queue.remove(0);

            let (syntax_name, text, line_count) = {
                if let Some(doc) = self.tab_manager.doc_by_id(id) {
                    match doc.syntax_name {
                        Some(ref name) => {
                            let text = buffer_text_no_leak(&doc.buffer);
                            let line_count = text.lines().count();
                            (name.clone(), text, line_count)
                        }
                        None => continue,
                    }
                } else {
                    continue;
                }
            };

            if line_count <= LARGE_FILE_THRESHOLD {
                // Small file: highlight synchronously and continue to next
                let result = self.highlighter.highlight_full(&text, &syntax_name);
                if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                    doc.style_buffer.set_text(&result.style_string);
                    doc.checkpoints = result.checkpoints;
                }
                // Refresh editor if this is the active doc
                if self.tab_manager.active_id() == Some(id) {
                    if let Some(doc) = self.tab_manager.doc_by_id(id) {
                        let style_buf = doc.style_buffer.clone();
                        let table = self.highlighter.style_table();
                        self.editor.set_highlight_data(style_buf, table);
                    }
                    self.editor.redraw();
                }
                continue;
            }

            // Large file: start chunked, show banner, schedule message
            self.start_chunked_highlight(id, text, &syntax_name);
            return;
        }

        // Queue is empty, no more work
        self.hide_highlight_banner();

    }

    /// Kick off deferred highlighting for queued documents.
    /// Call after the window is shown so chunked yields are visible.
    pub fn start_queued_highlights(&mut self) {
        if self.highlight_queue.is_empty() {
            return;
        }
        self.update_banner_frame.set_label("  Highlighting large file...");
        self.update_banner_frame.show();
        self.flex.fixed(&self.update_banner_frame, 30);
        self.window.redraw();

        let s = self.sender.clone();
        fltk::app::add_timeout3(0.0, move |_| {
            s.send(Message::ContinueHighlight);
        });
    }

    fn hide_highlight_banner(&mut self) {
        // Only hide if it's showing the highlight message (not an update banner)
        let label = self.update_banner_frame.label();
        if label.contains("Highlighting") {
            self.update_banner_frame.hide();
            self.flex.fixed(&self.update_banner_frame, 0);
            self.window.redraw();
        }
    }

    /// Incremental re-highlight a single document from an edit position.
    fn rehighlight_document(&mut self, id: DocumentId, pos: i32) {
        let (syntax_name, text, edit_line, checkpoints_empty) = {
            if let Some(doc) = self.tab_manager.doc_by_id(id) {
                match doc.syntax_name {
                    Some(ref name) => {
                        let text = buffer_text_no_leak(&doc.buffer);
                        let line = doc.buffer.count_lines(0, pos) as usize;
                        (name.clone(), text, line, doc.checkpoints.len() == 0)
                    }
                    None => return,
                }
            } else {
                return;
            }
        };

        // If no checkpoints exist yet (chunked highlight hasn't finished or
        // hasn't started), incremental would parse the entire file from scratch.
        // Instead, queue it for chunked highlighting which spreads the work.
        if checkpoints_empty {
            let line_count = text.lines().count();
            if line_count > 5000 {
                if !self.highlight_queue.contains(&id) {
                    let was_empty = self.highlight_queue.is_empty()
                        && self.highlighter.chunked_doc_id().is_none();
                    self.highlight_queue.push(id);
                    if was_empty {
                        let s = self.sender.clone();
                        fltk::app::add_timeout3(0.0, move |_| {
                            s.send(Message::ContinueHighlight);
                        });
                    }
                }
                return;
            }
        }

        // Take checkpoints out of the document to satisfy the borrow checker
        // (we need &mut self.highlighter and &mut doc.checkpoints simultaneously)
        let mut checkpoints = {
            if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                std::mem::take(&mut doc.checkpoints)
            } else {
                return;
            }
        };

        let result = self.highlighter.highlight_incremental(
            &text,
            edit_line,
            &mut checkpoints,
            &syntax_name,
        );

        if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
            // Only replace the changed portion of the style buffer
            let start = result.byte_start as i32;
            let end = start + result.style_chars.len() as i32;
            doc.style_buffer.replace(start, end, &result.style_chars);
            // Put checkpoints back (modified in place by highlight_incremental)
            doc.checkpoints = checkpoints;
        }

        // Only rebind highlight data if the style table grew (new colors discovered).
        // Otherwise the editor already has a pointer to doc.style_buffer and sees
        // changes from replace() automatically — just redraw.
        if self.tab_manager.active_id() == Some(id) {
            if self.highlighter.style_table_changed() {
                if let Some(doc) = self.tab_manager.doc_by_id(id) {
                    let style_buf = doc.style_buffer.clone();
                    let table = self.highlighter.style_table();
                    self.editor.set_highlight_data(style_buf, table);
                }
                self.highlighter.reset_style_table_changed();
            }
            self.editor.redraw();
        }
    }

    /// Re-highlight all open documents (called on theme/font change).
    pub fn rehighlight_all_documents(&mut self) {
        const LARGE_FILE_THRESHOLD: usize = 5000;

        let doc_ids: Vec<DocumentId> = self.tab_manager.documents().iter().map(|d| d.id).collect();
        for id in doc_ids {
            let (syntax_name, text) = {
                if let Some(doc) = self.tab_manager.doc_by_id(id) {
                    match doc.syntax_name {
                        Some(ref name) => (name.clone(), buffer_text_no_leak(&doc.buffer)),
                        None => continue,
                    }
                } else {
                    continue;
                }
            };

            let line_count = text.lines().count();
            if line_count <= LARGE_FILE_THRESHOLD {
                let result = self.highlighter.highlight_full(&text, &syntax_name);
                if let Some(doc) = self.tab_manager.doc_by_id_mut(id) {
                    doc.style_buffer.set_text(&result.style_string);
                    doc.checkpoints = result.checkpoints;
                }
            } else {
                let was_empty = self.highlight_queue.is_empty()
                    && self.highlighter.chunked_doc_id().is_none();
                self.highlight_queue.push(id);
                if was_empty {
                    let s = self.sender.clone();
                    fltk::app::add_timeout3(0.0, move |_| {
                        s.send(Message::ContinueHighlight);
                    });
                }
            }
        }
    }

    // --- Update banner ---

    pub fn show_update_banner(&mut self, version: &str) {
        self.update_banner_frame.set_label(&format!(
            "  \u{1f980} FerrisPad {} is available - Click to view details or press ESC to dismiss",
            version
        ));
        self.update_banner_frame.show();
        self.flex.fixed(&self.update_banner_frame, 30);
        self.window.redraw();
    }

    pub fn hide_update_banner(&mut self) {
        self.update_banner_frame.hide();
        self.flex.fixed(&self.update_banner_frame, 0);
        self.window.redraw();
    }
}
