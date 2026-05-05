//! Dispatch handler functions for the main event loop.
//!
//! Each function handles a group of related `Message` variants,
//! keeping `main.rs` as a thin dispatcher.

use fltk::prelude::*;

use ferris_pad::split_parent;

use crate::app::controllers::plugin::PluginController;
use crate::app::controllers::update::BannerWidgets;
use crate::app::domain::messages::Message;
use crate::app::domain::settings::TreePanelPosition;
use crate::app::infrastructure::defer::defer_send;
use crate::app::mcp;
use crate::app::plugins::widgets::SplitDisplayMode;
use crate::app::services::session;
use crate::app::services::updater::current_timestamp;
use crate::app::state::AppState;
use crate::ui::dialogs::about::show_about_dialog;
use crate::ui::dialogs::find::{show_find_dialog, show_replace_dialog};
use crate::ui::dialogs::goto_line::show_goto_line_dialog;
use crate::ui::dialogs::session_picker::{
    SessionPickerResult, show_new_session_dialog, show_session_picker,
};
use crate::ui::dialogs::update::check_for_updates_ui;
use crate::ui::main_window::LayoutWidgets;
use crate::ui::tab_bar::TAB_BAR_HEIGHT;
use crate::ui::theme::DIVIDER_WIDTH;

/// Result from a dispatch handler that may request quit.
pub enum DispatchResult {
    Continue,
    Quit,
}

// ---------------------------------------------------------------------------
// File
// ---------------------------------------------------------------------------

pub fn handle_file(msg: Message, state: &mut AppState) -> DispatchResult {
    match msg {
        Message::FileNew => {
            let actions = state
                .file
                .file_new(&mut state.tab_manager, state.tabs_enabled);
            state.dispatch_file_actions(actions);
            state.session.mark_dirty();
        }
        Message::FileReload => {
            state.file_reload_active();
            state.session.mark_dirty();
        }
        Message::FileReloadAll => {
            state.file_reload_all();
            state.session.mark_dirty();
        }
        Message::FileOpen => {
            let theme_bg = state.highlight.highlighter().theme_background();
            let actions = state.file.file_open(
                &mut state.tab_manager,
                &state.settings,
                theme_bg,
                state.tabs_enabled,
            );
            state.dispatch_file_actions(actions);
            state.session.mark_dirty();
        }
        Message::FileSave => {
            state.file_save();
            state.session.mark_dirty();
        }
        Message::FileSaveAs => {
            let actions =
                state
                    .file
                    .file_save_as(&mut state.tab_manager, &state.plugins, state.tabs_enabled);
            state.dispatch_file_actions(actions);
            state.session.mark_dirty();
        }
        Message::FileQuit | Message::WindowClose => {
            if state.file_quit() {
                return DispatchResult::Quit;
            }
            // User cancelled — reset the flag
            fltk::app::program_should_quit(false);
        }
        _ => {}
    }
    DispatchResult::Continue
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

pub fn handle_tab(msg: Message, state: &mut AppState, lw: &mut LayoutWidgets) -> DispatchResult {
    match msg {
        Message::TabSwitch(id) => {
            // If diff tab is active in tab mode, collapse the split panel first
            if let Some(ref mut tb) = state.tab_bar
                && tb.is_diff_tab_active()
                && lw.split_panel.is_tab_mode()
            {
                lw.split_panel.container.hide();
                let parent = split_parent!(lw);
                parent.fixed(lw.split_panel.widget(), 0);
                if let Some(ref mut div) = lw.split_panel.divider {
                    div.hide();
                    parent.fixed(div, 0);
                }
                parent.recalc();
                if let Some(sid) = tb.diff_tab_session_id() {
                    tb.set_diff_tab(sid, lw.split_panel.diff_title(), false);
                }
            }
            state.switch_to_document(id);
            state.rebuild_tab_bar();
            if let Some(ref mut tab_bar) = state.tab_bar {
                tab_bar.ensure_active_visible(Some(id));
            }
        }
        Message::TabClose(id) => {
            // close_tab returns true when no tabs remain.
            // Don't quit — sync_start_page will show the start page.
            state.close_tab(id);
            state.session.mark_dirty();
        }
        Message::TabCloseActive => {
            if let Some(id) = state.tab_manager.active_id() {
                state.close_tab(id);
                state.session.mark_dirty();
            }
        }
        Message::TabMove(from, to) => {
            state.tab_manager.move_tab(from, to);
            state.rebuild_tab_bar();
            state.session.mark_dirty();
        }
        Message::TabNext => state.switch_to_next_tab(),
        Message::TabPrevious => state.switch_to_previous_tab(),

        // Tab Groups
        Message::TabGroupCreate(doc_id) => {
            state.tab_manager.create_group(&[doc_id]);
            state.rebuild_tab_bar();
            state.session.mark_dirty();
        }
        Message::TabGroupDelete(group_id) => {
            state.tab_manager.delete_group(group_id);
            state.rebuild_tab_bar();
            state.session.mark_dirty();
        }
        Message::TabGroupClose(group_id) => {
            state.handle_group_close(group_id);
            state.session.mark_dirty();
        }
        Message::TabGroupRename(group_id) => {
            let current_name = state
                .tab_manager
                .group_by_id(group_id)
                .map(|g| g.name.clone())
                .unwrap_or_default();
            if let Some(new_name) = fltk::dialog::input_default("Group name:", &current_name) {
                state.tab_manager.rename_group(group_id, new_name);
                state.rebuild_tab_bar();
            }
            state.session.mark_dirty();
        }
        Message::TabGroupRecolor(group_id, color) => {
            state.tab_manager.recolor_group(group_id, color);
            state.rebuild_tab_bar();
            state.session.mark_dirty();
        }
        Message::TabGroupAddTab(doc_id, group_id) => {
            state.tab_manager.set_tab_group(doc_id, Some(group_id));
            state.rebuild_tab_bar();
            state.session.mark_dirty();
        }
        Message::TabGroupRemoveTab(doc_id) => {
            state.tab_manager.set_tab_group(doc_id, None);
            state.rebuild_tab_bar();
            state.session.mark_dirty();
        }
        Message::TabGroupToggle(group_id) => {
            state.tab_manager.toggle_group_collapsed(group_id);
            state.rebuild_tab_bar();
            state.session.mark_dirty();
        }
        Message::TabGroupByDrag(source_id, target_id) => {
            let target_group = state
                .tab_manager
                .documents()
                .iter()
                .find(|d| d.id == target_id)
                .and_then(|d| d.group_id);
            if let Some(gid) = target_group {
                state.tab_manager.set_tab_group(source_id, Some(gid));
            } else {
                state.tab_manager.create_group(&[target_id, source_id]);
            }
            state.rebuild_tab_bar();
            state.session.mark_dirty();
        }
        Message::TabGroupMove(group_id, to) => {
            state.tab_manager.move_group(group_id, to);
            state.rebuild_tab_bar();
            state.session.mark_dirty();
        }
        Message::TabMoveToGroup(doc_id, to, group_id) => {
            state.tab_manager.move_tab_to_group(doc_id, to, group_id);
            state.rebuild_tab_bar();
            state.session.mark_dirty();
        }
        _ => {}
    }
    DispatchResult::Continue
}

// ---------------------------------------------------------------------------
// Edit
// ---------------------------------------------------------------------------

pub fn handle_edit(msg: Message, state: &mut AppState) {
    match msg {
        Message::EditUndo => {
            let _ = state.active_buffer().undo();
        }
        Message::EditRedo => {
            let _ = state.active_buffer().redo();
        }
        Message::EditCut => {
            state.editor.cut();
        }
        Message::EditCopy => {
            state.editor.copy();
        }
        Message::EditPaste => {
            state.editor.paste();
        }
        Message::SelectAll => {
            let mut buf = state.active_buffer();
            buf.select(0, buf.length());
        }
        Message::ShowFind => {
            let theme_bg = state.highlight.highlighter().theme_background();
            show_find_dialog(&state.active_buffer(), &mut state.editor, theme_bg);
        }
        Message::ShowReplace => {
            let theme_bg = state.highlight.highlighter().theme_background();
            show_replace_dialog(&state.active_buffer(), &mut state.editor, theme_bg);
        }
        Message::ShowGoToLine => {
            let theme_bg = state.highlight.highlighter().theme_background();
            show_goto_line_dialog(&state.active_buffer(), &mut state.editor, theme_bg);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

pub fn handle_view(msg: Message, state: &mut AppState, lw: &mut LayoutWidgets) {
    match msg {
        Message::ToggleLineNumbers => state.view.toggle_line_numbers(&state.tab_manager),
        Message::ToggleWordWrap => state.view.toggle_word_wrap(),
        Message::ToggleDarkMode => {
            state.toggle_dark_mode();
            let theme_bg = state.highlight.highlighter().theme_background();
            if lw.tree_panel.is_visible() {
                lw.tree_panel.apply_theme(state.view.dark_mode, theme_bg);
            }
            if lw.split_panel.is_visible() {
                lw.split_panel.apply_theme(state.view.dark_mode, theme_bg);
            }
            if lw.terminal_panel.is_visible() {
                lw.terminal_panel
                    .apply_theme(state.view.dark_mode, theme_bg);
            }
            lw.diagnostic_panel.apply_theme(state.view.dark_mode);
            lw.status_bar.apply_theme(theme_bg);
            lw.toast.apply_theme(state.view.dark_mode);
            // Update FLTK foreground for menu text
            if state.view.dark_mode {
                fltk::app::foreground(230, 230, 230);
            } else {
                fltk::app::foreground(0, 0, 0);
            }
        }
        Message::ToggleHighlighting => state.toggle_highlighting(),
        Message::TogglePreview => state.preview_in_browser(),
        Message::SetFont(name) => {
            state.set_font(&name);
            propagate_font_to_panels(state, lw);
        }
        Message::SetFontSize(size) => {
            state.set_font_size(size);
            propagate_font_to_panels(state, lw);
        }
        Message::OpenFontPicker => {
            let theme_bg = state.highlight.highlighter().theme_background();
            let (current_name, current_size) = {
                let s = state.settings.borrow();
                (s.font.clone(), s.font_size)
            };
            if let Some((name, size)) = crate::ui::dialogs::font_picker::show_font_picker(
                &state.window,
                &current_name,
                current_size,
                theme_bg,
            ) {
                state.set_font(&name);
                state.set_font_size(size as i32);
                propagate_font_to_panels(state, lw);
            }
        }
        _ => {}
    }
}

/// Push the current editor font/size to non-editor code-display surfaces
/// (terminal panel, diagnostic panel, split panel) so they stay in sync.
fn propagate_font_to_panels(state: &AppState, lw: &mut LayoutWidgets) {
    let font = state.highlight.font();
    let size = state.highlight.font_size();
    lw.terminal_panel.set_code_font(font, size);
    lw.diagnostic_panel.set_code_font(font, size);
    lw.split_panel.set_code_font(font, size);
}

// ---------------------------------------------------------------------------
// Settings & Help
// ---------------------------------------------------------------------------

pub fn handle_settings(msg: Message, state: &mut AppState, lw: &mut LayoutWidgets) {
    match msg {
        Message::OpenSettings => {
            state.open_settings();
            let theme_bg = state.highlight.highlighter().theme_background();
            lw.status_bar.apply_theme(theme_bg);
            if lw.tree_panel.is_visible() {
                lw.tree_panel.apply_theme(state.view.dark_mode, theme_bg);
            }
            // Update foreground after settings may have changed the theme
            if state.view.dark_mode {
                fltk::app::foreground(230, 230, 230);
            } else {
                fltk::app::foreground(0, 0, 0);
            }
            // Settings dialog may have changed the editor font/size — propagate.
            propagate_font_to_panels(state, lw);
        }
        Message::CheckForUpdates => {
            let theme_bg = state.highlight.highlighter().theme_background();
            check_for_updates_ui(&state.settings, theme_bg);
        }
        Message::ShowAbout => {
            let theme_bg = state.highlight.highlighter().theme_background();
            show_about_dialog(theme_bg);
        }
        Message::ShowKeyShortcuts => state.show_key_shortcuts(),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Syntax Highlighting
// ---------------------------------------------------------------------------

pub fn handle_highlight(msg: Message, state: &mut AppState) {
    match msg {
        Message::BufferModified {
            id,
            pos,
            inserted,
            deleted,
        } => {
            if let Some(doc) = state.tab_manager.doc_by_id_mut(id) {
                doc.cached_tree = None;
                doc.cached_line_count = doc.buffer.count_lines(0, doc.buffer.length()) as usize;
            }
            state.schedule_rehighlight(id, pos);
            state.schedule_text_change_hook(id, pos, inserted, deleted);
            state.session.mark_dirty();
        }
        Message::DoRehighlight => {
            state.do_pending_rehighlight();
        }
        Message::ContinueHighlight => {
            state.continue_chunked_highlight();
        }
        Message::DoTextChangeHook => {
            state.do_pending_text_change_hook();
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Background Updates
// ---------------------------------------------------------------------------

pub fn handle_update(msg: Message, state: &mut AppState) {
    match msg {
        Message::BackgroundUpdateResult(Some(release)) => {
            state.update.receive_update(
                release,
                &mut BannerWidgets {
                    banner_frame: &mut state.update_banner_frame,
                    flex: &mut state.flex,
                    window: &mut state.window,
                },
            );
            let mut s = state.settings.borrow_mut();
            s.last_update_check = current_timestamp();
            let _ = s.save();
        }
        Message::BackgroundUpdateResult(None) => {
            let mut s = state.settings.borrow_mut();
            s.last_update_check = current_timestamp();
            let _ = s.save();
        }
        Message::ShowBannerUpdate => {
            let theme_bg = state.highlight.highlighter().theme_background();
            state.update.show_update_dialog(
                &state.settings,
                &mut BannerWidgets {
                    banner_frame: &mut state.update_banner_frame,
                    flex: &mut state.flex,
                    window: &mut state.window,
                },
                theme_bg,
            );
        }
        Message::DismissBanner => {
            state.update.dismiss_banner(&mut BannerWidgets {
                banner_frame: &mut state.update_banner_frame,
                flex: &mut state.flex,
                window: &mut state.window,
            });
        }
        Message::PreviewSyntaxTheme(theme) => state.preview_syntax_theme(theme),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

pub fn handle_plugin(msg: Message, state: &mut AppState) {
    match msg {
        Message::PluginsToggleGlobal => {
            let theme_bg = state.highlight.highlighter().theme_background();
            state.plugin_coord.handle_toggle_global(
                &mut state.plugins,
                &state.settings,
                &state.shortcut_registry,
                &mut state.widget.widget_manager,
                theme_bg,
            );
        }
        Message::PluginToggle(name) => {
            state.plugin_coord.handle_toggle(
                &mut state.plugins,
                &state.settings,
                &state.shortcut_registry,
                name,
                &mut state.widget.widget_manager,
            );
        }
        Message::PluginsReloadAll => {
            let theme_bg = state.highlight.highlighter().theme_background();
            state.plugin_coord.handle_reload(
                &mut state.plugins,
                &state.settings,
                &state.shortcut_registry,
                &mut state.widget.widget_manager,
                theme_bg,
            );
        }
        Message::CheckPluginPermissions => {
            let theme_bg = state.highlight.highlighter().theme_background();
            PluginController::check_permissions_deferred(
                &mut state.plugins,
                &state.settings,
                theme_bg,
            );
        }
        Message::PluginMenuAction {
            plugin_name,
            action,
        } => {
            state.widget.handle_plugin_menu_action(
                &plugin_name,
                &action,
                &mut state.plugins,
                &mut state.tab_manager,
                &mut state.view,
            );
        }
        Message::ShowPluginManager => {
            let theme_bg = state.highlight.highlighter().theme_background();
            state.plugin_coord.show_manager(
                &mut state.plugins,
                &state.settings,
                &state.shortcut_registry,
                theme_bg,
            );
        }
        Message::ShowPluginSettings => {
            let theme_bg = state.highlight.highlighter().theme_background();
            state.plugin_coord.show_settings(
                &state.plugins,
                &state.settings,
                &state.shortcut_registry,
                theme_bg,
            );
        }
        Message::ShowPluginConfig(name) => {
            let theme_bg = state.highlight.highlighter().theme_background();
            state.plugin_coord.show_config(
                &mut state.plugins,
                &state.settings,
                &state.shortcut_registry,
                &name,
                theme_bg,
            );
        }
        Message::CheckPluginUpdates => {
            PluginController::check_updates(&state.sender);
        }
        Message::PluginUpdatesChecked(updates) => {
            PluginController::handle_updates_checked(&state.settings, &updates);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

pub fn handle_diagnostic(msg: Message, state: &mut AppState, lw: &mut LayoutWidgets) {
    let sender = state.sender;
    match msg {
        Message::DiagnosticsUpdate(diagnostics) => {
            let is_success = diagnostics.is_empty();
            state.store_diagnostics(diagnostics.clone());
            lw.diagnostic_panel.update_diagnostics(diagnostics);
            let height = lw.diagnostic_panel.current_height();
            lw.flex.fixed(lw.diagnostic_panel.widget(), height);
            lw.flex.recalc();
            lw.wind.redraw();
            if is_success {
                defer_send(sender, 5.0, Message::DiagnosticsAutoDismiss);
            }
        }
        Message::DiagnosticsClear => {
            lw.diagnostic_panel.clear();
            let height = lw.diagnostic_panel.current_height();
            lw.flex.fixed(lw.diagnostic_panel.widget(), height);
            lw.flex.recalc();
            lw.wind.redraw();
        }
        Message::DiagnosticsAutoDismiss => {
            if lw.diagnostic_panel.is_showing_success() {
                lw.diagnostic_panel.clear();
                let height = lw.diagnostic_panel.current_height();
                lw.flex.fixed(lw.diagnostic_panel.widget(), height);
                lw.flex.recalc();
                lw.wind.redraw();
            }
        }
        Message::DiagnosticGoto(_idx) => {
            if let Some(line) = lw.diagnostic_panel.selected_line() {
                state.goto_line(line);
            }
        }
        Message::DiagnosticOpenDocs(_idx) => {
            if let Some(url) = lw.diagnostic_panel.selected_url()
                && let Err(e) = open::that(&url)
            {
                eprintln!("[diagnostic] Failed to open URL: {}", e);
            }
        }
        Message::ToggleDiagnosticsPanel => {
            if lw.diagnostic_panel.visible() {
                lw.diagnostic_panel.hide();
            } else {
                lw.diagnostic_panel.show();
            }
            let height = lw.diagnostic_panel.current_height();
            lw.flex.fixed(lw.diagnostic_panel.widget(), height);
            lw.flex.recalc();
            lw.wind.redraw();
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Annotations
// ---------------------------------------------------------------------------

pub fn handle_annotation(msg: Message, state: &mut AppState) {
    match msg {
        Message::AnnotationsUpdate(annotations) => {
            state
                .highlight
                .update_annotations(annotations, &state.tab_manager, &mut state.editor);
        }
        Message::AnnotationsClear => {
            state
                .highlight
                .clear_annotations(&mut state.tab_manager, &mut state.editor);
        }
        Message::ManualHighlight => {
            state.request_manual_highlight();
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Deferred actions
// ---------------------------------------------------------------------------

pub fn handle_deferred(
    msg: Message,
    state: &mut AppState,
    lw: &mut LayoutWidgets,
    tabs_enabled: bool,
) {
    match msg {
        Message::DeferredPluginHooks { path, content } => {
            state.run_open_hooks(path, content);
        }
        Message::DeferredTreeRefresh { path, content } => {
            fltk::app::flush();
            let content = if content.is_empty() {
                state
                    .tab_manager
                    .active_doc()
                    .map(|d| crate::app::infrastructure::buffer::buffer_text_no_leak(&d.buffer))
                    .unwrap_or_default()
            } else {
                content
            };
            state.run_tree_refresh(path, content);
        }
        Message::DeferredSessionRestore => {
            lw.toast
                .show(crate::ui::toast::ToastLevel::Info, "Restoring session...");
            let height = lw.toast.current_height();
            lw.flex.fixed(lw.toast.widget(), height);
            lw.flex.recalc();
            fltk::app::flush();

            state.restore_session();

            lw.toast.hide();
            lw.flex.fixed(lw.toast.widget(), 0);
            lw.flex.recalc();

            // If session restore left only the default empty untitled doc,
            // remove it so sync_start_page shows the start page instead.
            if tabs_enabled
                && state.tab_manager.count() == 1
                && let Some(doc) = state.tab_manager.active_doc()
                && doc.file_path.is_none()
                && !doc.is_dirty()
                && crate::app::infrastructure::buffer::buffer_text_no_leak(&doc.buffer).is_empty()
            {
                let id = doc.id;
                state.tab_manager.remove(id);
            }

            if tabs_enabled {
                state.rebuild_tab_bar();
            }
            state.start_queued_highlights();
        }
        Message::DeferredOpenFile(path) => {
            let theme_bg = state.highlight.highlighter().theme_background();
            let actions = state.file.open_file(
                path,
                &mut state.tab_manager,
                &state.settings,
                theme_bg,
                state.tabs_enabled,
            );
            state.dispatch_file_actions(actions);
        }
        Message::DeferredGotoLine(line) => {
            if let Some(doc) = state.tab_manager.active_doc() {
                let text = crate::app::infrastructure::buffer::buffer_text_no_leak(&doc.buffer);
                if let Some(pos) =
                    crate::app::services::text_ops::line_number_to_byte_position(&text, line)
                {
                    state.editor.set_insert_position(pos as i32);
                    state.editor.show_insert_position();
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

pub fn handle_toast(msg: Message, lw: &mut LayoutWidgets) {
    match msg {
        Message::ToastShow(level, text) => {
            lw.toast.show(level, &text);
            let height = lw.toast.current_height();
            lw.flex.fixed(lw.toast.widget(), height);
            lw.flex.recalc();
            lw.wind.redraw();
        }
        Message::ToastHide => {
            lw.toast.hide();
            lw.flex.fixed(lw.toast.widget(), 0);
            lw.flex.recalc();
            lw.wind.redraw();
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Split View
// ---------------------------------------------------------------------------

pub fn handle_split_view(msg: Message, state: &mut AppState, lw: &mut LayoutWidgets) {
    match msg {
        Message::SplitViewShow {
            session_id,
            plugin_name,
            request,
        } => {
            let is_tab_mode = request.display_mode == SplitDisplayMode::Tab;
            let settings = state.settings.borrow().clone();
            state.widget.show_split_view(
                session_id,
                &plugin_name,
                &request,
                &mut lw.split_panel,
                &mut state.highlight,
                &mut state.view,
                &state.tab_manager,
                &settings,
            );

            let parent = split_parent!(lw);
            if is_tab_mode {
                let full_height = parent.h() - TAB_BAR_HEIGHT;
                parent.fixed(lw.split_panel.widget(), full_height);
                if let Some(ref mut div) = lw.split_panel.divider {
                    div.hide();
                    parent.fixed(div, 0);
                }
                if let Some(ref mut tb) = state.tab_bar {
                    tb.set_diff_tab(session_id, lw.split_panel.diff_title(), true);
                }
            } else {
                let height = lw.split_panel.current_height();
                parent.fixed(lw.split_panel.widget(), height);
                if let Some(ref mut div) = lw.split_panel.divider {
                    div.show();
                    parent.fixed(div, DIVIDER_WIDTH);
                }
            }
            parent.recalc();
            lw.wind.redraw();
        }
        Message::SplitViewAccept(session_id) => {
            if let Some((path, fifo)) = state.pending_diff_reviews.remove(&session_id) {
                if let Some(ref fifo_path) = fifo {
                    // preview_edit: signal acceptance via FIFO and auto-approve in terminal
                    let _ = std::fs::write(fifo_path, "accept\n");
                    lw.terminal_panel.send_input(b"1\n");
                } else {
                    // show_diff: reload file from disk (edit already happened)
                    if let Some(doc_id) = state.tab_manager.find_by_path(&path) {
                        let actions = state.file.reload_file(doc_id, &mut state.tab_manager);
                        state.dispatch_file_actions(actions);
                    }
                }
                state
                    .widget
                    .handle_split_view_reject(session_id, &mut lw.split_panel);
            } else {
                // Plugin-based split view: existing flow
                state.widget.handle_split_view_accept(
                    session_id,
                    &mut lw.split_panel,
                    &mut state.plugins,
                    &state.tab_manager,
                    &mut state.editor,
                );
            }
            let parent = split_parent!(lw);
            parent.fixed(lw.split_panel.widget(), 0);
            if let Some(ref mut div) = lw.split_panel.divider {
                div.hide();
                parent.fixed(div, 0);
            }
            if let Some(ref mut tb) = state.tab_bar {
                tb.clear_diff_tab();
            }
            parent.recalc();
            lw.wind.redraw();
        }
        Message::SplitViewReject(session_id) => {
            if let Some((path, fifo)) = state.pending_diff_reviews.remove(&session_id) {
                if let Some(ref fifo_path) = fifo {
                    // preview_edit: signal rejection via FIFO and decline in terminal
                    let _ = std::fs::write(fifo_path, "reject\n");
                    lw.terminal_panel.send_input(b"3\n");
                } else {
                    // show_diff: update disk_mtime to suppress re-prompt
                    if let Some(doc_id) = state.tab_manager.find_by_path(&path)
                        && let Some(doc) = state.tab_manager.doc_by_id_mut(doc_id)
                    {
                        doc.disk_mtime = std::fs::metadata(&path)
                            .ok()
                            .and_then(|m| m.modified().ok());
                    }
                }
            }
            state
                .widget
                .handle_split_view_reject(session_id, &mut lw.split_panel);
            let parent = split_parent!(lw);
            parent.fixed(lw.split_panel.widget(), 0);
            if let Some(ref mut div) = lw.split_panel.divider {
                div.hide();
                parent.fixed(div, 0);
            }
            if let Some(ref mut tb) = state.tab_bar {
                tb.clear_diff_tab();
            }
            parent.recalc();
            lw.wind.redraw();
        }
        Message::SplitViewResize(mouse_y) => {
            if !lw.split_panel.is_tab_mode() {
                let parent = split_parent!(lw);
                let col_y = parent.y();
                let col_h = parent.h();
                let new_height = (col_y + col_h - mouse_y).clamp(100, col_h / 2);
                parent.fixed(lw.split_panel.widget(), new_height);
                parent.recalc();
                lw.wind.redraw();
            }
        }
        Message::DiffTabActivate(session_id) => {
            if lw.split_panel.session_id() == Some(session_id) {
                lw.split_panel.show_existing();
                let parent = split_parent!(lw);
                let full_height = parent.h() - TAB_BAR_HEIGHT;
                parent.fixed(lw.split_panel.widget(), full_height);
                if let Some(ref mut div) = lw.split_panel.divider {
                    div.hide();
                    parent.fixed(div, 0);
                }
                if let Some(ref mut tb) = state.tab_bar {
                    tb.set_diff_tab(session_id, lw.split_panel.diff_title(), true);
                }
                parent.recalc();
                lw.wind.redraw();
            }
        }
        Message::SplitViewToggleMode(session_id) => {
            if lw.split_panel.session_id() == Some(session_id) {
                let parent = split_parent!(lw);
                if lw.split_panel.is_tab_mode() {
                    lw.split_panel.set_tab_mode(false);
                    let height = lw.split_panel.current_height();
                    parent.fixed(lw.split_panel.widget(), height);
                    if let Some(ref mut div) = lw.split_panel.divider {
                        div.show();
                        parent.fixed(div, DIVIDER_WIDTH);
                    }
                    if let Some(ref mut tb) = state.tab_bar {
                        tb.clear_diff_tab();
                    }
                } else {
                    lw.split_panel.set_tab_mode(true);
                    let full_height = parent.h() - TAB_BAR_HEIGHT;
                    parent.fixed(lw.split_panel.widget(), full_height);
                    if let Some(ref mut div) = lw.split_panel.divider {
                        div.hide();
                        parent.fixed(div, 0);
                    }
                    if let Some(ref mut tb) = state.tab_bar {
                        tb.set_diff_tab(session_id, lw.split_panel.diff_title(), true);
                    }
                }
                lw.split_panel.refresh_action_buttons();
                parent.recalc();
                lw.wind.redraw();
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Tree View
// ---------------------------------------------------------------------------

pub fn handle_tree_view(msg: Message, state: &mut AppState, lw: &mut LayoutWidgets) {
    match msg {
        Message::TreeViewShow {
            session_id,
            plugin_name,
            request,
        } => {
            state.widget.show_tree_view(
                session_id,
                &plugin_name,
                &request,
                &mut lw.tree_panel,
                &state.highlight,
                &mut state.view,
                &mut state.tab_manager,
            );
            match lw.tree_position {
                TreePanelPosition::Bottom => {
                    let height = lw.tree_panel.current_height();
                    lw.flex.fixed(lw.tree_panel.widget(), height);
                    lw.flex.recalc();
                }
                TreePanelPosition::Left | TreePanelPosition::Right => {
                    let width = lw.tree_panel.current_width();
                    lw.content_row.fixed(lw.tree_panel.widget(), width);
                    if let Some(ref mut div) = lw.tree_panel.divider {
                        div.show();
                        lw.content_row.fixed(div, DIVIDER_WIDTH);
                    }
                    lw.content_row.recalc();
                }
            }
            if let Some(ref mut tb) = state.tab_bar {
                tb.handle_resize();
            }
            lw.wind.redraw();
        }
        Message::TreeViewHide(session_id) => {
            state.widget.hide_tree_view(session_id, &mut lw.tree_panel);
            match lw.tree_position {
                TreePanelPosition::Bottom => {
                    lw.flex.fixed(lw.tree_panel.widget(), 0);
                    lw.flex.recalc();
                }
                TreePanelPosition::Left | TreePanelPosition::Right => {
                    lw.content_row.fixed(lw.tree_panel.widget(), 0);
                    if let Some(ref mut div) = lw.tree_panel.divider {
                        div.hide();
                        lw.content_row.fixed(div, 0);
                    }
                    lw.content_row.recalc();
                }
            }
            if let Some(ref mut tb) = state.tab_bar {
                tb.handle_resize();
            }
            lw.wind.redraw();
        }
        Message::TreeViewLoading => {
            lw.tree_panel.show_loading();
        }
        Message::TreeViewNodeClicked {
            session_id,
            node_path,
        } => {
            state.widget.handle_tree_view_node_click(
                session_id,
                node_path,
                &mut state.plugins,
                &mut state.tab_manager,
                &mut state.view,
            );
        }
        Message::TreeViewNodeExpanded {
            session_id,
            node_path,
        } => {
            state.widget.handle_tree_view_node_expanded(
                session_id,
                node_path,
                &mut state.plugins,
                &mut state.tab_manager,
                &mut state.view,
            );
        }
        Message::TreeViewContextAction {
            session_id,
            action,
            node_path,
            input_text,
            target_path,
        } => {
            state.widget.handle_tree_view_context_action(
                session_id,
                action,
                node_path,
                input_text,
                target_path,
                &mut state.plugins,
                &mut state.tab_manager,
                &mut state.view,
            );
        }
        Message::TreeViewSearch { query } => {
            lw.tree_panel.apply_search(&query);
        }
        Message::TreeViewResize(mouse_x) => {
            if matches!(
                lw.tree_position,
                TreePanelPosition::Left | TreePanelPosition::Right
            ) {
                let content_x = lw.content_row.x();
                let content_w = lw.content_row.w();
                let max_width = content_w / 2;

                let new_width = match lw.tree_position {
                    TreePanelPosition::Left => (mouse_x - content_x).clamp(100, max_width),
                    TreePanelPosition::Right => {
                        (content_x + content_w - mouse_x).clamp(100, max_width)
                    }
                    _ => unreachable!(),
                };
                lw.content_row.fixed(lw.tree_panel.widget(), new_width);
                lw.content_row.recalc();
                if let Some(ref mut tb) = state.tab_bar {
                    tb.handle_resize();
                }
                lw.wind.redraw();
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Window events
// ---------------------------------------------------------------------------

pub fn handle_window(msg: Message, state: &mut AppState, lw: &mut LayoutWidgets) {
    match msg {
        Message::WindowResize => {
            if let Some(ref mut tab_bar) = state.tab_bar {
                tab_bar.handle_resize();
            }
            lw.terminal_panel.handle_resize();
            if lw.split_panel.is_visible() && lw.split_panel.is_tab_mode() {
                let parent = split_parent!(lw);
                let full_height = parent.h() - TAB_BAR_HEIGHT;
                parent.fixed(lw.split_panel.widget(), full_height);
                parent.recalc();
                lw.wind.redraw();
            }
        }
        Message::MallocTrim => {
            #[cfg(target_os = "linux")]
            {
                // SAFETY: malloc_trim is a glibc function that releases free memory
                // back to the OS. Passing 0 trims all reclaimable heap pages.
                // This is a no-op on non-glibc systems (guarded by cfg).
                unsafe {
                    unsafe extern "C" {
                        fn malloc_trim(pad: std::ffi::c_int) -> std::ffi::c_int;
                    }
                    malloc_trim(0);
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Terminal View
// ---------------------------------------------------------------------------

pub fn handle_terminal_view(msg: Message, state: &mut AppState, lw: &mut LayoutWidgets) {
    match msg {
        Message::TerminalViewShow {
            session_id,
            plugin_name: _,
            mut request,
        } => {
            // Default working_dir to the project root from the active document
            if request.working_dir.is_none()
                && let Some(path) = state
                    .tab_manager
                    .active_doc()
                    .and_then(|d| d.file_path.as_ref())
            {
                let project_root =
                    crate::app::plugins::security::find_project_root(std::path::Path::new(path));
                if let Some(root) = project_root {
                    request.working_dir = Some(root.to_string_lossy().to_string());
                } else if let Some(parent) = std::path::Path::new(path).parent() {
                    request.working_dir = Some(parent.to_string_lossy().to_string());
                }
            }

            let theme_bg = state.highlight.highlighter().theme_background();
            lw.terminal_panel
                .apply_theme(state.view.dark_mode, theme_bg);
            lw.terminal_panel.show_request(session_id, &request);

            // Expand window to accommodate terminal panel
            let term_width = lw.terminal_panel.current_width() + DIVIDER_WIDTH;
            let (scr_x, _scr_y, scr_w, _scr_h) = fltk::app::screen_work_area(0);
            let max_right = scr_x + scr_w;
            let current_right = lw.wind.x() + lw.wind.w();
            let available = (max_right - current_right).max(0);
            let grow = term_width.min(available);
            if grow > 0 {
                lw.wind.set_size(lw.wind.w() + grow, lw.wind.h());
                lw.flex.resize(0, 0, lw.wind.w(), lw.wind.h());
            }

            let width = lw.terminal_panel.current_width();
            lw.content_row.fixed(lw.terminal_panel.widget(), width);
            if let Some(ref mut div) = lw.terminal_panel.divider {
                div.show();
                lw.content_row.fixed(div, DIVIDER_WIDTH);
            }
            lw.content_row.recalc();
            if let Some(ref mut tb) = state.tab_bar {
                tb.handle_resize();
            }
            lw.wind.redraw();
        }
        Message::TerminalViewHide(_session_id) => {
            // Shrink window back after closing terminal panel
            let term_width = lw.terminal_panel.current_width() + DIVIDER_WIDTH;
            let (scr_x, _scr_y, _scr_w, _scr_h) = fltk::app::screen_work_area(0);
            let min_w = 400;
            let new_w = (lw.wind.w() - term_width).max(min_w);
            let new_x = lw.wind.x().max(scr_x);
            lw.wind.resize(new_x, lw.wind.y(), new_w, lw.wind.h());
            lw.flex.resize(0, 0, new_w, lw.wind.h());

            lw.terminal_panel.close();
            lw.content_row.fixed(lw.terminal_panel.widget(), 0);
            if let Some(ref mut div) = lw.terminal_panel.divider {
                div.hide();
                lw.content_row.fixed(div, 0);
            }
            lw.content_row.recalc();
            if let Some(ref mut tb) = state.tab_bar {
                tb.handle_resize();
            }
            lw.wind.redraw();
        }
        Message::TerminalOutput(_) => {
            lw.terminal_panel.process_output();
        }
        Message::TerminalExited => {
            eprintln!("[terminal] Child process exited");
        }
        Message::TerminalViewResize(mouse_x) => {
            let content_x = lw.content_row.x();
            let content_w = lw.content_row.w();
            let max_width = content_w * 4 / 5;
            let new_width = (content_x + content_w - mouse_x).clamp(200, max_width);
            lw.content_row.fixed(lw.terminal_panel.widget(), new_width);
            lw.content_row.recalc();
            lw.terminal_panel.handle_resize();
            if let Some(ref mut tb) = state.tab_bar {
                tb.handle_resize();
            }
            lw.wind.redraw();
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// MCP
// ---------------------------------------------------------------------------

pub fn handle_mcp(msg: Message, state: &mut AppState) {
    if let Message::McpRequest {
        request_id,
        json_rpc_id,
        method,
        params,
    } = msg
    {
        let response = match method.as_str() {
            "initialize" => mcp::protocol::handle_initialize(&json_rpc_id),
            "tools/list" => mcp::tools::handle_list(&json_rpc_id),
            "tools/call" => mcp::tools::handle_call(&json_rpc_id, &params, state),
            _ => mcp::protocol::json_rpc_error(&json_rpc_id, -32601, "Method not found"),
        };

        if let Some(tx) = state.mcp_responses.lock().unwrap().remove(&request_id) {
            let _ = tx.send(response);
        }
    }
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

pub fn handle_session(msg: Message, state: &mut AppState, lw: &mut LayoutWidgets) {
    match msg {
        Message::SessionShowPicker => {
            let theme_bg = state.highlight.highlighter().theme_background();
            let current = state.session.current_session_name().to_string();
            let result = show_session_picker(&lw.wind, &current, theme_bg);
            match result {
                SessionPickerResult::Switch(name) => {
                    state.switch_session(&name);
                    lw.wind.redraw();
                }
                SessionPickerResult::NewWindow(name) => {
                    if let Ok(exe) = std::env::current_exe() {
                        let _ = std::process::Command::new(exe)
                            .arg("--session")
                            .arg(&name)
                            .spawn();
                    }
                }
                SessionPickerResult::Delete(name) => {
                    if let Err(e) = session::delete_session(&name) {
                        eprintln!("Failed to delete session '{}': {}", name, e);
                    }
                }
                SessionPickerResult::Cancelled => {}
            }
        }
        Message::SessionSwitchTo(name) => {
            state.switch_session(&name);
            lw.wind.redraw();
        }
        Message::SessionOpenInNewWindow(name) => {
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe)
                    .arg("--session")
                    .arg(&name)
                    .spawn();
            }
        }
        Message::SessionSaveAs(name) => {
            if let Some(sanitized) = session::sanitize_session_name(&name) {
                // Force-save current state under the new name
                state.session.set_session_name(sanitized);
                state.session.force_save(
                    &state.tab_manager,
                    &state.settings,
                    state.file.last_open_directory.as_deref(),
                );
                state.update_window_title();
            }
        }
        Message::SessionDelete(name) => {
            if let Err(e) = session::delete_session(&name) {
                eprintln!("Failed to delete session '{}': {}", name, e);
            }
        }
        Message::SessionNewWindow => {
            let theme_bg = state.highlight.highlighter().theme_background();
            if let Some(name) = show_new_session_dialog(&lw.wind, theme_bg)
                && let Ok(exe) = std::env::current_exe()
            {
                let _ = std::process::Command::new(exe)
                    .arg("--session")
                    .arg(&name)
                    .spawn();
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Start Page sync
// ---------------------------------------------------------------------------

/// Show or hide the start page based on whether any tabs are open.
/// Called after every dispatched message in the main loop.
///
/// The start page is a child of `editor_col` (or `content_row` for Bottom tree position).
/// FLTK Flex skips hidden children in layout, so show/hide + layout() is sufficient.
pub fn sync_start_page(state: &mut AppState, lw: &mut LayoutWidgets) {
    let has_tabs = state.tab_manager.count() > 0;
    let start_visible = lw.start_page.visible();

    if has_tabs && start_visible {
        // Tabs opened — hide start page, show editor + tab bar
        lw.start_page.hide();
        state.editor.show();
        if let Some(ref mut tab_bar) = state.tab_bar {
            tab_bar.widget.show();
        }
        let parent = lw.editor_col.as_mut().unwrap_or(&mut lw.content_row);
        parent.layout();
    } else if !has_tabs && start_visible {
        // Start page visible — check if theme changed and re-render
        let theme_bg = state.highlight.highlighter().theme_background();
        if lw.start_page.last_theme_bg() != Some(theme_bg) {
            let session_name = state.session.current_session_name().to_string();
            lw.start_page.show(state.sender, theme_bg, &session_name);
            let parent = lw.editor_col.as_mut().unwrap_or(&mut lw.content_row);
            parent.layout();
            lw.wind.redraw();
        }
    } else if !has_tabs && !start_visible {
        // No tabs — hide editor + tab bar, show start page
        // Set empty buffer so the hidden editor doesn't paint stale text
        state.editor.set_buffer(fltk::text::TextBuffer::default());
        state.editor.hide();
        if let Some(ref mut tab_bar) = state.tab_bar {
            tab_bar.widget.hide();
        }
        let theme_bg = state.highlight.highlighter().theme_background();
        let session_name = state.session.current_session_name().to_string();
        lw.start_page.show(state.sender, theme_bg, &session_name);
        let parent = lw.editor_col.as_mut().unwrap_or(&mut lw.content_row);
        parent.layout();
        state.update_window_title();
        fltk::app::flush();
        lw.wind.redraw();
    }
}
