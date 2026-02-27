//! Tree panel UI for displaying hierarchical data.
//!
//! Used for showing file browsers, YAML/JSON viewers, and outline views.
//! Plugin-driven via the Widget API.

use fltk::{
    app::Sender,
    button::Button,
    enums::{Align, Color, Event, Font, FrameType, Key, Shortcut},
    frame::Frame,
    group::Flex,
    menu::{MenuButton, MenuFlag},
    prelude::*,
    tree::{Tree, TreeItem, TreeReason},
};

use crate::app::plugins::widgets::{ContextMenuItem, ContextMenuTarget, TreeClickMode, TreeNode, TreeViewRequest};
use crate::app::Message;
use super::dialogs::{darken, lighten, DialogTheme, SCROLLBAR_SIZE};

/// Height of the tree panel header (matches TAB_BAR_HEIGHT)
const HEADER_HEIGHT: i32 = 32;

/// Default height of the tree panel
const DEFAULT_HEIGHT: i32 = 200;

/// Tree panel widget for showing hierarchical data
pub struct TreePanel {
    /// The outer container (Flex column)
    pub container: Flex,
    /// Header frame showing title
    header: Frame,
    /// Tree widget
    tree: Tree,
    /// Refresh button in header
    refresh_btn: Button,
    /// Close button in header
    close_btn: Button,
    /// Message sender
    sender: Sender<Message>,
    /// Current session ID
    session_id: Option<u32>,
    /// Whether panel is currently visible
    visible: bool,
    /// Action to send on node click
    on_click_action: Option<String>,
    /// Whether to require double-click (true) or single-click (false)
    double_click: bool,
    /// Item text color for per-item styling
    item_fg: Color,
    /// Project root path from plugin (for Copy Path and context menu)
    context_path: Option<String>,
    /// Plugin-defined context menu items
    context_menu: Vec<ContextMenuItem>,
    /// Reusable context menu widget (created once, parented to container)
    ctx_menu: MenuButton,
}

impl TreePanel {
    /// Create a new tree panel
    pub fn new(sender: Sender<Message>) -> Self {
        let mut container = Flex::default().column();
        container.set_frame(FrameType::FlatBox);
        container.set_margin(0);
        container.set_pad(0);

        // Header bar with title and close button
        let mut header_row = Flex::default().row();
        header_row.set_frame(FrameType::FlatBox);
        header_row.set_margin(0);
        header_row.set_pad(0);
        header_row.set_color(Color::from_rgb(60, 60, 60));

        let mut header = Frame::default();
        header.set_frame(FrameType::FlatBox);
        header.set_color(Color::from_rgb(60, 60, 60));
        header.set_label_color(Color::White);
        header.set_label_font(Font::HelveticaBold);
        header.set_label_size(12);
        header.set_align(Align::Left | Align::Inside);
        header.set_label("  Tree View");

        let mut refresh_btn = Button::default().with_size(24, 24).with_label("\u{21BB}");
        refresh_btn.set_frame(FrameType::FlatBox);
        refresh_btn.set_color(Color::from_rgb(60, 60, 60));
        refresh_btn.set_label_color(Color::from_rgb(180, 180, 180));
        refresh_btn.set_label_size(14);
        refresh_btn.set_tooltip("Refresh");

        let mut close_btn = Button::default().with_size(24, 24).with_label("X");
        close_btn.set_frame(FrameType::FlatBox);
        close_btn.set_color(Color::from_rgb(60, 60, 60));
        close_btn.set_label_color(Color::from_rgb(180, 180, 180));
        close_btn.set_label_size(11);

        header_row.fixed(&refresh_btn, 24);
        header_row.fixed(&close_btn, 24);
        header_row.end();
        container.fixed(&header_row, HEADER_HEIGHT);

        // Tree widget
        let mut tree = Tree::default();
        tree.set_frame(FrameType::FlatBox);
        tree.set_color(Color::from_rgb(40, 40, 40));
        tree.set_selection_color(Color::from_rgb(70, 100, 130));
        tree.set_item_label_fgcolor(Color::from_rgb(220, 220, 220));
        tree.set_connector_color(Color::from_rgb(100, 100, 100));
        tree.set_margin_left(10);
        tree.set_item_draw_mode(fltk::tree::TreeItemDrawMode::LabelAndWidget);
        tree.set_item_reselect_mode(fltk::tree::TreeItemReselectMode::Always);
        tree.set_show_root(false);

        // Reusable context menu — parented to container so Wayland can
        // anchor the popup to our window via xdg_positioner.
        let mut ctx_menu = MenuButton::new(1, 1, 1, 1, None);
        ctx_menu.hide();

        container.end();
        container.hide();

        Self {
            container,
            header,
            tree,
            refresh_btn,
            close_btn,
            sender,
            session_id: None,
            visible: false,
            on_click_action: None,
            double_click: true,
            item_fg: Color::from_rgb(220, 220, 220),
            context_path: None,
            context_menu: Vec::new(),
            ctx_menu,
        }
    }

    /// Get a reference to the container widget for layout
    pub fn widget(&self) -> &Flex {
        &self.container
    }

    /// Show the tree panel with content from a plugin request
    pub fn show_request(&mut self, session_id: u32, request: &TreeViewRequest) {
        self.session_id = Some(session_id);
        self.on_click_action = request.on_click_action.clone();
        self.double_click = request.click_mode == TreeClickMode::DoubleClick;
        self.context_path = request.context_path.clone();
        self.context_menu = request.context_menu.clone();

        // Update header title
        if !request.title.is_empty() {
            self.header.set_label(&format!("  {}", request.title));
        } else {
            self.header.set_label("  Tree View");
        }

        // Clear existing tree
        self.tree.clear();

        // Populate tree from root node
        if let Some(ref root) = request.root {
            self.add_tree_node(None, root, request.expand_depth, 0);
        }

        // Set up callbacks
        self.setup_callbacks(session_id);

        // Show the panel
        self.container.show();
        self.visible = true;
        self.container.redraw();
    }

    /// Add a node to the tree recursively.
    /// `parent_path` is the full FLTK tree path of the parent node (used for
    /// `tree.add()` which interprets `/` as a hierarchy separator).
    fn add_tree_node(
        &mut self,
        parent_path: Option<&str>,
        node: &TreeNode,
        expand_depth: i32,
        current_depth: i32,
    ) {
        // Determine icon based on node properties
        let icon = if let Some(ref icon_hint) = node.icon {
            match icon_hint.as_str() {
                "folder" => "\u{1F4C1} ", // Folder emoji
                "file" => "\u{1F4C4} ",   // File emoji
                "error" => "\u{274C} ",   // Red X
                "warning" => "\u{26A0} ", // Warning
                "info" => "\u{2139} ",    // Info
                _ => "",
            }
        } else if node.has_children() {
            "\u{1F4C1} " // Folder for nodes with children
        } else {
            "\u{1F4C4} " // File for leaf nodes
        };

        let label = format!("{}{}", icon, node.label);

        // Build full FLTK tree path from root to this node
        let item_path = if let Some(pp) = parent_path {
            format!("{}/{}", pp, label)
        } else {
            label.clone()
        };

        self.tree.add(&item_path);

        // Find the item we just added
        if let Some(mut item) = self.tree.find_item(&item_path) {
            // Set text color per-item (tree-level default doesn't propagate)
            item.set_label_fgcolor(self.item_fg);

            // Set expansion state
            let should_expand = if expand_depth < 0 {
                true // -1 means expand all
            } else {
                current_depth < expand_depth
            };

            if should_expand && node.has_children() {
                item.open();
            } else {
                item.close();
            }

            // Add children recursively, passing our full path
            for child in &node.children {
                self.add_tree_node(Some(&item_path), child, expand_depth, current_depth + 1);
            }
        }
    }

    /// Build the node path from the currently selected tree item.
    /// Returns the path segments (excluding the root label and FLTK ROOT).
    fn selected_node_path(tree: &Tree) -> Option<Vec<String>> {
        let item = tree.first_selected_item()?;
        Self::node_path_for_item(&item)
    }

    /// Build the node path from any tree item (not just selected).
    /// Returns the path segments (excluding the root label and FLTK ROOT).
    fn node_path_for_item(item: &TreeItem) -> Option<Vec<String>> {
        let mut path = Vec::new();
        let mut current = Some(item.clone());
        while let Some(it) = current {
            if let Some(label) = it.label() {
                let clean_label = Self::strip_icon(&label);
                if clean_label != "ROOT" {
                    path.push(clean_label);
                }
            }
            current = it.parent();
        }
        path.reverse();
        // Skip the first element (project root node label)
        if path.len() > 1 {
            path.remove(0);
        }
        Some(path)
    }

    /// Strip icon emoji prefixes from a tree item label
    fn strip_icon(label: &str) -> String {
        label
            .trim_start_matches("\u{1F4C1} ")
            .trim_start_matches("\u{1F4C4} ")
            .trim_start_matches("\u{274C} ")
            .trim_start_matches("\u{26A0} ")
            .trim_start_matches("\u{2139} ")
            .to_string()
    }

    /// Check if a tree item represents a folder (has folder icon)
    fn is_folder_item(item: &TreeItem) -> bool {
        item.label()
            .map(|l| l.starts_with("\u{1F4C1}"))
            .unwrap_or(false)
    }

    /// Set up tree callbacks
    fn setup_callbacks(&mut self, session_id: u32) {
        let sender = self.sender;
        let on_click_action = self.on_click_action.clone();
        let double_click = self.double_click;

        // Tree callback — activate on single-click (Selected) or double-click (Reselected + event_clicks)
        self.tree.set_callback(move |tree| {
            let activate = if double_click {
                tree.callback_reason() == TreeReason::Reselected && fltk::app::event_clicks()
            } else {
                tree.callback_reason() == TreeReason::Selected
            };
            if activate {
                if let Some(path) = Self::selected_node_path(tree) {
                    if on_click_action.is_some() {
                        sender.send(Message::TreeViewNodeClicked {
                            session_id,
                            node_path: path,
                        });
                    }
                }
            }
        });

        // Enter key + right-click context menu
        let sender2 = self.sender;
        let on_click_action2 = self.on_click_action.clone();
        let context_path = self.context_path.clone();
        let context_menu = self.context_menu.clone();
        let ctx_menu_ptr = self.ctx_menu.as_widget_ptr();
        self.tree.handle(move |tree, ev| {
            // Enter key — open selected file
            if ev == Event::KeyDown && fltk::app::event_key() == Key::Enter {
                if let Some(path) = Self::selected_node_path(tree) {
                    if on_click_action2.is_some() {
                        sender2.send(Message::TreeViewNodeClicked {
                            session_id,
                            node_path: path,
                        });
                        return true;
                    }
                }
            }

            // Right-click context menu
            if ev == Event::Push && fltk::app::event_button() == 3 {
                Self::show_context_menu(tree, session_id, sender2, &context_path, &context_menu, ctx_menu_ptr);
                return true;
            }

            false
        });

        // Refresh button callback — re-scan the same folder, don't re-detect project root
        let sender3 = self.sender;
        self.refresh_btn.set_callback(move |_| {
            sender3.send(Message::TreeViewContextAction {
                session_id,
                action: "refresh".to_string(),
                node_path: vec![],
                input_text: None,
            });
        });

        // Close button callback
        let sender4 = self.sender;
        self.close_btn.set_callback(move |_| {
            sender4.send(Message::TreeViewHide(session_id));
        });
    }

    /// Show the right-click context menu built from plugin-defined items.
    /// Uses a pre-created MenuButton (parented to the container) for reliable
    /// Wayland popup positioning via xdg_positioner.
    fn show_context_menu(
        tree: &mut Tree,
        session_id: u32,
        sender: Sender<Message>,
        context_path: &Option<String>,
        context_menu: &[ContextMenuItem],
        ctx_menu_ptr: fltk::app::WidgetPtr,
    ) {
        // No menu items defined by plugin — do nothing
        if context_menu.is_empty() {
            return;
        }

        // Capture event coordinates before any other FLTK calls
        let mx = fltk::app::event_x();
        let my = fltk::app::event_y();

        // Find item under mouse and select it (visual feedback)
        let clicked_item = tree.find_clicked(true);
        if let Some(ref item) = clicked_item {
            tree.set_item_focus(item);
            tree.select_only(item, false);
        }

        // Extract info from clicked item
        let (node_path, item_name, is_folder) = if let Some(ref item) = clicked_item {
            let np = Self::node_path_for_item(item).unwrap_or_default();
            let name = item.label().map(|l| Self::strip_icon(&l)).unwrap_or_default();
            let folder = Self::is_folder_item(item);
            (np, name, folder)
        } else {
            (vec![], String::new(), false)
        };

        // Determine click target for filtering
        let click_target = match &clicked_item {
            Some(_) if is_folder => ContextMenuTarget::Folder,
            Some(_) => ContextMenuTarget::File,
            None => ContextMenuTarget::Empty,
        };

        // Build full path for clipboard actions
        let full_path = context_path.as_ref().map(|root| {
            if node_path.is_empty() {
                root.clone()
            } else {
                format!("{}/{}", root, node_path.join("/"))
            }
        });

        // Filter visible items for the clicked target type
        let visible_items: Vec<&ContextMenuItem> = context_menu
            .iter()
            .filter(|item_def| {
                item_def.target == click_target || item_def.target == ContextMenuTarget::All
            })
            .filter(|item_def| {
                // Skip clipboard items when there's no context_path
                !item_def.clipboard || full_path.is_some()
            })
            .collect();

        if visible_items.is_empty() {
            return;
        }

        // Reuse the pre-created MenuButton (parented to container) so Wayland
        // can anchor the popup to our window via xdg_positioner.
        let mut menu = unsafe { MenuButton::from_widget_ptr(ctx_menu_ptr) };
        menu.clear();
        menu.resize(mx, my, 1, 1);
        let sc = Shortcut::None;
        let fl = MenuFlag::Normal;
        for item_def in &visible_items {
            menu.add(&item_def.label, sc, fl, |_| {});
        }

        // popup() returns the chosen MenuItem (if any)
        let chosen = menu.popup();

        // Match selected item back to definition and dispatch
        if let Some(chosen) = chosen {
            let chosen_label = chosen.label().unwrap_or_default();
            if let Some(item_def) = visible_items.iter().find(|d| d.label == chosen_label) {
                if item_def.clipboard {
                    if let Some(ref fp) = full_path {
                        fltk::app::copy2(fp);
                    }
                } else if let Some(ref prompt) = item_def.input_prompt {
                    let prefill = if item_def.input_prefill_node_name {
                        &item_name
                    } else {
                        ""
                    };
                    if let Some(input) = fltk::dialog::input_default(prompt, prefill) {
                        let input = input.trim().to_string();
                        if !input.is_empty() {
                            sender.send(Message::TreeViewContextAction {
                                session_id,
                                action: item_def.action.clone(),
                                node_path: node_path.clone(),
                                input_text: Some(input),
                            });
                        }
                    }
                } else if let Some(ref prompt) = item_def.confirm_prompt {
                    let choice = fltk::dialog::choice2_default(prompt, "Cancel", "OK", "");
                    if choice == Some(1) {
                        sender.send(Message::TreeViewContextAction {
                            session_id,
                            action: item_def.action.clone(),
                            node_path: node_path.clone(),
                            input_text: None,
                        });
                    }
                } else {
                    // Plain action — send immediately
                    sender.send(Message::TreeViewContextAction {
                        session_id,
                        action: item_def.action.clone(),
                        node_path: node_path.clone(),
                        input_text: None,
                    });
                }
            }
        }
    }

    /// Hide the tree panel
    pub fn hide(&mut self) {
        self.container.hide();
        self.visible = false;
        self.session_id = None;
        self.tree.clear();
        self.on_click_action = None;
        self.double_click = true;
        self.context_path = None;
        self.context_menu.clear();
    }

    /// Check if the panel is visible
    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the current session ID
    #[allow(dead_code)]
    pub fn session_id(&self) -> Option<u32> {
        self.session_id
    }

    /// Default width when shown in left/right position
    pub const DEFAULT_WIDTH: i32 = 250;

    /// Get the current panel height for flex layout (bottom position)
    pub fn current_height(&self) -> i32 {
        if self.visible {
            HEADER_HEIGHT + DEFAULT_HEIGHT
        } else {
            0
        }
    }

    /// Get the current panel width for flex layout (left/right position)
    pub fn current_width(&self) -> i32 {
        if self.visible {
            Self::DEFAULT_WIDTH
        } else {
            0
        }
    }

    /// Apply theme colors derived from the syntax theme background.
    /// Uses DialogTheme for consistent color derivation across the app.
    pub fn apply_theme(&mut self, _is_dark: bool, theme_bg: (u8, u8, u8)) {
        let theme = DialogTheme::from_theme_bg(theme_bg);
        let (r, g, b) = theme_bg;

        // Tree background matches the editor background
        self.tree.set_color(Color::from_rgb(r, g, b));

        // Header uses the dialog/tab-bar background (darker/lighter than editor)
        self.header.set_color(theme.bg);
        self.header.set_label_color(theme.text);
        self.refresh_btn.set_color(theme.bg);
        self.refresh_btn.set_label_color(theme.text_dim);
        self.close_btn.set_color(theme.bg);
        self.close_btn.set_label_color(theme.text_dim);

        // Selection: more visible shift so it stands out from the menu background
        let selection = if theme.is_dark() {
            let (sr, sg, sb) = lighten(r, g, b, 0.10);
            Color::from_rgb(sr, sg, sb)
        } else {
            let (sr, sg, sb) = darken(r, g, b, 0.95);
            Color::from_rgb(sr, sg, sb)
        };
        self.tree.set_selection_color(selection);
        self.tree.set_select_frame(FrameType::FlatBox);

        // Item text color
        self.item_fg = theme.text;
        self.tree.set_item_label_fgcolor(theme.text);

        // Re-style existing items (theme toggle while tree is visible)
        let fg = self.item_fg;
        let mut item = self.tree.first();
        while let Some(mut it) = item {
            it.set_label_fgcolor(fg);
            item = it.next();
        }
        self.tree.set_connector_color(theme.text_dim);

        // Style context menu to match the main menu bar
        let mc = super::theme::menu_colors_from_bg(theme_bg);
        self.ctx_menu.set_frame(FrameType::FlatBox);
        self.ctx_menu.set_down_frame(FrameType::FlatBox);
        self.ctx_menu.set_text_size(fltk::app::font_size());
        self.ctx_menu.set_color(mc.color);
        self.ctx_menu.set_text_color(mc.text_color);
        self.ctx_menu.set_selection_color(mc.selection_color);

        // Style scrollbars to match the editor (Tree inherits Fl_Group)
        self.tree.set_scrollbar_size(SCROLLBAR_SIZE);
        unsafe extern "C" {
            fn Fl_Group_children(grp: *mut std::ffi::c_void) -> std::ffi::c_int;
            fn Fl_Group_child(
                grp: *mut std::ffi::c_void,
                index: std::ffi::c_int,
            ) -> *mut std::ffi::c_void;
        }
        unsafe {
            use fltk::valuator::Scrollbar;
            let group_ptr = self.tree.as_widget_ptr() as *mut std::ffi::c_void;
            let nchildren = Fl_Group_children(group_ptr);
            for i in 0..nchildren.min(2) {
                let ptr = Fl_Group_child(group_ptr, i);
                if !ptr.is_null() {
                    let mut sb = Scrollbar::from_widget_ptr(ptr as fltk::app::WidgetPtr);
                    sb.set_frame(FrameType::FlatBox);
                    sb.set_color(theme.scroll_track);
                    sb.set_slider_frame(FrameType::FlatBox);
                    sb.set_selection_color(theme.scroll_thumb);
                }
            }
        }
    }
}
