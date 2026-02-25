//! Editor API exposed to Lua plugins.
//!
//! This module provides a read-only API for plugins to access
//! document information and perform logging.
//!
//! Also provides controlled access to external commands for linting.
//!
//! ## Security
//!
//! All file system operations are sandboxed to the project root directory.
//! Path traversal attacks (e.g., `../../etc/passwd`) are blocked.

use mlua::{Lua, Result as LuaResult, UserData, UserDataMethods};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use super::security::{
    find_project_root, validate_command_arg, validate_path, PathValidation,
    DEFAULT_COMMAND_TIMEOUT,
};

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

    /// Project root directory for sandbox validation.
    /// File system operations are restricted to this directory.
    pub project_root: Option<PathBuf>,

    /// Plugin name (for permission checking and logging)
    pub plugin_name: Option<String>,

    /// Commands this plugin is allowed to execute.
    /// Empty means no commands are allowed (strict mode).
    pub allowed_commands: Vec<String>,

    /// Plugin-specific configuration from user settings.
    /// Key-value pairs configured via Plugins > {Plugin} > Settings.
    pub config: HashMap<String, String>,
}


impl EditorApi {
    /// Compute project root from a file path
    fn compute_project_root(path: Option<&str>) -> Option<PathBuf> {
        path.and_then(|p| find_project_root(std::path::Path::new(p)))
    }

    /// Create an EditorApi with just a file path (for open/close hooks)
    pub fn with_path(path: Option<String>) -> Self {
        let project_root = Self::compute_project_root(path.as_deref());
        Self {
            file_path: path,
            project_root,
            ..Default::default()
        }
    }

    /// Create an EditorApi with content for save hooks
    pub fn with_content(path: String, content: String) -> Self {
        let project_root = Self::compute_project_root(Some(&path));
        Self {
            text: Some(content),
            file_path: Some(path),
            project_root,
            ..Default::default()
        }
    }

    /// Create an EditorApi with optional path and content for highlight request hooks
    pub fn with_path_and_content(path: Option<String>, content: String) -> Self {
        let project_root = Self::compute_project_root(path.as_deref());
        Self {
            text: Some(content),
            file_path: path,
            project_root,
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
        let project_root = Self::compute_project_root(path.as_deref());
        Self {
            file_path: path,
            cursor_position: position,
            project_root,
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
        //
        // Security:
        // - Command must be in the plugin's approved commands list (from manifest)
        // - Arguments are validated to prevent shell injection
        // - Command runs with a timeout (30 seconds by default)
        // - Working directory is set to project root if available
        methods.add_method("run_command", |lua, this, args: mlua::Variadic<String>| {
            use std::io::Read;
            use std::process::Stdio;
            use std::time::Instant;

            let args: Vec<String> = args.into_iter().collect();
            if args.is_empty() {
                return Err(mlua::Error::RuntimeError(
                    "run_command requires at least one argument (the command)".to_string(),
                ));
            }

            let cmd = &args[0];
            let cmd_args = &args[1..];

            // Security: Check if command is in approved list
            // Compare against basename so "/path/to/venv/bin/ruff" matches "ruff"
            // If allowed_commands is empty, no commands are permitted (strict mode)
            let plugin_name = this.plugin_name.as_deref().unwrap_or("unknown");
            let cmd_basename = std::path::Path::new(cmd)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(cmd);

            if !this.allowed_commands.iter().any(|c| c == cmd_basename || c == cmd) {
                if this.allowed_commands.is_empty() {
                    eprintln!(
                        "[plugin:security] {} tried to run '{}' but has no approved commands. \
                        Add [permissions] execute = [\"{}\"] to plugin.toml",
                        plugin_name, cmd, cmd_basename
                    );
                    return Err(mlua::Error::RuntimeError(format!(
                        "No permissions. Add to plugin.toml: [permissions] execute = [\"{}\"]",
                        cmd_basename
                    )));
                } else {
                    eprintln!(
                        "[plugin:security] {} tried to run '{}' which is not in approved list: {:?}",
                        plugin_name, cmd, this.allowed_commands
                    );
                    return Err(mlua::Error::RuntimeError(format!(
                        "Command '{}' not approved. Allowed: {:?}",
                        cmd_basename, this.allowed_commands
                    )));
                }
            }

            // Security: Validate command name (no shell injection in command itself)
            if let Err(reason) = validate_command_arg(cmd) {
                eprintln!("[plugin:security] run_command blocked command '{}': {}", cmd, reason);
                return Err(mlua::Error::RuntimeError(format!(
                    "Invalid command: {}",
                    reason
                )));
            }

            // Security: Validate all arguments for shell injection
            for (i, arg) in cmd_args.iter().enumerate() {
                if let Err(reason) = validate_command_arg(arg) {
                    eprintln!(
                        "[plugin:security] run_command blocked argument {}: '{}' - {}",
                        i, arg, reason
                    );
                    return Err(mlua::Error::RuntimeError(format!(
                        "Invalid argument {}: {}",
                        i, reason
                    )));
                }
            }

            // Build command with pipes and optional working directory
            let mut command = Command::new(cmd);
            command
                .args(cmd_args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            // Set working directory to project root if available
            if let Some(ref project_root) = this.project_root {
                command.current_dir(project_root);
            }

            // Spawn process
            match command.spawn() {
                Ok(mut child) => {
                    let start = Instant::now();
                    let timeout = DEFAULT_COMMAND_TIMEOUT;

                    // Poll until complete or timeout
                    loop {
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                // Process completed - read output from pipes
                                let mut stdout_str = String::new();
                                let mut stderr_str = String::new();

                                if let Some(mut stdout) = child.stdout.take() {
                                    let _ = stdout.read_to_string(&mut stdout_str);
                                }
                                if let Some(mut stderr) = child.stderr.take() {
                                    let _ = stderr.read_to_string(&mut stderr_str);
                                }

                                let result = lua.create_table()?;
                                result.set("stdout", stdout_str)?;
                                result.set("stderr", stderr_str)?;
                                result.set("success", status.success())?;
                                return Ok(mlua::Value::Table(result));
                            }
                            Ok(None) => {
                                // Still running - check timeout
                                if start.elapsed() > timeout {
                                    // Timeout - kill the process
                                    let _ = child.kill();
                                    let _ = child.wait();
                                    eprintln!(
                                        "[plugin:security] run_command killed '{}' after {:?} timeout",
                                        cmd, timeout
                                    );
                                    let result = lua.create_table()?;
                                    result.set("stdout", "")?;
                                    result.set("stderr", format!(
                                        "Command timed out after {} seconds",
                                        timeout.as_secs()
                                    ))?;
                                    result.set("success", false)?;
                                    return Ok(mlua::Value::Table(result));
                                }
                                // Sleep briefly before polling again (10ms)
                                std::thread::sleep(std::time::Duration::from_millis(10));
                            }
                            Err(e) => {
                                let result = lua.create_table()?;
                                result.set("stdout", "")?;
                                result.set("stderr", format!("Command wait failed: {}", e))?;
                                result.set("success", false)?;
                                return Ok(mlua::Value::Table(result));
                            }
                        }
                    }
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
        // Returns true if the file exists AND is within project root, false otherwise
        // Useful for checking venv executables like "./venv/bin/ruff"
        //
        // Security: Path is validated against project root to prevent traversal attacks.
        // Paths outside project root return false (not an error, for backwards compatibility).
        methods.add_method("file_exists", |_, this, path: String| {
            let Some(ref project_root) = this.project_root else {
                // No project root - allow any path (untitled document)
                // This is less restrictive but necessary for some use cases
                return Ok(std::path::Path::new(&path).exists());
            };

            match validate_path(&path, project_root) {
                PathValidation::Valid(canonical) => Ok(canonical.exists()),
                PathValidation::NotFound => Ok(false),
                // For security, paths outside project root return false, not an error
                // This prevents plugins from probing the file system
                PathValidation::OutsideProjectRoot
                | PathValidation::TraversalAttempt
                | PathValidation::InvalidPath(_) => {
                    eprintln!(
                        "[plugin:security] file_exists blocked: '{}' outside project root",
                        path
                    );
                    Ok(false)
                }
            }
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

        // Get the project root directory
        // Returns nil for untitled documents or if no project markers found
        // Project markers: .git, Cargo.toml, package.json, pyproject.toml, etc.
        methods.add_method("get_project_root", |_, this, ()| {
            Ok(this
                .project_root
                .as_ref()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string()))
        });

        // Get a plugin configuration value as string
        // Returns nil if key not set
        methods.add_method("get_config", |_, this, key: String| {
            Ok(this.config.get(&key).cloned())
        });

        // Get a plugin configuration value as number
        // Returns nil if key not set or not a valid number
        methods.add_method("get_config_number", |_, this, key: String| {
            Ok(this.config.get(&key).and_then(|v| v.parse::<f64>().ok()))
        });

        // Get a plugin configuration value as boolean
        // Returns false if key not set
        // "true" (case-insensitive) returns true, anything else returns false
        methods.add_method("get_config_bool", |_, this, key: String| {
            Ok(this
                .config
                .get(&key)
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false))
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
