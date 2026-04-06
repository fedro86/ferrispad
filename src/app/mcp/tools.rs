//! MCP tool definitions and implementations.

use fltk::prelude::DisplayExt;
use serde_json::{Value, json};

use crate::app::domain::messages::Message;
use crate::app::infrastructure::buffer::{buffer_text_no_leak, selection_text_no_leak};
use crate::app::plugins::diff::compute_aligned_diff;
use crate::app::plugins::widgets::split_view::{
    HighlightColor, IntralineSpan as SplitIntralineSpan, LineHighlight, SplitDisplayMode, SplitPane,
    SplitViewAction, SplitViewRequest,
};
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
            },
            {
                "name": "refresh_tree",
                "description": "Refresh the file explorer tree view to reflect filesystem changes. Call this after creating, renaming, moving, or deleting files.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            },
            {
                "name": "show_diff",
                "description": "Show a diff view comparing the editor buffer with the file on disk. Call this after editing a file that is open in the editor so the user can review changes before accepting them.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the file that was edited"
                        }
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "preview_edit",
                "description": "Preview a proposed edit before applying it. Shows a diff view and blocks until the user accepts or rejects.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the file being edited"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "The text to be replaced"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "The replacement text"
                        },
                        "decision_fifo": {
                            "type": "string",
                            "description": "Path to a FIFO where the decision (accept/reject) will be written"
                        }
                    },
                    "required": ["path", "old_string", "new_string", "decision_fifo"]
                }
            },
            {
                "name": "reload_file",
                "description": "Reload a file from disk into the editor buffer.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the file to reload"
                        }
                    },
                    "required": ["path"]
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
        "refresh_tree" => tool_refresh_tree(state),
        "show_diff" => tool_show_diff(state, &arguments),
        "preview_edit" => tool_preview_edit(state, &arguments),
        "reload_file" => tool_reload_file(state, &arguments),
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

fn tool_refresh_tree(state: &mut AppState) -> Result<String, String> {
    if !state.widget.tree_view_active {
        return Ok(json!({"refreshed": false, "reason": "tree panel not visible"}).to_string());
    }

    let session_id = match state.widget.widget_manager.any_tree_view_session() {
        Some(id) => id,
        None => {
            return Ok(
                json!({"refreshed": false, "reason": "no tree view session"}).to_string(),
            );
        }
    };

    // Invalidate cached tree so it gets rebuilt from disk
    if let Some(doc) = state.tab_manager.active_doc_mut() {
        doc.cached_tree = None;
    }

    // Trigger the same "refresh" action as the tree panel's Refresh button
    state.sender.send(Message::TreeViewContextAction {
        session_id,
        action: "refresh".to_string(),
        node_path: vec![],
        input_text: None,
        target_path: None,
    });

    Ok(json!({"refreshed": true}).to_string())
}

/// Convert DiffLineHighlights to SplitView LineHighlights.
fn convert_highlights(
    highlights: &[crate::app::plugins::diff::DiffLineHighlight],
    added_color: &str,
) -> Vec<LineHighlight> {
    highlights
        .iter()
        .map(|h| LineHighlight {
            line: h.line,
            color: if h.color == added_color {
                HighlightColor::Added
            } else {
                HighlightColor::Removed
            },
            spans: h
                .spans
                .iter()
                .map(|s| SplitIntralineSpan {
                    start: s.start,
                    end: s.end,
                })
                .collect(),
        })
        .collect()
}

/// Show a diff between the SplitView and send a SplitViewShow message.
fn show_diff_view(
    state: &mut AppState,
    path: &str,
    old_content: &str,
    new_content: &str,
    left_label: &str,
    right_label: &str,
    decision_fifo: Option<String>,
) {
    // Dismiss any previous pending diff review
    let prev_keys: Vec<u32> = state.pending_diff_reviews.keys().copied().collect();
    for prev_session in prev_keys {
        if let Some((_, Some(ref fifo_path))) = state.pending_diff_reviews.remove(&prev_session) {
            let _ = std::fs::write(fifo_path, "accept\n");
        }
    }

    let diff = compute_aligned_diff(old_content, new_content);
    let left_highlights = convert_highlights(&diff.left_highlights, "added");
    let right_highlights = convert_highlights(&diff.right_highlights, "added");

    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);

    let request = SplitViewRequest {
        title: format!("Review: {}", file_name),
        left: SplitPane {
            content: diff.left_content,
            label: left_label.to_string(),
            line_numbers: true,
            read_only: true,
            highlights: left_highlights,
        },
        right: SplitPane {
            content: diff.right_content,
            label: right_label.to_string(),
            line_numbers: true,
            read_only: true,
            highlights: right_highlights,
        },
        actions: vec![
            SplitViewAction {
                label: "Close".to_string(),
                action: "reject".to_string(),
            },
        ],
        display_mode: SplitDisplayMode::Tab,
    };

    let session_id = crate::app::plugins::widgets::next_session_id();
    state
        .pending_diff_reviews
        .insert(session_id, (path.to_string(), decision_fifo));

    state.sender.send(Message::SplitViewShow {
        session_id,
        plugin_name: "_mcp_diff".to_string(),
        request,
    });
    fltk::app::awake();
}

fn tool_show_diff(state: &mut AppState, args: &Value) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(|p| p.as_str())
        .ok_or("Missing 'path' argument")?;

    let doc_id = match state.tab_manager.find_by_path(path) {
        Some(id) => id,
        None => {
            return Ok(
                json!({"shown": false, "reason": "file not open in editor"}).to_string(),
            );
        }
    };

    let buffer_content = {
        let doc = state
            .tab_manager
            .doc_by_id(doc_id)
            .ok_or("Document not found")?;
        buffer_text_no_leak(&doc.buffer)
    };

    let disk_content =
        std::fs::read_to_string(path).map_err(|e| format!("Cannot read file: {}", e))?;

    if buffer_content == disk_content {
        return Ok(json!({"shown": false, "reason": "no changes"}).to_string());
    }

    show_diff_view(
        state,
        path,
        &buffer_content,
        &disk_content,
        "Before",
        "After",
        None,
    );

    // Auto-reload the buffer from disk so the editor is already up to date
    let actions = state.file.reload_file(doc_id, &mut state.tab_manager);
    state.dispatch_file_actions(actions);

    Ok(json!({"shown": true, "path": path}).to_string())
}

fn tool_preview_edit(state: &mut AppState, args: &Value) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(|p| p.as_str())
        .ok_or("Missing 'path' argument")?;
    let old_string = args
        .get("old_string")
        .and_then(|s| s.as_str())
        .ok_or("Missing 'old_string' argument")?;
    let new_string = args
        .get("new_string")
        .and_then(|s| s.as_str())
        .ok_or("Missing 'new_string' argument")?;
    let decision_fifo = args
        .get("decision_fifo")
        .and_then(|s| s.as_str())
        .unwrap_or("");

    // Read the current file content
    let current_content =
        std::fs::read_to_string(path).map_err(|e| format!("Cannot read file: {}", e))?;

    // Apply the proposed edit to generate the new content
    if !current_content.contains(old_string) {
        return Ok(
            json!({"shown": false, "reason": "old_string not found in file"}).to_string(),
        );
    }
    let proposed_content = current_content.replacen(old_string, new_string, 1);

    show_diff_view(
        state,
        path,
        &current_content,
        &proposed_content,
        "Current",
        "Proposed",
        if decision_fifo.is_empty() {
            None
        } else {
            Some(decision_fifo.to_string())
        },
    );

    Ok(json!({"shown": true, "path": path}).to_string())
}

fn tool_reload_file(state: &mut AppState, args: &Value) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(|p| p.as_str())
        .ok_or("Missing 'path' argument")?;

    if let Some(doc_id) = state.tab_manager.find_by_path(path) {
        let actions = state.file.reload_file(doc_id, &mut state.tab_manager);
        state.dispatch_file_actions(actions);

        // Close any pending diff review for this file
        let session_to_close: Option<u32> = state
            .pending_diff_reviews
            .iter()
            .find(|(_, (p, _))| p == path)
            .map(|(id, _)| *id);
        if let Some(session_id) = session_to_close {
            state.pending_diff_reviews.remove(&session_id);
            state.sender.send(Message::SplitViewReject(session_id));
            fltk::app::awake();
        }

        Ok(json!({"reloaded": true, "path": path}).to_string())
    } else {
        Ok(json!({"reloaded": false, "reason": "file not open"}).to_string())
    }
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
