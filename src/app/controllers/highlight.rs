use std::collections::{BTreeMap, BTreeSet};

use fltk::{
    app::Sender,
    enums::Font,
    frame::Frame,
    group::Flex,
    prelude::*,
    text::{StyleTableEntryExt, TextEditor},
    window::Window,
};

use super::tabs::TabManager;
use crate::app::domain::document::DocumentId;
use crate::app::domain::messages::Message;
use crate::app::domain::settings::{SyntaxTheme, ThemeRef, default_syntax_theme};
use crate::app::infrastructure::buffer::buffer_text_no_leak;
use crate::app::infrastructure::defer::defer_send;
use crate::app::services::custom_themes::CustomTheme;
#[cfg(test)]
use crate::app::services::syntax::LONG_LINE_BYTES;
use crate::app::services::syntax::{SyntaxHighlighter, is_long_line};
use crate::ui::theme::{EditorColors, derived_editor_colors, effective_editor_colors};

/// Documents with more lines than this are highlighted in chunks.
const LARGE_FILE_THRESHOLD: usize = 5000;
/// Documents that would hand syntect more bytes than this are highlighted in
/// chunks (T0087). Lines syntect skips ([`is_long_line`]) do not count.
pub const SYNC_HIGHLIGHT_MAX_BYTES: usize = 1024 * 1024;

/// How a document's whole-text highlight runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightPlan {
    /// At once, inside the current event-loop step.
    Synchronous,
    /// In chunks, one per `ContinueHighlight` message, yielding to the
    /// event loop in between.
    Chunked,
}

/// What [`HighlightController::clear_annotations`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearAnnotations {
    /// No active document, or it has no syntax: nothing to restore.
    Nothing,
    /// Styles rewritten at once: the editor must be rebound to them.
    Restyled,
    /// Too much syntect work for one step: the caller queues the document
    /// for a chunked highlight ([`HighlightController::queue_chunked`]),
    /// which restyles (and rebinds) as it goes.
    Chunked(DocumentId),
    /// The document's chunked highlight was already running: it restarted
    /// from the top in place, driven by the `ContinueHighlight` already
    /// pending. Nothing to do.
    Restarted,
}

/// The one decision between a synchronous and a chunked highlight, shared by
/// every path that highlights a whole document. Bounded in lines and in the
/// bytes handed to syntect (T0087): a few long lines can be as much work as
/// many short ones.
pub fn highlight_plan(text: &str) -> HighlightPlan {
    let (mut lines, mut tokenized) = (0, 0);
    for line in text.split_inclusive('\n') {
        lines += 1;
        if !is_long_line(line) {
            tokenized += line.len();
        }
        if lines > LARGE_FILE_THRESHOLD || tokenized > SYNC_HIGHLIGHT_MAX_BYTES {
            return HighlightPlan::Chunked;
        }
    }
    HighlightPlan::Synchronous
}

/// Borrowed UI widgets needed by highlight operations.
pub struct HighlightWidgets<'a> {
    pub editor: &'a mut TextEditor,
    pub banner_frame: &'a mut Frame,
    pub flex: &'a mut Flex,
    pub window: &'a mut Window,
}

pub struct HighlightController {
    highlighter: SyntaxHighlighter,
    /// Custom themes loaded at startup (T0079), by id.
    custom_themes: BTreeMap<String, CustomTheme>,
    /// Missing custom themes already reported on stderr: the theme is set
    /// several times at startup (and on every toggle), one line is enough.
    reported_missing: BTreeSet<String>,
    /// The editor colors of the active theme.
    editor_colors: EditorColors,
    /// The theme last asked for (before any fallback).
    active: ThemeRef,
    pub pending_rehighlight: Option<(DocumentId, i32)>,
    pub rehighlight_timer_active: bool,
    pub highlight_queue: Vec<DocumentId>,
    pub highlighting_enabled: bool,
}

impl HighlightController {
    pub fn new(theme: SyntaxTheme, font: Font, font_size: i32, highlighting_enabled: bool) -> Self {
        let editor_colors = derived_editor_colors(theme.background(), theme.foreground());
        let mut highlighter = SyntaxHighlighter::new(theme, font, font_size);
        highlighter.set_current_line_bg(editor_colors.current_line);
        Self {
            highlighter,
            custom_themes: BTreeMap::new(),
            reported_missing: BTreeSet::new(),
            editor_colors,
            active: theme.into(),
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

    pub fn style_table(&self) -> Vec<StyleTableEntryExt> {
        self.highlighter.style_table()
    }

    /// Switch to the theme a light/dark slot references. A custom theme
    /// that is not loaded (deleted, or its file unreadable) falls back to
    /// the mode's default built-in; the slot itself is left as is.
    pub fn set_theme(&mut self, theme: &ThemeRef, is_dark: bool) {
        self.active = theme.clone();
        let builtin = match theme {
            ThemeRef::Builtin(builtin) => *builtin,
            ThemeRef::Custom(id) => match self.custom_themes.get(id) {
                Some(custom) => {
                    let colors = effective_editor_colors(&custom.theme, false);
                    self.highlighter.set_theme_key(
                        &theme.theme_key(),
                        colors.background,
                        colors.text,
                    );
                    self.editor_colors = colors;
                    self.highlighter.set_current_line_bg(colors.current_line);
                    return;
                }
                None => {
                    let fallback = default_syntax_theme(is_dark);
                    if self.reported_missing.insert(id.clone()) {
                        eprintln!(
                            "[theme] custom theme '{id}' is not available, using the mode default ('{}')",
                            fallback.display_name()
                        );
                    }
                    fallback
                }
            },
        };
        self.highlighter.set_theme(builtin);
        self.editor_colors = derived_editor_colors(builtin.background(), builtin.foreground());
        self.highlighter
            .set_current_line_bg(self.editor_colors.current_line);
    }

    /// The theme the editor was last asked to show (a Settings preview
    /// included), before any fallback for a missing custom theme.
    pub fn active_theme(&self) -> &ThemeRef {
        &self.active
    }

    /// The editor colors of the active theme.
    pub fn editor_colors(&self) -> EditorColors {
        self.editor_colors
    }

    /// Replace the loaded custom themes. Call [`Self::set_theme`] afterwards
    /// to show a changed active theme.
    pub fn set_custom_themes(&mut self, themes: Vec<CustomTheme>) {
        self.highlighter.set_custom_themes(
            themes
                .iter()
                .map(|t| (ThemeRef::Custom(t.id.clone()).theme_key(), t.theme.clone()))
                .collect(),
        );
        self.custom_themes = themes.into_iter().map(|t| (t.id.clone(), t)).collect();
    }

    /// The loaded custom themes, by id.
    pub fn custom_themes(&self) -> &BTreeMap<String, CustomTheme> {
        &self.custom_themes
    }

    pub fn font(&self) -> Font {
        self.highlighter.font()
    }

    pub fn font_size(&self) -> i32 {
        self.highlighter.font_size()
    }

    pub fn set_font(&mut self, font: Font, size: i32) {
        self.highlighter.set_font(font, size);
    }

    /// Get or insert a marker style for an RGB color.
    pub fn get_or_insert_marker_rgb(&mut self, r: u8, g: u8, b: u8) -> char {
        self.highlighter.get_or_insert_marker_rgb(r, g, b)
    }

    /// Perform a full syntax highlight on text.
    pub fn highlight_full(
        &mut self,
        text: &str,
        syntax_name: &str,
    ) -> crate::app::services::syntax::FullHighlightResult {
        self.highlighter.highlight_full(text, syntax_name)
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

        defer_send(*sender, 0.0, Message::ContinueHighlight);
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

        defer_send(*sender, 0.0, Message::ContinueHighlight);
    }

    /// Queue `id` for a chunked highlight (once), and start the
    /// `ContinueHighlight` loop if nothing is running.
    pub fn queue_chunked(&mut self, id: DocumentId, sender: &Sender<Message>) {
        if self.highlight_queue.contains(&id) {
            return;
        }
        let was_idle =
            self.highlight_queue.is_empty() && self.highlighter.chunked_doc_id().is_none();
        self.highlight_queue.push(id);
        if was_idle {
            defer_send(*sender, 0.0, Message::ContinueHighlight);
        }
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

            let text = match tab_manager.doc_by_id(id) {
                Some(doc) => buffer_text_no_leak(&doc.buffer),
                None => return,
            };

            if highlight_plan(&text) == HighlightPlan::Synchronous {
                let result = self.highlighter.highlight_full(&text, name);
                if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                    doc.set_style_text(&result.style_string);
                    doc.checkpoints = result.checkpoints;
                }
            } else {
                self.queue_chunked(id, sender);
            }
        } else if let Some(doc) = tab_manager.doc_by_id_mut(id) {
            doc.syntax_name = None;
            doc.checkpoints.clear();
        }
    }

    fn rehighlight_document(
        &mut self,
        id: DocumentId,
        pos: i32,
        tab_manager: &mut TabManager,
        sender: &Sender<Message>,
        widgets: &mut HighlightWidgets,
    ) {
        let (syntax_name, text, edit_line, checkpoints_empty) = {
            if let Some(doc) = tab_manager.doc_by_id(id) {
                match doc.syntax_name {
                    Some(ref name) => {
                        let text = buffer_text_no_leak(&doc.buffer);
                        let line = doc.buffer.count_lines(0, pos) as usize;
                        (name.clone(), text, line, doc.checkpoints.is_empty())
                    }
                    None => return,
                }
            } else {
                return;
            }
        };

        if checkpoints_empty && highlight_plan(&text) == HighlightPlan::Chunked {
            self.queue_chunked(id, sender);
            return;
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
            doc.replace_styles(start, end, &result.style_chars);
            doc.checkpoints = checkpoints;
        }

        if tab_manager.active_id() == Some(id) {
            if self.highlighter.style_table_changed() {
                if let Some(doc) = tab_manager.doc_by_id(id) {
                    let style_buf = doc.style_buffer.clone();
                    let table = self.highlighter.style_table();
                    widgets.editor.set_highlight_data_ext(style_buf, table);
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
            if let (Some(cp), Some(doc)) = (
                self.highlighter.cancel_chunked(),
                tab_manager.doc_by_id_mut(id),
            ) {
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
            defer_send(*sender, 0.05, Message::DoRehighlight);
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
                doc.replace_styles(start, end, &output.style_chars);
            }

            if is_active {
                if let Some(doc) = tab_manager.doc_by_id(doc_id) {
                    let style_buf = doc.style_buffer.clone();
                    let table = self.highlighter.style_table();
                    widgets.editor.set_highlight_data_ext(style_buf, table);
                }
                widgets.editor.redraw();
            }

            if output.done {
                if let (Some(doc), Some(cp)) =
                    (tab_manager.doc_by_id_mut(doc_id), output.final_checkpoints)
                {
                    doc.checkpoints = cp;
                }
                self.start_next_queued_highlight(tab_manager, sender, widgets);
            } else {
                defer_send(*sender, 0.0, Message::ContinueHighlight);
            }
        }
    }

    fn start_next_queued_highlight(
        &mut self,
        tab_manager: &mut TabManager,
        sender: &Sender<Message>,
        widgets: &mut HighlightWidgets,
    ) {
        // Process exactly ONE document per call, then yield to the event loop.
        // This prevents a tight loop from freezing the UI when many files are queued.
        while let Some(id) = self.highlight_queue.first().copied() {
            self.highlight_queue.remove(0);

            let (syntax_name, text) = {
                if let Some(doc) = tab_manager.doc_by_id(id) {
                    match doc.syntax_name {
                        Some(ref name) => (name.clone(), buffer_text_no_leak(&doc.buffer)),
                        // No syntax — skip this doc and try next (loop continues)
                        None => continue,
                    }
                } else {
                    // Doc closed — skip and try next
                    continue;
                }
            };

            if highlight_plan(&text) == HighlightPlan::Synchronous {
                let result = self.highlighter.highlight_full(&text, &syntax_name);
                if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                    doc.set_style_text(&result.style_string);
                    doc.checkpoints = result.checkpoints;
                }
                if tab_manager.active_id() == Some(id) {
                    if let Some(doc) = tab_manager.doc_by_id(id) {
                        let style_buf = doc.style_buffer.clone();
                        let table = self.highlighter.style_table();
                        widgets.editor.set_highlight_data_ext(style_buf, table);
                    }
                    widgets.editor.redraw();
                }
                // Yield to event loop before processing next queued doc
                if !self.highlight_queue.is_empty() {
                    defer_send(*sender, 0.0, Message::ContinueHighlight);
                } else {
                    self.hide_highlight_banner(widgets);
                }
                return;
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
        let doc_ids: Vec<DocumentId> = tab_manager.documents().iter().map(|d| d.id).collect();
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

            if highlight_plan(&text) == HighlightPlan::Synchronous {
                let result = self.highlighter.highlight_full(&text, &syntax_name);
                if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                    doc.set_style_text(&result.style_string);
                    doc.checkpoints = result.checkpoints;
                }
            } else {
                self.queue_chunked(id, sender);
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

        let doc_ids: Vec<DocumentId> = tab_manager.documents().iter().map(|d| d.id).collect();
        for id in doc_ids {
            if let Some(doc) = tab_manager.doc_by_id_mut(id) {
                doc.checkpoints.clear();
                let len = doc.buffer.length() as usize;
                let plain = "A".repeat(len);
                doc.set_style_text(&plain);
            }
        }
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
    pub fn update_annotations(
        &mut self,
        annotations: Vec<crate::app::plugins::LineAnnotation>,
        tab_manager: &TabManager,
        editor: &mut TextEditor,
    ) {
        use crate::app::plugins::AnnotationColor;
        use crate::app::services::syntax::style_map::StyleMap;

        let Some(doc) = tab_manager.active_doc() else {
            return;
        };
        let mut style_buf = doc.style_buffer.clone();
        let buf = doc.buffer.clone();
        // Marker writes below may overwrite the current-line twin (T0081).
        doc.mark_style_stale();

        let get_marker_char = |highlight: &mut Self, color: &AnnotationColor| -> char {
            match color {
                AnnotationColor::Rgb(r, g, b) => highlight.get_or_insert_marker_rgb(*r, *g, *b),
                _ => StyleMap::marker_style_char(color),
            }
        };

        // Merge annotations by line, keeping highest-priority gutter and all inlines
        let mut merged: BTreeMap<
            u32,
            (
                Option<crate::app::plugins::GutterMark>,
                Vec<crate::app::plugins::InlineHighlight>,
            ),
        > = BTreeMap::new();

        for ann in annotations {
            let entry = merged.entry(ann.line).or_insert((None, Vec::new()));

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

            entry.1.extend(ann.inline);
        }

        for (line_num, (gutter, inlines)) in merged {
            let target_line = line_num.saturating_sub(1) as i32;

            let mut line_start = 0;
            for _ in 0..target_line {
                if let Some(next_pos) = buf.find_char_forward(line_start, '\n') {
                    line_start = next_pos + 1;
                } else {
                    break;
                }
            }

            let line_end_with_newline = buf
                .find_char_forward(line_start, '\n')
                .map(|p| p + 1)
                .unwrap_or(buf.length());
            let line_end = buf
                .find_char_forward(line_start, '\n')
                .unwrap_or(buf.length());

            let line_len = line_end - line_start;

            if let Some(ref gutter_mark) = gutter
                && line_len > 0
            {
                let marker_char = get_marker_char(self, &gutter_mark.color);
                let marker_str: String =
                    std::iter::repeat_n(marker_char, (line_end_with_newline - line_start) as usize)
                        .collect();
                style_buf.replace(line_start, line_end_with_newline, &marker_str);
            }

            let mut sorted_inlines = inlines;
            sorted_inlines.sort_by_key(|i| std::cmp::Reverse(i.color.priority()));

            for inline in sorted_inlines {
                let marker_char = get_marker_char(self, &inline.color);

                let start_col = (inline.start_col.saturating_sub(1) as i32).min(line_len);
                let end_col = inline
                    .end_col
                    .map(|c| (c.saturating_sub(1) as i32).min(line_len))
                    .unwrap_or(line_len);

                if start_col >= end_col {
                    continue;
                }

                let highlight_start = line_start + start_col;
                let highlight_end = line_start + end_col;
                let highlight_len = (highlight_end - highlight_start) as usize;

                if highlight_len > 0 {
                    let marker_str: String =
                        std::iter::repeat_n(marker_char, highlight_len).collect();
                    style_buf.replace(highlight_start, highlight_end, &marker_str);
                }
            }
        }

        let table = self.style_table();
        editor.set_highlight_data_ext(style_buf, table);
        editor.redraw();
    }

    /// Clear line annotations by re-highlighting the active document only.
    /// This restores the original syntax highlighting without annotation
    /// overlays. The re-highlight follows [`highlight_plan`] like every
    /// other whole-document highlight (T0087): a large document goes to the
    /// chunked path instead of one synchronous pass. The caller applies the
    /// outcome: rebind the editor ([`Self::bind_active_styles`]) or queue
    /// the chunked highlight ([`Self::queue_chunked`]).
    pub fn clear_annotations(&mut self, tab_manager: &mut TabManager) -> ClearAnnotations {
        let Some(doc) = tab_manager.active_doc() else {
            return ClearAnnotations::Nothing;
        };
        let Some(syntax_name) = doc.syntax_name.clone() else {
            return ClearAnnotations::Nothing;
        };
        let id = doc.id;
        let text = buffer_text_no_leak(&doc.buffer);

        if highlight_plan(&text) == HighlightPlan::Chunked {
            // A chunked run already under way may have passed annotated
            // regions: start it over in place. Its pending ContinueHighlight
            // drives the new run, so no second message chain is started.
            if self.highlighter.chunked_doc_id() == Some(id) {
                self.highlighter.start_chunked(id, text, &syntax_name);
                return ClearAnnotations::Restarted;
            }
            return ClearAnnotations::Chunked(id);
        }

        let result = self.highlight_full(&text, &syntax_name);

        if let Some(doc) = tab_manager.active_doc_mut() {
            doc.set_style_text(&result.style_string);
            doc.checkpoints = result.checkpoints;
        }
        ClearAnnotations::Restyled
    }

    /// Bind the active document's style buffer and the style table to the
    /// editor.
    pub fn bind_active_styles(&self, tab_manager: &TabManager, editor: &mut TextEditor) {
        if let Some(doc) = tab_manager.active_doc() {
            let style_buf = doc.style_buffer.clone();
            let table = self.style_table();
            editor.set_highlight_data_ext(style_buf, table);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::services::custom_themes::ThemeStore;
    use tempfile::tempdir;

    /// T0087: the synchronous/chunked decision bounds bytes as well as lines.
    #[test]
    fn long_line_highlight_plan_bounds_lines_and_bytes() {
        let normal = "fn main() {}\n".repeat(100);
        assert_eq!(highlight_plan(&normal), HighlightPlan::Synchronous);
        let many_lines = "x\n".repeat(LARGE_FILE_THRESHOLD + 1);
        assert_eq!(highlight_plan(&many_lines), HighlightPlan::Chunked);
        // Few lines, over the byte bound: the line count alone says "small".
        let line = format!("{}\n", "a".repeat(1023));
        let over = line.repeat(SYNC_HIGHLIGHT_MAX_BYTES / 1024 + 1);
        assert_eq!(highlight_plan(&over), HighlightPlan::Chunked);
        let at_bound = line.repeat(SYNC_HIGHLIGHT_MAX_BYTES / 1024);
        assert_eq!(highlight_plan(&at_bound), HighlightPlan::Synchronous);
        // Many lines just under the long-line limit are all tokenized and
        // all count: 64 lines of exactly LONG_LINE_BYTES are exactly 1 MiB.
        let at_limit = format!("{}\n", "a".repeat(LONG_LINE_BYTES - 1));
        assert_eq!(64 * LONG_LINE_BYTES, SYNC_HIGHLIGHT_MAX_BYTES);
        assert_eq!(
            highlight_plan(&at_limit.repeat(64)),
            HighlightPlan::Synchronous
        );
        assert_eq!(highlight_plan(&at_limit.repeat(65)), HighlightPlan::Chunked);
        // A line syntect skips costs it nothing: one 8 MB line plus a few
        // short ones is synchronous (no "Highlighting large file" banner and
        // its relayouts).
        let huge = format!("{}\nvar x = 1;\n", "a".repeat(8 * 1024 * 1024));
        assert_eq!(highlight_plan(&huge), HighlightPlan::Synchronous);
    }

    /// A tab manager holding one active JavaScript document with `text`.
    /// (Nothing here may touch the FLTK runtime, e.g. `defer_send`: it pins
    /// fltk-rs's UI thread to this test's thread.)
    fn js_tab(text: &str) -> (TabManager, DocumentId) {
        let (sender, _receiver) = fltk::app::channel::<Message>();
        let mut tabs = TabManager::new(sender);
        let id = tabs.add_untitled();
        let doc = tabs.doc_by_id_mut(id).unwrap();
        doc.buffer.set_text(text);
        doc.syntax_name = Some("JavaScript".to_string());
        (tabs, id)
    }

    fn style_text(tabs: &TabManager, id: DocumentId) -> String {
        buffer_text_no_leak(&tabs.doc_by_id(id).unwrap().style_buffer)
    }

    /// T0087: clearing a plugin's annotations re-highlights through the same
    /// plan as every other path. A document over the plan's bound goes to
    /// the chunked highlight instead of one synchronous full pass.
    #[test]
    fn long_line_clear_annotations_follows_the_plan() {
        // Over the line bound: 6000 short lines.
        let big = "var x = 1;\n".repeat(LARGE_FILE_THRESHOLD + 1000);
        let (mut tabs, id) = js_tab(&big);
        let before = style_text(&tabs, id);
        let mut hc = controller();
        let outcome = hc.clear_annotations(&mut tabs);
        assert_eq!(outcome, ClearAnnotations::Chunked(id));
        assert_eq!(style_text(&tabs, id), before, "no synchronous pass");

        // Its chunked run already in flight: restarted in place from the
        // top, not queued a second time (one ContinueHighlight chain).
        hc.highlighter.start_chunked(id, big.clone(), "JavaScript");
        hc.highlighter
            .process_chunk()
            .expect("chunked highlight is active");
        let outcome = hc.clear_annotations(&mut tabs);
        assert_eq!(outcome, ClearAnnotations::Restarted);
        assert_eq!(hc.highlighter.chunked_doc_id(), Some(id));
        assert!(hc.highlight_queue.is_empty());
        let first = hc.highlighter.process_chunk().unwrap();
        assert_eq!(first.byte_start, 0, "from the top");

        // Within the plan: restyled at once, as before.
        let small = "var x = 1;\nfunction f(a) { return a; }\n";
        let (mut tabs, id) = js_tab(small);
        let outcome = hc.clear_annotations(&mut tabs);
        assert_eq!(outcome, ClearAnnotations::Restyled);
        assert_eq!(
            style_text(&tabs, id),
            hc.highlight_full(small, "JavaScript").style_string
        );
    }

    fn controller() -> HighlightController {
        HighlightController::new(SyntaxTheme::Base16OceanLight, Font::Courier, 14, true)
    }

    /// A slot pointing at a custom theme that is not loaded (deleted or
    /// unreadable) shows the mode's default built-in; the reference itself is
    /// untouched (nothing rewrites settings.json).
    #[test]
    fn custom_theme_missing_reference_falls_back_to_mode_default() {
        let mut hc = controller();
        let missing = ThemeRef::Custom("t-gone".to_string());
        for (is_dark, default) in [
            (true, SyntaxTheme::Base16OceanDark),
            (false, SyntaxTheme::Base16OceanLight),
        ] {
            hc.set_theme(&missing, is_dark);
            assert_eq!(hc.active_theme(), &missing, "the request is remembered");
            assert_eq!(hc.highlighter().theme_key(), default.theme_key());
            // Reported once, however often the theme is set.
            assert_eq!(hc.reported_missing.len(), 1);
            assert_eq!(
                hc.editor_colors(),
                derived_editor_colors(default.background(), default.foreground())
            );
        }
        assert_eq!(missing, ThemeRef::Custom("t-gone".to_string()));
    }

    #[test]
    fn custom_theme_loaded_reference_is_applied() {
        let dir = tempdir().unwrap();
        let mut store = ThemeStore::empty(dir.path().to_path_buf());
        let id = store
            .clone_from(&SyntaxTheme::SolarizedLight.into(), "Paper")
            .unwrap();
        store
            .set_color(
                &id,
                crate::app::services::custom_themes::EditorColorField::Background,
                (1, 2, 3),
            )
            .unwrap();
        let mut hc = controller();
        hc.set_custom_themes(store.into_themes());
        hc.set_theme(&ThemeRef::Custom(id.clone()), false);
        assert_eq!(hc.highlighter().theme_key(), format!("custom:{id}"));
        assert_eq!(hc.editor_colors().background, (1, 2, 3));
        assert_eq!(hc.highlighter().theme_background(), (1, 2, 3));
    }
}
