//! Plugin permission approval dialog.
//!
//! Shows a modal dialog asking the user to approve commands that a plugin
//! wants to execute. This implements the browser-style permission model
//! for the plugin sandbox.

use fltk::{
    button::{Button, CheckButton},
    enums::Color,
    frame::Frame,
    group::Group,
    prelude::*,
    window::Window,
};
use std::cell::RefCell;
use std::rc::Rc;

/// Request for permission approval
pub struct PermissionRequest {
    /// Name of the plugin requesting permissions
    pub plugin_name: String,
    /// Plugin description (if available)
    pub description: String,
    /// Commands the plugin wants to execute
    pub commands: Vec<String>,
}

/// Result of the permission dialog
#[derive(Debug, Clone)]
pub enum ApprovalResult {
    /// User approved these specific commands
    Approved(Vec<String>),
    /// User denied all permissions (plugin will be disabled)
    Denied,
    /// User closed the dialog without deciding
    Cancelled,
}

// Layout constants matching settings_dialog.rs style
const DIALOG_WIDTH: i32 = 400;
const MARGIN: i32 = 15;
const LABEL_HEIGHT: i32 = 20;
const ITEM_HEIGHT: i32 = 22;
const BUTTON_WIDTH: i32 = 80;
const BUTTON_HEIGHT: i32 = 28;
const SECTION_GAP: i32 = 15;

/// Show a modal dialog asking the user to approve plugin permissions.
/// Returns which commands were approved (if any).
pub fn show_permission_dialog(request: &PermissionRequest) -> ApprovalResult {
    // Calculate dialog height based on content
    let has_description = !request.description.is_empty();
    let desc_height = if has_description { LABEL_HEIGHT + 5 } else { 0 };
    let commands_height = request.commands.len() as i32 * ITEM_HEIGHT;

    let dialog_height = MARGIN
        + LABEL_HEIGHT
        + 5
        + desc_height
        + commands_height
        + SECTION_GAP
        + LABEL_HEIGHT
        + SECTION_GAP
        + BUTTON_HEIGHT
        + MARGIN;

    let mut dialog = Window::default()
        .with_size(DIALOG_WIDTH, dialog_height.max(200))
        .with_label("Plugin Permission Request")
        .center_screen();
    dialog.make_modal(true);

    let result = Rc::new(RefCell::new(ApprovalResult::Cancelled));

    // Main content group - exactly like settings_dialog
    let _vpack = Group::default()
        .with_size(DIALOG_WIDTH - 20, dialog_height - 50)
        .with_pos(10, 10);

    let mut y = MARGIN;
    let col_width = DIALOG_WIDTH - MARGIN * 2;

    // Title label
    Frame::default()
        .with_pos(MARGIN, y)
        .with_size(col_width, LABEL_HEIGHT)
        .with_label(&format!("\"{}\" wants to execute:", request.plugin_name))
        .with_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
    y += LABEL_HEIGHT + 5;

    // Description (if available)
    if has_description {
        let mut desc = Frame::default()
            .with_pos(MARGIN, y)
            .with_size(col_width, LABEL_HEIGHT);
        desc.set_label(&request.description);
        desc.set_label_size(11);
        desc.set_label_color(Color::from_rgb(100, 100, 100));
        desc.set_align(fltk::enums::Align::Left | fltk::enums::Align::Inside);
        y += LABEL_HEIGHT + 5;
    }

    // Checkboxes group - like theme_group in settings
    let cb_group = Group::default()
        .with_pos(MARGIN + 10, y)
        .with_size(col_width - 10, request.commands.len() as i32 * ITEM_HEIGHT);

    let mut checkboxes: Vec<CheckButton> = Vec::new();
    for (i, cmd) in request.commands.iter().enumerate() {
        let mut cb = CheckButton::default()
            .with_pos(MARGIN + 10, y + (i as i32 * ITEM_HEIGHT))
            .with_size(col_width - 20, ITEM_HEIGHT)
            .with_label(cmd);
        cb.set_value(true);
        checkboxes.push(cb);
    }
    cb_group.end();
    y += request.commands.len() as i32 * ITEM_HEIGHT + SECTION_GAP;

    // Warning text
    let mut warning = Frame::default()
        .with_pos(MARGIN, y)
        .with_size(col_width, LABEL_HEIGHT);
    warning.set_label("These commands will run in your project directory.");
    warning.set_label_size(11);
    warning.set_label_color(Color::from_rgb(100, 100, 100));
    warning.set_align(fltk::enums::Align::Center | fltk::enums::Align::Inside);

    _vpack.end();

    // Buttons - outside vpack, right aligned like settings_dialog
    let btn_y = dialog_height - MARGIN - BUTTON_HEIGHT;
    let allow_x = DIALOG_WIDTH - MARGIN - BUTTON_WIDTH;
    let deny_x = allow_x - BUTTON_WIDTH - 10;

    let mut deny_btn = Button::default()
        .with_pos(deny_x, btn_y)
        .with_size(BUTTON_WIDTH, BUTTON_HEIGHT)
        .with_label("Deny");

    let mut allow_btn = Button::default()
        .with_pos(allow_x, btn_y)
        .with_size(BUTTON_WIDTH, BUTTON_HEIGHT)
        .with_label("Allow");
    allow_btn.set_color(Color::from_rgb(76, 175, 80));

    dialog.end();
    dialog.show();

    // Callbacks - set after dialog.show() like settings_dialog
    let result_deny = result.clone();
    let mut dialog_deny = dialog.clone();
    deny_btn.set_callback(move |_| {
        *result_deny.borrow_mut() = ApprovalResult::Denied;
        dialog_deny.hide();
    });

    let result_allow = result.clone();
    let mut dialog_allow = dialog.clone();
    let checkboxes_rc = Rc::new(RefCell::new(checkboxes));
    let commands_clone = request.commands.clone();
    allow_btn.set_callback(move |_| {
        let cbs = checkboxes_rc.borrow();
        let approved: Vec<String> = commands_clone
            .iter()
            .enumerate()
            .filter(|(i, _)| cbs.get(*i).is_some_and(|cb| cb.value()))
            .map(|(_, cmd)| cmd.clone())
            .collect();

        *result_allow.borrow_mut() = ApprovalResult::Approved(approved);
        dialog_allow.hide();
    });

    super::run_dialog(&dialog);

    result.borrow().clone()
}
