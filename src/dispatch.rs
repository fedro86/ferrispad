//! Dispatch handler functions for the main event loop.
//!
//! Each function handles a group of related `Message` variants,
//! keeping `main.rs` as a thin dispatcher.

use fltk::prelude::*;

use crate::split_parent;

use crate::app::controllers::plugin::PluginController;
use crate::app::controllers::update::BannerWidgets;
use crate::app::domain::messages::Message;
use crate::app::infrastructure::defer::defer_send;
use crate::app::plugins::widgets::SplitDisplayMode;
use crate::app::services::updater::current_timestamp;
use crate::app::state::AppState;
use crate::ui::dialogs::about::show_about_dialog;
use crate::ui::dialogs::find::{show_find_dialog, show_replace_dialog};
use crate::ui::dialogs::goto_line::show_goto_line_dialog;
use crate::ui::dialogs::update::check_for_updates_ui;
use crate::ui::main_window::LayoutWidgets;
use crate::ui::split_panel::SplitPanel;
use crate::ui::tab_bar::TAB_BAR_HEIGHT;
use crate::ui::tree_panel::TreePanel;
use crate::app::domain::settings::TreePanelPosition;

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
            let actions = state.file.file_new(&mut state.tab_manager, state.tabs_enabled);
            state.dispatch_file_actions(actions);
            state.session.mark_dirty();
        }
        Message::FileOpen => {
            let theme_bg = state.highlight.highlighter().theme_background();
            let actions = state.file.file_open(
                &mut state.tab_manager, &state.settings, theme_bg,
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
            let actions = state.file.file_save_as(
                &mut state.tab_manager, &state.plugins, state.tabs_enabled,
            );
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
                && tb.is_diff_tab_active() && lw.split_panel.is_tab_mode()
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
            if state.close_tab(id) {
                if state.file_quit() {
                    return DispatchResult::Quit;
                }
            } else {
                state.session.mark_dirty();
            }
        }
        Message::TabCloseActive => {
            if let Some(id) = state.tab_manager.active_id()
                && state.close_tab(id)
            {
                if state.file_quit() {
                    return DispatchResult::Quit;
                }
            } else {
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
            let current_name = state.tab_manager.group_by_id(group_id)
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
            let target_group = state.tab_manager.documents()
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
        Message::EditUndo => { let _ = state.active_buffer().undo(); }
        Message::EditRedo => { let _ = state.active_buffer().redo(); }
        Message::EditCut => { state.editor.cut(); }
        Message::EditCopy => { state.editor.copy(); }
        Message::EditPaste => { state.editor.paste(); }
        Message::SelectAll => {
            let mut buf = state.active_buffer();
            buf.select(0, buf.length());
        }
        Message::ShowFind => show_find_dialog(&state.active_buffer(), &mut state.editor),
        Message::ShowReplace => show_replace_dialog(&state.active_buffer(), &mut state.editor),
        Message::ShowGoToLine => show_goto_line_dialog(&state.active_buffer(), &mut state.editor),
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
            lw.diagnostic_panel.apply_theme(state.view.dark_mode);
            lw.toast.apply_theme(state.view.dark_mode);
        }
        Message::ToggleHighlighting => state.toggle_highlighting(),
        Message::TogglePreview => state.preview_in_browser(),
        Message::SetFont(font) => state.view.set_font(font),
        Message::SetFontSize(size) => state.view.set_font_size(size),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Settings & Help
// ---------------------------------------------------------------------------

pub fn handle_settings(msg: Message, state: &mut AppState, lw: &mut LayoutWidgets) {
    match msg {
        Message::OpenSettings => {
            state.open_settings();
            if lw.tree_panel.is_visible() {
                let theme_bg = state.highlight.highlighter().theme_background();
                lw.tree_panel.apply_theme(state.view.dark_mode, theme_bg);
            }
        }
        Message::CheckForUpdates => check_for_updates_ui(&state.settings),
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
        Message::BufferModified(id, pos) => {
            if let Some(doc) = state.tab_manager.doc_by_id_mut(id) {
                doc.cached_tree = None;
                doc.cached_line_count = doc.buffer.count_lines(0, doc.buffer.length()) as usize;
            }
            state.schedule_rehighlight(id, pos);
            state.session.mark_dirty();
        }
        Message::DoRehighlight => {
            state.do_pending_rehighlight();
        }
        Message::ContinueHighlight => {
            state.continue_chunked_highlight();
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
            state.update.receive_update(release, &mut BannerWidgets {
                banner_frame: &mut state.update_banner_frame,
                flex: &mut state.flex,
                window: &mut state.window,
            });
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
            state.update.show_update_dialog(&state.settings, &mut BannerWidgets {
                banner_frame: &mut state.update_banner_frame,
                flex: &mut state.flex,
                window: &mut state.window,
            });
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
            state.plugin_coord.handle_toggle_global(
                &mut state.plugins, &state.settings, &state.shortcut_registry,
            );
        }
        Message::PluginToggle(name) => {
            state.plugin_coord.handle_toggle(
                &mut state.plugins, &state.settings, &state.shortcut_registry, name,
            );
        }
        Message::PluginsReloadAll => {
            state.plugin_coord.handle_reload(
                &mut state.plugins, &state.settings, &state.shortcut_registry,
            );
        }
        Message::CheckPluginPermissions => {
            PluginController::check_permissions_deferred(&mut state.plugins, &state.settings);
        }
        Message::PluginMenuAction { plugin_name, action } => {
            state.widget.handle_plugin_menu_action(
                &plugin_name, &action,
                &mut state.plugins, &mut state.tab_manager,
                &mut state.highlight, &mut state.editor, &mut state.view,
            );
        }
        Message::ShowPluginManager => {
            let theme_bg = state.highlight.highlighter().theme_background();
            state.plugin_coord.show_manager(
                &mut state.plugins, &state.settings, &state.shortcut_registry, theme_bg,
            );
        }
        Message::ShowPluginSettings => {
            let theme_bg = state.highlight.highlighter().theme_background();
            state.plugin_coord.show_settings(
                &state.plugins, &state.settings, &state.shortcut_registry, theme_bg,
            );
        }
        Message::ShowPluginConfig(name) => {
            let theme_bg = state.highlight.highlighter().theme_background();
            state.plugin_coord.show_config(
                &mut state.plugins, &state.settings, &state.shortcut_registry, &name, theme_bg,
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
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Annotations
// ---------------------------------------------------------------------------

pub fn handle_annotation(msg: Message, state: &mut AppState) {
    match msg {
        Message::AnnotationsUpdate(annotations) => {
            state.highlight.update_annotations(annotations, &state.tab_manager, &mut state.editor);
        }
        Message::AnnotationsClear => {
            state.highlight.clear_annotations(&mut state.tab_manager, &mut state.editor);
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
                state.tab_manager.active_doc()
                    .map(|d| crate::app::infrastructure::buffer::buffer_text_no_leak(&d.buffer))
                    .unwrap_or_default()
            } else {
                content
            };
            state.run_tree_refresh(path, content);
        }
        Message::DeferredSessionRestore => {
            lw.toast.show(crate::ui::toast::ToastLevel::Info, "Restoring session...");
            let height = lw.toast.current_height();
            lw.flex.fixed(lw.toast.widget(), height);
            lw.flex.recalc();
            fltk::app::flush();

            state.restore_session();

            lw.toast.hide();
            lw.flex.fixed(lw.toast.widget(), 0);
            lw.flex.recalc();

            if tabs_enabled {
                state.rebuild_tab_bar();
            }
            state.start_queued_highlights();
        }
        Message::DeferredOpenFile(path) => {
            let theme_bg = state.highlight.highlighter().theme_background();
            let actions = state.file.open_file(
                path, &mut state.tab_manager, &state.settings,
                theme_bg, state.tabs_enabled,
            );
            state.dispatch_file_actions(actions);
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
        Message::SplitViewShow { session_id, plugin_name, request } => {
            let is_tab_mode = request.display_mode == SplitDisplayMode::Tab;
            let settings = state.settings.borrow().clone();
            state.widget.show_split_view(
                session_id, &plugin_name, &request, &mut lw.split_panel,
                &mut state.highlight, &mut state.view, &state.tab_manager, &settings,
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
                    parent.fixed(div, SplitPanel::DIVIDER_HEIGHT);
                }
            }
            parent.recalc();
            lw.wind.redraw();
        }
        Message::SplitViewAccept(session_id) => {
            state.widget.handle_split_view_accept(
                session_id, &mut lw.split_panel,
                &mut state.plugins, &state.tab_manager, &mut state.editor,
            );
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
            state.widget.handle_split_view_reject(session_id, &mut lw.split_panel);
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
                        parent.fixed(div, SplitPanel::DIVIDER_HEIGHT);
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
        Message::TreeViewShow { session_id, plugin_name, request } => {
            state.widget.show_tree_view(
                session_id, &plugin_name, &request, &mut lw.tree_panel,
                &state.highlight, &mut state.view, &mut state.tab_manager,
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
                        lw.content_row.fixed(div, TreePanel::DIVIDER_WIDTH);
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
        Message::TreeViewNodeClicked { session_id, node_path } => {
            state.widget.handle_tree_view_node_click(
                session_id, node_path,
                &mut state.plugins, &mut state.tab_manager,
                &mut state.highlight, &mut state.editor, &mut state.view,
            );
        }
        Message::TreeViewContextAction { session_id, action, node_path, input_text, target_path } => {
            state.widget.handle_tree_view_context_action(
                session_id, action, node_path, input_text, target_path,
                &mut state.plugins, &mut state.tab_manager,
                &mut state.highlight, &mut state.editor, &mut state.view,
            );
        }
        Message::TreeViewSearch { query } => {
            lw.tree_panel.apply_search(&query);
        }
        Message::TreeViewResize(mouse_x) => {
            if matches!(lw.tree_position, TreePanelPosition::Left | TreePanelPosition::Right) {
                let content_x = lw.content_row.x();
                let content_w = lw.content_row.w();
                let max_width = content_w / 2;

                let new_width = match lw.tree_position {
                    TreePanelPosition::Left => {
                        (mouse_x - content_x).clamp(100, max_width)
                    }
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
                unsafe {
                    unsafe extern "C" { fn malloc_trim(pad: std::ffi::c_int) -> std::ffi::c_int; }
                    malloc_trim(0);
                }
            }
        }
        _ => {}
    }
}
