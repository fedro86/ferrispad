//! Editor context file writer for AI agent integration.
//!
//! Writes `~/.config/ferrispad/editor-context.txt` with the current selection
//! so external tools (like Claude Code hooks) can consume it.
//! Only writes to disk when selection state or file path actually changes.

use std::fs;
use std::path::PathBuf;

use fltk::{prelude::*, text::TextEditor};

use crate::app::infrastructure::buffer::selection_text_no_leak;

pub struct EditorContextWriter {
    path: Option<PathBuf>,
    /// (start, end, file_path_hash) of last written selection
    last_key: Option<(i32, i32, u64)>,
}

impl Default for EditorContextWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorContextWriter {
    pub fn new() -> Self {
        let path = dirs::config_dir().map(|d| d.join("ferrispad").join("editor-context.txt"));
        Self {
            path,
            last_key: None,
        }
    }

    /// Update the context file if selection or file path changed.
    pub fn update(&mut self, editor: &TextEditor, file_path: Option<&str>) {
        let Some(ref path) = self.path else { return };

        let buf = editor.buffer().unwrap();
        let current = buf.selection_position().filter(|(s, e)| s != e);

        let Some((start, end)) = current else {
            // No selection — keep the last content in the file so the hook
            // can still read it when the user clicks into the terminal.
            return;
        };

        // Include file path in change detection so tab switches are caught
        let path_hash = file_path.map(Self::hash_str).unwrap_or(0);
        let key = (start, end, path_hash);
        if self.last_key == Some(key) {
            return;
        }

        let sel_lines = buf.count_lines(start, end) + 1;
        let filename = file_path
            .map(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(p)
            })
            .unwrap_or("untitled");

        let selected_text = selection_text_no_leak(&buf);
        let full_path = file_path.unwrap_or("untitled");
        let content = format!(
            "The user has selected the following {} lines in their editor (file: {}).\n\
             This is the text they are referring to:\n\
             \n\
             File: {}\n\
             ```\n{}\n```\n",
            sel_lines, full_path, filename, selected_text
        );

        if fs::write(path, content).is_ok() {
            // Only update tracking after successful write
            self.last_key = Some(key);
        }
    }

    /// Clean up the context file on exit.
    pub fn cleanup(&self) {
        if let Some(ref path) = self.path {
            let _ = fs::remove_file(path);
        }
    }

    /// Simple FNV-1a hash for change detection (not cryptographic).
    fn hash_str(s: &str) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in s.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}
