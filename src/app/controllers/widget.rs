//! Widget controller — manages tree view and split view lifecycle.
//!
//! Extracted from AppState to isolate widget management from core coordination.

use fltk::{
    app::Sender,
    prelude::*,
    text::TextEditor,
};

use super::highlight::HighlightController;
use super::hook_dispatch::{self, HookContext, process_widget_requests};
use super::tabs::TabManager;
use super::view::ViewController;
use crate::app::domain::messages::Message;
use crate::app::infrastructure::buffer::buffer_text_no_leak;
use crate::app::infrastructure::defer::defer_send;
use crate::app::plugins::{
    HookResult, PluginHook, PluginManager, WidgetActionData, WidgetManager,
};
use crate::ui::split_panel::SplitPanel;
use crate::ui::tree_panel::TreePanel;

pub struct WidgetController {
    pub widget_manager: WidgetManager,
    /// Whether the tree view is logically active (opened and not user-closed).
    /// Stays true when the tree is hidden for a non-tree-view file type.
    pub tree_view_active: bool,
    sender: Sender<Message>,
}

#[allow(clippy::too_many_arguments)]
impl WidgetController {
    pub fn new(sender: Sender<Message>) -> Self {
        Self {
            widget_manager: WidgetManager::new(),
            tree_view_active: false,
            sender,
        }
    }

    /// Show a split view panel from a plugin request.
    pub fn show_split_view(
        &self,
        session_id: u32,
        _plugin_name: &str,
        request: &crate::app::plugins::SplitViewRequest,
        split_panel: &mut SplitPanel,
        highlight: &mut HighlightController,
        view: &mut ViewController,
        tab_manager: &TabManager,
        settings: &crate::app::domain::settings::AppSettings,
    ) {
        let theme_bg = highlight.highlighter().theme_background();
        let theme_fg = highlight.highlighter().theme_foreground();
        split_panel.apply_theme(view.dark_mode, theme_bg);

        // Get syntax name from active document
        let syntax_name = tab_manager.active_doc().and_then(|d| d.syntax_name.clone());

        // Run syntect on both panes if syntax is known
        let left_syntax = syntax_name.as_ref().map(|name| {
            highlight.highlight_full(&request.left.content, name)
        });
        let right_syntax = syntax_name.as_ref().map(|name| {
            highlight.highlight_full(&request.right.content, name)
        });

        let style_table = highlight.style_table();

        let font = settings.font.to_fltk_font();
        let font_size = settings.font_size as i32;

        split_panel.show_request_with_syntax(
            session_id,
            request,
            left_syntax.as_ref(),
            right_syntax.as_ref(),
            &style_table,
            theme_bg,
            theme_fg,
            font,
            font_size,
        );
    }

    /// Hide the split view panel.
    pub fn hide_split_view(&mut self, session_id: u32, split_panel: &mut SplitPanel) {
        if split_panel.session_id() == Some(session_id) {
            split_panel.hide();
        }
        self.widget_manager.remove_session(session_id);
    }

    /// Handle split view accept action.
    pub fn handle_split_view_accept(
        &mut self,
        session_id: u32,
        split_panel: &mut SplitPanel,
        plugins: &mut PluginManager,
        tab_manager: &TabManager,
        editor: &mut TextEditor,
    ) {
        let session = match self.widget_manager.get_session(session_id) {
            Some(s) => s.clone(),
            None => return,
        };

        let right_content = Some(split_panel.right_content());

        let result = plugins.call_hook_on_plugin(
            &session.plugin_name,
            PluginHook::OnWidgetAction {
                widget_type: "split_view".to_string(),
                action: "accept".to_string(),
                session_id,
                data: WidgetActionData {
                    right_content,
                    node_path: None,
                    input_text: None,
                    content: None,
                    target_path: None,
                },
                path: tab_manager.active_doc().and_then(|d| d.file_path.clone()),
            },
        );

        if let Some(result) = result
            && let Some(content) = result.modified_content
            && let Some(mut buf) = editor.buffer()
        {
            buf.set_text(&content);
        }

        self.hide_split_view(session_id, split_panel);
    }

    /// Handle split view reject action.
    pub fn handle_split_view_reject(&mut self, session_id: u32, split_panel: &mut SplitPanel) {
        self.hide_split_view(session_id, split_panel);
    }

    /// Show a tree view panel from a plugin request.
    pub fn show_tree_view(
        &mut self,
        session_id: u32,
        plugin_name: &str,
        request: &crate::app::plugins::TreeViewRequest,
        tree_panel: &mut TreePanel,
        highlight: &HighlightController,
        view: &mut ViewController,
        tab_manager: &mut TabManager,
    ) {
        let theme_bg = highlight.highlighter().theme_background();
        tree_panel.apply_theme(view.dark_mode, theme_bg);

        // If YAML content is provided, parse it into a tree
        let final_request = if request.yaml_content.is_some() && request.root.is_none() {
            let yaml_content = request.yaml_content.as_ref().unwrap();
            let root = crate::app::services::yaml_parser::parse_yaml_to_tree(yaml_content, &request.title);
            crate::app::plugins::TreeViewRequest {
                title: request.title.clone(),
                root: Some(root),
                yaml_content: None,
                on_click_action: request.on_click_action.clone(),
                expand_depth: request.expand_depth,
                click_mode: request.click_mode,
                context_path: request.context_path.clone(),
                context_menu: request.context_menu.clone(),
                persistent: request.persistent,
            }
        } else {
            request.clone()
        };

        // Cache the parsed tree on the active document for instant tab-switch
        if let Some(doc) = tab_manager.active_doc_mut() {
            doc.cached_tree = Some((plugin_name.to_string(), final_request.clone()));
        }

        self.tree_view_active = true;
        tree_panel.show_request(session_id, &final_request);
    }

    /// Hide the tree view panel.
    pub fn hide_tree_view(&mut self, session_id: u32, tree_panel: &mut TreePanel) {
        tree_panel.hide();
        self.widget_manager.remove_session(session_id);

        // session_id 0 = system hide (file type mismatch), keep tree_view_active true
        // session_id > 0 = user clicked X, deactivate tree view
        if session_id > 0 {
            self.tree_view_active = false;
        }
    }

    /// Handle tree view node click.
    pub fn handle_tree_view_node_click(
        &mut self,
        session_id: u32,
        node_path: Vec<String>,
        plugins: &mut PluginManager,
        tab_manager: &mut TabManager,
        highlight: &mut HighlightController,
        editor: &mut TextEditor,
        view: &mut ViewController,
    ) {
        let session = match self.widget_manager.get_session(session_id) {
            Some(s) => s.clone(),
            None => return,
        };

        let plugin_name = session.plugin_name.clone();

        let current_path = tab_manager.active_doc()
            .and_then(|d| d.file_path.clone());

        let buffer_content = tab_manager.active_doc()
            .map(|d| buffer_text_no_leak(&d.buffer))
            .unwrap_or_default();

        let result = plugins.call_hook_on_plugin(
            &plugin_name,
            PluginHook::OnWidgetAction {
                widget_type: "tree_view".to_string(),
                action: "node_clicked".to_string(),
                session_id,
                data: WidgetActionData {
                    right_content: None,
                    node_path: Some(node_path),
                    input_text: None,
                    content: Some(buffer_content),
                    target_path: None,
                },
                path: current_path,
            },
        );

        if let Some(result) = result {
            process_widget_requests(&result, &plugin_name, &mut self.widget_manager, self.sender);
            let mut ctx = HookContext {
                tab_manager,
                highlight,
                editor,
                view,
                widget_manager: &mut self.widget_manager,
                sender: self.sender,
            };
            hook_dispatch::dispatch_hook_result(result, &plugin_name, &mut ctx);
        }
    }

    /// Handle tree view context menu action (new file, rename, delete, etc.).
    pub fn handle_tree_view_context_action(
        &mut self,
        session_id: u32,
        action: String,
        node_path: Vec<String>,
        input_text: Option<String>,
        target_path: Option<Vec<String>>,
        plugins: &mut PluginManager,
        tab_manager: &mut TabManager,
        highlight: &mut HighlightController,
        editor: &mut TextEditor,
        view: &mut ViewController,
    ) {
        let session = match self.widget_manager.get_session(session_id) {
            Some(s) => s.clone(),
            None => return,
        };

        let plugin_name = session.plugin_name.clone();

        let current_path = tab_manager.active_doc()
            .and_then(|d| d.file_path.clone());

        let result = plugins.call_hook_on_plugin(
            &plugin_name,
            PluginHook::OnWidgetAction {
                widget_type: "tree_view".to_string(),
                action,
                session_id,
                data: WidgetActionData {
                    right_content: None,
                    node_path: Some(node_path),
                    input_text,
                    content: None,
                    target_path,
                },
                path: current_path,
            },
        );

        if let Some(result) = result {
            process_widget_requests(&result, &plugin_name, &mut self.widget_manager, self.sender);
            let mut ctx = HookContext {
                tab_manager,
                highlight,
                editor,
                view,
                widget_manager: &mut self.widget_manager,
                sender: self.sender,
            };
            hook_dispatch::dispatch_hook_result(result, &plugin_name, &mut ctx);
        }
    }

    /// Handle a plugin's custom menu action.
    pub fn handle_plugin_menu_action(
        &mut self,
        plugin_name: &str,
        action: &str,
        plugins: &mut PluginManager,
        tab_manager: &mut TabManager,
        highlight: &mut HighlightController,
        editor: &mut TextEditor,
        view: &mut ViewController,
    ) {
        // If any tree view is already open, remove it so process_widget_requests
        // will create a fresh one (refresh, not toggle off).
        if let Some(existing_id) = self.widget_manager.any_tree_view_session() {
            let is_persistent = self.widget_manager.get_session(existing_id)
                .is_some_and(|s| s.persistent);
            if !is_persistent {
                self.sender.send(Message::TreeViewHide(existing_id));
            }
        }

        // Get current document info for the hook
        let path = tab_manager.active_doc().and_then(|d| {
            d.file_path.as_ref().cloned()
        });
        let content = tab_manager.active_doc()
            .map(|d| buffer_text_no_leak(&d.buffer))
            .unwrap_or_default();
        let hook = PluginHook::OnMenuAction {
            action: action.to_string(),
            path,
            content,
        };

        let result = plugins.call_hook_on_plugin(plugin_name, hook);

        if let Some(result) = result {
            eprintln!(
                "[debug:menu] hook returned: split_view={}, tree_view={}, status_msg={:?}, modified_content={}",
                result.split_view.is_some(),
                result.tree_view.is_some(),
                result.status_message.as_ref().map(|m| &m.text),
                result.modified_content.is_some(),
            );

            process_widget_requests(&result, plugin_name, &mut self.widget_manager, self.sender);
            let mut ctx = HookContext {
                tab_manager,
                highlight,
                editor,
                view,
                widget_manager: &mut self.widget_manager,
                sender: self.sender,
            };
            hook_dispatch::dispatch_hook_result(result, plugin_name, &mut ctx);
        } else {
            eprintln!(
                "[plugins] Plugin '{}' not found or not enabled for action '{}'",
                plugin_name, action
            );
        }
    }

    /// Refresh the tree view panel for the current active document.
    pub fn refresh_tree_view_for_active_doc(
        &mut self,
        tab_manager: &TabManager,
    ) {
        // Handle persistent + non-persistent coexistence
        if let Some(persistent_id) = self.widget_manager.persistent_tree_session() {
            if let Some(non_persistent_id) = self.widget_manager.non_persistent_tree_session() {
                self.widget_manager.remove_session(non_persistent_id);
                self.widget_manager.remove_session(persistent_id);
                let path = tab_manager.active_doc().and_then(|doc| doc.file_path.clone());
                self.sender.send(Message::TreeViewLoading);
                defer_send(self.sender, 0.0, Message::DeferredTreeRefresh { path, content: String::new() });
                return;
            }
            // Only persistent, no overlay — nothing to do
            return;
        }

        let existing_id = self.widget_manager.any_tree_view_session();

        // If active doc has a cached tree, show it
        if self.tree_view_active
            && let Some((cached_plugin, cached_request)) = tab_manager.active_doc()
                .and_then(|doc| doc.cached_tree.clone())
        {
            if let Some(id) = existing_id {
                self.widget_manager.remove_session(id);
            }
            let persistent = cached_request.persistent;
            let new_id = self.widget_manager.create_tree_view_session(&cached_plugin, persistent);
            self.sender.send(Message::TreeViewShow {
                session_id: new_id,
                plugin_name: cached_plugin,
                request: cached_request,
            });
            return;
        }

        // No cached tree — need an existing session to proceed
        let existing_id = match existing_id {
            Some(id) => id,
            None => {
                if self.tree_view_active {
                    let path = tab_manager.active_doc().and_then(|doc| doc.file_path.clone());
                    self.sender.send(Message::TreeViewLoading);
                    defer_send(self.sender, 0.0, Message::DeferredTreeRefresh { path, content: String::new() });
                }
                return;
            }
        };

        // Cache miss: defer to avoid blocking tab switch
        let path = tab_manager.active_doc().and_then(|doc| doc.file_path.clone());
        self.widget_manager.remove_session(existing_id);
        self.sender.send(Message::TreeViewLoading);
        defer_send(self.sender, 0.0, Message::DeferredTreeRefresh { path, content: String::new() });
    }

    /// Process widget requests from a hook result (convenience wrapper).
    pub fn process_widget_requests(&mut self, result: &HookResult, plugin_name: &str) {
        process_widget_requests(result, plugin_name, &mut self.widget_manager, self.sender);
    }
}
