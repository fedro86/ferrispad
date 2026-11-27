use std::cell::Cell;
use std::ffi::c_void;
use std::rc::Rc;

use fltk::app::Sender;
use fltk::text::TextBuffer;

use super::messages::Message;
use crate::app::controllers::tabs::GroupId;
use crate::app::plugins::{Diagnostic, TreeViewRequest};
use crate::app::services::current_line::CurrentLineTwin;
use crate::app::services::syntax::checkpoint::SparseCheckpoints;
use crate::app::services::text_ops::extract_filename;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(pub u64);

/// The closure type stored behind the FFI `void* cbArg`.
type ModifyCb = dyn FnMut(i32, i32, i32, i32, *const std::ffi::c_char);

/// Module-level shim passed to FLTK as the C callback.  Because this is a
/// single, fixed function pointer, `remove_modify_callback` can find it by
/// pointer equality (unlike fltk-rs's wrapper which creates a new shim each
/// time and therefore can never match).
///
/// # Safety
///
/// This function is called by FLTK's C++ code. The `cb_arg` parameter must be
/// a valid pointer to `Box<ModifyCb>` created in `register_modify_callback()`.
/// The pointer remains valid until `cleanup()` removes the callback and frees
/// the box.
unsafe extern "C" fn modify_shim(
    pos: std::ffi::c_int,
    n_inserted: std::ffi::c_int,
    n_deleted: std::ffi::c_int,
    n_restyled: std::ffi::c_int,
    deleted_text: *const std::ffi::c_char,
    cb_arg: *mut c_void,
) {
    // SAFETY: cb_arg was created via Box::into_raw in register_modify_callback
    // and is guaranteed valid until cleanup() is called. We add a null check
    // as defensive programming against unexpected FLTK behavior.
    if cb_arg.is_null() {
        eprintln!("WARN: modify_shim called with null callback argument");
        return;
    }
    // SAFETY: After the null check above, cb_arg points to a valid Box<ModifyCb>
    // that was allocated in register_modify_callback and will remain valid until
    // cleanup() is called for this Document.
    unsafe {
        let cb: &mut Box<ModifyCb> = &mut *(cb_arg as *mut Box<ModifyCb>);
        cb(pos, n_inserted, n_deleted, n_restyled, deleted_text);
    }
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

// A `DirtyChanged` is queued and not yet handled (T0093). Every document's
// modify callback runs on the UI thread and shares it, so N clean -> dirty
// flips in one step (e.g. a tab-size change touching every buffer) queue one
// refresh, not N.
thread_local! {
    static DIRTY_REFRESH_QUEUED: Cell<bool> = const { Cell::new(false) };
}

/// The queued `DirtyChanged` was taken off the queue: later flips queue a
/// new refresh.
pub fn dirty_refresh_handled() {
    DIRTY_REFRESH_QUEUED.with(|q| q.set(false));
}

/// Called for every message the main loop takes off the queue, before
/// dispatch, through the one dequeue hook `AppState::note_dequeued`. Clearing the flag here, not in the handler, keeps the reset
/// independent of whether the handler runs; otherwise one skipped handler
/// would leave the flag set and silence every later flip.
pub fn note_dequeued(msg: &Message) {
    if matches!(msg, Message::DirtyChanged(_)) {
        dirty_refresh_handled();
    }
}

/// Queue a `DirtyChanged` unless one is already waiting.
fn queue_dirty_refresh(sender: &Sender<Message>, doc_id: DocumentId) {
    if !DIRTY_REFRESH_QUEUED.with(|q| q.replace(true)) {
        sender.send(Message::DirtyChanged(doc_id));
    }
}

/// Box the closure and register it via FFI.  Returns the `cbArg` pointer
/// that must be stored for later cleanup.
fn register_modify_callback(
    buffer: &TextBuffer,
    style_buffer: &TextBuffer,
    has_unsaved_changes: &Rc<Cell<bool>>,
    current_line: &Rc<Cell<CurrentLineTwin>>,
    doc_id: DocumentId,
    sender: Sender<Message>,
) -> *mut c_void {
    let changes = has_unsaved_changes.clone();
    let current_line = current_line.clone();
    let mut style_buf = style_buffer.clone();

    let cb: Box<ModifyCb> = Box::new(
        move |pos: i32,
              inserted: i32,
              deleted: i32,
              _restyled: i32,
              _deleted_text: *const std::ffi::c_char| {
            if inserted > 0 || deleted > 0 {
                // Report the clean -> dirty flip once, so the window title
                // and the tab bar show it (T0093); later edits don't.
                let was_clean = !changes.get();
                changes.set(true);
                if inserted > 0 {
                    let filler = "A".repeat(inserted as usize);
                    style_buf.insert(pos, &filler);
                }
                if deleted > 0 {
                    style_buf.remove(pos, pos + deleted);
                }
                // The style bytes moved with the text: follow the twinned
                // current line (T0081). A Cell, not a RefCell: no borrow
                // can fail inside this FLTK callback.
                let mut twin = current_line.get();
                twin.on_modify(pos, inserted, deleted);
                current_line.set(twin);
                sender.send(Message::BufferModified {
                    id: doc_id,
                    pos,
                    inserted,
                    deleted,
                });
                if was_clean {
                    queue_dirty_refresh(&sender, doc_id);
                }
            }
        },
    );

    let data = Box::into_raw(Box::new(cb)) as *mut c_void;

    // SAFETY: We register our module-level modify_shim as the callback function.
    // The `data` pointer (Box<Box<ModifyCb>>) will be passed to modify_shim on
    // every buffer modification. This pointer must remain valid until we call
    // remove_modify_callback in cleanup(). The Document struct stores this
    // pointer in modify_cb_data and cleanup() is called via Drop.
    unsafe {
        Fl_Text_Buffer_add_modify_callback(buffer.as_ptr() as *mut c_void, Some(modify_shim), data);
    }

    data
}

/// Describes how a document maps to its source file on disk.
#[derive(Debug, Clone, Default)]
pub enum PartialFileInfo {
    /// Normal file — the buffer contains the entire file.
    #[default]
    Full,
    /// Tail view — buffer contains content starting at `start_byte` to the end of the file.
    Tail { start_byte: u64 },
    /// Chunk view — buffer contains content from `start_byte` to `end_byte` (exclusive).
    Chunk { start_byte: u64, end_byte: u64 },
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
    /// Diagnostics from the last lint run (persisted across tab switches)
    pub diagnostics: Vec<Diagnostic>,
    /// Whether this document has been linted at least once
    pub has_been_linted: bool,
    /// Cached tree view (plugin_name, request) — invalidated on content change
    pub cached_tree: Option<(String, TreeViewRequest)>,
    /// Cached line count — updated on every edit, avoids O(n) scan on tab switch
    pub cached_line_count: usize,
    /// Last-known modification time of the on-disk file (None for untitled docs)
    pub disk_mtime: Option<std::time::SystemTime>,
    /// Disambiguated tab label when multiple files share the same filename.
    /// Set by TabManager; None means display_name is already unique.
    pub disambiguated_name: Option<String>,
    /// For tail/chunk documents: byte offsets into the original file so edits
    /// can be saved back to the correct position.
    pub partial_info: PartialFileInfo,
    /// Current-line twin state (T0081), shared with the modify callback.
    pub current_line: Rc<Cell<CurrentLineTwin>>,
    /// The `n` of "Untitled n" this document was created with (T0091). It
    /// only counts while the document has no path: see
    /// [`Document::untitled_number`].
    untitled_number: Option<u32>,
    /// Pointer to the heap-allocated closure passed to FLTK's modify callback.
    /// Must be freed in cleanup() after removing the callback.
    modify_cb_data: *mut c_void,
}

impl Document {
    /// Rewrite the whole style buffer. Every whole-buffer style write goes
    /// through here so the current-line twin is re-applied afterwards (T0081).
    pub fn set_style_text(&mut self, styles: &str) {
        self.style_buffer.set_text(styles);
        self.mark_style_stale();
    }

    /// Rewrite part of the style buffer (incremental highlight, chunks) with
    /// the same number of style bytes. Only a rewrite that touches the
    /// twinned current line makes the twin stale (T0096).
    pub fn replace_styles(&mut self, start: i32, end: i32, styles: &str) {
        self.style_buffer.replace(start, end, styles);
        let mut twin = self.current_line.get();
        twin.mark_stale_in(start, end);
        self.current_line.set(twin);
    }

    /// The style bytes were rewritten: twin chars there may be gone.
    pub fn mark_style_stale(&self) {
        let mut twin = self.current_line.get();
        twin.mark_stale();
        self.current_line.set(twin);
    }

    /// The display name of untitled document number `n`: "Untitled" for 1,
    /// "Untitled n" from 2 on.
    pub fn untitled_name(n: u32) -> String {
        if n <= 1 {
            "Untitled".to_string()
        } else {
            format!("Untitled {n}")
        }
    }

    /// The untitled number this document holds: its creation number while
    /// it has no path, `None` once it has one (saved or opened) — a saved
    /// document frees its number.
    pub fn untitled_number(&self) -> Option<u32> {
        if self.file_path.is_none() {
            self.untitled_number
        } else {
            None
        }
    }

    /// Turn this document back into untitled document number `n` (the
    /// tabs-off "New", which reuses the single document).
    pub fn set_untitled(&mut self, n: u32) {
        self.file_path = None;
        self.untitled_number = Some(n);
        self.display_name = Self::untitled_name(n);
        self.disambiguated_name = None;
    }

    pub fn new_untitled(id: DocumentId, number: u32, sender: Sender<Message>) -> Self {
        let display_name = Self::untitled_name(number);

        let buffer = TextBuffer::default();
        let style_buffer = TextBuffer::default();
        let has_unsaved_changes = Rc::new(Cell::new(false));
        let current_line = Rc::new(Cell::new(CurrentLineTwin::default()));

        let modify_cb_data = register_modify_callback(
            &buffer,
            &style_buffer,
            &has_unsaved_changes,
            &current_line,
            id,
            sender,
        );

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
            diagnostics: Vec::new(),
            has_been_linted: false,
            cached_tree: None,
            cached_line_count: 0,
            disk_mtime: None,
            disambiguated_name: None,
            partial_info: PartialFileInfo::Full,
            current_line,
            untitled_number: Some(number),
            modify_cb_data,
        }
    }

    pub fn new_from_file(
        id: DocumentId,
        path: String,
        content: &str,
        sender: Sender<Message>,
    ) -> Self {
        let display_name = extract_filename(&path);

        let buffer = TextBuffer::default();
        let mut style_buffer = TextBuffer::default();
        let has_unsaved_changes = Rc::new(Cell::new(false));
        let current_line = Rc::new(Cell::new(CurrentLineTwin::default()));

        let modify_cb_data = register_modify_callback(
            &buffer,
            &style_buffer,
            &has_unsaved_changes,
            &current_line,
            id,
            sender,
        );

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
            diagnostics: Vec::new(),
            has_been_linted: false,
            cached_tree: None,
            cached_line_count: content.lines().count(),
            disk_mtime: None,
            disambiguated_name: None,
            partial_info: PartialFileInfo::Full,
            current_line,
            untitled_number: None,
            modify_cb_data,
        }
    }

    /// Create a document from a pre-populated TextBuffer (memory-optimized path).
    ///
    /// This avoids copying the file content: the buffer already contains the text
    /// from streaming load. We only need to initialize the style buffer.
    ///
    /// Note: For large files (>100MB), we skip the style buffer initialization
    /// to avoid another full-size allocation.
    pub fn new_from_buffer(
        id: DocumentId,
        path: String,
        buffer: TextBuffer,
        sender: Sender<Message>,
        skip_style_init: bool,
    ) -> Self {
        let display_name = extract_filename(&path);

        let mut style_buffer = TextBuffer::default();
        let has_unsaved_changes = Rc::new(Cell::new(false));
        let current_line = Rc::new(Cell::new(CurrentLineTwin::default()));

        let modify_cb_data = register_modify_callback(
            &buffer,
            &style_buffer,
            &has_unsaved_changes,
            &current_line,
            id,
            sender,
        );

        // Initialize style buffer for syntax highlighting
        // For large files, skip this to save memory (no highlighting anyway)
        if !skip_style_init {
            let content_len = buffer.length() as usize;
            let default_style = "A".repeat(content_len);
            style_buffer.set_text(&default_style);
        }
        has_unsaved_changes.set(false);

        let cached_line_count = buffer.count_lines(0, buffer.length()) as usize;

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
            diagnostics: Vec::new(),
            has_been_linted: false,
            cached_tree: None,
            cached_line_count,
            disk_mtime: None,
            disambiguated_name: None,
            partial_info: PartialFileInfo::Full,
            current_line,
            untitled_number: None,
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
            self.disambiguated_name = None; // recomputed by TabManager
        }
    }

    /// Return the label for tabs and window title.
    /// Uses the disambiguated name if set, otherwise the plain display name.
    pub fn tab_label(&self) -> &str {
        self.disambiguated_name
            .as_deref()
            .unwrap_or(&self.display_name)
    }

    /// Clean up FFI resources. Called automatically by Drop.
    ///
    /// This method is idempotent - safe to call multiple times.
    pub fn cleanup(&mut self) {
        // Guard: already cleaned up (Drop may call after explicit cleanup)
        if self.modify_cb_data.is_null() {
            return;
        }

        // SAFETY: We remove the modify callback from FLTK FIRST (so it stops
        // calling modify_shim with our pointer), then free the closure data.
        // Order is critical: if we freed first, FLTK could call modify_shim
        // with a dangling pointer.
        //
        // We use the same `modify_shim` function pointer that was passed to
        // add_modify_callback, so FLTK can find the entry by pointer equality.
        //
        // After this, we set modify_cb_data to null to prevent double-free.
        unsafe {
            Fl_Text_Buffer_remove_modify_callback(
                self.buffer.as_ptr() as *mut c_void,
                Some(modify_shim),
                self.modify_cb_data,
            );
            // Reconstruct the Box and let it drop to free memory
            let _ = Box::from_raw(self.modify_cb_data as *mut Box<ModifyCb>);
        }
        self.modify_cb_data = std::ptr::null_mut();

        self.buffer.set_text("");
        self.style_buffer.set_text("");
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Checks that build documents (hence FLTK `TextBuffer`s), so they must run
/// on the process main thread: on macOS the first `Fl::lock()` opens the
/// display through AppKit, which aborts off the main thread, where libtest
/// runs unit tests. Called by the `harness = false` test binary
/// `tests/fltk_main_thread.rs` (T0089); not part of the public API.
#[doc(hidden)]
pub mod main_thread_checks {
    use super::*;

    /// How many `DirtyChanged` messages for `ids` are waiting. fltk-rs
    /// channels share one global queue: drain it and filter by this test's
    /// own ids. This check is the ONLY drainer of the channel (main.rs is the
    /// only other consumer): extend it rather than adding another drainer.
    fn dirty_flips(receiver: &fltk::app::Receiver<Message>, ids: &[DocumentId]) -> usize {
        let mut n = 0;
        while let Some(msg) = receiver.recv() {
            if matches!(msg, Message::DirtyChanged(m) if ids.contains(&m)) {
                n += 1;
            }
        }
        n
    }

    /// T0093: the first edit of a clean document reports the clean → dirty
    /// flip once, so the title and the tab bar can show it; later edits do
    /// not; after a save (mark_clean) and the handler run, the next edit does
    /// again. N documents flipping in one step (a tab-size change touches
    /// every buffer) queue ONE refresh, i.e. one tab-bar rebuild, not N.
    pub fn dirty_flip_is_reported_once() {
        let (sender, receiver) = fltk::app::channel::<Message>();
        dirty_refresh_handled();
        let id = DocumentId(930_093);
        let mut doc = Document::new_untitled(id, 1, sender);
        assert!(!doc.is_dirty());

        doc.buffer.insert(0, "x");
        assert!(doc.is_dirty());
        assert_eq!(dirty_flips(&receiver, &[id]), 1, "first edit");
        dirty_refresh_handled();

        doc.buffer.insert(1, "y");
        doc.buffer.remove(0, 1);
        assert_eq!(dirty_flips(&receiver, &[id]), 0, "already dirty");

        doc.mark_clean();
        doc.buffer.insert(0, "z");
        assert_eq!(dirty_flips(&receiver, &[id]), 1, "dirty again after save");
        dirty_refresh_handled();
        doc.cleanup();

        // Ten clean documents flipping before the handler runs: one message.
        let ids: Vec<DocumentId> = (0..10).map(|i| DocumentId(930_100 + i)).collect();
        let mut docs: Vec<Document> = ids
            .iter()
            .map(|&id| Document::new_untitled(id, 1, sender))
            .collect();
        for doc in &mut docs {
            doc.buffer.insert(0, "t");
        }
        assert!(docs.iter().all(Document::is_dirty));
        assert_eq!(dirty_flips(&receiver, &ids), 1, "coalesced");
        // Once handled, the next flip queues a new refresh.
        dirty_refresh_handled();
        docs[3].mark_clean();
        docs[3].buffer.insert(0, "u");
        assert_eq!(dirty_flips(&receiver, &ids), 1, "next refresh");

        // The main loop takes the message off the queue but never reaches
        // the refresh handler (e.g. it is skipped with 0 tabs): the next
        // flip must still send, or "*" is gone for the rest of the process.
        note_dequeued(&Message::DirtyChanged(ids[3]));
        docs[4].mark_clean();
        docs[4].buffer.insert(0, "w");
        assert_eq!(
            dirty_flips(&receiver, &ids),
            1,
            "after an unhandled dequeue"
        );
        for doc in &mut docs {
            doc.cleanup();
        }
    }
}
