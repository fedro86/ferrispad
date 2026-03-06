use fltk::{
    app::Sender,
    enums::Color,
    frame::Frame,
    group::Flex,
    image::PngImage,
    menu::MenuBar,
    prelude::*,
    window::Window,
};

use crate::app::domain::settings::TreePanelPosition;
use crate::app::Message;
use super::diagnostic_panel::DiagnosticPanel;
use super::editor_container::EditorContainer;
use super::tab_bar::{TabBar, TAB_BAR_HEIGHT};
use super::toast::Toast;
use super::split_panel::SplitPanel;
use super::tree_panel::TreePanel;

/// Subset of MainWidgets used during the dispatch loop.
/// Created after `editor_container` and `tab_bar` are moved into AppState.
pub struct LayoutWidgets {
    pub wind: Window,
    pub flex: Flex,
    pub split_panel: SplitPanel,
    pub diagnostic_panel: DiagnosticPanel,
    pub tree_panel: TreePanel,
    pub toast: Toast,
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
        $lw.right_col.as_mut().map(|rc| rc as &mut fltk::group::Flex).unwrap_or(&mut $lw.flex)
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
    /// Inner row flex for left/right tree panel positioning
    pub content_row: Flex,
    /// Column flex holding editor + split panel (for Left/Right tree positions)
    pub right_col: Option<Flex>,
    /// Current tree panel position setting
    pub tree_position: TreePanelPosition,
}

pub fn build_main_window(tabs_enabled: bool, sender: &Sender<Message>, tree_position: TreePanelPosition) -> MainWidgets {
    let mut wind = Window::new(100, 100, 640, 480, "Untitled - \u{1f980} FerrisPad");
    wind.set_xclass("FerrisPad");

    // Load and set the crab emoji as window icon
    let icon_data = include_bytes!("../../assets/crab-notepad-emoji-8bit.png");
    if let Ok(mut icon) = PngImage::from_data(icon_data) {
        icon.scale(32, 32, true, true);
        #[cfg(target_os = "linux")]
        wind.set_icon(Some(icon));
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
            content_row.end();
            tree_panel = tp;
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

            // Draggable divider before tree panel (4px, hidden until tree panel is shown)
            let mut tp = TreePanel::new(*sender);
            tp.create_divider(*sender);
            content_row.fixed(tp.divider.as_ref().unwrap(), 0);

            // Tree panel after divider in the row
            tp.hide();
            content_row.fixed(tp.widget(), 0);

            content_row.end();
            tree_panel = tp;
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
            right_col = None;
        }
    }

    // Diagnostic panel (below everything, initially hidden)
    let mut diagnostic_panel = DiagnosticPanel::new(*sender);
    diagnostic_panel.hide();
    flex.fixed(diagnostic_panel.widget(), 0);

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
        content_row,
        right_col,
        tree_position,
    }
}
