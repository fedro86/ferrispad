//! Tree panel UI for displaying hierarchical data.
//!
//! Used for showing file browsers, YAML/JSON viewers, and outline views.
//! Plugin-driven via the Widget API.

use fltk::{
    app::Sender,
    button::Button,
    enums::{Align, Color, Font, FrameType},
    frame::Frame,
    group::Flex,
    prelude::*,
    tree::{Tree, TreeItem, TreeReason},
};

use crate::app::plugins::widgets::{TreeNode, TreeViewRequest};
use crate::app::Message;

/// Height of the tree panel header
const HEADER_HEIGHT: i32 = 24;

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
}

impl TreePanel {
    /// Create a new tree panel
    pub fn new(sender: Sender<Message>) -> Self {
        let mut container = Flex::default().column();
        container.set_frame(FrameType::FlatBox);

        // Header bar with title and close button
        let mut header_row = Flex::default().row();
        header_row.set_frame(FrameType::FlatBox);
        header_row.set_color(Color::from_rgb(60, 60, 60));

        let mut header = Frame::default();
        header.set_frame(FrameType::FlatBox);
        header.set_color(Color::from_rgb(60, 60, 60));
        header.set_label_color(Color::White);
        header.set_label_font(Font::HelveticaBold);
        header.set_label_size(12);
        header.set_align(Align::Left | Align::Inside);
        header.set_label("  Tree View");

        let mut close_btn = Button::default().with_size(24, 24).with_label("X");
        close_btn.set_frame(FrameType::FlatBox);
        close_btn.set_color(Color::from_rgb(60, 60, 60));
        close_btn.set_label_color(Color::from_rgb(180, 180, 180));
        close_btn.set_label_size(11);

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
        tree.set_show_root(false);

        container.end();
        container.hide();

        Self {
            container,
            header,
            tree,
            close_btn,
            sender,
            session_id: None,
            visible: false,
            on_click_action: None,
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

    /// Add a node to the tree recursively
    fn add_tree_node(
        &mut self,
        parent: Option<&TreeItem>,
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

        // Add item to tree
        let item_path = if let Some(parent_item) = parent {
            if let Some(path) = parent_item.label() {
                format!("{}/{}", path, label)
            } else {
                label.clone()
            }
        } else {
            label.clone()
        };

        self.tree.add(&item_path);

        // Find the item we just added
        if let Some(mut item) = self.tree.find_item(&item_path) {
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

            // Add children recursively
            for child in &node.children {
                self.add_tree_node(Some(&item), child, expand_depth, current_depth + 1);
            }
        }
    }

    /// Set up tree callbacks
    fn setup_callbacks(&mut self, session_id: u32) {
        let sender = self.sender;
        let on_click_action = self.on_click_action.clone();

        // Tree selection callback
        self.tree.set_callback(move |tree| {
            if tree.callback_reason() == TreeReason::Selected {
                if let Some(item) = tree.first_selected_item() {
                    // Build node path from item
                    let mut path = Vec::new();
                    let mut current = Some(item);
                    while let Some(item) = current {
                        if let Some(label) = item.label() {
                            // Remove icon prefix if present
                            let clean_label = label
                                .trim_start_matches("\u{1F4C1} ")
                                .trim_start_matches("\u{1F4C4} ")
                                .trim_start_matches("\u{274C} ")
                                .trim_start_matches("\u{26A0} ")
                                .trim_start_matches("\u{2139} ")
                                .to_string();
                            path.push(clean_label);
                        }
                        current = item.parent();
                    }
                    path.reverse();

                    // Send click message
                    if on_click_action.is_some() {
                        sender.send(Message::TreeViewNodeClicked {
                            session_id,
                            node_path: path,
                        });
                    }
                }
            }
        });

        // Close button callback
        let sender2 = self.sender;
        self.close_btn.set_callback(move |_| {
            sender2.send(Message::TreeViewHide(session_id));
        });
    }

    /// Hide the tree panel
    pub fn hide(&mut self) {
        self.container.hide();
        self.visible = false;
        self.session_id = None;
        self.tree.clear();
        self.on_click_action = None;
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

    /// Get the current panel height for flex layout
    pub fn current_height(&self) -> i32 {
        if self.visible {
            HEADER_HEIGHT + DEFAULT_HEIGHT
        } else {
            0
        }
    }

    /// Apply theme colors
    pub fn apply_theme(&mut self, is_dark: bool) {
        if is_dark {
            self.header.set_color(Color::from_rgb(60, 60, 60));
            self.header.set_label_color(Color::White);
            self.close_btn.set_color(Color::from_rgb(60, 60, 60));
            self.close_btn.set_label_color(Color::from_rgb(180, 180, 180));
            self.tree.set_color(Color::from_rgb(40, 40, 40));
            self.tree.set_selection_color(Color::from_rgb(70, 100, 130));
            self.tree.set_item_label_fgcolor(Color::from_rgb(220, 220, 220));
            self.tree.set_connector_color(Color::from_rgb(100, 100, 100));
        } else {
            self.header.set_color(Color::from_rgb(220, 220, 220));
            self.header.set_label_color(Color::from_rgb(30, 30, 30));
            self.close_btn.set_color(Color::from_rgb(220, 220, 220));
            self.close_btn.set_label_color(Color::from_rgb(60, 60, 60));
            self.tree.set_color(Color::White);
            self.tree.set_selection_color(Color::from_rgb(180, 210, 240));
            self.tree.set_item_label_fgcolor(Color::from_rgb(30, 30, 30));
            self.tree.set_connector_color(Color::from_rgb(180, 180, 180));
        }
    }
}
