//! Free functions for processing plugin hook results.
//!
//! Extracted from AppState to be callable from both AppState and WidgetController.

use fltk::app::Sender;

use crate::app::controllers::tabs::TabManager;
use crate::app::controllers::view::ViewController;
use crate::app::domain::messages::Message;
use crate::app::plugins::{HookResult, WidgetManager};

/// Bundles the mutable references needed by hook/lint result processing.
pub struct HookContext<'a> {
    pub tab_manager: &'a mut TabManager,
    pub view: &'a mut ViewController,
    pub widget_manager: &'a mut WidgetManager,
    pub sender: Sender<Message>,
}

/// Process the result from a plugin hook (diagnostics, annotations, status message, open_file, clipboard, goto_line).
pub fn dispatch_hook_result(result: HookResult, plugin_name: &str, ctx: &mut HookContext<'_>) {
    // Handle modified content (for format actions)
    if let Some(modified_content) = result.modified_content
        && let Some(doc) = ctx.tab_manager.active_doc()
    {
        let mut buf = doc.buffer.clone();
        buf.set_text(&modified_content);
    }

    // Update diagnostics — send even when empty if a lint plugin ran,
    // so the diagnostic panel shows "All checks passed"
    if !result.diagnostics.is_empty() || result.had_lint_results {
        ctx.sender
            .send(Message::DiagnosticsUpdate(result.diagnostics));
    }

    // Update line annotations
    if !result.line_annotations.is_empty() {
        ctx.sender
            .send(Message::AnnotationsUpdate(result.line_annotations));
    }

    // Show status message
    if let Some(status) = result.status_message {
        ctx.sender
            .send(Message::ToastShow(status.level, status.text));
    }

    // Handle open_file request with security validation
    if let Some(ref file_path) = result.open_file {
        use crate::app::plugins::security::{PathValidation, find_project_root, validate_path};

        // Determine project root from current document
        let project_root = ctx
            .tab_manager
            .active_doc()
            .and_then(|d| d.file_path.as_ref())
            .and_then(|p| find_project_root(std::path::Path::new(p)));

        if let Some(ref root) = project_root {
            match validate_path(file_path, root) {
                PathValidation::Valid(_) => {
                    eprintln!("[plugin:{}] open_file approved: {}", plugin_name, file_path);
                    ctx.sender
                        .send(Message::DeferredOpenFile(file_path.clone()));
                }
                other => {
                    eprintln!(
                        "[plugin:security] open_file BLOCKED for '{}': '{}' - {:?}",
                        plugin_name, file_path, other
                    );
                }
            }
        } else {
            // No project root - allow (same as file_exists behavior for untitled docs)
            eprintln!(
                "[plugin:{}] open_file (no project root): {}",
                plugin_name, file_path
            );
            ctx.sender
                .send(Message::DeferredOpenFile(file_path.clone()));
        }
    }

    // Handle clipboard_text request
    if let Some(ref text) = result.clipboard_text {
        crate::app::infrastructure::platform::copy_to_clipboard(text);
    }

    // Handle goto_line request
    if let Some(line) = result.goto_line
        && let Some(doc) = ctx.tab_manager.active_doc()
    {
        let buf = doc.buffer.clone();
        ctx.view.goto_line(&buf, line);
    }
}

/// Process lint result from plugin hook: send diagnostics, annotations, and toast.
pub fn dispatch_lint_result(result: HookResult, ctx: &mut HookContext<'_>) {
    // Process any widget requests (e.g., tree view updates from on_document_lint)
    process_widget_requests(&result, "", ctx.widget_manager, ctx.sender);

    // Only send diagnostics if at least one plugin actually linted this file.
    if result.had_lint_results {
        ctx.sender
            .send(Message::DiagnosticsUpdate(result.diagnostics));

        // Update or clear annotations
        if !result.line_annotations.is_empty() {
            ctx.sender
                .send(Message::AnnotationsUpdate(result.line_annotations));
        } else {
            // Clear any existing annotations when no issues found
            ctx.sender.send(Message::AnnotationsClear);
        }
    }

    if let Some(status) = result.status_message {
        ctx.sender
            .send(Message::ToastShow(status.level, status.text));
    }
}

/// Process widget requests (split view, tree view) from a hook result.
pub fn process_widget_requests(
    result: &HookResult,
    plugin_name: &str,
    widget_manager: &mut WidgetManager,
    sender: Sender<Message>,
) {
    // Use source_plugin from broadcast hooks when caller passes ""
    let effective_name = if plugin_name.is_empty() {
        result.source_plugin.as_deref().unwrap_or("")
    } else {
        plugin_name
    };

    // Check for split view request
    if let Some(ref split_request) = result.split_view
        && split_request.is_valid()
    {
        let session_id = widget_manager.create_split_view_session(effective_name);
        sender.send(Message::SplitViewShow {
            session_id,
            plugin_name: effective_name.to_string(),
            request: split_request.clone(),
        });
    }

    // Check for tree view request
    if let Some(ref tree_request) = result.tree_view
        && tree_request.is_valid()
    {
        let session_id =
            widget_manager.create_tree_view_session(effective_name, tree_request.persistent);
        sender.send(Message::TreeViewShow {
            session_id,
            plugin_name: effective_name.to_string(),
            request: tree_request.clone(),
        });
    }
}
