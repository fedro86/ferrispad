//! Editor query methods exposed to Lua plugins.

use super::EditorApi;

/// Get full document text.
pub fn get_text(_: &mlua::Lua, this: &EditorApi, _: ()) -> mlua::Result<Option<String>> {
    Ok(this.text.clone())
}

/// Get current file path.
pub fn get_file_path(_: &mlua::Lua, this: &EditorApi, _: ()) -> mlua::Result<Option<String>> {
    Ok(this.file_path.clone())
}

/// Get detected language.
pub fn get_language(_: &mlua::Lua, this: &EditorApi, _: ()) -> mlua::Result<Option<String>> {
    Ok(this.language.clone())
}

/// Check if document has unsaved changes.
pub fn is_dirty(_: &mlua::Lua, this: &EditorApi, _: ()) -> mlua::Result<bool> {
    Ok(this.is_dirty)
}

/// Get cursor position.
pub fn get_cursor_position(_: &mlua::Lua, this: &EditorApi, _: ()) -> mlua::Result<i32> {
    Ok(this.cursor_position)
}

/// Get selected text.
pub fn get_selection(_: &mlua::Lua, this: &EditorApi, _: ()) -> mlua::Result<Option<String>> {
    Ok(this.selection.clone())
}

/// Get a specific line by number (1-indexed).
/// Returns nil if line doesn't exist.
pub fn get_line(_: &mlua::Lua, this: &EditorApi, line_num: i32) -> mlua::Result<Option<String>> {
    if line_num < 1 {
        return Ok(None);
    }
    let Some(ref text) = this.text else {
        return Ok(None);
    };
    let line = text.lines().nth((line_num - 1) as usize);
    Ok(line.map(|s| s.to_string()))
}

/// Get the file extension (without the dot).
/// Returns nil for files without extension or untitled documents.
pub fn get_file_extension(_: &mlua::Lua, this: &EditorApi, _: ()) -> mlua::Result<Option<String>> {
    let Some(ref path) = this.file_path else {
        return Ok(None);
    };
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_string());
    Ok(ext)
}

/// Log a message to stderr (for debugging).
pub fn log(_: &mlua::Lua, _this: &EditorApi, msg: String) -> mlua::Result<()> {
    eprintln!("[plugin] {}", msg);
    Ok(())
}

/// Get the directory containing the current file.
/// Returns nil for untitled documents.
pub fn get_file_dir(_: &mlua::Lua, this: &EditorApi, _: ()) -> mlua::Result<Option<String>> {
    let Some(ref path) = this.file_path else {
        return Ok(None);
    };
    let dir = std::path::Path::new(path)
        .parent()
        .and_then(|p| p.to_str())
        .map(|s| s.to_string());
    Ok(dir)
}

/// Get the project root directory.
/// Returns nil for untitled documents or if no project markers found.
pub fn get_project_root(_: &mlua::Lua, this: &EditorApi, _: ()) -> mlua::Result<Option<String>> {
    Ok(this
        .project_root
        .as_ref()
        .and_then(|p| p.to_str())
        .map(|s| s.to_string()))
}

/// Get a plugin configuration value as string.
/// Returns nil if key not set.
pub fn get_config(_: &mlua::Lua, this: &EditorApi, key: String) -> mlua::Result<Option<String>> {
    Ok(this.config.get(&key).cloned())
}

/// Get a plugin configuration value as number.
/// Returns nil if key not set or not a valid number.
pub fn get_config_number(
    _: &mlua::Lua,
    this: &EditorApi,
    key: String,
) -> mlua::Result<Option<f64>> {
    Ok(this.config.get(&key).and_then(|v| v.parse::<f64>().ok()))
}

/// Get a plugin configuration value as boolean.
/// Returns false if key not set.
pub fn get_config_bool(_: &mlua::Lua, this: &EditorApi, key: String) -> mlua::Result<bool> {
    Ok(this
        .config
        .get(&key)
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false))
}

/// Get the MCP server port (read from ~/.config/ferrispad/mcp-port).
/// Returns nil if the port file doesn't exist or is invalid.
pub fn get_mcp_port(_: &mlua::Lua, _this: &EditorApi, _: ()) -> mlua::Result<Option<u16>> {
    Ok(crate::app::mcp::port_file_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse().ok()))
}

/// Write `.mcp.json` to a project root directory.
/// Resolves the binary path and MCP port internally — never exposes them to Lua.
/// Also appends `.mcp.json` to `.gitignore` if not already present.
/// Skips if `.mcp.json` already exists (don't overwrite user customizations).
/// Returns `(true, "")` on success, `(false, error_msg)` on failure.
pub fn setup_mcp_config(
    _: &mlua::Lua,
    this: &EditorApi,
    root: String,
) -> mlua::Result<(bool, String)> {
    let Some(ref project_root) = this.project_root else {
        return Ok((false, "No project root".to_string()));
    };

    // Validate root path is inside the sandbox
    let resolved_root = match super::sandbox::resolve_and_validate(&root, project_root)? {
        Some(p) => p,
        None => return Ok((false, "Path outside project root".to_string())),
    };

    // Read MCP port
    let port = match crate::app::mcp::port_file_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse::<u16>().ok())
    {
        Some(p) => p,
        None => return Ok((false, "MCP port not available".to_string())),
    };
    // Suppress unused-variable warning — port is reserved for future
    // transport modes (e.g. SSE) but stdio mode doesn't need it.
    let _ = port;

    // Resolve binary path
    let binary = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => return Ok((false, format!("Cannot determine binary path: {}", e))),
    };

    let mcp_path = resolved_root.join(".mcp.json");

    // Don't overwrite existing .mcp.json (user may have customized it)
    if mcp_path.exists() {
        return Ok((true, String::new()));
    }

    // Escape backslashes and quotes for JSON string value
    let escaped = binary.replace('\\', "\\\\").replace('"', "\\\"");
    let config = format!(
        "{{\n  \"mcpServers\": {{\n    \"ferrispad\": {{\n      \"type\": \"stdio\",\n      \"command\": \"{}\",\n      \"args\": [\"--mcp-server\"]\n    }}\n  }}\n}}",
        escaped
    );

    if let Err(e) = std::fs::write(&mcp_path, &config) {
        return Ok((false, format!("Failed to write .mcp.json: {}", e)));
    }

    // Ensure .mcp.json is in .gitignore
    let gitignore_path = resolved_root.join(".gitignore");
    let content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    if !content.contains(".mcp.json") {
        let prefix = if !content.is_empty() && !content.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        let new_content = format!("{}{}.mcp.json\n", content, prefix);
        if let Err(e) = std::fs::write(&gitignore_path, &new_content) {
            return Ok((false, format!("Failed to update .gitignore: {}", e)));
        }
    }

    Ok((true, String::new()))
}
