use std::cell::Cell;
use std::ffi::c_void;
use std::rc::Rc;

use fltk::app::Sender;
use fltk::text::TextBuffer;

use super::messages::Message;
use super::syntax::checkpoint::SparseCheckpoints;
use super::tab_manager::GroupId;
use super::text_ops::extract_filename;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(pub u64);

/// The closure type stored behind the FFI `void* cbArg`.
type ModifyCb = dyn FnMut(i32, i32, i32, i32, *const std::ffi::c_char);

/// Module-level shim passed to FLTK as the C callback.  Because this is a
/// single, fixed function pointer, `remove_modify_callback` can find it by
/// pointer equality (unlike fltk-rs's wrapper which creates a new shim each
/// time and therefore can never match).
unsafe extern "C" fn modify_shim(
    pos: std::ffi::c_int,
    n_inserted: std::ffi::c_int,
    n_deleted: std::ffi::c_int,
    n_restyled: std::ffi::c_int,
    deleted_text: *const std::ffi::c_char,
    cb_arg: *mut c_void,
) {
    let cb: &mut Box<ModifyCb> = unsafe { &mut *(cb_arg as *mut Box<ModifyCb>) };
    cb(pos, n_inserted, n_deleted, n_restyled, deleted_text);
}

unsafe extern "C" {
    fn Fl_Text_Buffer_add_modify_callback(
        buf: *mut c_void,
        cb: Option<
            unsafe extern "C" fn(
                std::ffi::c_int,
                std::ffi::c_int,
                std::ffi::c_int,
                std::ffi::c_int,
                *const std::ffi::c_char,
                *mut c_void,
            ),
        >,
        cb_arg: *mut c_void,
    );
    fn Fl_Text_Buffer_remove_modify_callback(
        buf: *mut c_void,
        cb: Option<
            unsafe extern "C" fn(
                std::ffi::c_int,
                std::ffi::c_int,
                std::ffi::c_int,
                std::ffi::c_int,
                *const std::ffi::c_char,
                *mut c_void,
            ),
        >,
        cb_arg: *mut c_void,
    );
}

/// Box the closure and register it via FFI.  Returns the `cbArg` pointer
/// that must be stored for later cleanup.
fn register_modify_callback(
    buffer: &TextBuffer,
    style_buffer: &TextBuffer,
    has_unsaved_changes: &Rc<Cell<bool>>,
    doc_id: DocumentId,
    sender: Sender<Message>,
) -> *mut c_void {
    let changes = has_unsaved_changes.clone();
    let mut style_buf = style_buffer.clone();

    let cb: Box<ModifyCb> = Box::new(
        move |pos: i32,
              inserted: i32,
              deleted: i32,
              _restyled: i32,
              _deleted_text: *const std::ffi::c_char| {
            if inserted > 0 || deleted > 0 {
                changes.set(true);
                if inserted > 0 {
                    let filler = "A".repeat(inserted as usize);
                    style_buf.insert(pos, &filler);
                }
                if deleted > 0 {
                    style_buf.remove(pos, pos + deleted);
                }
                sender.send(Message::BufferModified(doc_id, pos));
            }
        },
    );

    let data = Box::into_raw(Box::new(cb)) as *mut c_void;

    unsafe {
        Fl_Text_Buffer_add_modify_callback(
            buffer.as_ptr() as *mut c_void,
            Some(modify_shim),
            data,
        );
    }

    data
}

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
    pub group_id: Option<GroupId>,
    /// Pointer to the heap-allocated closure passed to FLTK's modify callback.
    /// Must be freed in cleanup() after removing the callback.
    modify_cb_data: *mut c_void,
}

impl Document {
    pub fn new_untitled(id: DocumentId, counter: u32, sender: Sender<Message>) -> Self {
        let display_name = if counter == 1 {
            "Untitled".to_string()
        } else {
            format!("Untitled {}", counter)
        };

        let buffer = TextBuffer::default();
        let style_buffer = TextBuffer::default();
        let has_unsaved_changes = Rc::new(Cell::new(false));

        let modify_cb_data =
            register_modify_callback(&buffer, &style_buffer, &has_unsaved_changes, id, sender);

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
            group_id: None,
            modify_cb_data,
        }
    }

    pub fn new_from_file(id: DocumentId, path: String, content: &str, sender: Sender<Message>) -> Self {
        let display_name = extract_filename(&path);

        let buffer = TextBuffer::default();
        let mut style_buffer = TextBuffer::default();
        let has_unsaved_changes = Rc::new(Cell::new(false));

        let modify_cb_data =
            register_modify_callback(&buffer, &style_buffer, &has_unsaved_changes, id, sender);

        // These trigger the modify callback, which keeps style_buffer in sync
        buffer.clone().set_text(content);
        let default_style = "A".repeat(content.len());
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
            group_id: None,
            modify_cb_data,
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
        // Remove the modify callback from the C++ buffer FIRST, then free the
        // closure data.  Using the same `modify_shim` function pointer that was
        // passed to `add`, so FLTK can find the entry by pointer equality.
        if !self.modify_cb_data.is_null() {
            unsafe {
                Fl_Text_Buffer_remove_modify_callback(
                    self.buffer.as_ptr() as *mut c_void,
                    Some(modify_shim),
                    self.modify_cb_data,
                );
                // Free the heap-allocated Box<Box<dyn FnMut(...)>>
                let _ = Box::from_raw(self.modify_cb_data as *mut Box<ModifyCb>);
            }
            self.modify_cb_data = std::ptr::null_mut();
        }

        self.buffer.set_text("");
        self.style_buffer.set_text("");
    }
}
