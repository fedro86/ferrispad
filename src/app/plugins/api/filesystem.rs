//! File system operations exposed to Lua plugins.
//!
//! All paths are sandboxed to project_root via validate_path().

use std::process::Command;

use super::super::security::{PathValidation, validate_path};
use super::EditorApi;
use super::sandbox::{resolve_and_validate, scan_dir_recursive};

/// Check if a file exists at the given path.
/// Returns true if the file exists AND is within project root, false otherwise.
pub fn file_exists(_: &mlua::Lua, this: &EditorApi, path: String) -> mlua::Result<bool> {
    let Some(ref project_root) = this.project_root else {
        return Ok(false);
    };

    match validate_path(&path, project_root) {
        PathValidation::Valid(canonical) => Ok(canonical.exists()),
        PathValidation::NotFound => Ok(false),
        PathValidation::OutsideProjectRoot | PathValidation::InvalidPath(_) => {
            eprintln!(
                "[plugin:security] file_exists blocked: '{}' outside project root",
                path
            );
            Ok(false)
        }
    }
}

/// Read the contents of a file inside the project root.
/// Returns (content, nil) on success, (nil, error_msg) on failure.
pub fn read_file(
    lua: &mlua::Lua,
    this: &EditorApi,
    path: String,
) -> mlua::Result<(mlua::Value, Option<String>)> {
    let Some(ref project_root) = this.project_root else {
        return Ok((mlua::Value::Nil, Some("No project root".to_string())));
    };

    match validate_path(&path, project_root) {
        PathValidation::Valid(canonical) => match std::fs::read_to_string(&canonical) {
            Ok(content) => {
                let lua_str = lua.create_string(&content)?;
                Ok((mlua::Value::String(lua_str), None))
            }
            Err(e) => Ok((mlua::Value::Nil, Some(e.to_string()))),
        },
        _ => {
            eprintln!(
                "[plugin:security] read_file blocked: '{}' outside project root",
                path
            );
            Ok((
                mlua::Value::Nil,
                Some("Path outside project root".to_string()),
            ))
        }
    }
}

/// Check if a path is a regular file (not a directory).
/// Returns false for directories, non-existent paths, and paths outside project root.
pub fn is_file(_: &mlua::Lua, this: &EditorApi, path: String) -> mlua::Result<bool> {
    let Some(ref project_root) = this.project_root else {
        return Ok(false);
    };
    match resolve_and_validate(&path, project_root)? {
        Some(p) => Ok(p.is_file()),
        None => Ok(false),
    }
}

/// List entries in a single directory (non-recursive).
/// Returns array of { name = "...", is_dir = bool } or nil on failure.
pub fn list_dir(lua: &mlua::Lua, this: &EditorApi, path: String) -> mlua::Result<mlua::Value> {
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
        t.set(
            "name",
            entry.file_name().to_string_lossy().as_ref().to_owned(),
        )?;
        t.set("is_dir", entry.path().is_dir())?;
        result.set(idx, t)?;
        idx += 1;
    }
    Ok(mlua::Value::Table(result))
}

/// Recursively scan a directory up to max_depth (default 5, cap 10).
/// Returns array of { name, rel_path, is_dir } or nil on failure.
pub fn scan_dir(
    lua: &mlua::Lua,
    this: &EditorApi,
    (path, max_depth): (String, Option<u32>),
) -> mlua::Result<mlua::Value> {
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
}

/// Create a new empty file. Fails if the file already exists (no truncation).
/// Returns (true, "") on success, (false, error_msg) on failure.
pub fn create_file(_: &mlua::Lua, this: &EditorApi, path: String) -> mlua::Result<(bool, String)> {
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
}

/// Create a directory (and all missing parents).
/// Returns (true, "") on success, (false, error_msg) on failure.
pub fn create_dir(_: &mlua::Lua, this: &EditorApi, path: String) -> mlua::Result<(bool, String)> {
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
}

/// Rename (move) a file or directory. Both paths must be inside project root.
/// Returns (true, "") on success, (false, error_msg) on failure.
pub fn rename(
    _: &mlua::Lua,
    this: &EditorApi,
    (old, new): (String, String),
) -> mlua::Result<(bool, String)> {
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
}

/// Remove a file or directory (recursive for directories).
/// Returns (true, "") on success, (false, error_msg) on failure.
pub fn remove(_: &mlua::Lua, this: &EditorApi, path: String) -> mlua::Result<(bool, String)> {
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
}

/// Query git status for a directory.
/// Returns a table mapping relative file paths to status codes, or nil on error.
pub fn git_status(lua: &mlua::Lua, this: &EditorApi, path: String) -> mlua::Result<mlua::Value> {
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
}

/// Compute an aligned diff between old_text and new_text.
/// Returns: { left_content, right_content, left_highlights, right_highlights }
pub fn diff_text(
    lua: &mlua::Lua,
    _this: &EditorApi,
    (old_text, new_text): (String, String),
) -> mlua::Result<mlua::Value> {
    use super::super::diff::compute_aligned_diff;

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
}
