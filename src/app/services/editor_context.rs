//! Editor context file writer for AI agent integration.
//!
//! Writes `~/.config/ferrispad/editor-context.txt` with the current selection
//! so external tools (like Claude Code hooks) can consume it.
//! Only writes to disk when selection state or file path actually changes.

use std::fs;
use std::path::{Path, PathBuf};

use fltk::{prelude::*, text::TextEditor};

use crate::app::infrastructure::buffer::selection_text_no_leak;

/// Write the context file owner-only: it holds the user's selected text.
///
/// On Unix a new file is created 0600, and an existing one (e.g. left by an
/// older build at the default umask) is tightened to 0600 before any content
/// is written.
fn write_context_file(path: &Path, content: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(content.as_bytes())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, content)
    }
}

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
        Self::with_path(dirs::config_dir().map(|d| d.join("ferrispad").join("editor-context.txt")))
    }

    /// Writer for an explicit context-file path (`None` disables writing).
    fn with_path(path: Option<PathBuf>) -> Self {
        Self {
            path,
            last_key: None,
        }
    }

    /// Update the context file if selection or file path changed.
    pub fn update(&mut self, editor: &TextEditor, file_path: Option<&str>) {
        let Some(ref path) = self.path else { return };

        let Some(buf) = editor.buffer() else { return };
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

        if write_context_file(path, &content).is_ok() {
            // Only update tracking after successful write
            self.last_key = Some(key);
        }
    }

    /// Remove the context file. Called at the end of `main`, and again by
    /// `Drop` so a panic unwinding out of the event loop removes it too.
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

impl Drop for EditorContextWriter {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- T0038: the context file holds the user's selection; keep it private ---

    // `update` wrote it with `fs::write`, i.e. at the process umask (usually
    // 0644, world-readable). A fresh file must be owner-only.
    #[cfg(unix)]
    #[test]
    fn context_file_is_created_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("editor-context.txt");
        write_context_file(&path, "secret selection").expect("write");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "context file mode is {mode:o}");
    }

    // A file left by an older build (or created at 0644 by anything else) must
    // be tightened on the next write, not just rewritten in place.
    #[cfg(unix)]
    #[test]
    fn existing_world_readable_context_file_is_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("editor-context.txt");
        fs::write(&path, "old selection").expect("seed");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
        write_context_file(&path, "new").expect("write");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "context file mode is {mode:o}");
        assert_eq!(fs::read_to_string(&path).expect("read"), "new");
    }

    // `cleanup` ran only at the end of `main`, so a panic in the event loop
    // (which unwinds past it) left the last selection on disk. Dropping the
    // writer — which unwinding does — must remove the file.
    #[test]
    fn dropping_the_writer_removes_the_context_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("editor-context.txt");
        fs::write(&path, "selection").expect("seed");
        drop(EditorContextWriter::with_path(Some(path.clone())));
        assert!(
            !path.exists(),
            "context file survived the writer being dropped"
        );
    }
}
