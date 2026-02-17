use std::cell::RefCell;
use std::rc::Rc;

use fltk::{
    frame::Frame,
    group::Flex,
    prelude::*,
    window::Window,
};

use super::settings::AppSettings;
use super::updater::ReleaseInfo;
use crate::ui::dialogs::update::show_update_available_dialog;

pub struct BannerWidgets<'a> {
    pub banner_frame: &'a mut Frame,
    pub flex: &'a mut Flex,
    pub window: &'a mut Window,
}

pub struct UpdateController {
    pub pending_update: Option<ReleaseInfo>,
}

impl UpdateController {
    pub fn new() -> Self {
        Self {
            pending_update: None,
        }
    }

    pub fn show_banner(&self, version: &str, widgets: &mut BannerWidgets) {
        widgets.banner_frame.set_label(&format!(
            "  \u{1f980} FerrisPad {} is available - Click to view details or press ESC to dismiss",
            version
        ));
        widgets.banner_frame.show();
        widgets.flex.fixed(&*widgets.banner_frame, 30);
        widgets.window.redraw();
    }

    pub fn hide_banner(&self, widgets: &mut BannerWidgets) {
        widgets.banner_frame.hide();
        widgets.flex.fixed(&*widgets.banner_frame, 0);
        widgets.window.redraw();
    }

    pub fn receive_update(&mut self, release: ReleaseInfo, widgets: &mut BannerWidgets) {
        let version = release.version();
        self.pending_update = Some(release);
        self.show_banner(&version, widgets);
    }

    pub fn show_update_dialog(&mut self, settings: &Rc<RefCell<AppSettings>>, widgets: &mut BannerWidgets) {
        if let Some(release) = self.pending_update.take() {
            show_update_available_dialog(release, settings);
            self.hide_banner(widgets);
        }
    }

    pub fn dismiss_banner(&self, widgets: &mut BannerWidgets) {
        self.hide_banner(widgets);
    }
}
