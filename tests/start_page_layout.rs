//! T0108: the start page fits its window at every width down to its
//! minimum: nothing visible extends past the window horizontally, and a
//! vertical overflow only happens inside a scroll whose scrollbar is shown.
//! Builds real FLTK widgets, so it needs a display: like
//! `tests/ui_dialogs.rs` it is skipped without one unless
//! `FERRISPAD_REQUIRE_UI_TESTS=1` (CI runs it under `xvfb-run`).
//!
//! One test function on purpose: fltk-rs pins its "UI thread" to the first
//! thread that creates a window.

mod support;

use ferris_pad::app::domain::messages::Message;
use ferris_pad::ui::start_page::StartPage;
use fltk::group::{Flex, Scroll};
use fltk::prelude::*;
use fltk::widget::Widget;
use fltk::window::Window;
use support::ui_layout::{display_available, lay_out};

const DARK: (u8, u8, u8) = (43, 48, 59);
const LIGHT: (u8, u8, u8) = (239, 241, 245);

/// Every visible widget under `group`, depth first, and whether it lies
/// inside a `Scroll` (so it may extend below the window: it scrolls).
fn visible_widgets(group: &fltk::group::Group, in_scroll: bool, out: &mut Vec<(Widget, bool)>) {
    for i in 0..group.children() {
        let Some(child) = group.child(i) else {
            continue;
        };
        if !child.visible() {
            continue;
        }
        out.push((child.clone(), in_scroll));
        let scrolls = in_scroll || Scroll::from_dyn_widget(&child).is_some();
        if let Some(g) = child.as_group() {
            visible_widgets(&g, scrolls, out);
        }
    }
}

/// Widgets of the start page that extend past the window: horizontally
/// always a failure; vertically only outside a `Scroll` (inside one, the
/// content scrolls into view).
fn clipped(window: &Window, name: &str) -> Vec<String> {
    let mut widgets = Vec::new();
    visible_widgets(&window.as_group().unwrap(), false, &mut widgets);
    let (ww, wh) = (window.w(), window.h());
    widgets
        .iter()
        .filter(|(w, _)| w.w() > 0 && w.h() > 0)
        .filter_map(|(w, in_scroll)| {
            let horizontal = w.x() < 0 || w.x() + w.w() > ww;
            let vertical = !in_scroll && (w.y() < 0 || w.y() + w.h() > wh);
            (horizontal || vertical).then(|| {
                format!(
                    "{name}: '{}' at {},{} {}x{} outside the {ww}x{wh} window",
                    w.label(),
                    w.x(),
                    w.y(),
                    w.w(),
                    w.h()
                )
            })
        })
        .collect()
}

/// Lay the window out, then answer the page's `StartPageRelayout` requests
/// the way `dispatch::relayout_start_page` does, until it is stable.
fn settle(
    page: &mut StartPage,
    window: &Window,
    sender: fltk::app::Sender<Message>,
    receiver: &fltk::app::Receiver<Message>,
    theme_bg: (u8, u8, u8),
) {
    lay_out(window);
    for _ in 0..3 {
        let mut relayout = false;
        while let Some(msg) = receiver.recv() {
            relayout |= matches!(msg, Message::StartPageRelayout);
        }
        if !relayout {
            return;
        }
        page.show(sender, theme_bg, "default");
        lay_out(window);
    }
    panic!("the start page keeps asking for a relayout");
}

#[test]
fn start_page_fits_every_width() {
    let root = tempfile::tempdir().unwrap();
    // SAFETY: set before this test creates any thread or reads the
    // environment elsewhere; this binary has a single test function. Keeps
    // the recent-files and session lists away from the user's real dirs.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", root.path().join("config"));
        std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    }
    if !display_available() {
        return;
    }
    let (sender, receiver) = fltk::app::channel::<Message>();

    let mut failures = Vec::new();
    for (theme_name, theme_bg) in [("dark", DARK), ("light", LIGHT)] {
        for (w, h) in [(1100, 760), (760, 620), (500, 600), (380, 420)] {
            let window = Window::new(0, 0, w, h, None);
            let column = Flex::default_fill().column();
            let mut page = StartPage::new();
            column.end();
            window.end();
            page.show(sender, theme_bg, "default");
            settle(&mut page, &window, sender, &receiver, theme_bg);
            failures.extend(clipped(&window, &format!("{theme_name} {w}x{h}")));
            drop(window);
        }
    }

    // Resizing a shown page across the column threshold: the page asks for
    // a rebuild (StartPageRelayout), which dispatch answers with show().
    let mut window = Window::new(0, 0, 1100, 760, None);
    let mut column = Flex::default_fill().column();
    let mut page = StartPage::new();
    column.end();
    window.end();
    page.show(sender, DARK, "default");
    settle(&mut page, &window, sender, &receiver, DARK);
    for (w, h) in [(500, 600), (1100, 760), (420, 380)] {
        window.resize(0, 0, w, h);
        column.resize(0, 0, w, h);
        // A drag fires several resizes before the request is handled: one
        // request per build.
        for dw in [1, 2, 3] {
            window.resize(0, 0, w + dw, h);
            column.resize(0, 0, w + dw, h);
            lay_out(&window);
        }
        let mut requests = 0;
        while let Some(msg) = receiver.recv() {
            requests += usize::from(matches!(msg, Message::StartPageRelayout));
        }
        assert!(requests <= 1, "{requests} relayout requests for one build");
        if requests == 1 {
            page.show(sender, DARK, "default");
        }
        settle(&mut page, &window, sender, &receiver, DARK);
        failures.extend(clipped(&window, &format!("resized to {w}x{h}")));
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
