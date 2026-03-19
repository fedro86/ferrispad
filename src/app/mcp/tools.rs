//! MCP tool definitions and implementations.

use fltk::prelude::DisplayExt;
use serde_json::{Value, json};

use crate::app::infrastructure::buffer::{buffer_text_no_leak, selection_text_no_leak};
use crate::app::state::AppState;

use super::protocol::{json_rpc_error, json_rpc_result};

/// Handle `tools/list` — return all available tools with JSON schemas.
pub fn handle_list(id: &Value) -> String {
    let tools = json!({
        "tools": [
            {
                "name": "get_active_file",
                "description": "Get information about the currently active file in the editor: path, language, line count, cursor position, and modified status.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            },
            {
                "name": "get_buffer_content",
                "description": "Get the full text content of the active editor buffer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            },
            {
                "name": "get_selection",
                "description": "Get the currently selected text and its position in the active editor.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            },
            {
                "name": "list_open_files",
                "description": "List all files currently open in editor tabs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            },
            {
                "name": "open_file",
                "description": "Open a file in the editor.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the file to open"
                        }
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "goto_line",
                "description": "Move the cursor to a specific line number in the active file.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "line": {
                            "type": "integer",
                            "description": "Line number (1-indexed)"
                        }
                    },
                    "required": ["line"]
                }
            }
        ]
    });
    json_rpc_result(id, tools)
}

/// Handle `tools/call` — dispatch to the appropriate tool handler.
pub fn handle_call(id: &Value, params: &Value, state: &mut AppState) -> String {
    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::new()));

    let result = match tool_name {
        "get_active_file" => tool_get_active_file(state),
        "get_buffer_content" => tool_get_buffer_content(state),
        "get_selection" => tool_get_selection(state),
        "list_open_files" => tool_list_open_files(state),
        "open_file" => tool_open_file(state, &arguments),
        "goto_line" => tool_goto_line(state, &arguments),
        _ => Err(format!("Unknown tool: {}", tool_name)),
    };

    match result {
        Ok(content) => json_rpc_result(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": content
                }]
            }),
        ),
        Err(msg) => json_rpc_error(id, -32602, &msg),
    }
}

fn tool_get_active_file(state: &AppState) -> Result<String, String> {
    let doc = state.tab_manager.active_doc().ok_or("No active document")?;

    let info = json!({
        "path": doc.file_path,
        "name": doc.display_name,
        "language": doc.syntax_name,
        "line_count": doc.cached_line_count,
        "cursor_line": cursor_line(state),
        "modified": doc.is_dirty()
    });

    Ok(serde_json::to_string_pretty(&info).unwrap())
}

fn tool_get_buffer_content(state: &AppState) -> Result<String, String> {
    let doc = state.tab_manager.active_doc().ok_or("No active document")?;
    Ok(buffer_text_no_leak(&doc.buffer))
}

fn tool_get_selection(state: &AppState) -> Result<String, String> {
    let doc = state.tab_manager.active_doc().ok_or("No active document")?;

    let buf = &doc.buffer;
    let Some((start, end)) = buf.selection_position() else {
        let info = json!({
            "has_selection": false,
            "text": "",
            "start": 0,
            "end": 0
        });
        return Ok(serde_json::to_string_pretty(&info).unwrap());
    };

    let text = selection_text_no_leak(buf);
    let info = json!({
        "has_selection": true,
        "text": text,
        "start": start,
        "end": end
    });
    Ok(serde_json::to_string_pretty(&info).unwrap())
}

fn tool_list_open_files(state: &AppState) -> Result<String, String> {
    let files: Vec<Value> = state
        .tab_manager
        .documents()
        .iter()
        .map(|d| {
            json!({
                "path": d.file_path,
                "name": d.display_name,
                "modified": d.is_dirty()
            })
        })
        .collect();
    Ok(serde_json::to_string_pretty(&files).unwrap())
}

fn tool_open_file(state: &mut AppState, args: &Value) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(|p| p.as_str())
        .ok_or("Missing 'path' argument")?;

    if !std::path::Path::new(path).exists() {
        return Err(format!("File not found: {}", path));
    }

    // Check if already open
    if let Some(id) = state.tab_manager.find_by_path(path) {
        state.switch_to_document(id);
        state.rebuild_tab_bar();
        return Ok(json!({"opened": true, "path": path, "already_open": true}).to_string());
    }

    // Open via the file controller
    let theme_bg = state.highlight.highlighter().theme_background();
    let actions = state.file.open_file(
        path.to_string(),
        &mut state.tab_manager,
        &state.settings,
        theme_bg,
        state.tabs_enabled,
    );
    state.dispatch_file_actions(actions);

    Ok(json!({"opened": true, "path": path, "already_open": false}).to_string())
}

fn tool_goto_line(state: &mut AppState, args: &Value) -> Result<String, String> {
    let line = args
        .get("line")
        .and_then(|l| l.as_u64())
        .ok_or("Missing 'line' argument")? as u32;

    state.goto_line(line);
    Ok(json!({"jumped_to_line": line}).to_string())
}

/// Get the 1-indexed line number of the cursor position.
fn cursor_line(state: &AppState) -> i32 {
    let pos = state.editor.insert_position();
    let buf = state.editor.buffer();
    match buf {
        Some(b) => b.count_lines(0, pos) + 1,
        None => 1,
    }
}
