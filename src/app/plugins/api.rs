//! Editor API exposed to Lua plugins.
//!
//! This module provides a read-only API for plugins to access
//! document information and perform logging.
//!
//! Also provides controlled access to external commands for linting.

use mlua::{Lua, Result as LuaResult, UserData, UserDataMethods};
use std::process::Command;

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

    /// Create an EditorApi with optional path and content for highlight request hooks
    pub fn with_path_and_content(path: Option<String>, content: String) -> Self {
        Self {
            text: Some(content),
            file_path: path,
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

        // Get a specific line by number (1-indexed)
        // Returns nil if line doesn't exist
        methods.add_method("get_line", |_, this, line_num: i32| {
            if line_num < 1 {
                return Ok(None);
            }
            let Some(ref text) = this.text else {
                return Ok(None);
            };
            // Get the line at 1-indexed position
            let line = text.lines().nth((line_num - 1) as usize);
            Ok(line.map(|s| s.to_string()))
        });

        // Get the file extension (without the dot)
        // Returns nil for files without extension or untitled documents
        methods.add_method("get_file_extension", |_, this, ()| {
            let Some(ref path) = this.file_path else {
                return Ok(None);
            };
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string());
            Ok(ext)
        });

        // Log a message to stderr (for debugging)
        // Uses add_method so it can be called as api:log("msg") in Lua
        methods.add_method("log", |_, _this, msg: String| {
            eprintln!("[plugin] {}", msg);
            Ok(())
        });

        // Run an external command and return its output
        // Returns: { stdout = "...", stderr = "...", success = true/false }
        // This allows plugins to run linters like ruff, mypy, etc.
        methods.add_method("run_command", |lua, _this, args: mlua::Variadic<String>| {
            let args: Vec<String> = args.into_iter().collect();
            if args.is_empty() {
                return Err(mlua::Error::RuntimeError(
                    "run_command requires at least one argument (the command)".to_string(),
                ));
            }

            let cmd = &args[0];
            let cmd_args = &args[1..];

            match Command::new(cmd).args(cmd_args).output() {
                Ok(output) => {
                    let result = lua.create_table()?;
                    result.set("stdout", String::from_utf8_lossy(&output.stdout).to_string())?;
                    result.set("stderr", String::from_utf8_lossy(&output.stderr).to_string())?;
                    result.set("success", output.status.success())?;
                    Ok(mlua::Value::Table(result))
                }
                Err(e) => {
                    // Command not found or failed to execute
                    let result = lua.create_table()?;
                    result.set("stdout", "")?;
                    result.set("stderr", format!("Command failed: {}", e))?;
                    result.set("success", false)?;
                    Ok(mlua::Value::Table(result))
                }
            }
        });

        // Check if a command exists in PATH
        // Returns true if the command is found, false otherwise
        methods.add_method("command_exists", |_, _this, cmd: String| {
            // Use `which` on Unix or `where` on Windows
            #[cfg(unix)]
            let check = Command::new("which").arg(&cmd).output();
            #[cfg(windows)]
            let check = Command::new("where").arg(&cmd).output();

            match check {
                Ok(output) => Ok(output.status.success()),
                Err(_) => Ok(false),
            }
        });

        // Check if a file exists at the given path
        // Returns true if the file exists, false otherwise
        // Useful for checking venv executables like "./venv/bin/ruff"
        methods.add_method("file_exists", |_, _this, path: String| {
            Ok(std::path::Path::new(&path).exists())
        });

        // Get the directory containing the current file
        // Returns nil for untitled documents
        // Useful for finding project root or venv directories
        methods.add_method("get_file_dir", |_, this, ()| {
            let Some(ref path) = this.file_path else {
                return Ok(None);
            };
            let dir = std::path::Path::new(path)
                .parent()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string());
            Ok(dir)
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
