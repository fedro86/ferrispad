use std::cell::Cell;
use std::rc::Rc;

use fltk::app::Sender;
use fltk::text::TextBuffer;

use super::messages::Message;
use super::syntax::checkpoint::SparseCheckpoints;
use super::text_ops::extract_filename;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(pub u64);

pub struct Document {
    pub id: DocumentId,
    pub buffer: TextBuffer,
    pub style_buffer: TextBuffer,
    pub file_path: Option<String>,
    pub has_unsaved_changes: Rc<Cell<bool>>,
    pub display_name: String,
    pub cursor_position: i32,
    pub checkpoints: SparseCheckpoints,
    pub syntax_name: Option<String>,
}

impl Document {
    pub fn new_untitled(id: DocumentId, counter: u32, sender: Sender<Message>) -> Self {
        let display_name = if counter == 1 {
            "Untitled".to_string()
        } else {
            format!("Untitled {}", counter)
        };

        let mut buffer = TextBuffer::default();
        let style_buffer = TextBuffer::default();
        let has_unsaved_changes = Rc::new(Cell::new(false));

        let changes = has_unsaved_changes.clone();
        let mut style_buf = style_buffer.clone();
        let doc_id = id;
        buffer.add_modify_callback(move |pos, inserted, deleted, _restyled, _deleted_text| {
            if inserted > 0 || deleted > 0 {
                changes.set(true);
                if inserted > 0 {
                    let filler: String = std::iter::repeat('A').take(inserted as usize).collect();
                    style_buf.insert(pos, &filler);
                }
                if deleted > 0 {
                    style_buf.remove(pos, pos + deleted);
                }
                sender.send(Message::BufferModified(doc_id, pos));
            }
        });

        Self {
            id,
            buffer,
            style_buffer,
            file_path: None,
            has_unsaved_changes,
            display_name,
            cursor_position: 0,
            checkpoints: SparseCheckpoints::new(),
            syntax_name: None,
        }
    }

    pub fn new_from_file(id: DocumentId, path: String, content: &str, sender: Sender<Message>) -> Self {
        let display_name = extract_filename(&path);

        let mut buffer = TextBuffer::default();
        let mut style_buffer = TextBuffer::default();
        let has_unsaved_changes = Rc::new(Cell::new(false));

        let changes = has_unsaved_changes.clone();
        let mut style_buf = style_buffer.clone();
        let doc_id = id;
        buffer.add_modify_callback(move |pos, inserted, deleted, _restyled, _deleted_text| {
            if inserted > 0 || deleted > 0 {
                changes.set(true);
                if inserted > 0 {
                    let filler: String = std::iter::repeat('A').take(inserted as usize).collect();
                    style_buf.insert(pos, &filler);
                }
                if deleted > 0 {
                    style_buf.remove(pos, pos + deleted);
                }
                sender.send(Message::BufferModified(doc_id, pos));
            }
        });

        buffer.set_text(content);
        let default_style: String = std::iter::repeat('A').take(content.len()).collect();
        style_buffer.set_text(&default_style);
        has_unsaved_changes.set(false);

        Self {
            id,
            buffer,
            style_buffer,
            file_path: Some(path),
            has_unsaved_changes,
            display_name,
            cursor_position: 0,
            checkpoints: SparseCheckpoints::new(),
            syntax_name: None,
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

    pub fn cleanup(&mut self) {
        self.buffer.set_text("");
        self.style_buffer.set_text("");
    }
}
