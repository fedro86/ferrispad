//! Hook dispatch — calls plugin hooks and collects results.
//!
//! All functions are free functions that take `&LuaRuntime` and `&[LoadedPlugin]`
//! as parameters instead of requiring `&self` on `PluginManager`.

use super::api::EditorApi;
use super::hook_result_parser;
use super::hooks::{Diagnostic, DiagnosticLevel, HookResult, PluginHook, StatusMessage};
use super::runtime::LuaRuntime;
use super::LoadedPlugin;

/// Call a hook on a specific plugin by name.
/// Returns None if the plugin is not found or not enabled.
pub(super) fn call_hook_on_plugin(
    runtime: &LuaRuntime,
    plugins: &[LoadedPlugin],
    plugin_name: &str,
    hook: PluginHook,
) -> Option<HookResult> {
    let plugin = plugins.iter().find(|p| p.name == plugin_name)?;

    if !plugin.enabled {
        return None;
    }

    let mut result = HookResult::default();

    match call_plugin_hook(runtime, plugin, &hook) {
        Ok(hook_output) => {
            result = hook_output;
        }
        Err(e) => {
            eprintln!("[plugins] {} hook error: {}", plugin.name, e);
            result.status_message = Some(StatusMessage {
                level: crate::ui::toast::ToastLevel::Error,
                text: format!("Plugin '{}' failed", plugin.name),
            });
            let error_msg = e.to_string();
            let clean_msg = error_msg
                .lines()
                .next()
                .unwrap_or(&error_msg)
                .trim_start_matches("runtime error: ")
                .to_string();

            result.diagnostics.push(Diagnostic {
                line: 1,
                column: None,
                message: clean_msg,
                level: DiagnosticLevel::Error,
                source: plugin.name.clone(),
                fix_message: None,
                url: None,
            });
        }
    }

    Some(result)
}

/// Call a hook on all enabled plugins
pub(super) fn call_hook(
    runtime: &LuaRuntime,
    plugins: &[LoadedPlugin],
    hook: PluginHook,
) -> HookResult {
    let mut result = HookResult::default();

    for plugin in plugins {
        if !plugin.enabled {
            continue;
        }

        match call_plugin_hook(runtime, plugin, &hook) {
            Ok(hook_output) => {
                if let Some(modified) = hook_output.modified_content {
                    // For OnDocumentSave, plugins can chain modifications
                    result.modified_content = Some(modified);
                }
                // Collect diagnostics from all plugins
                result.diagnostics.extend(hook_output.diagnostics);
                // Collect line annotations from all plugins
                result.line_annotations.extend(hook_output.line_annotations);
                // Propagate lint flag: true if ANY plugin produced results
                result.had_lint_results |= hook_output.had_lint_results;
                // Use the last plugin's status message (or first non-None)
                if hook_output.status_message.is_some() {
                    result.status_message = hook_output.status_message;
                }
                // Propagate widget requests (last plugin wins)
                if hook_output.split_view.is_some() {
                    result.split_view = hook_output.split_view;
                    result.source_plugin = Some(plugin.name.clone());
                }
                if hook_output.tree_view.is_some() {
                    result.tree_view = hook_output.tree_view;
                    result.source_plugin = Some(plugin.name.clone());
                }
                if hook_output.open_file.is_some() {
                    result.open_file = hook_output.open_file;
                }
                if hook_output.clipboard_text.is_some() {
                    result.clipboard_text = hook_output.clipboard_text;
                }
                if hook_output.goto_line.is_some() {
                    result.goto_line = hook_output.goto_line;
                }
            }
            Err(e) => {
                eprintln!("[plugins] {} hook error: {}", plugin.name, e);
                // Short toast notification
                result.status_message = Some(StatusMessage {
                    level: crate::ui::toast::ToastLevel::Error,
                    text: format!("Plugin '{}' failed", plugin.name),
                });
                // Extract just the error message, not the stack trace
                let error_msg = e.to_string();
                let clean_msg = error_msg
                    .lines()
                    .next()
                    .unwrap_or(&error_msg)
                    .trim_start_matches("runtime error: ")
                    .to_string();

                // Check if this is a permission error - add clickable action to open plugin folder
                let (fix_message, url) = if clean_msg.contains("No permissions")
                    || clean_msg.contains("not approved")
                {
                    // Create file:// URL to the plugin directory
                    // On Windows paths are C:\..., need file:///C:/...
                    let plugin_url = {
                        let p = plugin.path.to_string_lossy();
                        let slash_path = p.replace('\\', "/");
                        if slash_path.starts_with('/') {
                            format!("file://{}", slash_path)
                        } else {
                            format!("file:///{}", slash_path)
                        }
                    };
                    (
                        Some("Double-click to open plugin folder".to_string()),
                        Some(plugin_url),
                    )
                } else {
                    (None, None)
                };

                // Error in diagnostic panel
                result.diagnostics.push(Diagnostic {
                    line: 1,
                    column: None,
                    message: clean_msg,
                    level: DiagnosticLevel::Error,
                    source: plugin.name.clone(),
                    fix_message,
                    url,
                });
            }
        }
    }

    // Sort diagnostics by severity (errors first)
    result.diagnostics.sort_by(|a, b| a.level.cmp(&b.level));

    // Sort line annotations by line number
    result.line_annotations.sort_by_key(|a| a.line);

    result
}

/// Call a specific hook on a single plugin
fn call_plugin_hook(
    runtime: &LuaRuntime,
    plugin: &LoadedPlugin,
    hook: &PluginHook,
) -> Result<HookResult, mlua::Error> {
    let hook_name = hook.lua_name();
    let mut result = HookResult::default();

    // Create the API object for this hook with plugin context for permissions
    let api = create_api_for_hook(hook, plugin);

    // Call the hook with appropriate arguments
    let value = match hook {
        PluginHook::Init | PluginHook::Shutdown => {
            runtime.call_hook(&plugin.table, hook_name, api)?
        }

        PluginHook::OnDocumentOpen { path, .. } => {
            let value = runtime.call_hook(&plugin.table, hook_name, (api, path.clone()))?;
            if let mlua::Value::Table(return_table) = value {
                hook_result_parser::parse_lint_result(&return_table, &plugin.name, &mut result);
            }
            return Ok(result);
        }

        PluginHook::OnDocumentSave { path, content } => {
            let value = runtime.call_hook(
                &plugin.table,
                hook_name,
                (api, path.clone(), content.clone()),
            )?;

            // If the hook returns a string, use it as modified content
            if let mlua::Value::String(s) = value {
                result.modified_content = Some(s.to_str()?.to_string());
            }
            return Ok(result);
        }

        PluginHook::OnDocumentClose { path } => {
            runtime.call_hook(&plugin.table, hook_name, (api, path.clone()))?
        }

        PluginHook::OnTextChanged {
            position,
            inserted_len,
            deleted_len,
        } => runtime.call_hook(
            &plugin.table,
            hook_name,
            (api, *position, *inserted_len, *deleted_len),
        )?,

        PluginHook::OnThemeChanged { is_dark } => {
            runtime.call_hook(&plugin.table, hook_name, (api, *is_dark))?
        }

        PluginHook::OnDocumentLint { path, content } => {
            let value = runtime.call_hook(
                &plugin.table,
                hook_name,
                (api, path.clone(), content.clone()),
            )?;

            // Parse diagnostics and highlights from the returned table.
            // Only mark had_lint_results if the table contains actual lint data
            // (diagnostics or highlights), not just widget requests like tree_view.
            if let mlua::Value::Table(return_table) = value {
                let has_lint_data = return_table.contains_key("diagnostics").unwrap_or(false)
                    || return_table.raw_len() > 0; // old format: array of diagnostics
                if has_lint_data {
                    result.had_lint_results = true;
                }
                hook_result_parser::parse_lint_result(&return_table, &plugin.name, &mut result);
            }
            return Ok(result);
        }

        PluginHook::OnHighlightRequest { path, content } => {
            let value = runtime.call_hook(
                &plugin.table,
                hook_name,
                (api, path.clone(), content.clone()),
            )?;

            // Parse highlights from the returned table
            if let mlua::Value::Table(return_table) = value {
                let has_lint_data = return_table.contains_key("diagnostics").unwrap_or(false)
                    || return_table.raw_len() > 0;
                if has_lint_data {
                    result.had_lint_results = true;
                }
                hook_result_parser::parse_lint_result(&return_table, &plugin.name, &mut result);
            }
            return Ok(result);
        }

        PluginHook::OnMenuAction {
            action,
            path,
            content,
        } => {
            let value = runtime.call_hook(
                &plugin.table,
                hook_name,
                (api, action.clone(), path.clone(), content.clone()),
            )?;

            // Parse result similar to lint hooks (diagnostics, highlights, modified_content, status_message)
            if let mlua::Value::Table(return_table) = value {
                // Check for modified_content
                if let Ok(mlua::Value::String(s)) =
                    return_table.get::<mlua::Value>("modified_content")
                {
                    result.modified_content = Some(s.to_str()?.to_string());
                }
                let has_lint_data = return_table.contains_key("diagnostics").unwrap_or(false)
                    || return_table.raw_len() > 0;
                if has_lint_data {
                    result.had_lint_results = true;
                }
                hook_result_parser::parse_lint_result(&return_table, &plugin.name, &mut result);
            }
            return Ok(result);
        }

        PluginHook::OnWidgetAction {
            widget_type,
            action,
            session_id,
            data,
            path: _,
        } => {
            // Convert WidgetActionData to a Lua table
            let lua = runtime.lua();
            let data_table = lua.create_table()?;
            if let Some(ref content) = data.right_content {
                data_table.set("right_content", content.as_str())?;
            }
            if let Some(ref path) = data.node_path {
                let path_table = lua.create_table()?;
                for (i, segment) in path.iter().enumerate() {
                    path_table.set(i + 1, segment.as_str())?;
                }
                data_table.set("node_path", path_table)?;
            }
            if let Some(ref text) = data.input_text {
                data_table.set("input_text", text.as_str())?;
            }
            if let Some(ref target) = data.target_path {
                let target_table = lua.create_table()?;
                for (i, segment) in target.iter().enumerate() {
                    target_table.set(i + 1, segment.as_str())?;
                }
                data_table.set("target_path", target_table)?;
            }

            let value = runtime.call_hook(
                &plugin.table,
                hook_name,
                (
                    api,
                    widget_type.clone(),
                    action.clone(),
                    *session_id,
                    data_table,
                ),
            )?;

            // Parse result similar to menu action hooks
            if let mlua::Value::Table(return_table) = value {
                if let Ok(mlua::Value::String(s)) =
                    return_table.get::<mlua::Value>("modified_content")
                {
                    result.modified_content = Some(s.to_str()?.to_string());
                }
                hook_result_parser::parse_lint_result(&return_table, &plugin.name, &mut result);
            }
            return Ok(result);
        }
    };

    // Most hooks don't return anything useful
    let _ = value;
    Ok(result)
}

/// Create an EditorApi instance for a specific hook with plugin context
fn create_api_for_hook(hook: &PluginHook, plugin: &LoadedPlugin) -> EditorApi {
    let mut api = match hook {
        PluginHook::Init | PluginHook::Shutdown => EditorApi::default(),

        PluginHook::OnDocumentOpen { path, content } => {
            // Use passed content (avoids disk re-read for large files).
            // Fall back to disk read for callers that don't provide content.
            let text = content.clone().or_else(|| {
                path.as_deref()
                    .and_then(|p| std::fs::read_to_string(p).ok())
            });
            match text {
                Some(t) => EditorApi::with_path_and_content(path.clone(), t),
                None => EditorApi::with_path(path.clone()),
            }
        }

        PluginHook::OnDocumentSave { path, content } => {
            EditorApi::with_content(path.clone(), content.clone())
        }

        PluginHook::OnDocumentClose { path } => EditorApi::with_path(path.clone()),

        PluginHook::OnTextChanged {
            position,
            inserted_len,
            deleted_len,
        } => EditorApi::for_text_change(*position, *inserted_len, *deleted_len, None),

        PluginHook::OnThemeChanged { .. } => EditorApi::default(),

        PluginHook::OnDocumentLint { path, content } => {
            EditorApi::with_content(path.clone(), content.clone())
        }

        PluginHook::OnHighlightRequest { path, content } => {
            EditorApi::with_path_and_content(path.clone(), content.clone())
        }

        PluginHook::OnMenuAction { path, content, .. } => {
            EditorApi::with_path_and_content(path.clone(), content.clone())
        }

        PluginHook::OnWidgetAction { path, data, .. } => {
            // Prefer buffer content (avoids stale reads for unsaved files),
            // fall back to reading from disk
            let content = data.content.clone().or_else(|| {
                path.as_deref()
                    .and_then(|p| std::fs::read_to_string(p).ok())
            });
            match content {
                Some(text) => EditorApi::with_path_and_content(path.clone(), text),
                None => EditorApi::with_path(path.clone()),
            }
        }
    };

    // Add plugin context for permission checking
    api.plugin_name = Some(plugin.name.clone());
    api.allowed_commands = plugin.approved_commands.clone();

    // Add plugin-specific configuration
    api.config = plugin.config_params.clone();

    api
}
