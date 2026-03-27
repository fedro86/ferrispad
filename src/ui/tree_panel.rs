//! Tree panel UI for displaying hierarchical data.
//!
//! Used for showing file browsers, YAML/JSON viewers, and outline views.
//! Plugin-driven via the Widget API.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use fltk::{
    app::Sender,
    button::Button,
    enums::{Align, Color, Cursor, Event, Font, FrameType, Key, Shortcut},
    frame::Frame,
    group::Flex,
    input::Input,
    menu::{MenuButton, MenuFlag},
    prelude::*,
    tree::{Tree, TreeItem, TreeReason},
};

use super::dialogs::{DialogTheme, SCROLLBAR_SIZE, darken, lighten};
use super::theme::divider_color_from_bg;
use crate::app::Message;
use crate::app::plugins::widgets::{
    ContextMenuItem, ContextMenuTarget, TreeClickMode, TreeNode, TreeViewRequest,
};

/// Height of the tree panel header (matches TAB_BAR_HEIGHT)
const HEADER_HEIGHT: i32 = 32;

/// Height of the search bar row
const SEARCH_HEIGHT: i32 = 24;

/// Default height of the tree panel
const DEFAULT_HEIGHT: i32 = 200;

/// Placeholder text for the search input (magnifying glass + hint)
const SEARCH_PLACEHOLDER: &str = "\u{1F50D} Find...";

/// Sentinel label used on placeholder children of lazy-load nodes.
const LAZY_PLACEHOLDER_LABEL: &str = "\u{231B}"; // hourglass

/// Tree panel widget for showing hierarchical data
pub struct TreePanel {
    /// The outer container (Flex column)
    pub container: Flex,
    /// Header frame showing title
    header: Frame,
    /// Search row containing the search input
    search_row: Flex,
    /// Search input below header
    search_input: Input,
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
    /// Draggable divider between tree panel and editor (4px, managed externally).
    /// None until `create_divider()` is called from main_window.rs.
    pub divider: Option<Frame>,
    /// Stored root node for search/filter rebuilds
    current_nodes: Option<TreeNode>,
    /// Stored expand depth for search/filter rebuilds
    current_expand_depth: i32,
    /// Whether the current theme is dark (for resolving semantic label colors)
    is_dark: bool,
    /// Map of FLTK tree item paths to their resolved custom colors.
    /// Used to re-apply per-item colors after selection (FLTK's fl_contrast
    /// can override per-item labelfgcolor for selected items).
    colored_items: Rc<RefCell<Vec<(String, Color)>>>,
    /// Node path of the item being dragged (set on Push, used on Released)
    drag_source_path: Rc<RefCell<Option<Vec<String>>>>,
    /// Whether a drag is in progress (Push captured source, Drag detected movement)
    dragging: Rc<Cell<bool>>,
    /// Set of expanded folder paths (semantic node paths like "src/ui/dialogs").
    /// Maintained via TreeReason::Opened/Closed callbacks. Used to preserve
    /// expansion state across tree rebuilds (refresh, move, rename, etc.).
    expanded_paths: Rc<RefCell<HashSet<String>>>,
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

        // Title takes remaining space; only buttons are fixed
        header_row.fixed(&refresh_btn, 24);
        header_row.fixed(&close_btn, 24);
        header_row.end();
        container.fixed(&header_row, HEADER_HEIGHT);

        // Search row (separate from header, below it)
        let mut search_row = Flex::default().row();
        search_row.set_frame(FrameType::FlatBox);
        search_row.set_margin(0);
        search_row.set_pad(0);
        search_row.set_color(Color::from_rgb(50, 50, 50));

        let mut search_input = Input::default();
        search_input.set_frame(FrameType::FlatBox);
        search_input.set_color(Color::from_rgb(50, 50, 50));
        search_input.set_text_color(Color::from_rgb(160, 160, 160));
        search_input.set_text_size(13);
        search_input.set_value(SEARCH_PLACEHOLDER);
        search_input.set_tooltip("Search tree...");

        search_row.end();
        container.fixed(&search_row, SEARCH_HEIGHT);

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
        tree.set_select_frame(FrameType::FlatBox);
        tree.set_show_root(false);

        // Reusable context menu — parented to container so Wayland can
        // anchor the popup to our window via xdg_positioner.
        let mut ctx_menu = MenuButton::new(1, 1, 1, 1, None);
        ctx_menu.hide();

        container.end();
        container.hide();

        // Search input callback — placeholder text + search on each key press
        let search_sender = sender;
        let active_text_color = Color::from_rgb(220, 220, 220);
        let dim_color = Color::from_rgb(160, 160, 160);
        search_input.handle(move |input, ev| {
            match ev {
                Event::Focus => {
                    // Clear placeholder on focus
                    if input.value() == SEARCH_PLACEHOLDER {
                        input.set_value("");
                        input.set_text_color(active_text_color);
                    }
                    false // let FLTK handle focus
                }
                Event::Unfocus => {
                    // Restore placeholder if empty
                    if input.value().is_empty() {
                        input.set_value(SEARCH_PLACEHOLDER);
                        input.set_text_color(dim_color);
                    }
                    false
                }
                Event::KeyUp => {
                    let query = input.value();
                    // Don't search for placeholder text
                    if query != SEARCH_PLACEHOLDER {
                        search_sender.send(Message::TreeViewSearch { query });
                    }
                    false
                }
                _ => false,
            }
        });

        Self {
            container,
            header,
            search_row,
            search_input,
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
            divider: None,
            current_nodes: None,
            current_expand_depth: 2,
            is_dark: true,
            colored_items: Rc::new(RefCell::new(Vec::new())),
            drag_source_path: Rc::new(RefCell::new(None)),
            dragging: Rc::new(Cell::new(false)),
            expanded_paths: Rc::new(RefCell::new(HashSet::new())),
        }
    }

    /// Create the draggable divider widget. Must be called within the parent
    /// Flex group (content_row) so the divider is properly parented.
    /// Call this after inserting the tree panel container into the layout.
    pub fn create_divider(&mut self, sender: Sender<Message>) {
        let mut divider = Frame::default();
        divider.set_frame(FrameType::FlatBox);
        divider.hide();

        let dragging = Rc::new(Cell::new(false));
        let drag_flag = dragging.clone();
        divider.handle(move |div, ev| match ev {
            Event::Enter => {
                if let Some(mut win) = div.window() {
                    win.set_cursor(Cursor::WE);
                }
                true
            }
            Event::Leave => {
                if !drag_flag.get()
                    && let Some(mut win) = div.window()
                {
                    win.set_cursor(Cursor::Default);
                }
                true
            }
            Event::Push => {
                drag_flag.set(true);
                true
            }
            Event::Drag => {
                if drag_flag.get() {
                    let mouse_x = fltk::app::event_x();
                    sender.send(Message::TreeViewResize(mouse_x));
                }
                true
            }
            Event::Released => {
                drag_flag.set(false);
                if let Some(mut win) = div.window() {
                    win.set_cursor(Cursor::Default);
                }
                true
            }
            _ => false,
        });

        self.divider = Some(divider);
    }


    /// Get a reference to the container widget for layout
    pub fn widget(&self) -> &Flex {
        &self.container
    }

    /// Convert a semantic node path (e.g., ["src", "ui"]) to a key for the
    /// expanded_paths set.
    fn node_path_key(node_path: &[String]) -> String {
        node_path.join("/")
    }

    /// Show the tree panel with content from a plugin request
    pub fn show_request(&mut self, session_id: u32, request: &TreeViewRequest) {
        // On first show, clear expanded_paths so it gets populated from expand_depth.
        // On refresh (already visible), keep the existing set.
        if !self.visible {
            self.expanded_paths.borrow_mut().clear();
        }

        self.session_id = Some(session_id);
        self.on_click_action = request.on_click_action.clone();
        self.double_click = request.click_mode == TreeClickMode::DoubleClick;
        self.context_path = request.context_path.clone();
        self.context_menu = request.context_menu.clone();

        // Store root node for search/filter rebuilds
        self.current_nodes = request.root.clone();
        self.current_expand_depth = request.expand_depth;
        self.search_input.set_value(SEARCH_PLACEHOLDER);
        self.search_input
            .set_text_color(Color::from_rgb(160, 160, 160));

        // Update header title
        if !request.title.is_empty() {
            self.header.set_label(&format!("  {}", request.title));
        } else {
            self.header.set_label("  Tree View");
        }

        // Clear existing tree and color tracking
        self.tree.clear();
        self.colored_items.borrow_mut().clear();

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

    /// Show a "Loading..." placeholder in the tree panel.
    /// Keeps the panel visible (no hide/show flash) while new content is being prepared.
    pub fn show_loading(&mut self) {
        self.header.set_label("  Loading...");
        self.tree.clear();
        self.colored_items.borrow_mut().clear();
        self.container.redraw();
    }

    /// Resolve a semantic label color name to an FLTK Color, theme-aware.
    fn resolve_label_color(&self, name: &str) -> Option<Color> {
        let (r, g, b) = if self.is_dark {
            match name {
                "modified" => (229, 192, 123),
                "added" | "untracked" => (152, 195, 121),
                "conflict" => (224, 108, 117),
                "ignored" => (120, 120, 120),
                _ => return None,
            }
        } else {
            match name {
                "modified" => (190, 140, 40),
                "added" | "untracked" => (60, 140, 50),
                "conflict" => (190, 50, 50),
                "ignored" => (160, 160, 160),
                _ => return None,
            }
        };
        Some(Color::from_rgb(r, g, b))
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
                "none" => "",             // No icon (e.g., YAML/JSON structured data)
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

        if let Some(mut item) = self.tree.add(&item_path) {
            // Set text color: use semantic label_color if set, else default item_fg
            let custom_color = node
                .label_color
                .as_deref()
                .and_then(|name| self.resolve_label_color(name));
            let fg = custom_color.unwrap_or(self.item_fg);
            item.set_label_fgcolor(fg);

            // Track custom-colored items for re-apply after selection
            if let Some(color) = custom_color {
                self.colored_items
                    .borrow_mut()
                    .push((item_path.clone(), color));
            }

            // Build semantic node path for this item (e.g., "src/ui/dialogs")
            // by stripping icons from the FLTK path components and skipping root
            let semantic_path: Vec<String> = item_path
                .split('/')
                .skip(1) // skip project root label
                .map(Self::strip_icon)
                .collect();
            let semantic_key = semantic_path.join("/");

            // Decide expansion: if expanded_paths is populated (refresh),
            // use it; otherwise fall back to expand_depth (first show).
            let has_saved_state = !self.expanded_paths.borrow().is_empty();
            let should_expand = if !node.has_children() {
                false
            } else if has_saved_state {
                self.expanded_paths.borrow().contains(&semantic_key)
            } else if expand_depth < 0 {
                true // -1 means expand all
            } else {
                current_depth < expand_depth
            };

            if should_expand {
                item.open();
                self.expanded_paths.borrow_mut().insert(semantic_key);
            } else {
                item.close();
            }

            // Add children recursively, or a placeholder for lazy nodes
            if node.lazy && node.children.is_empty() {
                // Lazy node: add a placeholder child so FLTK shows expand arrow
                self.tree.add(&format!("{}/{}", item_path, LAZY_PLACEHOLDER_LABEL));
            } else {
                for child in &node.children {
                    self.add_tree_node(Some(&item_path), child, expand_depth, current_depth + 1);
                }
            }

            // Re-apply open state after children are added (FLTK may reset
            // the state when children are added to a closed node via add())
            if should_expand && node.has_children() {
                item.open();
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
        // Skip the first element (project root node label).
        // When clicking the root itself, this produces an empty vec,
        // which means "root directory" to the plugin/clipboard logic.
        if !path.is_empty() {
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

    /// Check if a tree item represents a folder/container (has folder icon or has children)
    fn is_folder_item(item: &TreeItem) -> bool {
        item.label()
            .map(|l| l.starts_with("\u{1F4C1}"))
            .unwrap_or(false)
            || item.children() > 0
    }

    /// Set up tree callbacks
    fn setup_callbacks(&mut self, session_id: u32) {
        let sender = self.sender;
        let on_click_action = self.on_click_action.clone();
        let double_click = self.double_click;

        // Tree callback — activate on click + track expand/collapse
        let expanded_paths = self.expanded_paths.clone();
        self.tree.set_callback(move |tree| {
            let reason = tree.callback_reason();

            // Track user-initiated expand/collapse for state preservation
            if reason == TreeReason::Opened || reason == TreeReason::Closed {
                if let Some(item) = tree.callback_item()
                    && let Some(node_path) = Self::node_path_for_item(&item)
                {
                    let key = Self::node_path_key(&node_path);
                    if reason == TreeReason::Opened {
                        expanded_paths.borrow_mut().insert(key.clone());
                        // Check if this node has a lazy placeholder child
                        if let Some(first_child) = item.next() {
                            let child_label = first_child.label().unwrap_or_default();
                            if child_label.contains(LAZY_PLACEHOLDER_LABEL) {
                                sender.send(Message::TreeViewNodeExpanded {
                                    session_id,
                                    node_path,
                                });
                            }
                        }
                    } else {
                        expanded_paths.borrow_mut().remove(&key);
                    }
                }
                return;
            }

            let activate = if double_click {
                reason == TreeReason::Reselected && fltk::app::event_clicks()
            } else {
                reason == TreeReason::Selected
            };
            if activate
                && let Some(path) = Self::selected_node_path(tree)
                && on_click_action.is_some()
            {
                sender.send(Message::TreeViewNodeClicked {
                    session_id,
                    node_path: path,
                });
            }
        });

        // Drag-and-drop + Enter key + right-click context menu + color re-apply
        let sender2 = self.sender;
        let on_click_action2 = self.on_click_action.clone();
        let context_path = self.context_path.clone();
        let context_menu = self.context_menu.clone();
        let ctx_menu_ptr = self.ctx_menu.as_widget_ptr();
        let colored_items = self.colored_items.clone();
        let drag_source = self.drag_source_path.clone();
        let dragging = self.dragging.clone();
        self.tree.handle(move |tree, ev| {
            match ev {
                // Left-click: record potential drag source BEFORE FLTK processes
                Event::Push if fltk::app::event_button() == 1 => {
                    // Capture the item under the mouse as potential drag source
                    let item = tree.find_clicked(true);
                    if let Some(ref it) = item {
                        *drag_source.borrow_mut() = Self::node_path_for_item(it);
                    } else {
                        *drag_source.borrow_mut() = None;
                    }
                    dragging.set(false);

                    // Re-apply per-item colors after FLTK processes the selection
                    let ci = colored_items.clone();
                    let tp = tree.as_widget_ptr();
                    fltk::app::add_timeout3(0.0, move |_| {
                        // SAFETY: tp is a valid Tree widget pointer obtained via
                        // as_widget_ptr() above. The Tree outlives this timeout
                        // callback (it's owned by the panel which owns this handler).
                        let t = unsafe { Tree::from_widget_ptr(tp) };
                        for (path, color) in ci.borrow().iter() {
                            if let Some(mut item) = t.find_item(path) {
                                item.set_label_fgcolor(*color);
                            }
                        }
                    });

                    false // let FLTK handle normal selection
                }

                // Right-click context menu
                Event::Push if fltk::app::event_button() == 3 => {
                    Self::show_context_menu(
                        tree,
                        session_id,
                        sender2,
                        &context_path,
                        &context_menu,
                        ctx_menu_ptr,
                    );
                    true
                }

                // Mouse movement with button held: mark as dragging
                Event::Drag => {
                    if drag_source.borrow().is_some() {
                        dragging.set(true);
                    }
                    false // don't consume — let FLTK handle scroll etc.
                }

                // Mouse released: if dragging, detect drop target and send move
                Event::Released if dragging.get() => {
                    dragging.set(false);
                    let source_node_path = drag_source.borrow_mut().take();

                    if let Some(source_node_path) = source_node_path
                        && !source_node_path.is_empty()
                    {
                        // Find the item under the mouse at drop time
                        let target_node_path = match tree.find_clicked(true) {
                            Some(ref item) => Self::node_path_for_item(item).unwrap_or_default(),
                            None => vec![], // Dropped on empty area → project root
                        };

                        // Don't move onto self
                        if source_node_path != target_node_path {
                            sender2.send(Message::TreeViewContextAction {
                                session_id,
                                action: "move".to_string(),
                                node_path: source_node_path,
                                input_text: None,
                                target_path: Some(target_node_path),
                            });
                            return true;
                        }
                    }
                    false
                }

                // Normal release (no drag): clear source
                Event::Released => {
                    *drag_source.borrow_mut() = None;
                    false
                }

                // Enter key — open selected file
                Event::KeyDown if fltk::app::event_key() == Key::Enter => {
                    if let Some(path) = Self::selected_node_path(tree)
                        && on_click_action2.is_some()
                    {
                        sender2.send(Message::TreeViewNodeClicked {
                            session_id,
                            node_path: path,
                        });
                        return true;
                    }
                    false
                }

                _ => false,
            }
        });

        // Refresh button callback — re-scan the same folder, don't re-detect project root
        let sender3 = self.sender;
        self.refresh_btn.set_callback(move |_| {
            sender3.send(Message::TreeViewContextAction {
                session_id,
                action: "refresh".to_string(),
                node_path: vec![],
                input_text: None,
                target_path: None,
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
            let _ = tree.select_only(item, false);
        }

        // Extract info from clicked item
        let (node_path, item_name, is_folder) = if let Some(ref item) = clicked_item {
            let np = Self::node_path_for_item(item).unwrap_or_default();
            let name = item
                .label()
                .map(|l| Self::strip_icon(&l))
                .unwrap_or_default();
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

        // SAFETY: ctx_menu_ptr is a valid MenuButton widget pointer created in
        // the constructor and stored for the lifetime of this panel. We reuse it
        // (rather than creating a new one) so Wayland can anchor the popup.
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
                        crate::app::infrastructure::platform::copy_to_clipboard(fp);
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
                                target_path: None,
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
                            target_path: None,
                        });
                    }
                } else {
                    // Plain action — send immediately
                    sender.send(Message::TreeViewContextAction {
                        session_id,
                        action: item_def.action.clone(),
                        node_path: node_path.clone(),
                        input_text: None,
                        target_path: None,
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
        self.colored_items.borrow_mut().clear();
        self.on_click_action = None;
        self.double_click = true;
        self.context_path = None;
        self.context_menu.clear();
        self.current_nodes = None;
        self.search_input.set_value(SEARCH_PLACEHOLDER);
        self.search_input
            .set_text_color(Color::from_rgb(160, 160, 160));
        *self.drag_source_path.borrow_mut() = None;
        self.dragging.set(false);
        self.expanded_paths.borrow_mut().clear();
    }

    /// Check if the panel is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Default width when shown in left/right position
    pub const DEFAULT_WIDTH: i32 = 250;

    /// Get the current panel height for flex layout (bottom position)
    pub fn current_height(&self) -> i32 {
        if self.visible {
            HEADER_HEIGHT + SEARCH_HEIGHT + DEFAULT_HEIGHT
        } else {
            0
        }
    }

    /// Get the current panel width for flex layout (left/right position)
    pub fn current_width(&self) -> i32 {
        if self.visible { Self::DEFAULT_WIDTH } else { 0 }
    }

    /// Apply search filter to the tree. Rebuilds the tree showing only matching nodes.
    pub fn apply_search(&mut self, query: &str) {
        let root = match &self.current_nodes {
            Some(r) => r.clone(),
            None => return,
        };

        self.tree.clear();
        self.colored_items.borrow_mut().clear();

        if query.is_empty() {
            // Restore full tree
            self.add_tree_node(None, &root, self.current_expand_depth, 0);
        } else {
            let query_lower = query.to_lowercase();
            // Build filtered tree showing only matching nodes and their ancestors
            self.add_filtered_tree_node(None, &root, &query_lower, 0);
        }

        self.tree.redraw();
    }

    /// Recursively check if any node in the subtree matches the query
    fn node_subtree_matches(node: &TreeNode, query: &str) -> bool {
        if node.label.to_lowercase().contains(query) {
            return true;
        }
        node.children
            .iter()
            .any(|child| Self::node_subtree_matches(child, query))
    }

    /// Add a filtered tree node — only includes subtrees containing matches.
    /// All matching nodes and their ancestors are shown; matches are expanded.
    #[allow(clippy::only_used_in_recursion)]
    fn add_filtered_tree_node(
        &mut self,
        parent_path: Option<&str>,
        node: &TreeNode,
        query: &str,
        current_depth: i32,
    ) {
        let self_matches = node.label.to_lowercase().contains(query);
        let children_match: Vec<bool> = node
            .children
            .iter()
            .map(|c| Self::node_subtree_matches(c, query))
            .collect();
        let any_child_matches = children_match.iter().any(|&m| m);

        // Skip this subtree entirely if nothing matches
        if !self_matches && !any_child_matches {
            return;
        }

        // Determine icon
        let icon = if let Some(ref icon_hint) = node.icon {
            match icon_hint.as_str() {
                "folder" => "\u{1F4C1} ",
                "file" => "\u{1F4C4} ",
                "error" => "\u{274C} ",
                "warning" => "\u{26A0} ",
                "info" => "\u{2139} ",
                _ => "",
            }
        } else if node.has_children() {
            "\u{1F4C1} "
        } else {
            "\u{1F4C4} "
        };

        let label = format!("{}{}", icon, node.label);
        let item_path = if let Some(pp) = parent_path {
            format!("{}/{}", pp, label)
        } else {
            label.clone()
        };

        if let Some(mut item) = self.tree.add(&item_path) {
            // Set text color: use semantic label_color if set, else default item_fg
            let custom_color = node
                .label_color
                .as_deref()
                .and_then(|name| self.resolve_label_color(name));
            let fg = custom_color.unwrap_or(self.item_fg);
            item.set_label_fgcolor(fg);

            // Track custom-colored items for re-apply after selection
            if let Some(color) = custom_color {
                self.colored_items
                    .borrow_mut()
                    .push((item_path.clone(), color));
            }
            // Expand nodes that have matching descendants
            item.open();

            // Recurse only into children that have matches
            for (i, child) in node.children.iter().enumerate() {
                if self_matches || children_match[i] {
                    self.add_filtered_tree_node(
                        Some(&item_path),
                        child,
                        query,
                        current_depth + 1,
                    );
                }
            }
        }
    }

    /// Apply theme colors derived from the syntax theme background.
    /// Uses DialogTheme for consistent color derivation across the app.
    pub fn apply_theme(&mut self, is_dark: bool, theme_bg: (u8, u8, u8)) {
        self.is_dark = is_dark;
        let theme = DialogTheme::from_theme_bg(theme_bg);
        let (r, g, b) = theme_bg;

        if let Some(ref mut div) = self.divider {
            div.set_color(divider_color_from_bg(theme_bg));
        }

        // Tree background matches the editor background
        self.tree.set_color(Color::from_rgb(r, g, b));

        // Header uses the dialog/tab-bar background (darker/lighter than editor)
        self.header.set_color(theme.bg);
        self.header.set_label_color(theme.text);
        self.refresh_btn.set_color(theme.bg);
        self.refresh_btn.set_label_color(theme.text_dim);
        self.close_btn.set_color(theme.bg);
        self.close_btn.set_label_color(theme.text_dim);

        // Search row background matches header
        self.search_row.set_color(theme.bg);

        // Search input styling
        let search_bg = if theme.is_dark() {
            let (sr, sg, sb) = lighten(r, g, b, 0.05);
            Color::from_rgb(sr, sg, sb)
        } else {
            let (sr, sg, sb) = darken(r, g, b, 0.95);
            Color::from_rgb(sr, sg, sb)
        };
        self.search_input.set_color(search_bg);
        self.search_input.set_frame(FrameType::FlatBox);
        // Update text color: dim for placeholder, normal for active search
        if self.search_input.value() == SEARCH_PLACEHOLDER {
            self.search_input.set_text_color(theme.text_dim);
        } else {
            self.search_input.set_text_color(theme.text);
        }

        let selection = if theme.is_dark() {
            let (sr, sg, sb) = lighten(r, g, b, 0.10);
            Color::from_rgb(sr, sg, sb)
        } else {
            let (sr, sg, sb) = darken(r, g, b, 0.90);
            Color::from_rgb(sr, sg, sb)
        };
        self.tree.set_selection_color(selection);
        self.tree.set_select_frame(FrameType::FlatBox);

        // Item text color
        self.item_fg = theme.text;
        self.tree.set_item_label_fgcolor(theme.text);

        // Re-build tree from stored nodes so label_color is re-resolved for the new theme.
        // Falls back to iterating items if no stored nodes (shouldn't happen).
        if let Some(ref root) = self.current_nodes.clone() {
            self.tree.clear();
            self.colored_items.borrow_mut().clear();
            self.add_tree_node(None, root, self.current_expand_depth, 0);
        } else {
            let fg = self.item_fg;
            let mut item = self.tree.first();
            while let Some(mut it) = item {
                it.set_label_fgcolor(fg);
                item = it.next();
            }
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
        // SAFETY: Tree inherits Fl_Group. Fl_Group_children/Fl_Group_child are
        // stable FLTK C API. We null-check child pointers and clamp index to
        // min(2) before reconstructing Scrollbar widgets.
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
