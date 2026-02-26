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
use super::tree_panel::TreePanel;

pub struct MainWidgets {
    pub wind: Window,
    pub flex: Flex,
    pub menu: MenuBar,
    pub tab_bar: Option<TabBar>,
    pub toast: Toast,
    pub update_banner_frame: Frame,
    pub editor_container: EditorContainer,
    pub diagnostic_panel: DiagnosticPanel,
    pub tree_panel: TreePanel,
    /// Inner row flex for left/right tree panel positioning
    pub content_row: Flex,
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
    let tree_panel;
    let tab_bar;
    let editor_container;

    match tree_position {
        TreePanelPosition::Left => {
            // Tree panel first in the row
            let mut tp = TreePanel::new(*sender);
            tp.hide();
            content_row.fixed(tp.widget(), 0);

            // Editor column: tab bar + editor (takes remaining width)
            let mut editor_col = Flex::default().column();
            tab_bar = if tabs_enabled {
                let tb = TabBar::new(0, 0, 640, *sender);
                editor_col.fixed(&tb.widget, TAB_BAR_HEIGHT);
                Some(tb)
            } else {
                None
            };
            editor_container = EditorContainer::new(&editor_col);
            editor_col.end();

            content_row.end();
            tree_panel = tp;
        }
        TreePanelPosition::Right => {
            // Editor column first
            let mut editor_col = Flex::default().column();
            tab_bar = if tabs_enabled {
                let tb = TabBar::new(0, 0, 640, *sender);
                editor_col.fixed(&tb.widget, TAB_BAR_HEIGHT);
                Some(tb)
            } else {
                None
            };
            editor_container = EditorContainer::new(&editor_col);
            editor_col.end();

            // Tree panel after editor in the row
            let mut tp = TreePanel::new(*sender);
            tp.hide();
            content_row.fixed(tp.widget(), 0);

            content_row.end();
            tree_panel = tp;
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
        diagnostic_panel,
        tree_panel,
        content_row,
        tree_position,
    }
}
