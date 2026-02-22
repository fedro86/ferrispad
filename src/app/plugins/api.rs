//! Editor API exposed to Lua plugins.
//!
//! This module provides a read-only API for plugins to access
//! document information and perform logging.

use mlua::{Lua, Result as LuaResult, UserData, UserDataMethods};

/// Editor state passed to plugin hooks.
/// This is a snapshot of the current document state.
#[derive(Debug, Clone, Default)]
pub struct EditorApi {
    /// Current document content (for hooks that need it)
    pub text: Option<String>,

    /// Current file path (None for untitled documents)
    pub file_path: Option<String>,

    /// Detected language/syntax name
    pub language: Option<String>,

    /// Whether the document has unsaved changes
    pub is_dirty: bool,

    /// Cursor position in the buffer
    pub cursor_position: i32,

    /// Selected text (if any)
    pub selection: Option<String>,
}


impl EditorApi {
    /// Create an EditorApi with just a file path (for open/close hooks)
    pub fn with_path(path: Option<String>) -> Self {
        Self {
            file_path: path,
            ..Default::default()
        }
    }

    /// Create an EditorApi with content for save hooks
    pub fn with_content(path: String, content: String) -> Self {
        Self {
            text: Some(content),
            file_path: Some(path),
            ..Default::default()
        }
    }

    /// Create an EditorApi for text change hooks
    pub fn for_text_change(
        position: i32,
        _inserted_len: i32,
        _deleted_len: i32,
        path: Option<String>,
    ) -> Self {
        Self {
            file_path: path,
            cursor_position: position,
            ..Default::default()
        }
    }
}

impl UserData for EditorApi {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Get full document text
        methods.add_method("get_text", |_, this, ()| Ok(this.text.clone()));

        // Get current file path
        methods.add_method("get_file_path", |_, this, ()| Ok(this.file_path.clone()));

        // Get detected language
        methods.add_method("get_language", |_, this, ()| Ok(this.language.clone()));

        // Check if document has unsaved changes
        methods.add_method("is_dirty", |_, this, ()| Ok(this.is_dirty));

        // Get cursor position
        methods.add_method("get_cursor_position", |_, this, ()| {
            Ok(this.cursor_position)
        });

        // Get selected text
        methods.add_method("get_selection", |_, this, ()| Ok(this.selection.clone()));

        // Log a message to stderr (for debugging)
        // Uses add_method so it can be called as api:log("msg") in Lua
        methods.add_method("log", |_, _this, msg: String| {
            eprintln!("[plugin] {}", msg);
            Ok(())
        });
    }
}

/// Register the EditorApi type with a Lua instance
#[allow(dead_code)]  // Reserved for future plugin API expansion
pub fn register_api(lua: &Lua) -> LuaResult<()> {
    // EditorApi is registered automatically when passed to Lua functions
    // This function is here for future expansion if we need to register
    // additional global functions or types
    let _ = lua;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_api_default() {
        let api = EditorApi::default();
        assert!(api.text.is_none());
        assert!(api.file_path.is_none());
        assert!(!api.is_dirty);
        assert_eq!(api.cursor_position, 0);
    }

    #[test]
    fn test_editor_api_with_path() {
        let api = EditorApi::with_path(Some("/test/file.txt".to_string()));
        assert_eq!(api.file_path, Some("/test/file.txt".to_string()));
    }

    #[test]
    fn test_editor_api_with_content() {
        let api = EditorApi::with_content("/test/file.txt".to_string(), "hello world".to_string());
        assert_eq!(api.file_path, Some("/test/file.txt".to_string()));
        assert_eq!(api.text, Some("hello world".to_string()));
    }
}
