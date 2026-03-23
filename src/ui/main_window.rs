use fltk::{
    app::Sender, enums::Color, frame::Frame, group::Flex, menu::MenuBar, prelude::*,
    window::Window,
};
#[cfg(not(target_os = "macos"))]
use fltk::image::PngImage;

use super::diagnostic_panel::DiagnosticPanel;
use super::editor_container::EditorContainer;
use super::split_panel::SplitPanel;
use super::status_bar::{STATUS_BAR_HEIGHT, StatusBar};
use super::tab_bar::{TAB_BAR_HEIGHT, TabBar};
use super::terminal_panel::TerminalPanel;
use super::toast::Toast;
use super::tree_panel::TreePanel;
use crate::app::Message;
use crate::app::domain::settings::TreePanelPosition;

/// Subset of MainWidgets used during the dispatch loop.
/// Created after `editor_container` and `tab_bar` are moved into AppState.
pub struct LayoutWidgets {
    pub wind: Window,
    pub flex: Flex,
    pub split_panel: SplitPanel,
    pub diagnostic_panel: DiagnosticPanel,
    pub tree_panel: TreePanel,
    pub terminal_panel: TerminalPanel,
    pub toast: Toast,
    pub status_bar: StatusBar,
    pub content_row: Flex,
    pub right_col: Option<Flex>,
    pub tree_position: TreePanelPosition,
}

/// Resolve the parent Flex that owns the split panel.
/// For Left/Right tree positions it's right_col; for Bottom it's the outer flex.
///
/// This is a macro rather than a method to avoid borrowing the whole `LayoutWidgets`
/// struct, which would conflict with simultaneous borrows of `split_panel` and other fields.
#[macro_export]
macro_rules! split_parent {
    ($lw:expr) => {
        $lw.right_col
            .as_mut()
            .map(|rc| rc as &mut fltk::group::Flex)
            .unwrap_or(&mut $lw.flex)
    };
}

pub struct MainWidgets {
    pub wind: Window,
    pub flex: Flex,
    pub menu: MenuBar,
    pub tab_bar: Option<TabBar>,
    pub toast: Toast,
    pub update_banner_frame: Frame,
    pub editor_container: EditorContainer,
    pub split_panel: SplitPanel,
    pub diagnostic_panel: DiagnosticPanel,
    pub tree_panel: TreePanel,
    pub terminal_panel: TerminalPanel,
    pub status_bar: StatusBar,
    /// Inner row flex for left/right tree panel positioning
    pub content_row: Flex,
    /// Column flex holding editor + split panel (for Left/Right tree positions)
    pub right_col: Option<Flex>,
    /// Current tree panel position setting
    pub tree_position: TreePanelPosition,
}

pub fn build_main_window(
    tabs_enabled: bool,
    sender: &Sender<Message>,
    tree_position: TreePanelPosition,
) -> MainWidgets {
    let title = if cfg!(target_os = "windows") {
        "Untitled - FerrisPad"
    } else {
        "Untitled - \u{1f980} FerrisPad"
    };
    let mut wind = Window::new(100, 100, 640, 480, title);
    wind.set_xclass("FerrisPad");

    // Load and set the crab icon (title bar on Linux/Windows).
    // macOS uses the .app bundle icon instead.
    // Use the pre-rendered 32x32 icon to avoid decompressing the 1024x1024 source.
    #[cfg(not(target_os = "macos"))]
    {
        let icon_data = include_bytes!("../../icons/hicolor/32x32/apps/ferrispad.png");
        if let Ok(icon) = PngImage::from_data(icon_data) {
            wind.set_icon(Some(icon));
        }
    }

    let mut flex = Flex::new(0, 0, 640, 480, None);
    flex.set_type(fltk::group::FlexType::Column);
    flex.set_margin(0);
    flex.set_pad(0);

    let menu = MenuBar::new(0, 0, 0, 30, "");
    flex.fixed(&menu, 30);

    // Toast notification bar (initially hidden, auto-hides after 4 seconds)
    let toast = Toast::new(*sender);
    flex.fixed(toast.widget(), 0);

    // Update notification banner (initially hidden)
    let mut update_banner_frame = Frame::default().with_size(0, 0);
    update_banner_frame.set_frame(fltk::enums::FrameType::FlatBox);
    update_banner_frame.set_color(Color::from_rgb(255, 250, 205));
    update_banner_frame.set_label_color(Color::Black);
    update_banner_frame.set_label_size(13);
    update_banner_frame.hide();
    flex.fixed(&update_banner_frame, 0);

    // Content area layout depends on tree panel position:
    //
    // Left/Right: row flex with tree panel beside [tab_bar + editor] column
    //   content_row (row):
    //     [tree_panel]  [editor_col: tab_bar + editor]   (Left)
    //     [editor_col: tab_bar + editor]  [tree_panel]   (Right)
    //
    // Bottom: no row wrapper needed, tab_bar + editor + tree_panel all in outer column
    let mut content_row = Flex::default().row();
    content_row.set_margin(0);
    content_row.set_pad(0);
    let tree_panel;
    let terminal_panel;
    let tab_bar;
    let editor_container;
    let split_panel;
    let right_col;

    match tree_position {
        TreePanelPosition::Left => {
            // Tree panel first in the row
            let mut tp = TreePanel::new(*sender);
            tp.hide();
            content_row.fixed(tp.widget(), 0);

            // Draggable divider (4px, hidden until tree panel is shown)
            tp.create_divider(*sender);
            content_row.fixed(tp.divider.as_ref().unwrap(), 0);

            // Right column: editor + split divider + split panel
            let mut rc = Flex::default().column();
            rc.set_margin(0);
            rc.set_pad(0);

            // Editor column: tab bar + editor (takes remaining height)
            let mut editor_col = Flex::default().column();
            editor_col.set_margin(0);
            editor_col.set_pad(0);
            tab_bar = if tabs_enabled {
                let tb = TabBar::new(0, 0, 640, *sender);
                editor_col.fixed(&tb.widget, TAB_BAR_HEIGHT);
                Some(tb)
            } else {
                None
            };
            editor_container = EditorContainer::new(&editor_col);
            editor_col.end();

            // Split divider BEFORE split panel so it appears above in column
            let split_div = SplitPanel::new_divider(*sender);
            rc.fixed(&split_div, 0);
            let mut sp = SplitPanel::new(*sender);
            sp.divider = Some(split_div);
            rc.fixed(sp.widget(), 0);

            rc.end();

            // Terminal panel divider + panel (right side, hidden until requested)
            let term_div = TerminalPanel::new_divider(*sender);
            content_row.fixed(&term_div, 0);
            let mut term = TerminalPanel::new(*sender);
            term.divider = Some(term_div);
            term.hide();
            content_row.fixed(term.widget(), 0);

            content_row.end();
            tree_panel = tp;
            terminal_panel = term;
            split_panel = sp;
            right_col = Some(rc);
        }
        TreePanelPosition::Right => {
            // Right column (left side here): editor + split divider + split panel
            let mut rc = Flex::default().column();
            rc.set_margin(0);
            rc.set_pad(0);

            // Editor column first
            let mut editor_col = Flex::default().column();
            editor_col.set_margin(0);
            editor_col.set_pad(0);
            tab_bar = if tabs_enabled {
                let tb = TabBar::new(0, 0, 640, *sender);
                editor_col.fixed(&tb.widget, TAB_BAR_HEIGHT);
                Some(tb)
            } else {
                None
            };
            editor_container = EditorContainer::new(&editor_col);
            editor_col.end();

            // Split divider BEFORE split panel so it appears above in column
            let split_div = SplitPanel::new_divider(*sender);
            rc.fixed(&split_div, 0);
            let mut sp = SplitPanel::new(*sender);
            sp.divider = Some(split_div);
            rc.fixed(sp.widget(), 0);

            rc.end();

            // Terminal panel divider + panel (right side, hidden until requested)
            let term_div = TerminalPanel::new_divider(*sender);
            content_row.fixed(&term_div, 0);
            let mut term = TerminalPanel::new(*sender);
            term.divider = Some(term_div);
            term.hide();
            content_row.fixed(term.widget(), 0);

            // Draggable divider before tree panel (4px, hidden until tree panel is shown)
            let mut tp = TreePanel::new(*sender);
            tp.create_divider(*sender);
            content_row.fixed(tp.divider.as_ref().unwrap(), 0);

            // Tree panel after divider in the row
            tp.hide();
            content_row.fixed(tp.widget(), 0);

            content_row.end();
            tree_panel = tp;
            terminal_panel = term;
            split_panel = sp;
            right_col = Some(rc);
        }
        TreePanelPosition::Bottom => {
            // No tree panel in the row — just tab bar + editor
            tab_bar = if tabs_enabled {
                let tb = TabBar::new(0, 0, 640, *sender);
                content_row.fixed(&tb.widget, TAB_BAR_HEIGHT);
                Some(tb)
            } else {
                None
            };
            editor_container = EditorContainer::new(&content_row);

            // Terminal panel divider + panel (right side, hidden until requested)
            let term_div = TerminalPanel::new_divider(*sender);
            content_row.fixed(&term_div, 0);
            let mut term = TerminalPanel::new(*sender);
            term.divider = Some(term_div);
            term.hide();
            content_row.fixed(term.widget(), 0);

            content_row.end();

            // Tree panel in the outer column flex (below editor area)
            let mut tp = TreePanel::new(*sender);
            tp.hide();
            flex.fixed(tp.widget(), 0);
            tree_panel = tp;

            // Split divider BEFORE split panel so it appears above in column
            let split_div = SplitPanel::new_divider(*sender);
            flex.fixed(&split_div, 0);
            let mut sp = SplitPanel::new(*sender);
            sp.divider = Some(split_div);
            flex.fixed(sp.widget(), 0);
            split_panel = sp;
            terminal_panel = term;
            right_col = None;
        }
    }

    // Diagnostic panel (below everything, initially hidden)
    let mut diagnostic_panel = DiagnosticPanel::new(*sender);
    diagnostic_panel.hide();
    flex.fixed(diagnostic_panel.widget(), 0);

    // Status bar (bottom of window, always visible)
    let status_bar = StatusBar::new();
    flex.fixed(status_bar.widget(), STATUS_BAR_HEIGHT);

    flex.end();
    wind.resizable(&flex);

    MainWidgets {
        wind,
        flex,
        menu,
        tab_bar,
        toast,
        update_banner_frame,
        editor_container,
        split_panel,
        diagnostic_panel,
        tree_panel,
        terminal_panel,
        status_bar,
        content_row,
        right_col,
        tree_position,
    }
}
