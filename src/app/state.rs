use std::cell::RefCell;
use std::rc::Rc;

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
use super::session::{self, SessionRestore};
use super::tab_manager::TabManager;
use super::platform::detect_system_dark_mode;
use super::messages::Message;
use super::settings::{AppSettings, FontChoice, ThemeMode};
use super::update_controller::UpdateController;
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
    pub update: UpdateController,
    pub highlight: HighlightController,
    /// Last directory used in a file open/save dialog.
    pub last_open_directory: Option<String>,
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
        let highlight = HighlightController::new(dark_mode, font, font_size, highlighting_enabled);

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
            update: UpdateController::new(),
            highlight,
            last_open_directory: None,
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
            if let Err(e) = session::save_session(&self.tab_manager, session_mode, self.last_open_directory.as_deref()) {
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

        self.highlight.set_dark_mode(self.dark_mode);
        self.highlight.rehighlight_all_documents(&mut self.tab_manager, &self.sender);
        self.bind_active_buffer();
    }

    pub fn toggle_highlighting(&mut self) {
        self.highlight.highlighting_enabled = !self.highlight.highlighting_enabled;
        {
            let mut s = self.settings.borrow_mut();
            s.highlighting_enabled = self.highlight.highlighting_enabled;
            let _ = s.save();
        }
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

        self.highlight.set_dark_mode(is_dark);
        self.highlight.set_font(font, new_settings.font_size as i32);

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
