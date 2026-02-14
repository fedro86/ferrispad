use std::cell::Cell;
use std::rc::Rc;

use fltk::text::TextBuffer;

use super::text_ops::extract_filename;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(pub u64);

pub struct Document {
    pub id: DocumentId,
    pub buffer: TextBuffer,
    pub file_path: Option<String>,
    pub has_unsaved_changes: Rc<Cell<bool>>,
    pub display_name: String,
    pub cursor_position: i32,
}

impl Document {
    pub fn new_untitled(id: DocumentId, counter: u32) -> Self {
        let display_name = if counter == 1 {
            "Untitled".to_string()
        } else {
            format!("Untitled {}", counter)
        };

        let mut buffer = TextBuffer::default();
        let has_unsaved_changes = Rc::new(Cell::new(false));

        // Set up modify callback for this document's buffer
        let changes = has_unsaved_changes.clone();
        buffer.add_modify_callback(move |_, _, _, _, _| {
            changes.set(true);
        });

        Self {
            id,
            buffer,
            file_path: None,
            has_unsaved_changes,
            display_name,
            cursor_position: 0,
        }
    }

    pub fn new_from_file(id: DocumentId, path: String, content: &str) -> Self {
        let display_name = extract_filename(&path);

        let mut buffer = TextBuffer::default();
        let has_unsaved_changes = Rc::new(Cell::new(false));

        // Set up modify callback before setting text so initial load doesn't trigger dirty
        let changes = has_unsaved_changes.clone();
        buffer.add_modify_callback(move |_, _, _, _, _| {
            changes.set(true);
        });

        buffer.set_text(content);
        // Reset dirty flag since we just loaded the file
        has_unsaved_changes.set(false);

        Self {
            id,
            buffer,
            file_path: Some(path),
            has_unsaved_changes,
            display_name,
            cursor_position: 0,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.has_unsaved_changes.get()
    }

    pub fn mark_clean(&self) {
        self.has_unsaved_changes.set(false);
    }

    pub fn update_display_name(&mut self) {
        if let Some(ref path) = self.file_path {
            self.display_name = extract_filename(path);
        }
    }

    /// Release buffer memory before drop — clears text
    /// so the underlying FLTK buffer frees its allocation immediately.
    pub fn cleanup(&mut self) {
        self.buffer.set_text("");
    }
}
