//! Plugin Manager dialog for viewing, enabling/disabling, and installing plugins.
//!
//! VSCode-inspired design with:
//! - Themed tabs and buttons
//! - Icon letter placeholders
//! - Compact row layout with truncated descriptions
//! - Three tabs: Installed, Official, Community

use fltk::{
    button::Button,
    enums::{Align, CallbackTrigger, Color, Font, FrameType},
    frame::Frame,
    group::{Group, Pack, PackType, Scroll, Tabs},
    input::Input,
    prelude::*,
    window::Window,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::app::plugins::loader::parse_plugin_toml_content;
use crate::app::plugins::{PluginManager, plugin_display_name};
use crate::app::services::plugin_registry::{
    AvailablePluginInfo, CommunityPluginInfo, PluginTier, determine_plugin_tier,
    fetch_community_plugin_toml, fetch_community_registry, fetch_plugin_registry,
    install_community_plugin, install_plugin, is_plugin_installed, is_update_available,
    parse_github_url,
};
use crate::app::services::plugin_verify::{
    LuaScanResult, detects_text_change_hook, scan_lua_source,
};

use super::community_install::{CommunityInstallReview, show_community_install_dialog};
use super::{DialogTheme, SCROLLBAR_SIZE};

// Layout constants
const DIALOG_WIDTH: i32 = 550;
const DIALOG_HEIGHT: i32 = 480;
const PADDING: i32 = 10;
const TAB_HEIGHT: i32 = 30;
const BUTTON_HEIGHT: i32 = 30;
const PLUGIN_ROW_HEIGHT: i32 = 70;
const SEARCH_HEIGHT: i32 = 25;
const SEARCH_MARGIN: i32 = 5;

// Row layout constants
const ICON_SIZE: i32 = 32;
const ICON_MARGIN: i32 = 8;
const ACTION_BUTTON_WIDTH: i32 = 80;

// Description truncation (approximate chars that fit)
const DESC_MAX_CHARS: usize = 60; // Increased due to wider rows

/// Result from the plugin manager dialog
#[derive(Debug, Clone)]
pub enum PluginManagerResult {
    /// User requested reload all
    ReloadAll,
    /// User made changes (toggles, installs, and/or uninstalls)
    Changed {
        toggled: Vec<(String, bool)>,
        installed: Vec<String>,
        uninstalled: Vec<String>,
    },
    /// Dialog closed with no changes
    Cancelled,
}

/// Determine the row width based on whether a vertical scrollbar is needed.
pub fn row_width_for_scroll(content_h: i32, viewport_h: i32, full_width: i32) -> i32 {
    if content_h > viewport_h {
        full_width - SCROLLBAR_SIZE
    } else {
        full_width
    }
}

fn adjust_pack_width(pack: &mut Pack, full_width: i32) {
    let viewport_h = pack.parent().map(|p| p.h()).unwrap_or(i32::MAX);
    let spacing = pack.spacing();
    let n = pack.children();
    let visible_count = (0..n)
        .filter_map(|i| pack.child(i))
        .filter(|c| c.visible())
        .count() as i32;
    let content_h: i32 = (0..n)
        .filter_map(|i| pack.child(i))
        .filter(|c| c.visible())
        .map(|c| c.h())
        .sum::<i32>()
        + spacing * (visible_count.max(1) - 1);
    let target_w = row_width_for_scroll(content_h, viewport_h, full_width);
    if pack.w() != target_w || pack.h() != content_h {
        pack.set_size(target_w, content_h);
        for i in 0..pack.children() {
            if let Some(mut child) = pack.child(i) {
                child.set_size(target_w, child.h());
            }
        }
        pack.redraw();
    }
}

fn create_placeholder_input(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    placeholder: &str,
    theme: &DialogTheme,
) -> Input {
    let mut input = Input::default().with_pos(x, y).with_size(w, h);
    input.set_frame(FrameType::FlatBox);
    input.set_color(theme.input_bg);
    input.set_text_color(theme.text);

    // Enable FLTK's base class draw (renders input background, text, cursor),
    // then overlay placeholder text on top when the input is empty.
    let placeholder = placeholder.to_string();
    let dim_color = theme.text_dim;
    let text_size = input.text_size();
    input.super_draw(true);
    input.super_draw_first(true);
    input.draw(move |input| {
        // Base class already drew the input with cursor — just overlay placeholder
        if input.value().is_empty() {
            fltk::draw::set_draw_color(dim_color);
            fltk::draw::set_font(Font::Helvetica, text_size);
            fltk::draw::draw_text2(
                &placeholder,
                input.x() + 6,
                input.y(),
                input.w() - 12,
                input.h(),
                Align::Left | Align::Inside,
            );
        }
    });

    input
}

/// Truncate text to max_chars, adding "..." if truncated
fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated.trim_end())
    }
}

/// Get a color for the icon background based on the first letter
fn icon_color_for_letter(letter: char, is_dark: bool) -> Color {
    // Simple palette - 6 distinct colors that work in both dark and light
    let palette_dark: [(u8, u8, u8); 6] = [
        (100, 140, 180), // Blue-ish
        (140, 100, 160), // Purple-ish
        (100, 160, 140), // Teal-ish
        (160, 120, 100), // Brown-ish
        (140, 140, 100), // Olive-ish
        (160, 100, 120), // Rose-ish
    ];
    let palette_light: [(u8, u8, u8); 6] = [
        (70, 110, 150),
        (110, 70, 130),
        (70, 130, 110),
        (130, 90, 70),
        (110, 110, 70),
        (130, 70, 90),
    ];
    let idx = (letter.to_ascii_uppercase() as usize) % 6;
    let (r, g, b) = if is_dark {
        palette_dark[idx]
    } else {
        palette_light[idx]
    };
    Color::from_rgb(r, g, b)
}

/// Show the plugin manager dialog
///
/// Returns the result indicating what actions were taken
pub fn show_plugin_manager_dialog(
    plugins: &PluginManager,
    theme_bg: (u8, u8, u8),
) -> PluginManagerResult {
    let theme = DialogTheme::from_theme_bg(theme_bg);
    let is_dark = theme.is_dark();

    let mut dialog = Window::default()
        .with_size(DIALOG_WIDTH, DIALOG_HEIGHT)
        .with_label("Plugin Manager")
        .center_screen();
    dialog.make_modal(true);
    dialog.set_color(theme.bg);

    // Track result
    let result: Rc<RefCell<PluginManagerResult>> =
        Rc::new(RefCell::new(PluginManagerResult::Cancelled));

    // Track toggled plugins
    let toggles: Rc<RefCell<Vec<(String, bool)>>> = Rc::new(RefCell::new(Vec::new()));

    // Track installed plugins
    let installed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    // Track uninstalled plugins
    let uninstalled: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    // Cross-tab sync: map of plugin name -> Available tab button (to update on uninstall)
    let available_buttons: Rc<RefCell<HashMap<String, Button>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // Cross-tab sync: map of plugin name -> Community tab button (to update on uninstall)
    let community_buttons: Rc<RefCell<HashMap<String, Button>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // Cross-tab sync: map of plugin name -> Installed tab row (to hide on update)
    let installed_rows: Rc<RefCell<HashMap<String, Group>>> = Rc::new(RefCell::new(HashMap::new()));

    // Create tabs
    let tabs_y = PADDING;
    let tabs_height = DIALOG_HEIGHT - PADDING * 3 - BUTTON_HEIGHT - 10;
    let mut tabs = Tabs::default()
        .with_pos(PADDING, tabs_y)
        .with_size(DIALOG_WIDTH - PADDING * 2, tabs_height);
    // Use flat frame to avoid GTK scheme overriding colors
    tabs.set_frame(FrameType::FlatBox);
    // Inactive tabs use Tabs widget's color - use row_bg for subtle difference
    tabs.set_color(theme.row_bg);
    // Active tab uses Tabs widget's selection_color
    tabs.set_selection_color(theme.tab_active_bg);

    // ============ INSTALLED TAB ============
    let mut installed_group = Group::default()
        .with_pos(PADDING, tabs_y + TAB_HEIGHT)
        .with_size(DIALOG_WIDTH - PADDING * 2, tabs_height - TAB_HEIGHT)
        .with_label("Installed");
    installed_group.set_label_color(theme.text);
    // Child group color should match active tab color so panel blends with tab
    installed_group.set_color(theme.tab_active_bg);

    // Search bar for installed tab
    let search_installed_y = tabs_y + TAB_HEIGHT + SEARCH_MARGIN;
    let search_installed_w = DIALOG_WIDTH - PADDING * 2 - 10;
    let mut search_installed_input = create_placeholder_input(
        PADDING + 5,
        search_installed_y,
        search_installed_w,
        SEARCH_HEIGHT,
        "Search installed plugins...",
        &theme,
    );

    // Original working scroll/pack positions (shifted down for search bar)
    let scroll_installed_y = search_installed_y + SEARCH_HEIGHT + SEARCH_MARGIN;
    let scroll_installed_h = tabs_height - TAB_HEIGHT - 10 - SEARCH_HEIGHT - SEARCH_MARGIN * 2;
    let mut scroll_installed = Scroll::default()
        .with_pos(PADDING + 5, scroll_installed_y)
        .with_size(DIALOG_WIDTH - PADDING * 2 - 10, scroll_installed_h);
    scroll_installed.set_color(theme.bg);
    scroll_installed.set_type(fltk::group::ScrollType::Vertical);
    theme.style_scroll(&mut scroll_installed);

    // Row width: use scroll width
    let scroll_w = DIALOG_WIDTH - PADDING * 2 - 10;
    let row_width = scroll_w;

    let mut pack_installed_inner = Pack::default()
        .with_pos(PADDING + 5, scroll_installed_y)
        .with_size(row_width, 0);
    pack_installed_inner.set_type(PackType::Vertical);
    pack_installed_inner.set_spacing(5);

    // Add installed plugins
    let plugin_list = plugins.list_plugins();
    // Track the empty label so we can hide it when a plugin is installed from Available tab
    let empty_label_for_tracking: Rc<RefCell<Option<Frame>>> = Rc::new(RefCell::new(None));
    // Store row_width for use in callbacks
    let row_width_for_callback = row_width;

    // Track installed data for search: (Group, name, description)
    type InstalledRow = (Group, String, String);
    let installed_search_data: Rc<RefCell<Vec<InstalledRow>>> = Rc::new(RefCell::new(Vec::new()));

    if plugin_list.is_empty() {
        let mut empty_label = Frame::default()
            .with_size(row_width, 40)
            .with_label("No plugins installed");
        empty_label.set_label_color(theme.text_dim);
        *empty_label_for_tracking.borrow_mut() = Some(empty_label);
    } else {
        for plugin in plugin_list {
            let dir_name = plugin.name.to_lowercase().replace(' ', "-");
            let tier = determine_plugin_tier(&dir_name);
            let row = create_installed_plugin_row(
                &plugin.name,
                &plugin.version,
                &plugin.description,
                plugin.enabled,
                &theme,
                row_width,
                toggles.clone(),
                installed.clone(),
                uninstalled.clone(),
                available_buttons.clone(),
                community_buttons.clone(),
                installed_rows.clone(),
                tier,
                Rc::new(RefCell::new(pack_installed_inner.clone())),
                row_width,
            );
            // Track installed row for cross-tab sync (hide on update)
            installed_rows
                .borrow_mut()
                .insert(plugin.name.clone(), row.clone());
            installed_search_data.borrow_mut().push((
                row.clone(),
                plugin.name.clone(),
                plugin.description.clone(),
            ));
            pack_installed_inner.add(&row);
        }
    }

    pack_installed_inner.end();
    adjust_pack_width(&mut pack_installed_inner, row_width);
    scroll_installed.end();
    installed_group.end();

    // Wrap pack_installed for cross-tab sharing
    let pack_installed: Rc<RefCell<Pack>> = Rc::new(RefCell::new(pack_installed_inner));

    // Installed tab search callback
    {
        search_installed_input.set_trigger(CallbackTrigger::Changed);
        let installed_data_search = installed_search_data.clone();
        let pack_installed_search = pack_installed.clone();
        let rw = row_width;
        search_installed_input.set_callback(move |input| {
            let query = input.value().to_lowercase();
            let data = installed_data_search.borrow();
            for (row, name, desc) in data.iter() {
                let matches = query.is_empty()
                    || name.to_lowercase().contains(&query)
                    || desc.to_lowercase().contains(&query);
                let mut row = row.clone();
                if matches {
                    row.show();
                } else {
                    row.hide();
                }
            }
            adjust_pack_width(&mut pack_installed_search.borrow_mut(), rw);
            pack_installed_search.borrow_mut().redraw();
        });
    }

    // Copy the empty label reference for cross-tab sync
    let empty_installed_label = empty_label_for_tracking.clone();

    // ============ OFFICIAL TAB ============
    let mut available_group = Group::default()
        .with_pos(PADDING, tabs_y + TAB_HEIGHT)
        .with_size(DIALOG_WIDTH - PADDING * 2, tabs_height - TAB_HEIGHT)
        .with_label("Official");
    available_group.set_label_color(theme.text);
    // Child group color should match active tab color so panel blends with tab
    available_group.set_color(theme.tab_active_bg);

    // Search bar for official tab
    let search_official_y = tabs_y + TAB_HEIGHT + SEARCH_MARGIN;
    let search_official_w = DIALOG_WIDTH - PADDING * 2 - 10;
    let mut search_official_input = create_placeholder_input(
        PADDING + 5,
        search_official_y,
        search_official_w,
        SEARCH_HEIGHT,
        "Search official plugins...",
        &theme,
    );

    let scroll_official_y = search_official_y + SEARCH_HEIGHT + SEARCH_MARGIN;
    let scroll_official_h = tabs_height - TAB_HEIGHT - 10 - SEARCH_HEIGHT - SEARCH_MARGIN * 2;
    let mut scroll_available = Scroll::default()
        .with_pos(PADDING + 5, scroll_official_y)
        .with_size(DIALOG_WIDTH - PADDING * 2 - 10, scroll_official_h);
    scroll_available.set_color(theme.bg);
    scroll_available.set_type(fltk::group::ScrollType::Vertical);
    theme.style_scroll(&mut scroll_available);

    let mut pack_available = Pack::default()
        .with_pos(PADDING + 5, scroll_official_y)
        .with_size(row_width, 0);
    pack_available.set_type(PackType::Vertical);
    pack_available.set_spacing(5);

    // Track official data for search: (Group, name, description, tags)
    type OfficialRow = (Group, String, String, Vec<String>);
    let official_search_data: Rc<RefCell<Vec<OfficialRow>>> = Rc::new(RefCell::new(Vec::new()));

    // Fetch available plugins
    let fetch_result = fetch_plugin_registry();

    match fetch_result {
        Ok(registry) => {
            if registry.plugins.is_empty() {
                let mut empty_label = Frame::default()
                    .with_size(row_width, 40)
                    .with_label("No plugins available in registry");
                empty_label.set_label_color(theme.text_dim);
            } else {
                // Get installed plugin versions for update detection
                let installed_versions: Vec<(String, String)> = plugin_list
                    .iter()
                    .map(|p| (p.name.to_lowercase().replace(' ', "-"), p.version.clone()))
                    .collect();

                for plugin_info in &registry.plugins {
                    let dir_name = plugin_info.path.trim_end_matches('/');
                    let already_installed = is_plugin_installed(dir_name);

                    // Check if update available
                    let update_available = installed_versions
                        .iter()
                        .find(|(name, _)| name == dir_name)
                        .is_some_and(|(_, installed_ver)| {
                            is_update_available(installed_ver, &plugin_info.version)
                        });

                    let row = create_available_plugin_row(
                        plugin_info,
                        already_installed,
                        update_available,
                        &theme,
                        row_width_for_callback,
                        installed.clone(),
                        toggles.clone(),
                        uninstalled.clone(),
                        pack_installed.clone(),
                        available_buttons.clone(),
                        community_buttons.clone(),
                        empty_installed_label.clone(),
                        installed_rows.clone(),
                    );
                    official_search_data.borrow_mut().push((
                        row.clone(),
                        plugin_info.name.clone(),
                        plugin_info.description.clone(),
                        plugin_info.tags.clone(),
                    ));
                    pack_available.add(&row);
                }
            }
        }
        Err(e) => {
            let error_color = if is_dark {
                Color::from_rgb(255, 120, 120)
            } else {
                Color::from_rgb(180, 0, 0)
            };
            let mut error_label = Frame::default()
                .with_size(row_width, 60)
                .with_label(&format!("Failed to fetch plugins:\n{}", e));
            error_label.set_label_color(error_color);
            pack_available.add(&error_label);
        }
    }

    pack_available.end();
    adjust_pack_width(&mut pack_available, row_width);
    scroll_available.end();
    available_group.end();

    // Official tab search callback
    {
        search_official_input.set_trigger(CallbackTrigger::Changed);
        let official_data_search = official_search_data.clone();
        let pack_available_rc = Rc::new(RefCell::new(pack_available));
        let rw = row_width;
        search_official_input.set_callback(move |input| {
            let query = input.value().to_lowercase();
            let data = official_data_search.borrow();
            for (row, name, desc, tags) in data.iter() {
                let matches = query.is_empty()
                    || name.to_lowercase().contains(&query)
                    || desc.to_lowercase().contains(&query)
                    || tags.iter().any(|t| t.to_lowercase().contains(&query));
                let mut row = row.clone();
                if matches {
                    row.show();
                } else {
                    row.hide();
                }
            }
            adjust_pack_width(&mut pack_available_rc.borrow_mut(), rw);
            pack_available_rc.borrow_mut().redraw();
        });
    }

    // ============ COMMUNITY TAB ============
    let mut community_group = Group::default()
        .with_pos(PADDING, tabs_y + TAB_HEIGHT)
        .with_size(DIALOG_WIDTH - PADDING * 2, tabs_height - TAB_HEIGHT)
        .with_label("Community");
    community_group.set_label_color(theme.text);
    community_group.set_color(theme.tab_active_bg);

    // Header area: fixed position widgets
    let header_y = tabs_y + TAB_HEIGHT + 5;
    let header_width = DIALOG_WIDTH - PADDING * 2 - 10;

    // Search input
    let mut search_input = create_placeholder_input(
        PADDING + 5,
        header_y,
        header_width,
        25,
        "Search plugins...",
        &theme,
    );

    // "or install from URL" label
    let url_label_y = header_y + 30;
    let mut or_label = Frame::default()
        .with_pos(PADDING + 5, url_label_y)
        .with_size(header_width, 16)
        .with_label("--- or install from URL ---");
    or_label.set_label_color(theme.text_dim);
    or_label.set_label_size(10);

    // URL input + Install button
    let url_y = url_label_y + 20;
    let url_btn_width = 70;
    let url_input_width = header_width - url_btn_width - 5;
    let url_input = create_placeholder_input(
        PADDING + 5,
        url_y,
        url_input_width,
        25,
        "Paste GitHub URL...",
        &theme,
    );

    let mut url_install_btn = Button::default()
        .with_pos(PADDING + 5 + url_input_width + 5, url_y)
        .with_size(url_btn_width, 25)
        .with_label("Install");
    url_install_btn.set_frame(FrameType::RFlatBox);
    url_install_btn.set_color(theme.button_bg);
    url_install_btn.set_label_color(theme.text);
    url_install_btn.set_label_size(11);

    // Scrollable plugin list area
    let list_y = url_y + 30;
    let disclaimer_height = 30;
    let list_height = (tabs_y + tabs_height) - list_y - disclaimer_height - 5;

    let mut scroll_community = Scroll::default()
        .with_pos(PADDING + 5, list_y)
        .with_size(header_width, list_height);
    scroll_community.set_color(theme.bg);
    scroll_community.set_type(fltk::group::ScrollType::Vertical);
    theme.style_scroll(&mut scroll_community);

    let mut pack_community_inner = Pack::default()
        .with_pos(PADDING + 5, list_y)
        .with_size(row_width, 0);
    pack_community_inner.set_type(PackType::Vertical);
    pack_community_inner.set_spacing(5);

    // Loading label (shown until fetch completes)
    let mut loading_label = Frame::default()
        .with_size(row_width, 40)
        .with_label("Switch to this tab to load community plugins...");
    loading_label.set_label_color(theme.text_dim);

    pack_community_inner.end();
    scroll_community.end();

    // Disclaimer footer
    let disclaimer_y = list_y + list_height + 2;
    let mut disclaimer = Frame::default()
        .with_pos(PADDING + 5, disclaimer_y)
        .with_size(header_width, disclaimer_height)
        .with_label("\u{2139} Community plugins are not reviewed or endorsed by FerrisPad.");
    disclaimer.set_label_color(theme.text_dim);
    disclaimer.set_label_size(10);
    disclaimer.set_align(Align::Left | Align::Inside);

    community_group.end();

    // Set up lazy loading state for Community tab
    let community_loaded = Rc::new(RefCell::new(false));
    let pack_community = Rc::new(RefCell::new(pack_community_inner));
    type CommunityRow = (Group, String, String, Vec<String>);
    let community_data: Rc<RefCell<Vec<CommunityRow>>> = Rc::new(RefCell::new(Vec::new()));

    tabs.end();
    tabs.auto_layout();

    // ============ COMMUNITY TAB LAZY LOADING ============
    {
        let community_loaded_cb = community_loaded.clone();
        let pack_community_cb = pack_community.clone();
        let community_data_cb = community_data.clone();
        let community_buttons_cb = community_buttons.clone();
        let installed_cb = installed.clone();
        let toggles_cb = toggles.clone();
        let uninstalled_cb = uninstalled.clone();
        let pack_installed_cb = pack_installed.clone();
        let available_buttons_cb = available_buttons.clone();
        let empty_installed_label_cb = empty_installed_label.clone();
        let installed_rows_cb = installed_rows.clone();
        let theme_cb = theme;
        let row_width_cb = row_width_for_callback;
        let mut loading_label_cb = loading_label.clone();

        tabs.set_callback(move |t| {
            if let Some(child) = t.value()
                && child.label() == "Community"
                && !*community_loaded_cb.borrow()
            {
                *community_loaded_cb.borrow_mut() = true;

                // Remove loading label
                loading_label_cb.hide();

                match fetch_community_registry() {
                    Ok(registry) => {
                        if registry.plugins.is_empty() {
                            let mut empty = Frame::default()
                                .with_size(row_width_cb, 40)
                                .with_label("No community plugins available");
                            empty.set_label_color(theme_cb.text_dim);
                            pack_community_cb.borrow_mut().add(&empty);
                        } else {
                            for plugin_info in &registry.plugins {
                                let already_installed = is_plugin_installed(&plugin_info.name);
                                let row = create_community_plugin_row(
                                    plugin_info,
                                    already_installed,
                                    &theme_cb,
                                    row_width_cb,
                                    installed_cb.clone(),
                                    toggles_cb.clone(),
                                    uninstalled_cb.clone(),
                                    pack_installed_cb.clone(),
                                    available_buttons_cb.clone(),
                                    community_buttons_cb.clone(),
                                    empty_installed_label_cb.clone(),
                                    installed_rows_cb.clone(),
                                );
                                community_data_cb.borrow_mut().push((
                                    row.clone(),
                                    plugin_info.name.clone(),
                                    plugin_info.description.clone(),
                                    plugin_info.tags.clone(),
                                ));
                                pack_community_cb.borrow_mut().add(&row);
                            }
                        }
                    }
                    Err(e) => {
                        let error_color = if theme_cb.is_dark() {
                            Color::from_rgb(255, 120, 120)
                        } else {
                            Color::from_rgb(180, 0, 0)
                        };
                        let mut error_label = Frame::default()
                            .with_size(row_width_cb, 60)
                            .with_label(&format!("Failed to fetch community plugins:\n{}", e));
                        error_label.set_label_color(error_color);
                        pack_community_cb.borrow_mut().add(&error_label);
                    }
                }
                adjust_pack_width(&mut pack_community_cb.borrow_mut(), row_width_cb);
                pack_community_cb.borrow_mut().redraw();
            }
        });
    }

    // ============ COMMUNITY SEARCH CALLBACK ============
    {
        search_input.set_trigger(CallbackTrigger::Changed);
        let community_data_search = community_data.clone();
        let pack_community_search = pack_community.clone();
        let rw = row_width;
        search_input.set_callback(move |input| {
            let query = input.value().to_lowercase();
            let data = community_data_search.borrow();
            for (row, name, desc, tags) in data.iter() {
                let matches = query.is_empty()
                    || name.to_lowercase().contains(&query)
                    || desc.to_lowercase().contains(&query)
                    || tags.iter().any(|t| t.to_lowercase().contains(&query));
                let mut row = row.clone();
                if matches {
                    row.show();
                } else {
                    row.hide();
                }
            }
            adjust_pack_width(&mut pack_community_search.borrow_mut(), rw);
            pack_community_search.borrow_mut().redraw();
        });
    }

    // ============ URL INSTALL CALLBACK ============
    {
        let url_input_for_cb = url_input.clone();
        let theme_for_url = theme;
        let row_width_for_url = row_width_for_callback;
        let installed_rc = installed.clone();
        let toggles_url = toggles.clone();
        let uninstalled_url = uninstalled.clone();
        let pack_installed_url = pack_installed.clone();
        let available_buttons_url = available_buttons.clone();
        let community_buttons_url = community_buttons.clone();
        let empty_installed_label_url = empty_installed_label.clone();
        let installed_rows_url = installed_rows.clone();
        url_install_btn.set_callback(move |btn| {
            let url = url_input_for_cb.value().trim().to_string();
            if url.is_empty() {
                return;
            }

            btn.set_label("...");
            btn.deactivate();
            fltk::app::awake();

            // 1. Parse URL
            if let Err(e) = parse_github_url(&url) {
                btn.set_label("Install");
                btn.activate();
                fltk::dialog::alert_default(&format!("Invalid URL:\n{}", e));
                return;
            }

            // 2. Fetch plugin.toml
            let plugin_toml = match fetch_community_plugin_toml(&url, "main") {
                Ok(content) => content,
                Err(e) => {
                    btn.set_label("Install");
                    btn.activate();
                    fltk::dialog::alert_default(&format!("Failed to fetch plugin.toml:\n{}", e));
                    return;
                }
            };

            // 3. Parse metadata
            let metadata = parse_plugin_toml_content(&plugin_toml);
            let name = metadata
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| {
                    parse_github_url(&url)
                        .map(|(_, repo)| repo)
                        .unwrap_or_else(|_| "unknown".to_string())
                });
            let version = metadata
                .as_ref()
                .map(|m| m.version.clone())
                .unwrap_or_default();
            let permissions: Vec<String> = metadata
                .as_ref()
                .map(|m| m.permissions.execute.clone())
                .unwrap_or_default();
            let has_text_change = detects_text_change_hook(&plugin_toml);
            let scan_warnings = match scan_lua_source(&plugin_toml) {
                LuaScanResult::Warnings(w) => w,
                _ => vec![],
            };

            // 4. Show review dialog
            let review = CommunityInstallReview {
                plugin_name: name.clone(),
                version: version.clone(),
                author: String::new(),
                source_url: url.clone(),
                permissions,
                is_manual: true,
                has_text_change_hook: has_text_change,
                scan_warnings,
            };

            if !show_community_install_dialog(&review, &theme_for_url) {
                btn.set_label("Install");
                btn.activate();
                return;
            }

            // 5. Derive dir name for install
            let dir_name = name.to_lowercase().replace(' ', "-");

            // 6. Install
            match install_community_plugin(
                &dir_name,
                &url,
                "main",
                &plugin_toml,
                PluginTier::Manual,
                None,
            ) {
                Ok(()) => {
                    btn.set_label("Done!");
                    // Name normalization: remove from uninstalled if present
                    uninstalled_url.borrow_mut().retain(|n| {
                        n.to_lowercase().replace(' ', "-") != name.to_lowercase().replace(' ', "-")
                    });
                    installed_rc.borrow_mut().push(name.clone());

                    if let Some(ref mut label) = *empty_installed_label_url.borrow_mut() {
                        label.hide();
                    }

                    let desc = metadata
                        .as_ref()
                        .map(|m| m.description.clone())
                        .unwrap_or_default();
                    let new_row = create_installed_plugin_row(
                        &name,
                        &version,
                        &desc,
                        true,
                        &theme_for_url,
                        row_width_for_url,
                        toggles_url.clone(),
                        installed_rc.clone(),
                        uninstalled_url.clone(),
                        available_buttons_url.clone(),
                        community_buttons_url.clone(),
                        installed_rows_url.clone(),
                        PluginTier::Manual,
                        pack_installed_url.clone(),
                        row_width_for_url,
                    );
                    installed_rows_url
                        .borrow_mut()
                        .insert(name.clone(), new_row.clone());
                    pack_installed_url.borrow_mut().add(&new_row);
                    adjust_pack_width(&mut pack_installed_url.borrow_mut(), row_width_for_url);
                    pack_installed_url.borrow_mut().redraw();
                }
                Err(e) => {
                    btn.set_label("Install");
                    btn.activate();
                    fltk::dialog::alert_default(&format!("Failed to install:\n{}", e));
                }
            }
        });
    }

    // ============ BUTTONS ============
    let btn_y = DIALOG_HEIGHT - PADDING - BUTTON_HEIGHT;

    let mut reload_btn = Button::default()
        .with_pos(PADDING, btn_y)
        .with_size(100, BUTTON_HEIGHT)
        .with_label("Reload All");
    reload_btn.set_frame(FrameType::RFlatBox);
    reload_btn.set_color(theme.button_bg);
    reload_btn.set_label_color(theme.text);

    let mut close_btn = Button::default()
        .with_pos(DIALOG_WIDTH - PADDING - 80, btn_y)
        .with_size(80, BUTTON_HEIGHT)
        .with_label("Close");
    close_btn.set_frame(FrameType::RFlatBox);
    close_btn.set_color(theme.button_bg);
    close_btn.set_label_color(theme.text);

    dialog.end();

    // Set up callbacks
    let result_reload = result.clone();
    let dialog_reload = dialog.clone();
    reload_btn.set_callback(move |_| {
        *result_reload.borrow_mut() = PluginManagerResult::ReloadAll;
        dialog_reload.clone().hide();
    });

    let result_close = result.clone();
    let toggles_close = toggles.clone();
    let installed_close = installed.clone();
    let uninstalled_close = uninstalled.clone();
    let dialog_close = dialog.clone();
    close_btn.set_callback(move |_| {
        let toggled = toggles_close.borrow().clone();
        let inst = installed_close.borrow().clone();
        let uninst = uninstalled_close.borrow().clone();

        if !toggled.is_empty() || !inst.is_empty() || !uninst.is_empty() {
            *result_close.borrow_mut() = PluginManagerResult::Changed {
                toggled,
                installed: inst,
                uninstalled: uninst,
            };
        } else {
            *result_close.borrow_mut() = PluginManagerResult::Cancelled;
        }
        dialog_close.clone().hide();
    });

    // Window close (X button)
    let result_x = result.clone();
    let toggles_x = toggles.clone();
    let installed_x = installed.clone();
    let uninstalled_x = uninstalled.clone();
    dialog.set_callback(move |w| {
        let toggled = toggles_x.borrow().clone();
        let inst = installed_x.borrow().clone();
        let uninst = uninstalled_x.borrow().clone();

        if !toggled.is_empty() || !inst.is_empty() || !uninst.is_empty() {
            *result_x.borrow_mut() = PluginManagerResult::Changed {
                toggled,
                installed: inst,
                uninstalled: uninst,
            };
        }
        w.hide();
    });

    dialog.show();
    super::run_dialog(&dialog);

    result.borrow().clone()
}

/// Create a row for an installed plugin (VSCode-style layout)
#[allow(clippy::too_many_arguments)]
fn create_installed_plugin_row(
    name: &str,
    version: &str,
    description: &str,
    enabled: bool,
    theme: &DialogTheme,
    row_width: i32,
    toggles: Rc<RefCell<Vec<(String, bool)>>>,
    installed: Rc<RefCell<Vec<String>>>,
    uninstalled: Rc<RefCell<Vec<String>>>,
    available_buttons: Rc<RefCell<HashMap<String, Button>>>,
    community_buttons: Rc<RefCell<HashMap<String, Button>>>,
    installed_rows: Rc<RefCell<HashMap<String, Group>>>,
    tier: PluginTier,
    pack_installed: Rc<RefCell<Pack>>,
    pack_row_width: i32,
) -> Group {
    let is_dark = theme.is_dark();

    let mut row = Group::default().with_size(row_width, PLUGIN_ROW_HEIGHT);
    row.set_frame(FrameType::FlatBox);
    row.set_color(theme.row_bg);

    // Layout: [margin 8] [icon 32] [margin 8] [content...] [buttons 72] [margin 8]
    let x = ICON_MARGIN;

    // Icon with first letter
    let first_letter = name.chars().next().unwrap_or('?');
    let icon_color = icon_color_for_letter(first_letter, is_dark);
    let mut icon = Frame::default()
        .with_pos(x, (PLUGIN_ROW_HEIGHT - ICON_SIZE) / 2)
        .with_size(ICON_SIZE, ICON_SIZE)
        .with_label(&first_letter.to_uppercase().to_string());
    icon.set_frame(FrameType::FlatBox);
    icon.set_color(icon_color);
    icon.set_label_color(Color::White);
    icon.set_label_font(Font::HelveticaBold);
    icon.set_label_size(16);
    let content_x = x + ICON_SIZE + ICON_MARGIN;

    // Button dimensions for stacked layout
    let btn_width = 72;
    let btn_height = 22;
    let btn_x = row_width - btn_width - ICON_MARGIN;

    // Content area width (excluding button area)
    let content_width = btn_x - content_x - ICON_MARGIN;

    // Title: name + version (line 1)
    let mut title = Frame::default()
        .with_pos(content_x, 10)
        .with_size(content_width, 18)
        .with_label(&format!("{}  v{}", name, version));
    title.set_label_color(theme.text);
    title.set_align(Align::Left | Align::Inside);

    // Description (line 2) - truncated
    let desc_text = truncate_text(description, DESC_MAX_CHARS);
    let mut desc = Frame::default()
        .with_pos(content_x, 28)
        .with_size(content_width, 16)
        .with_label(&desc_text);
    desc.set_label_color(theme.text_dim);
    desc.set_align(Align::Left | Align::Inside);
    desc.set_label_size(11);

    // Meta line (line 3) - show enabled/disabled status + trust tier badge
    let status_label = if enabled { "Enabled" } else { "Disabled" };
    let (badge_text, meta_color) = match tier {
        PluginTier::Official => (
            "\u{2713} Verified",
            if is_dark {
                Color::from_rgb(100, 180, 100)
            } else {
                Color::from_rgb(50, 120, 50)
            },
        ),
        PluginTier::Community => (
            "\u{2637} Community",
            if is_dark {
                Color::from_rgb(100, 150, 200)
            } else {
                Color::from_rgb(50, 100, 160)
            },
        ),
        PluginTier::Manual => (
            "\u{26A0} Unverified",
            if is_dark {
                Color::from_rgb(200, 160, 80)
            } else {
                Color::from_rgb(160, 120, 40)
            },
        ),
    };
    let meta_text = format!("{} | {}", status_label, badge_text);
    let mut meta = Frame::default()
        .with_pos(content_x, 46)
        .with_size(content_width, 14)
        .with_label(&meta_text);
    meta.set_label_color(meta_color);
    meta.set_align(Align::Left | Align::Inside);
    meta.set_label_size(10);

    // Disable/Enable button (top, subtle rounded flat style)
    let toggle_label = if enabled { "Disable" } else { "Enable" };
    let mut toggle_btn = Button::default()
        .with_pos(btn_x, 12)
        .with_size(btn_width, btn_height)
        .with_label(toggle_label);
    toggle_btn.set_frame(FrameType::RFlatBox);
    toggle_btn.set_color(theme.button_bg);
    toggle_btn.set_label_color(theme.text);
    toggle_btn.set_label_size(11);

    // Uninstall button (bottom, subtle rounded flat style, normal theme colors)
    let mut uninstall_btn = Button::default()
        .with_pos(btn_x, 12 + btn_height + 6)
        .with_size(btn_width, btn_height)
        .with_label("Uninstall");
    uninstall_btn.set_frame(FrameType::RFlatBox);
    uninstall_btn.set_color(theme.button_bg);
    uninstall_btn.set_label_color(theme.text);
    uninstall_btn.set_label_size(11);

    // Toggle callback - switches between Disable/Enable
    let plugin_name = name.to_string();
    let plugin_name_uninstall = name.to_string();
    let mut meta_clone = meta.clone();
    let current_enabled = Rc::new(RefCell::new(enabled));
    let current_enabled_toggle = current_enabled.clone();
    let tier_for_toggle = tier.clone();
    toggle_btn.set_callback(move |btn| {
        let mut is_enabled = current_enabled_toggle.borrow_mut();
        *is_enabled = !*is_enabled;
        let new_state = *is_enabled;

        // Update button label
        btn.set_label(if new_state { "Disable" } else { "Enable" });

        // Update status label with tier badge
        let new_status = if new_state { "Enabled" } else { "Disabled" };
        let badge = match tier_for_toggle {
            PluginTier::Official => "\u{2713} Verified",
            PluginTier::Community => "\u{2637} Community",
            PluginTier::Manual => "\u{26A0} Unverified",
        };
        meta_clone.set_label(&format!("{} | {}", new_status, badge));

        // Track the toggle
        let mut t = toggles.borrow_mut();
        t.retain(|(n, _)| n != &plugin_name);
        t.push((plugin_name.clone(), new_state));

        btn.redraw();
        meta_clone.redraw();
    });

    // Uninstall callback - clone row to hide it after confirmation
    let mut row_to_hide = row.clone();
    let pack_for_uninstall = pack_installed;
    let rw_for_uninstall = pack_row_width;
    uninstall_btn.set_callback(move |_btn| {
        // Confirmation dialog
        let choice = fltk::dialog::choice2_default(
            &format!(
                "Uninstall \"{}\"?\n\nThis will delete the plugin files.",
                plugin_name_uninstall
            ),
            "Cancel",
            "Uninstall",
            "",
        );

        if choice == Some(1) {
            uninstalled.borrow_mut().push(plugin_name_uninstall.clone());
            // Remove from installed list (install-then-uninstall in same session)
            let norm = plugin_name_uninstall.to_lowercase().replace(' ', "-");
            installed.borrow_mut().retain(|n| {
                n.to_lowercase().replace(' ', "-") != norm
            });
            // Hide the entire row immediately
            row_to_hide.hide();
            // Adjust pack width after hiding
            adjust_pack_width(&mut pack_for_uninstall.borrow_mut(), rw_for_uninstall);
            // Trigger parent redraw
            if let Some(mut parent) = row_to_hide.parent() {
                parent.redraw();
            }

            // Remove from installed_rows tracking
            installed_rows.borrow_mut().remove(&plugin_name_uninstall);

            // Cross-tab sync: reset the Available tab button to "Install"
            // Note: registry uses lowercase-hyphenated names (e.g., "python-lint")
            // while installed plugins use display names (e.g., "Python Lint")
            let registry_name = plugin_name_uninstall.to_lowercase().replace(' ', "-");
            if let Some(btn) = available_buttons.borrow_mut().get_mut(&registry_name) {
                btn.set_label("Install");
                btn.activate();
            }

            // Also check community_buttons for cross-tab sync
            if let Some(btn) = community_buttons.borrow_mut().get_mut(&registry_name) {
                btn.set_label("Install");
                btn.activate();
            }
        }
    });

    row.end();
    row
}

/// Create a row for an available plugin (VSCode-style layout)
#[allow(clippy::too_many_arguments)]
fn create_available_plugin_row(
    plugin_info: &AvailablePluginInfo,
    already_installed: bool,
    update_available: bool,
    theme: &DialogTheme,
    row_width: i32,
    installed: Rc<RefCell<Vec<String>>>,
    toggles: Rc<RefCell<Vec<(String, bool)>>>,
    uninstalled: Rc<RefCell<Vec<String>>>,
    pack_installed: Rc<RefCell<Pack>>,
    available_buttons: Rc<RefCell<HashMap<String, Button>>>,
    community_buttons: Rc<RefCell<HashMap<String, Button>>>,
    empty_installed_label: Rc<RefCell<Option<Frame>>>,
    installed_rows: Rc<RefCell<HashMap<String, Group>>>,
) -> Group {
    let is_dark = theme.is_dark();

    let mut row = Group::default().with_size(row_width, PLUGIN_ROW_HEIGHT);
    row.set_frame(FrameType::FlatBox);
    row.set_color(theme.row_bg);

    // Layout: [margin 8] [icon 32] [margin 8] [content...] [button 80] [margin 8]
    let mut x = ICON_MARGIN;

    // Icon with first letter
    let first_letter = plugin_info.name.chars().next().unwrap_or('?');
    let icon_color = icon_color_for_letter(first_letter, is_dark);
    let mut icon = Frame::default()
        .with_pos(x, (PLUGIN_ROW_HEIGHT - ICON_SIZE) / 2)
        .with_size(ICON_SIZE, ICON_SIZE)
        .with_label(&first_letter.to_uppercase().to_string());
    icon.set_frame(FrameType::FlatBox);
    icon.set_color(icon_color);
    icon.set_label_color(Color::White);
    icon.set_label_font(Font::HelveticaBold);
    icon.set_label_size(16);
    x += ICON_SIZE + ICON_MARGIN;

    // Content area width (excluding button area)
    let content_width = row_width - x - ACTION_BUTTON_WIDTH - ICON_MARGIN;

    // Title: name + version (line 1)
    let mut title = Frame::default()
        .with_pos(x, 6)
        .with_size(content_width, 18)
        .with_label(&format!("{}  v{}", plugin_info.name, plugin_info.version));
    title.set_label_color(theme.text);
    title.set_align(Align::Left | Align::Inside);

    // Description (line 2) - truncated
    let desc_text = truncate_text(&plugin_info.description, DESC_MAX_CHARS);
    let mut desc = Frame::default()
        .with_pos(x, 24)
        .with_size(content_width, 16)
        .with_label(&desc_text);
    desc.set_label_color(theme.text_dim);
    desc.set_align(Align::Left | Align::Inside);
    desc.set_label_size(11);

    // Meta line (line 3): author + tags + verification badge
    let is_verified = plugin_info.is_verified();
    let badge_text = if is_verified {
        "\u{2713} Verified"
    } else {
        "\u{26A0} Unverified"
    };
    let meta_text = if plugin_info.author.is_empty() {
        format!("{} \u{00B7} {}", plugin_info.tags.join(", "), badge_text)
    } else {
        format!(
            "by {} \u{00B7} {} \u{00B7} {}",
            plugin_info.author,
            plugin_info.tags.join(", "),
            badge_text
        )
    };
    let mut meta = Frame::default()
        .with_pos(x, 40)
        .with_size(content_width, 14)
        .with_label(&meta_text);
    // Color the meta line based on verification
    let meta_color = if is_verified {
        if is_dark {
            Color::from_rgb(100, 180, 100) // Green-ish
        } else {
            Color::from_rgb(50, 120, 50)
        }
    } else if is_dark {
        Color::from_rgb(200, 160, 80) // Yellow-ish
    } else {
        Color::from_rgb(160, 120, 40)
    };
    meta.set_label_color(meta_color);
    meta.set_align(Align::Left | Align::Inside);
    meta.set_label_size(10);

    // Install/Update button (right side, vertically centered, subtle rounded flat style)
    let btn_width = 72;
    let btn_height = 22;
    let btn_x = row_width - btn_width - ICON_MARGIN;
    let mut install_btn = Button::default()
        .with_pos(btn_x, (PLUGIN_ROW_HEIGHT - btn_height) / 2)
        .with_size(btn_width, btn_height);
    install_btn.set_frame(FrameType::RFlatBox);
    install_btn.set_label_size(11);
    install_btn.set_color(theme.button_bg);
    install_btn.set_label_color(theme.text);

    // Details button (to the left of Install, only if readme_url is present)
    if let Some(readme_url) = &plugin_info.readme_url {
        let details_btn_width = 60;
        let details_btn_x = btn_x - details_btn_width - 8; // 8px gap between buttons
        let mut details_btn = Button::default()
            .with_pos(details_btn_x, (PLUGIN_ROW_HEIGHT - btn_height) / 2)
            .with_size(details_btn_width, btn_height)
            .with_label("Details");
        details_btn.set_frame(FrameType::RFlatBox);
        details_btn.set_label_size(11);
        details_btn.set_color(theme.button_bg);
        details_btn.set_label_color(theme.text);

        let url = readme_url.clone();
        details_btn.set_callback(move |_| {
            let choice = fltk::dialog::choice2_default(
                &format!("This will open your browser to:\n\n{}\n\nContinue?", url),
                "Cancel",
                "Open in Browser",
                "",
            );
            if choice == Some(1) {
                let _ = open::that(&url);
            }
        });
    }

    if already_installed && !update_available {
        install_btn.set_label("Installed");
        install_btn.deactivate();
    } else if update_available {
        install_btn.set_label("Update");
        // Green accent for update
        install_btn.set_label_color(if is_dark {
            Color::from_rgb(100, 200, 100)
        } else {
            Color::from_rgb(40, 140, 40)
        });
    } else {
        install_btn.set_label("Install");
    }

    // Store button reference for cross-tab sync (uninstall -> re-enable Install button)
    available_buttons
        .borrow_mut()
        .insert(plugin_info.name.clone(), install_btn.clone());

    // Capture theme values for callback
    let theme_clone = *theme;
    let row_width_clone = row_width;

    // Install callback
    let info = plugin_info.clone();
    let installed_rc = installed.clone();
    let community_buttons_cb = community_buttons.clone();
    install_btn.set_callback(move |btn| {
        btn.set_label("...");
        btn.deactivate();
        fltk::app::awake(); // Force UI update

        match install_plugin(&info) {
            Ok(status) => {
                use crate::app::services::plugin_verify::VerificationStatus;
                match status {
                    VerificationStatus::Verified | VerificationStatus::Unverified => {
                        btn.set_label("Done!");
                        // Name normalization: remove from uninstalled if present
                        uninstalled.borrow_mut().retain(|n| {
                            n.to_lowercase().replace(' ', "-")
                                != info.name.to_lowercase().replace(' ', "-")
                        });
                        installed_rc.borrow_mut().push(info.name.clone());

                        // Hide "No plugins installed" label if present
                        if let Some(ref mut label) = *empty_installed_label.borrow_mut() {
                            label.hide();
                        }

                        // Cross-tab sync: hide old row if this is an update
                        // Try multiple key formats: registry name uses "python-lint",
                        // but installed plugins use display name "Python Lint"
                        let display_name = plugin_display_name(&info.name);

                        // Try display name first, then registry name
                        // Must split borrows to avoid RefCell double-borrow panic
                        let old_row = {
                            let mut rows = installed_rows.borrow_mut();
                            rows.remove(&display_name)
                                .or_else(|| rows.remove(&info.name))
                        };

                        if let Some(mut old_row) = old_row {
                            old_row.hide();
                            if let Some(mut parent) = old_row.parent() {
                                parent.redraw();
                            }
                        }

                        // Cross-tab sync: add row to Installed tab
                        let new_row = create_installed_plugin_row(
                            &info.name,
                            &info.version,
                            &info.description,
                            true, // enabled by default
                            &theme_clone,
                            row_width_clone,
                            toggles.clone(),
                            installed_rc.clone(),
                            uninstalled.clone(),
                            available_buttons.clone(),
                            community_buttons_cb.clone(),
                            installed_rows.clone(),
                            PluginTier::Official,
                            pack_installed.clone(),
                            row_width_clone,
                        );
                        // Track the new row for future updates
                        installed_rows
                            .borrow_mut()
                            .insert(info.name.clone(), new_row.clone());
                        pack_installed.borrow_mut().add(&new_row);
                        adjust_pack_width(&mut pack_installed.borrow_mut(), row_width_clone);
                        pack_installed.borrow_mut().redraw();
                    }
                    VerificationStatus::Invalid(reason) => {
                        btn.set_label("Failed");
                        btn.activate();
                        fltk::dialog::alert_default(&format!(
                            "Signature verification failed for {}:\n{}",
                            info.name, reason
                        ));
                    }
                }
            }
            Err(e) => {
                btn.set_label("Error");
                btn.activate();
                eprintln!("[plugins] Install error: {}", e);
                fltk::dialog::alert_default(&format!("Failed to install {}:\n{}", info.name, e));
            }
        }
    });

    row.end();
    row
}

/// Create a row for a community plugin
#[allow(clippy::too_many_arguments)]
fn create_community_plugin_row(
    plugin_info: &CommunityPluginInfo,
    already_installed: bool,
    theme: &DialogTheme,
    row_width: i32,
    installed: Rc<RefCell<Vec<String>>>,
    toggles: Rc<RefCell<Vec<(String, bool)>>>,
    uninstalled: Rc<RefCell<Vec<String>>>,
    pack_installed: Rc<RefCell<Pack>>,
    available_buttons: Rc<RefCell<HashMap<String, Button>>>,
    community_buttons: Rc<RefCell<HashMap<String, Button>>>,
    empty_installed_label: Rc<RefCell<Option<Frame>>>,
    installed_rows: Rc<RefCell<HashMap<String, Group>>>,
) -> Group {
    let is_dark = theme.is_dark();

    let mut row = Group::default().with_size(row_width, PLUGIN_ROW_HEIGHT);
    row.set_frame(FrameType::FlatBox);
    row.set_color(theme.row_bg);

    // Layout: [margin 8] [icon 32] [margin 8] [content...] [button 80] [margin 8]
    let mut x = ICON_MARGIN;

    // Icon with first letter
    let first_letter = plugin_info.name.chars().next().unwrap_or('?');
    let icon_color = icon_color_for_letter(first_letter, is_dark);
    let mut icon = Frame::default()
        .with_pos(x, (PLUGIN_ROW_HEIGHT - ICON_SIZE) / 2)
        .with_size(ICON_SIZE, ICON_SIZE)
        .with_label(&first_letter.to_uppercase().to_string());
    icon.set_frame(FrameType::FlatBox);
    icon.set_color(icon_color);
    icon.set_label_color(Color::White);
    icon.set_label_font(Font::HelveticaBold);
    icon.set_label_size(16);
    x += ICON_SIZE + ICON_MARGIN;

    // Content area width (excluding button area)
    let content_width = row_width - x - ACTION_BUTTON_WIDTH - ICON_MARGIN;

    // Title: name + version (line 1)
    let mut title = Frame::default()
        .with_pos(x, 6)
        .with_size(content_width, 18)
        .with_label(&format!("{}  v{}", plugin_info.name, plugin_info.version));
    title.set_label_color(theme.text);
    title.set_align(Align::Left | Align::Inside);

    // Description (line 2) - truncated
    let desc_text = truncate_text(&plugin_info.description, DESC_MAX_CHARS);
    let mut desc = Frame::default()
        .with_pos(x, 24)
        .with_size(content_width, 16)
        .with_label(&desc_text);
    desc.set_label_color(theme.text_dim);
    desc.set_align(Align::Left | Align::Inside);
    desc.set_label_size(11);

    // Meta line (line 3): badge + author
    let badge_text = "\u{2637} Community";
    let meta_color = if is_dark {
        Color::from_rgb(100, 150, 200)
    } else {
        Color::from_rgb(50, 100, 160)
    };
    let meta_text = if plugin_info.author.is_empty() {
        badge_text.to_string()
    } else {
        format!("{} \u{00B7} by {}", badge_text, plugin_info.author)
    };
    let mut meta = Frame::default()
        .with_pos(x, 40)
        .with_size(content_width, 14)
        .with_label(&meta_text);
    meta.set_label_color(meta_color);
    meta.set_align(Align::Left | Align::Inside);
    meta.set_label_size(10);

    // Install button (right side, vertically centered)
    let btn_width = 72;
    let btn_height = 22;
    let btn_x = row_width - btn_width - ICON_MARGIN;
    let mut install_btn = Button::default()
        .with_pos(btn_x, (PLUGIN_ROW_HEIGHT - btn_height) / 2)
        .with_size(btn_width, btn_height);
    install_btn.set_frame(FrameType::RFlatBox);
    install_btn.set_label_size(11);
    install_btn.set_color(theme.button_bg);
    install_btn.set_label_color(theme.text);

    // Details button (to the left of Install, only if repo URL is present)
    if !plugin_info.repo.is_empty() {
        let details_btn_width = 60;
        let details_btn_x = btn_x - details_btn_width - 8;
        let mut details_btn = Button::default()
            .with_pos(details_btn_x, (PLUGIN_ROW_HEIGHT - btn_height) / 2)
            .with_size(details_btn_width, btn_height)
            .with_label("Details");
        details_btn.set_frame(FrameType::RFlatBox);
        details_btn.set_label_size(11);
        details_btn.set_color(theme.button_bg);
        details_btn.set_label_color(theme.text);

        let url = plugin_info.repo.clone();
        details_btn.set_callback(move |_| {
            let choice = fltk::dialog::choice2_default(
                &format!("This will open your browser to:\n\n{}\n\nContinue?", url),
                "Cancel",
                "Open in Browser",
                "",
            );
            if choice == Some(1) {
                let _ = open::that(&url);
            }
        });
    }

    if already_installed {
        install_btn.set_label("Installed");
        install_btn.deactivate();
    } else {
        install_btn.set_label("Install");
    }

    // Store button reference for cross-tab sync
    community_buttons
        .borrow_mut()
        .insert(plugin_info.name.clone(), install_btn.clone());

    // Capture values for callback
    let theme_clone = *theme;
    let row_width_clone = row_width;
    let info = plugin_info.clone();
    let installed_rc = installed.clone();
    install_btn.set_callback(move |btn| {
        btn.set_label("...");
        btn.deactivate();
        fltk::app::awake();

        // 1. Fetch plugin.toml
        let plugin_toml = match fetch_community_plugin_toml(&info.repo, &info.git_ref) {
            Ok(content) => content,
            Err(e) => {
                btn.set_label("Error");
                btn.activate();
                fltk::dialog::alert_default(&format!("Failed to fetch plugin.toml:\n{}", e));
                return;
            }
        };

        // 2. Parse permissions
        let metadata = parse_plugin_toml_content(&plugin_toml);
        let permissions: Vec<String> = metadata
            .as_ref()
            .map(|m| m.permissions.execute.clone())
            .unwrap_or_default();

        // 3. Check for text change hook
        let has_text_change = detects_text_change_hook(&plugin_toml);

        // 4. Preliminary scan (on plugin.toml -- actual init.lua scan happens in install)
        let scan_warnings = match scan_lua_source(&plugin_toml) {
            LuaScanResult::Warnings(w) => w,
            _ => vec![],
        };

        // 5. Show review dialog
        let review = CommunityInstallReview {
            plugin_name: info.name.clone(),
            version: info.version.clone(),
            author: info.author.clone(),
            source_url: info.repo.clone(),
            permissions,
            is_manual: false,
            has_text_change_hook: has_text_change,
            scan_warnings,
        };

        if !show_community_install_dialog(&review, &theme_clone) {
            btn.set_label("Install");
            btn.activate();
            return;
        }

        // 6. Install
        match install_community_plugin(
            &info.name,
            &info.repo,
            &info.git_ref,
            &plugin_toml,
            PluginTier::Community,
            Some(&info.checksums),
        ) {
            Ok(()) => {
                btn.set_label("Done!");
                // Name normalization: remove from uninstalled if present
                uninstalled.borrow_mut().retain(|n| {
                    n.to_lowercase().replace(' ', "-") != info.name.to_lowercase().replace(' ', "-")
                });
                installed_rc.borrow_mut().push(info.name.clone());

                // Hide "No plugins installed" label
                if let Some(ref mut label) = *empty_installed_label.borrow_mut() {
                    label.hide();
                }

                // Add row to Installed tab
                let desc = metadata
                    .as_ref()
                    .map(|m| m.description.clone())
                    .unwrap_or_default();
                let new_row = create_installed_plugin_row(
                    &info.name,
                    &info.version,
                    &desc,
                    true,
                    &theme_clone,
                    row_width_clone,
                    toggles.clone(),
                    installed_rc.clone(),
                    uninstalled.clone(),
                    available_buttons.clone(),
                    community_buttons.clone(),
                    installed_rows.clone(),
                    PluginTier::Community,
                    pack_installed.clone(),
                    row_width_clone,
                );
                installed_rows
                    .borrow_mut()
                    .insert(info.name.clone(), new_row.clone());
                pack_installed.borrow_mut().add(&new_row);
                adjust_pack_width(&mut pack_installed.borrow_mut(), row_width_clone);
                pack_installed.borrow_mut().redraw();
            }
            Err(e) => {
                btn.set_label("Error");
                btn.activate();
                fltk::dialog::alert_default(&format!("Failed to install {}:\n{}", info.name, e));
            }
        }
    });

    row.end();
    row
}
