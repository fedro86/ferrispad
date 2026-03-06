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

use mlua::{UserData, UserDataMethods};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::security::{
    find_project_root, validate_command_arg, validate_path, PathValidation,
    DEFAULT_COMMAND_TIMEOUT,
};

/// Resolve a user-supplied path against project_root and validate it stays inside the sandbox.
/// Returns `Ok(Some(canonical))` on success, `Ok(None)` if the path is blocked or invalid.
fn resolve_and_validate(path: &str, project_root: &Path) -> mlua::Result<Option<PathBuf>> {
    match validate_path(path, project_root) {
        PathValidation::Valid(canonical) => Ok(Some(canonical)),
        PathValidation::NotFound => {
            // For write ops the target doesn't exist yet — still valid if it
            // would land inside the sandbox. Walk up to the nearest existing
            // ancestor and verify it's within the project root.
            let full = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                project_root.join(path)
            };
            let canonical_root = std::fs::canonicalize(project_root).map_err(|e| {
                mlua::Error::RuntimeError(format!(
                    "Cannot canonicalize project root: {}",
                    e
                ))
            })?;
            // Walk up ancestors until we find one that exists and can be canonicalized
            let mut ancestor = full.as_path();
            loop {
                match std::fs::canonicalize(ancestor) {
                    Ok(canonical_ancestor)
                        if canonical_ancestor.starts_with(&canonical_root) =>
                    {
                        return Ok(Some(full));
                    }
                    Ok(_) => {
                        // Exists but outside project root
                        eprintln!(
                            "[plugin:security] path blocked: '{}' resolves outside project root",
                            path
                        );
                        return Ok(None);
                    }
                    Err(_) => {
                        // Doesn't exist — try parent
                        match ancestor.parent() {
                            Some(parent) if parent != ancestor => ancestor = parent,
                            _ => {
                                // Reached filesystem root without finding an existing ancestor
                                eprintln!(
                                    "[plugin:security] path blocked: '{}' no valid ancestor in project root",
                                    path
                                );
                                return Ok(None);
                            }
                        }
                    }
                }
            }
        }
        PathValidation::OutsideProjectRoot
        | PathValidation::InvalidPath(_) => {
            eprintln!(
                "[plugin:security] path blocked: '{}' outside project root",
                path
            );
            Ok(None)
        }
    }
}

/// Recursively scan a directory, collecting entries up to `max_depth`.
/// Paths are returned with `/` separators on all platforms.
fn scan_dir_recursive(
    root: &Path,
    current: &Path,
    max_depth: u32,
    current_depth: u32,
    results: &mut Vec<(String, String, bool)>, // (name, rel_path, is_dir)
) {
    if current_depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    sorted.sort_by_key(|e| e.file_name());

    for entry in sorted {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let is_dir = path.is_dir();

        // Build relative path with forward slashes
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        results.push((name, rel, is_dir));

        if is_dir {
            scan_dir_recursive(root, &path, max_depth, current_depth + 1, results);
        }
    }
}

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

                    // Take stdout/stderr handles BEFORE the poll loop.
                    // Drain them in background threads so the child never blocks
                    // on a full pipe buffer (classic deadlock: child blocks writing
                    // to a full pipe, parent waits for child to exit before reading).
                    let stdout_handle = child.stdout.take();
                    let stderr_handle = child.stderr.take();

                    let stdout_thread = std::thread::spawn(move || {
                        let mut s = String::new();
                        if let Some(mut out) = stdout_handle {
                            let _ = out.read_to_string(&mut s);
                        }
                        s
                    });

                    let stderr_thread = std::thread::spawn(move || {
                        let mut s = String::new();
                        if let Some(mut err) = stderr_handle {
                            let _ = err.read_to_string(&mut s);
                        }
                        s
                    });

                    // Wait for process using a channel instead of polling.
                    // The child is moved into a thread that blocks on wait();
                    // we extract the PID first so we can kill on timeout.
                    let pid = child.id();
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(child.wait());
                    });

                    let remaining = timeout.saturating_sub(start.elapsed());
                    match rx.recv_timeout(remaining) {
                        Ok(Ok(status)) => {
                            let stdout_str = stdout_thread.join().unwrap_or_default();
                            let stderr_str = stderr_thread.join().unwrap_or_default();

                            let result = lua.create_table()?;
                            result.set("stdout", stdout_str)?;
                            result.set("stderr", stderr_str)?;
                            result.set("success", status.success())?;
                            Ok(mlua::Value::Table(result))
                        }
                        Ok(Err(e)) => {
                            let result = lua.create_table()?;
                            result.set("stdout", "")?;
                            result.set("stderr", format!("Command wait failed: {}", e))?;
                            result.set("success", false)?;
                            Ok(mlua::Value::Table(result))
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            // Kill the timed-out process by PID
                            #[cfg(unix)]
                            unsafe {
                                unsafe extern "C" {
                                    fn kill(pid: i32, sig: i32) -> i32;
                                }
                                kill(pid as i32, 9); // SIGKILL
                            }
                            #[cfg(windows)]
                            {
                                // On Windows, use taskkill as a fallback
                                let _ = Command::new("taskkill")
                                    .args(["/F", "/PID", &pid.to_string()])
                                    .output();
                            }
                            // Wait for the thread to finish (it will see the killed status)
                            let _ = rx.recv();
                            eprintln!(
                                "[plugin:security] run_command killed '{}' (pid {}) after {:?} timeout",
                                cmd, pid, timeout
                            );
                            let result = lua.create_table()?;
                            result.set("stdout", "")?;
                            result.set("stderr", format!(
                                "Command timed out after {} seconds",
                                timeout.as_secs()
                            ))?;
                            result.set("success", false)?;
                            Ok(mlua::Value::Table(result))
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            let result = lua.create_table()?;
                            result.set("stdout", "")?;
                            result.set("stderr", "Command wait thread disconnected".to_string())?;
                            result.set("success", false)?;
                            Ok(mlua::Value::Table(result))
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
            // Security: validate command name for consistency with run_command
            if let Err(reason) = validate_command_arg(&cmd) {
                eprintln!(
                    "[plugin:security] command_exists blocked '{}': {}",
                    cmd, reason
                );
                return Ok(false);
            }

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
                // No project root (untitled document) — deny to prevent filesystem probing
                return Ok(false);
            };

            match validate_path(&path, project_root) {
                PathValidation::Valid(canonical) => Ok(canonical.exists()),
                PathValidation::NotFound => Ok(false),
                // For security, paths outside project root return false, not an error
                // This prevents plugins from probing the file system
                PathValidation::OutsideProjectRoot
                | PathValidation::InvalidPath(_) => {
                    eprintln!(
                        "[plugin:security] file_exists blocked: '{}' outside project root",
                        path
                    );
                    Ok(false)
                }
            }
        });

        // Read the contents of a file inside the project root.
        // Returns (content, nil) on success, (nil, error_msg) on failure.
        // Returns (nil, "No project root") when there is no project root.
        // Paths outside the project root are blocked.
        methods.add_method("read_file", |lua, this, path: String| {
            let Some(ref project_root) = this.project_root else {
                return Ok((mlua::Value::Nil, Some("No project root".to_string())));
            };

            match validate_path(&path, project_root) {
                PathValidation::Valid(canonical) => {
                    match std::fs::read_to_string(&canonical) {
                        Ok(content) => {
                            let lua_str = lua.create_string(&content)?;
                            Ok((mlua::Value::String(lua_str), None))
                        }
                        Err(e) => Ok((mlua::Value::Nil, Some(e.to_string()))),
                    }
                }
                _ => {
                    eprintln!(
                        "[plugin:security] read_file blocked: '{}' outside project root",
                        path
                    );
                    Ok((mlua::Value::Nil, Some("Path outside project root".to_string())))
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

        // ── Git Status API ────────────────────────────────────────────────
        // Controlled read-only access to git status. No execute permission needed.

        // Query git status for a directory. Returns a table mapping relative file
        // paths to status codes ("M", "A", "??", "D", "R", "UU"), or nil on error.
        methods.add_method("git_status", |lua, this, path: String| {
            // Security: validate path against project root to prevent probing arbitrary directories
            let validated_path = if let Some(ref project_root) = this.project_root {
                match validate_path(&path, project_root) {
                    PathValidation::Valid(canonical) => canonical,
                    _ => {
                        eprintln!(
                            "[plugin:security] git_status blocked: '{}' outside project root",
                            path
                        );
                        return Ok(mlua::Value::Nil);
                    }
                }
            } else {
                return Ok(mlua::Value::Nil);
            };
            let path_str = validated_path.to_string_lossy();

            let output = match Command::new("git")
                .args(["-C", &path_str, "status", "--porcelain=v1", "-uall"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
            {
                Ok(o) if o.status.success() => o,
                _ => return Ok(mlua::Value::Nil),
            };

            let stdout = match String::from_utf8(output.stdout) {
                Ok(s) => s,
                Err(_) => return Ok(mlua::Value::Nil),
            };

            let result = lua.create_table()?;
            for line in stdout.lines() {
                if line.len() < 4 {
                    continue;
                }
                // Porcelain v1 format: XY <space> <path>
                let status = line[..2].trim().to_string();
                let file_path = &line[3..];
                // Handle renames: "R  old -> new" — use the new path
                let effective_path = if let Some(arrow_pos) = file_path.find(" -> ") {
                    &file_path[arrow_pos + 4..]
                } else {
                    file_path
                };
                result.set(effective_path.to_string(), status)?;
            }

            Ok(mlua::Value::Table(result))
        });

        // ── Diff API ──────────────────────────────────────────────────────
        // Pure computation — no file/command access, no security concerns.

        // Compute an aligned diff between old_text and new_text.
        // Returns: { left_content, right_content, left_highlights, right_highlights }
        // Highlights include intraline `spans` for character-level emphasis.
        methods.add_method("diff_text", |lua, _this, (old_text, new_text): (String, String)| {
            use super::diff::compute_aligned_diff;

            let result = compute_aligned_diff(&old_text, &new_text);

            let table = lua.create_table()?;
            table.set("left_content", result.left_content)?;
            table.set("right_content", result.right_content)?;

            // Convert left highlights
            let left_hl_table = lua.create_table()?;
            for (i, hl) in result.left_highlights.iter().enumerate() {
                let hl_table = lua.create_table()?;
                hl_table.set("line", hl.line)?;
                hl_table.set("color", hl.color)?;
                if !hl.spans.is_empty() {
                    let spans_table = lua.create_table()?;
                    for (j, span) in hl.spans.iter().enumerate() {
                        let span_table = lua.create_table()?;
                        span_table.set("start", span.start)?;
                        span_table.set("end", span.end)?;
                        spans_table.set(j + 1, span_table)?;
                    }
                    hl_table.set("spans", spans_table)?;
                }
                left_hl_table.set(i + 1, hl_table)?;
            }
            table.set("left_highlights", left_hl_table)?;

            // Convert right highlights
            let right_hl_table = lua.create_table()?;
            for (i, hl) in result.right_highlights.iter().enumerate() {
                let hl_table = lua.create_table()?;
                hl_table.set("line", hl.line)?;
                hl_table.set("color", hl.color)?;
                if !hl.spans.is_empty() {
                    let spans_table = lua.create_table()?;
                    for (j, span) in hl.spans.iter().enumerate() {
                        let span_table = lua.create_table()?;
                        span_table.set("start", span.start)?;
                        span_table.set("end", span.end)?;
                        spans_table.set(j + 1, span_table)?;
                    }
                    hl_table.set("spans", spans_table)?;
                }
                right_hl_table.set(i + 1, hl_table)?;
            }
            table.set("right_highlights", right_hl_table)?;

            Ok(mlua::Value::Table(table))
        });

        // ── Cross-platform filesystem API ───────────────────────────────
        // All paths are sandboxed to project_root via validate_path().

        // Check if a path is a regular file (not a directory)
        // Returns false for directories, non-existent paths, and paths outside project root
        methods.add_method("is_file", |_, this, path: String| {
            let Some(ref project_root) = this.project_root else {
                // No project root (untitled document) — deny to prevent filesystem probing
                return Ok(false);
            };
            match resolve_and_validate(&path, project_root)? {
                Some(p) => Ok(p.is_file()),
                None => Ok(false),
            }
        });

        // List entries in a single directory (non-recursive)
        // Returns array of { name = "...", is_dir = bool } or nil on failure
        methods.add_method("list_dir", |lua, this, path: String| {
            let Some(ref project_root) = this.project_root else {
                return Ok(mlua::Value::Nil);
            };
            let resolved = match resolve_and_validate(&path, project_root)? {
                Some(p) => p,
                None => return Ok(mlua::Value::Nil),
            };
            let entries = match std::fs::read_dir(&resolved) {
                Ok(e) => e,
                Err(_) => return Ok(mlua::Value::Nil),
            };

            let result = lua.create_table()?;
            let mut idx = 1;
            let mut sorted: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            sorted.sort_by_key(|e| e.file_name());

            for entry in sorted {
                let t = lua.create_table()?;
                t.set("name", entry.file_name().to_string_lossy().as_ref().to_owned())?;
                t.set("is_dir", entry.path().is_dir())?;
                result.set(idx, t)?;
                idx += 1;
            }
            Ok(mlua::Value::Table(result))
        });

        // Recursively scan a directory up to max_depth (default 5, cap 10)
        // Returns array of { name, rel_path, is_dir } or nil on failure
        methods.add_method("scan_dir", |lua, this, (path, max_depth): (String, Option<u32>)| {
            let Some(ref project_root) = this.project_root else {
                return Ok(mlua::Value::Nil);
            };
            let resolved = match resolve_and_validate(&path, project_root)? {
                Some(p) => p,
                None => return Ok(mlua::Value::Nil),
            };
            if !resolved.is_dir() {
                return Ok(mlua::Value::Nil);
            }

            let depth = max_depth.unwrap_or(5).min(10);
            let mut raw: Vec<(String, String, bool)> = Vec::new();
            scan_dir_recursive(&resolved, &resolved, depth, 1, &mut raw);

            let result = lua.create_table()?;
            for (i, (name, rel_path, is_dir)) in raw.iter().enumerate() {
                let t = lua.create_table()?;
                t.set("name", name.as_str())?;
                t.set("rel_path", rel_path.as_str())?;
                t.set("is_dir", *is_dir)?;
                result.set(i + 1, t)?;
            }
            Ok(mlua::Value::Table(result))
        });

        // Create a new empty file. Fails if the file already exists (no truncation).
        // Returns (true, "") on success, (false, error_msg) on failure.
        methods.add_method("create_file", |_, this, path: String| {
            let Some(ref project_root) = this.project_root else {
                return Ok((false, "No project root".to_string()));
            };
            let resolved = match resolve_and_validate(&path, project_root)? {
                Some(p) => p,
                None => return Ok((false, "Path outside project root".to_string())),
            };
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&resolved)
            {
                Ok(_) => Ok((true, String::new())),
                Err(e) => Ok((false, e.to_string())),
            }
        });

        // Create a directory (and all missing parents).
        // Returns (true, "") on success, (false, error_msg) on failure.
        methods.add_method("create_dir", |_, this, path: String| {
            let Some(ref project_root) = this.project_root else {
                return Ok((false, "No project root".to_string()));
            };
            let resolved = match resolve_and_validate(&path, project_root)? {
                Some(p) => p,
                None => return Ok((false, "Path outside project root".to_string())),
            };
            match std::fs::create_dir_all(&resolved) {
                Ok(_) => Ok((true, String::new())),
                Err(e) => Ok((false, e.to_string())),
            }
        });

        // Rename (move) a file or directory. Both paths must be inside project root.
        // Returns (true, "") on success, (false, error_msg) on failure.
        methods.add_method("rename", |_, this, (old, new): (String, String)| {
            let Some(ref project_root) = this.project_root else {
                return Ok((false, "No project root".to_string()));
            };
            let old_resolved = match resolve_and_validate(&old, project_root)? {
                Some(p) => p,
                None => return Ok((false, "Source path outside project root".to_string())),
            };
            let new_resolved = match resolve_and_validate(&new, project_root)? {
                Some(p) => p,
                None => return Ok((false, "Destination path outside project root".to_string())),
            };
            match std::fs::rename(&old_resolved, &new_resolved) {
                Ok(_) => Ok((true, String::new())),
                Err(e) => Ok((false, e.to_string())),
            }
        });

        // Remove a file or directory (recursive for directories).
        // Returns (true, "") on success, (false, error_msg) on failure.
        methods.add_method("remove", |_, this, path: String| {
            let Some(ref project_root) = this.project_root else {
                return Ok((false, "No project root".to_string()));
            };
            let resolved = match resolve_and_validate(&path, project_root)? {
                Some(p) => p,
                None => return Ok((false, "Path outside project root".to_string())),
            };
            let result = if resolved.is_dir() {
                std::fs::remove_dir_all(&resolved)
            } else if resolved.is_file() {
                std::fs::remove_file(&resolved)
            } else {
                return Ok((false, "Path does not exist".to_string()));
            };
            match result {
                Ok(_) => Ok((true, String::new())),
                Err(e) => Ok((false, e.to_string())),
            }
        });
    }
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

    // ── Filesystem API tests ────────────────────────────────────────

    use mlua::ObjectLike;
    use tempfile::tempdir;

    /// Helper: create an EditorApi with a temp dir as project root
    fn api_with_root(root: &Path) -> EditorApi {
        EditorApi {
            project_root: Some(root.to_path_buf()),
            ..Default::default()
        }
    }

    #[test]
    fn test_resolve_and_validate_inside() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("file.txt"), "hi").unwrap();

        let result = resolve_and_validate("file.txt", root).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_resolve_and_validate_outside_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let result = resolve_and_validate("/etc/passwd", root).unwrap();
        assert!(result.is_none(), "Path outside project root should be None");
    }

    #[test]
    fn test_scan_dir_recursive_depth() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // Create: root/a/b/c/d.txt (depth 3 dirs + 1 file)
        let deep = root.join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("d.txt"), "deep").unwrap();

        // max_depth=2 should NOT reach c/ or d.txt
        let mut results = Vec::new();
        scan_dir_recursive(root, root, 2, 1, &mut results);
        let paths: Vec<&str> = results.iter().map(|(_, r, _)| r.as_str()).collect();
        assert!(paths.contains(&"a"), "Should find a/");
        assert!(paths.contains(&"a/b"), "Should find a/b/");
        assert!(!paths.contains(&"a/b/c"), "Depth 2 should not reach a/b/c/");
        assert!(!paths.contains(&"a/b/c/d.txt"), "Depth 2 should not reach a/b/c/d.txt");
    }

    #[test]
    fn test_scan_dir_recursive_full() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub").join("hello.txt"), "hi").unwrap();
        std::fs::write(root.join("top.txt"), "top").unwrap();

        let mut results = Vec::new();
        scan_dir_recursive(root, root, 5, 1, &mut results);

        let names: Vec<&str> = results.iter().map(|(n, _, _)| n.as_str()).collect();
        assert!(names.contains(&"sub"));
        assert!(names.contains(&"hello.txt"));
        assert!(names.contains(&"top.txt"));

        // Check is_dir flag
        let sub_entry = results.iter().find(|(n, _, _)| n == "sub").unwrap();
        assert!(sub_entry.2, "sub should be a directory");
        let file_entry = results.iter().find(|(n, _, _)| n == "top.txt").unwrap();
        assert!(!file_entry.2, "top.txt should not be a directory");
    }

    // The following tests exercise the methods via mlua to confirm they work end-to-end.

    #[test]
    fn test_lua_is_file() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("exists.txt"), "data").unwrap();
        std::fs::create_dir(root.join("adir")).unwrap();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);
        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let is_file: mlua::Function = ud.get("is_file").unwrap();

            let yes: bool = is_file.call((&ud, root.join("exists.txt").to_str().unwrap().to_string())).unwrap();
            assert!(yes, "exists.txt should be a file");

            let no: bool = is_file.call((&ud, root.join("adir").to_str().unwrap().to_string())).unwrap();
            assert!(!no, "adir is a directory, not a file");

            let no2: bool = is_file.call((&ud, root.join("nope.txt").to_str().unwrap().to_string())).unwrap();
            assert!(!no2, "nope.txt does not exist");
            Ok(())
        }).unwrap();
    }

    #[test]
    fn test_lua_list_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.txt"), "").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);
        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let list_dir: mlua::Function = ud.get("list_dir").unwrap();

            let tbl: mlua::Table = list_dir.call((&ud, root.to_str().unwrap().to_string())).unwrap();
            let len = tbl.len().unwrap();
            assert!(len >= 2, "Should have at least 2 entries, got {}", len);
            Ok(())
        }).unwrap();
    }

    #[test]
    fn test_lua_create_file_and_remove() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);
        let file_path = root.join("new.txt");
        let file_str = file_path.to_str().unwrap().to_string();

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();

            // create_file
            let create: mlua::Function = ud.get("create_file").unwrap();
            let (ok, err): (bool, String) = create.call((&ud, file_str.clone())).unwrap();
            assert!(ok, "create_file should succeed: {}", err);
            assert!(file_path.exists());

            // create_file again should fail (create_new)
            let (ok2, _): (bool, String) = create.call((&ud, file_str.clone())).unwrap();
            assert!(!ok2, "create_file on existing file should fail");

            // remove
            let remove: mlua::Function = ud.get("remove").unwrap();
            let (ok3, err3): (bool, String) = remove.call((&ud, file_str.clone())).unwrap();
            assert!(ok3, "remove should succeed: {}", err3);
            assert!(!file_path.exists());
            Ok(())
        }).unwrap();
    }

    #[test]
    fn test_lua_create_dir_and_rename() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);
        let dir_path = root.join("nested").join("deep");
        let dir_str = dir_path.to_str().unwrap().to_string();
        let renamed = root.join("nested").join("renamed");
        let renamed_str = renamed.to_str().unwrap().to_string();

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();

            // create_dir (nested)
            let mkdir: mlua::Function = ud.get("create_dir").unwrap();
            let (ok, err): (bool, String) = mkdir.call((&ud, dir_str.clone())).unwrap();
            assert!(ok, "create_dir should succeed: {}", err);
            assert!(dir_path.is_dir());

            // rename
            let rename_fn: mlua::Function = ud.get("rename").unwrap();
            let (ok2, err2): (bool, String) = rename_fn.call((&ud, dir_str, renamed_str)).unwrap();
            assert!(ok2, "rename should succeed: {}", err2);
            assert!(renamed.is_dir());
            assert!(!dir_path.exists());
            Ok(())
        }).unwrap();
    }

    #[test]
    fn test_lua_remove_dir_recursive() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let sub = root.join("to_delete");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("child.txt"), "data").unwrap();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let remove: mlua::Function = ud.get("remove").unwrap();
            let (ok, err): (bool, String) = remove.call((&ud, sub.to_str().unwrap().to_string())).unwrap();
            assert!(ok, "remove dir should succeed: {}", err);
            assert!(!sub.exists());
            Ok(())
        }).unwrap();
    }

    #[test]
    fn test_lua_scan_dir() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src").join("main.rs"), "fn main(){}").unwrap();
        std::fs::write(root.join("README.md"), "# Hi").unwrap();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let scan: mlua::Function = ud.get("scan_dir").unwrap();
            let tbl: mlua::Table = scan.call((&ud, root.to_str().unwrap().to_string())).unwrap();

            let len = tbl.len().unwrap();
            assert!(len >= 3, "Should have src/, src/main.rs, README.md; got {}", len);
            Ok(())
        }).unwrap();
    }

    #[test]
    fn test_lua_blocked_outside_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();

            // create_file outside root
            let create: mlua::Function = ud.get("create_file").unwrap();
            let (ok, _): (bool, String) = create.call((&ud, "/tmp/should_not_exist_ferrispad_test.txt".to_string())).unwrap();
            assert!(!ok, "create_file outside root should fail");

            // remove outside root
            let remove: mlua::Function = ud.get("remove").unwrap();
            let (ok2, _): (bool, String) = remove.call((&ud, "/etc/passwd".to_string())).unwrap();
            assert!(!ok2, "remove outside root should fail");
            Ok(())
        }).unwrap();
    }

    // ── S1: resolve_and_validate NotFound bypass tests ────────────────

    #[test]
    fn test_resolve_and_validate_notfound_absolute_outside_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Absolute path where parent doesn't exist — should be blocked, not bypassed
        let result = resolve_and_validate("/nonexistent_dir_xyz/evil.txt", root).unwrap();
        assert!(
            result.is_none(),
            "Absolute path with nonexistent parent outside root should be None"
        );
    }

    #[test]
    fn test_resolve_and_validate_traversal_nonexistent_parent_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Traversal where intermediate dir doesn't exist
        let result = resolve_and_validate("../../nonexistent/file.txt", root).unwrap();
        assert!(
            result.is_none(),
            "Traversal through nonexistent parent should be blocked"
        );
    }

    #[test]
    fn test_resolve_and_validate_notfound_inside_allowed() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // Create a subdir so the parent exists inside the root
        std::fs::create_dir(root.join("subdir")).unwrap();

        // New file in existing subdir inside root — should be allowed
        let result = resolve_and_validate("subdir/new_file.txt", root).unwrap();
        assert!(
            result.is_some(),
            "New file in existing subdir inside root should be allowed"
        );
    }

    // ── S4: file_exists / is_file without project root ────────────────

    #[test]
    fn test_file_exists_no_project_root_returns_false() {
        let lua = mlua::Lua::new();
        let api = EditorApi::default(); // no project_root

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let file_exists: mlua::Function = ud.get("file_exists").unwrap();
            let result: bool = file_exists.call((&ud, "/etc/hosts".to_string())).unwrap();
            assert!(!result, "file_exists without project root should return false");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_is_file_no_project_root_returns_false() {
        let lua = mlua::Lua::new();
        let api = EditorApi::default(); // no project_root

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let is_file: mlua::Function = ud.get("is_file").unwrap();
            let result: bool = is_file.call((&ud, "/etc/hosts".to_string())).unwrap();
            assert!(!result, "is_file without project root should return false");
            Ok(())
        })
        .unwrap();
    }

    // ── S2: git_status path validation ────────────────────────────────

    #[test]
    fn test_lua_git_status_outside_root_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let git_status: mlua::Function = ud.get("git_status").unwrap();
            // Attempting to query git status on a path outside project root
            let result: mlua::Value = git_status.call((&ud, "/tmp".to_string())).unwrap();
            assert!(
                matches!(result, mlua::Value::Nil),
                "git_status outside project root should return nil"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_git_status_no_project_root_returns_nil() {
        let lua = mlua::Lua::new();
        let api = EditorApi::default(); // no project_root

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let git_status: mlua::Function = ud.get("git_status").unwrap();
            let result: mlua::Value = git_status.call((&ud, "/tmp".to_string())).unwrap();
            assert!(
                matches!(result, mlua::Value::Nil),
                "git_status without project root should return nil"
            );
            Ok(())
        })
        .unwrap();
    }

    // ── A4: read_file tests ─────────────────────────────────────────

    #[test]
    fn test_lua_read_file_success() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("hello.txt"), "Hello, world!").unwrap();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let read_file: mlua::Function = ud.get("read_file").unwrap();
            let (content, err): (String, mlua::Value) =
                read_file.call((&ud, root.join("hello.txt").to_str().unwrap().to_string())).unwrap();
            assert_eq!(content, "Hello, world!");
            assert!(matches!(err, mlua::Value::Nil), "error should be nil on success");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_read_file_outside_root_blocked() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let lua = mlua::Lua::new();
        let api = api_with_root(root);

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let read_file: mlua::Function = ud.get("read_file").unwrap();
            let (content, err): (mlua::Value, Option<String>) =
                read_file.call((&ud, "/etc/hosts".to_string())).unwrap();
            assert!(matches!(content, mlua::Value::Nil), "content should be nil for blocked path");
            assert!(err.is_some(), "should return an error message");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_lua_read_file_no_project_root() {
        let lua = mlua::Lua::new();
        let api = EditorApi::default(); // no project_root

        lua.scope(|scope| {
            let ud = scope.create_userdata(api).unwrap();
            let read_file: mlua::Function = ud.get("read_file").unwrap();
            let (content, err): (mlua::Value, Option<String>) =
                read_file.call((&ud, "/etc/hosts".to_string())).unwrap();
            assert!(matches!(content, mlua::Value::Nil), "content should be nil without project root");
            assert_eq!(err, Some("No project root".to_string()));
            Ok(())
        })
        .unwrap();
    }
}
