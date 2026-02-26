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
}

pub fn build_main_window(tabs_enabled: bool, sender: &Sender<Message>) -> MainWidgets {
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

    // Tab bar (only when tabs enabled)
    let tab_bar = if tabs_enabled {
        let tb = TabBar::new(0, 30, 640, *sender);
        flex.fixed(&tb.widget, TAB_BAR_HEIGHT);
        Some(tb)
    } else {
        None
    };

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

    // Editor container — the TextEditor is added directly to flex (no wrapper)
    let editor_container = EditorContainer::new(&flex);

    // Tree panel (below editor, initially hidden - used by file-explorer etc.)
    let mut tree_panel = TreePanel::new(*sender);
    tree_panel.hide();
    flex.fixed(tree_panel.widget(), 0);

    // Diagnostic panel (below editor, initially hidden)
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
    }
}
