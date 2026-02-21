use fltk::{
    app::Sender,
    enums::Font,
    frame::Frame,
    group::Flex,
    prelude::*,
    text::{StyleTableEntry, TextEditor},
    window::Window,
};

use super::document::DocumentId;
use super::messages::Message;
use super::buffer_utils::buffer_text_no_leak;
use super::settings::SyntaxTheme;
use super::syntax::SyntaxHighlighter;
use super::tab_manager::TabManager;

const LARGE_FILE_THRESHOLD: usize = 5000;

/// Borrowed UI widgets needed by highlight operations.
pub struct HighlightWidgets<'a> {
    pub editor: &'a mut TextEditor,
    pub banner_frame: &'a mut Frame,
    pub flex: &'a mut Flex,
    pub window: &'a mut Window,
}

pub struct HighlightController {
    highlighter: SyntaxHighlighter,
    pub pending_rehighlight: Option<(DocumentId, i32)>,
    pub rehighlight_timer_active: bool,
    pub highlight_queue: Vec<DocumentId>,
    pub highlighting_enabled: bool,
}

impl HighlightController {
    pub fn new(theme: SyntaxTheme, font: Font, font_size: i32, highlighting_enabled: bool) -> Self {
        Self {
            highlighter: SyntaxHighlighter::new(theme, font, font_size),
            pending_rehighlight: None,
            rehighlight_timer_active: false,
            highlight_queue: Vec::new(),
            highlighting_enabled,
        }
    }

    // --- Thin delegators wrapping SyntaxHighlighter ---

    /// Get a reference to the underlying highlighter for theme color access.
    pub fn highlighter(&self) -> &SyntaxHighlighter {
        &self.highlighter
    }

    pub fn style_table(&self) -> Vec<StyleTableEntry> {
        self.highlighter.style_table()
    }

    pub fn set_theme(&mut self, theme: SyntaxTheme) {
        self.highlighter.set_theme(theme);
    }

    pub fn set_font(&mut self, font: Font, size: i32) {
        self.highlighter.set_font(font, size);
    }

    // --- Highlight methods ---

    fn hide_highlight_banner(&self, widgets: &mut HighlightWidgets) {
        let label = widgets.banner_frame.label();
        if label.contains("Highlighting") {
            widgets.banner_frame.hide();
            widgets.flex.fixed(widgets.banner_frame, 0);
            widgets.window.redraw();
        }
    }

    fn start_chunked_highlight(
        &mut self,
        id: DocumentId,
        text: String,
        syntax_name: &str,
        sender: &Sender<Message>,
        widgets: &mut HighlightWidgets,
    ) {
        widgets
            .banner_frame
            .set_label("  Highlighting large file...");
        widgets.banner_frame.show();
        widgets.flex.fixed(widgets.banner_frame, 30);
        widgets.window.redraw();

        self.highlighter.start_chunked(id, text, syntax_name);

        let s = *sender;
        fltk::app::add_timeout3(0.0, move |_| {
            s.send(Message::ContinueHighlight);
        });
    }

    pub fn start_queued_highlights(
        &mut self,
        sender: &Sender<Message>,
        widgets: &mut HighlightWidgets,
    ) {
        if self.highlight_queue.is_empty() {
            return;
        }
        widgets
            .banner_frame
            .set_label("  Highlighting large file...");
        widgets.banner_frame.show();
        widgets.flex.fixed(widgets.banner_frame, 30);
        widgets.window.redraw();

        let s = *sender;
        fltk::app::add_timeout3(0.0, move |_| {
            s.send(Message::ContinueHighlight);
        });
    }

    pub fn detect_and_highlight(
        &mut self,
        id: DocumentId,
        path: &str,
        tab_manager: &mut TabManager,
        sender: &Sender<Message>,
    ) {
        if !self.highlighting_enabled {
            let syntax_name = self.highlighter.detect_syntax(path);
            if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                doc.syntax_name = syntax_name;
            }
            return;
        }

        let syntax_name = self.highlighter.detect_syntax(path);
        if let Some(ref name) = syntax_name {
            if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                doc.syntax_name = syntax_name.clone();
            }

            let (text, line_count) = {
                if let Some(doc) = tab_manager.doc_by_id(id) {
                    let text = buffer_text_no_leak(&doc.buffer);
                    let line_count = text.lines().count();
                    (text, line_count)
                } else {
                    return;
                }
            };

            if line_count <= LARGE_FILE_THRESHOLD {
                let result = self.highlighter.highlight_full(&text, name);
                if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                    doc.style_buffer.set_text(&result.style_string);
                    doc.checkpoints = result.checkpoints;
                }
            } else {
                let was_empty = self.highlight_queue.is_empty()
                    && self.highlighter.chunked_doc_id().is_none();
                self.highlight_queue.push(id);
                if was_empty {
                    let s = *sender;
                    fltk::app::add_timeout3(0.0, move |_| {
                        s.send(Message::ContinueHighlight);
                    });
                }
            }
        } else if let Some(doc) = tab_manager.doc_by_id_mut(id) {
            doc.syntax_name = None;
            doc.checkpoints.clear();
        }
    }

    fn rehighlight_document(&mut self, id: DocumentId, pos: i32, tab_manager: &mut TabManager, sender: &Sender<Message>, widgets: &mut HighlightWidgets) {
        let (syntax_name, text, edit_line, checkpoints_empty) = {
            if let Some(doc) = tab_manager.doc_by_id(id) {
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

        if checkpoints_empty {
            let line_count = text.lines().count();
            if line_count > LARGE_FILE_THRESHOLD {
                if !self.highlight_queue.contains(&id) {
                    let was_empty = self.highlight_queue.is_empty()
                        && self.highlighter.chunked_doc_id().is_none();
                    self.highlight_queue.push(id);
                    if was_empty {
                        let s = *sender;
                        fltk::app::add_timeout3(0.0, move |_| {
                            s.send(Message::ContinueHighlight);
                        });
                    }
                }
                return;
            }
        }

        let mut checkpoints = {
            if let Some(doc) = tab_manager.doc_by_id_mut(id) {
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

        if let Some(doc) = tab_manager.doc_by_id_mut(id) {
            let start = result.byte_start as i32;
            let end = start + result.style_chars.len() as i32;
            doc.style_buffer.replace(start, end, &result.style_chars);
            doc.checkpoints = checkpoints;
        }

        if tab_manager.active_id() == Some(id) {
            if self.highlighter.style_table_changed() {
                if let Some(doc) = tab_manager.doc_by_id(id) {
                    let style_buf = doc.style_buffer.clone();
                    let table = self.highlighter.style_table();
                    widgets.editor.set_highlight_data(style_buf, table);
                }
                self.highlighter.reset_style_table_changed();
            }
            widgets.editor.redraw();
        }
    }

    pub fn schedule_rehighlight(
        &mut self,
        id: DocumentId,
        pos: i32,
        tab_manager: &mut TabManager,
        sender: &Sender<Message>,
        widgets: &mut HighlightWidgets,
    ) {
        if !self.highlighting_enabled {
            return;
        }
        if self.highlight_queue.contains(&id) {
            return;
        }

        if self.highlighter.chunked_doc_id() == Some(id) {
            if let (Some(cp), Some(doc)) = (self.highlighter.cancel_chunked(), tab_manager.doc_by_id_mut(id)) {
                doc.checkpoints = cp;
            }
            self.hide_highlight_banner(widgets);
        }

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
            let s = *sender;
            fltk::app::add_timeout3(0.05, move |_| {
                s.send(Message::DoRehighlight);
            });
        }
    }

    pub fn do_pending_rehighlight(
        &mut self,
        tab_manager: &mut TabManager,
        sender: &Sender<Message>,
        widgets: &mut HighlightWidgets,
    ) {
        self.rehighlight_timer_active = false;
        if let Some((id, pos)) = self.pending_rehighlight.take() {
            self.rehighlight_document(id, pos, tab_manager, sender, widgets);
        }
    }

    pub fn continue_chunked_highlight(
        &mut self,
        tab_manager: &mut TabManager,
        sender: &Sender<Message>,
        widgets: &mut HighlightWidgets,
    ) {
        if self.highlighter.chunked_doc_id().is_none() {
            self.start_next_queued_highlight(tab_manager, sender, widgets);
            return;
        }

        let doc_id = self.highlighter.chunked_doc_id().unwrap();

        if tab_manager.doc_by_id(doc_id).is_none() {
            self.highlighter.cancel_chunked();
            self.start_next_queued_highlight(tab_manager, sender, widgets);
            return;
        }

        if let Some(output) = self.highlighter.process_chunk() {
            let is_active = tab_manager.active_id() == Some(doc_id);

            if let Some(doc) = tab_manager.doc_by_id_mut(doc_id) {
                let start = output.byte_start as i32;
                let end = start + output.style_chars.len() as i32;
                doc.style_buffer.replace(start, end, &output.style_chars);
            }

            if is_active {
                if let Some(doc) = tab_manager.doc_by_id(doc_id) {
                    let style_buf = doc.style_buffer.clone();
                    let table = self.highlighter.style_table();
                    widgets.editor.set_highlight_data(style_buf, table);
                }
                widgets.editor.redraw();
            }

            if output.done {
                if let (Some(doc), Some(cp)) = (tab_manager.doc_by_id_mut(doc_id), output.final_checkpoints) {
                    doc.checkpoints = cp;
                }
                self.start_next_queued_highlight(tab_manager, sender, widgets);
            } else {
                let s = *sender;
                fltk::app::add_timeout3(0.0, move |_| {
                    s.send(Message::ContinueHighlight);
                });
            }
        }
    }

    fn start_next_queued_highlight(
        &mut self,
        tab_manager: &mut TabManager,
        sender: &Sender<Message>,
        widgets: &mut HighlightWidgets,
    ) {
        while let Some(id) = self.highlight_queue.first().copied() {
            self.highlight_queue.remove(0);

            let (syntax_name, text, line_count) = {
                if let Some(doc) = tab_manager.doc_by_id(id) {
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
                let result = self.highlighter.highlight_full(&text, &syntax_name);
                if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                    doc.style_buffer.set_text(&result.style_string);
                    doc.checkpoints = result.checkpoints;
                }
                if tab_manager.active_id() == Some(id) {
                    if let Some(doc) = tab_manager.doc_by_id(id) {
                        let style_buf = doc.style_buffer.clone();
                        let table = self.highlighter.style_table();
                        widgets.editor.set_highlight_data(style_buf, table);
                    }
                    widgets.editor.redraw();
                }
                continue;
            }

            self.start_chunked_highlight(id, text, &syntax_name, sender, widgets);
            return;
        }

        self.hide_highlight_banner(widgets);
    }

    pub fn rehighlight_all_documents(
        &mut self,
        tab_manager: &mut TabManager,
        sender: &Sender<Message>,
    ) {
        let doc_ids: Vec<DocumentId> =
            tab_manager.documents().iter().map(|d| d.id).collect();
        for id in doc_ids {
            let (syntax_name, text) = {
                if let Some(doc) = tab_manager.doc_by_id(id) {
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
                if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                    doc.style_buffer.set_text(&result.style_string);
                    doc.checkpoints = result.checkpoints;
                }
            } else {
                let was_empty = self.highlight_queue.is_empty()
                    && self.highlighter.chunked_doc_id().is_none();
                self.highlight_queue.push(id);
                if was_empty {
                    let s = *sender;
                    fltk::app::add_timeout3(0.0, move |_| {
                        s.send(Message::ContinueHighlight);
                    });
                }
            }
        }
    }

    /// Disable highlighting: cancel chunked, clear queues, reset all style buffers.
    pub fn disable_highlighting(
        &mut self,
        tab_manager: &mut TabManager,
        widgets: &mut HighlightWidgets,
    ) {
        self.highlighter.cancel_chunked();
        self.highlight_queue.clear();
        self.hide_highlight_banner(widgets);
        self.pending_rehighlight = None;

        let doc_ids: Vec<DocumentId> =
            tab_manager.documents().iter().map(|d| d.id).collect();
        for id in doc_ids {
            if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                doc.checkpoints.clear();
                let len = doc.buffer.length() as usize;
                let plain = "A".repeat(len);
                doc.style_buffer.set_text(&plain);
            }
        }
    }
}
