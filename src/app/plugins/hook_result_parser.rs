//! Parses Lua table results from plugin hooks into Rust types.
//!
//! All functions are free functions — no dependency on `PluginManager` fields.

use super::annotations::{AnnotationColor, GutterMark, InlineHighlight, LineAnnotation};
use super::hooks::{Diagnostic, DiagnosticLevel, HookResult, StatusMessage};
use super::widgets::{SplitViewRequest, TerminalViewRequest, TreeViewRequest};

use mlua::Table;

/// Parse lint/highlight result from Lua table.
/// Supports both old format (array of diagnostics) and new extended format:
/// - Old: { {line=1, message="..."}, ... }
/// - New: { diagnostics = {...}, highlights = {...}, status_message = {...}, split_view = {...}, tree_view = {...} }
pub(super) fn parse_lint_result(table: &Table, plugin_name: &str, result: &mut HookResult) {
    let has_diagnostics_key: bool = table.contains_key("diagnostics").unwrap_or(false);
    let has_highlights_key: bool = table.contains_key("highlights").unwrap_or(false);
    let has_status_key: bool = table.contains_key("status_message").unwrap_or(false);
    let has_split_view_key: bool = table.contains_key("split_view").unwrap_or(false);
    let has_tree_view_key: bool = table.contains_key("tree_view").unwrap_or(false);
    let has_terminal_view_key: bool = table.contains_key("terminal_view").unwrap_or(false);
    let has_open_file_key: bool = table.contains_key("open_file").unwrap_or(false);
    let has_clipboard_text_key: bool = table.contains_key("clipboard_text").unwrap_or(false);
    let has_goto_line_key: bool = table.contains_key("goto_line").unwrap_or(false);

    if has_diagnostics_key
        || has_highlights_key
        || has_status_key
        || has_split_view_key
        || has_tree_view_key
        || has_terminal_view_key
        || has_open_file_key
        || has_clipboard_text_key
        || has_goto_line_key
    {
        // New extended format
        if let Ok(mlua::Value::Table(diags_table)) = table.get::<mlua::Value>("diagnostics") {
            result
                .diagnostics
                .extend(parse_diagnostics(&diags_table, plugin_name));
        }
        if let Ok(mlua::Value::Table(highlights_table)) = table.get::<mlua::Value>("highlights") {
            result
                .line_annotations
                .extend(parse_line_annotations(&highlights_table, plugin_name));
        }
        // Parse optional status message for toast notification
        if let Ok(mlua::Value::Table(status_table)) = table.get::<mlua::Value>("status_message") {
            result.status_message = parse_status_message(&status_table);
        }
        // Parse optional split view request
        if let Ok(mlua::Value::Table(split_view_table)) = table.get::<mlua::Value>("split_view") {
            result.split_view = SplitViewRequest::from_lua_table(&split_view_table);
        }
        // Parse optional tree view request
        if let Ok(mlua::Value::Table(tree_view_table)) = table.get::<mlua::Value>("tree_view") {
            result.tree_view = TreeViewRequest::from_lua_table(&tree_view_table);
        }
        // Parse optional terminal view request
        if let Ok(mlua::Value::Table(terminal_view_table)) =
            table.get::<mlua::Value>("terminal_view")
        {
            result.terminal_view = TerminalViewRequest::from_lua_table(&terminal_view_table);
        }
        // Parse optional open_file request
        if let Ok(mlua::Value::String(s)) = table.get::<mlua::Value>("open_file")
            && let Ok(path) = s.to_str()
        {
            result.open_file = Some(path.to_string());
        }
        // Parse optional clipboard_text request
        if let Ok(mlua::Value::String(s)) = table.get::<mlua::Value>("clipboard_text")
            && let Ok(text) = s.to_str()
        {
            result.clipboard_text = Some(text.to_string());
        }
        // Parse optional goto_line request
        if let Ok(line) = table.get::<u32>("goto_line") {
            result.goto_line = Some(line);
        }
    } else {
        // Old format: array of diagnostics directly
        result
            .diagnostics
            .extend(parse_diagnostics(table, plugin_name));
    }
}

/// Parse a Lua table of diagnostics into Rust Diagnostic structs
fn parse_diagnostics(table: &Table, plugin_name: &str) -> Vec<Diagnostic> {
    table
        .clone()
        .pairs::<i32, mlua::Table>()
        .flatten()
        .filter_map(|(_, diag_table)| parse_single_diagnostic(&diag_table, plugin_name))
        .collect()
}

/// Parse a single diagnostic from a Lua table
fn parse_single_diagnostic(table: &Table, plugin_name: &str) -> Option<Diagnostic> {
    // Required: line number
    let line: u32 = table.get("line").ok()?;

    // Required: message
    let message: String = table.get("message").ok()?;

    // Optional: column
    let column: Option<u32> = table.get("column").ok();

    // Optional: level (defaults to "info")
    let level_str: String = table.get("level").unwrap_or_else(|_| "info".to_string());
    let level = DiagnosticLevel::from_str(&level_str);

    // Optional: fix message (e.g., "Organize imports")
    let fix_message: Option<String> = table.get("fix_message").ok();

    // Optional: documentation URL
    let url: Option<String> = table.get("url").ok();

    Some(Diagnostic {
        line,
        column,
        message,
        level,
        source: plugin_name.to_string(),
        fix_message,
        url,
    })
}

/// Parse a status message from a Lua table
fn parse_status_message(table: &Table) -> Option<StatusMessage> {
    use crate::ui::toast::ToastLevel;

    // Required: text
    let text: String = table.get("text").ok()?;

    // Optional: level (defaults to "info")
    let level_str: String = table.get("level").unwrap_or_else(|_| "info".to_string());
    let level = match level_str.to_lowercase().as_str() {
        "success" => ToastLevel::Success,
        "info" => ToastLevel::Info,
        "warning" | "warn" => ToastLevel::Warning,
        "error" => ToastLevel::Error,
        _ => ToastLevel::Info,
    };

    Some(StatusMessage { level, text })
}

/// Parse a Lua table of line annotations
fn parse_line_annotations(table: &Table, plugin_name: &str) -> Vec<LineAnnotation> {
    table
        .clone()
        .pairs::<i32, mlua::Table>()
        .flatten()
        .filter_map(|(_, ann_table)| parse_single_annotation(&ann_table, plugin_name))
        .collect()
}

/// Parse a single line annotation from a Lua table
fn parse_single_annotation(table: &Table, _plugin_name: &str) -> Option<LineAnnotation> {
    // Required: line number
    let line: u32 = table.get("line").ok()?;

    // Optional: gutter mark
    let gutter = if let Ok(mlua::Value::Table(gutter_table)) = table.get::<mlua::Value>("gutter") {
        parse_gutter_mark(&gutter_table)
    } else {
        None
    };

    // Optional: inline highlights (array)
    let inline = if let Ok(mlua::Value::Table(inline_table)) = table.get::<mlua::Value>("inline") {
        parse_inline_highlights(&inline_table)
    } else {
        Vec::new()
    };

    // Only return if we have at least gutter or inline
    if gutter.is_some() || !inline.is_empty() {
        Some(LineAnnotation {
            line,
            gutter,
            inline,
        })
    } else {
        None
    }
}

/// Parse a gutter mark from a Lua table
fn parse_gutter_mark(table: &Table) -> Option<GutterMark> {
    let color = parse_annotation_color(table)?;
    Some(GutterMark { color })
}

/// Parse inline highlights array from a Lua table
fn parse_inline_highlights(table: &Table) -> Vec<InlineHighlight> {
    table
        .clone()
        .pairs::<i32, mlua::Table>()
        .flatten()
        .filter_map(|(_, hl_table)| parse_single_inline_highlight(&hl_table))
        .collect()
}

/// Parse a single inline highlight from a Lua table
fn parse_single_inline_highlight(table: &Table) -> Option<InlineHighlight> {
    // Required: start_col
    let start_col: u32 = table.get("start_col").ok()?;

    // Optional: end_col (None means end of line)
    let end_col: Option<u32> = table.get("end_col").ok();

    // Required: color
    let color = parse_annotation_color(table)?;

    Some(InlineHighlight {
        start_col,
        end_col,
        color,
    })
}

/// Parse an annotation color from a Lua table
fn parse_annotation_color(table: &Table) -> Option<AnnotationColor> {
    // Try string color name first
    if let Ok(color_str) = table.get::<String>("color")
        && let Some(color) = AnnotationColor::from_str(&color_str)
    {
        return Some(color);
    }

    // Try RGB table: color = { r = 255, g = 0, b = 0 }
    if let Ok(mlua::Value::Table(color_table)) = table.get::<mlua::Value>("color") {
        let r: u8 = color_table.get("r").unwrap_or(0);
        let g: u8 = color_table.get("g").unwrap_or(0);
        let b: u8 = color_table.get("b").unwrap_or(0);
        return Some(AnnotationColor::Rgb(r, g, b));
    }

    None
}
