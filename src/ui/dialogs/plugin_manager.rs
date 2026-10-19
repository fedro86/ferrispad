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
    fetch_community_plugin_toml, fetch_community_registry_cached, fetch_plugin_registry_cached,
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

    // State shared by the rows of all three tabs. The empty label is tracked so
    // it can be hidden when a plugin is installed from another tab.
    let row_ctx = PluginRowContext {
        theme,
        row_width,
        toggles: toggles.clone(),
        installed: installed.clone(),
        uninstalled: uninstalled.clone(),
        available_buttons,
        community_buttons,
        installed_rows,
        pack_installed: Rc::new(RefCell::new(pack_installed_inner.clone())),
        empty_installed_label: Rc::new(RefCell::new(None)),
    };

    // Track installed data for search: (Group, name, description)
    type InstalledRow = (Group, String, String);
    let installed_search_data: Rc<RefCell<Vec<InstalledRow>>> = Rc::new(RefCell::new(Vec::new()));

    if plugin_list.is_empty() {
        let mut empty_label = Frame::default()
            .with_size(row_width, 40)
            .with_label("No plugins installed");
        empty_label.set_label_color(theme.text_dim);
        *row_ctx.empty_installed_label.borrow_mut() = Some(empty_label);
    } else {
        for plugin in plugin_list {
            let dir_name = plugin.name.to_lowercase().replace(' ', "-");
            let tier = determine_plugin_tier(&dir_name);
            let row = create_plugin_row(
                RowKind::Installed(InstalledPlugin {
                    name: &plugin.name,
                    version: &plugin.version,
                    description: &plugin.description,
                    enabled: plugin.enabled,
                    tier,
                }),
                &row_ctx,
            );
            // Track installed row for cross-tab sync (hide on update)
            row_ctx
                .installed_rows
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

    // Installed tab search callback
    {
        search_installed_input.set_trigger(CallbackTrigger::Changed);
        let installed_data_search = installed_search_data.clone();
        let pack_installed_search = row_ctx.pack_installed.clone();
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
    let fetch_result = fetch_plugin_registry_cached();

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

                    let row = create_plugin_row(
                        RowKind::Official {
                            info: plugin_info,
                            already_installed,
                            update_available,
                        },
                        &row_ctx,
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
            let mut error_label = Frame::default()
                .with_size(row_width, 60)
                .with_label(&format!("Failed to fetch plugins:\n{}", e));
            error_label.set_label_color(theme.error_color());
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
        let row_ctx_cb = row_ctx.clone();
        let theme_cb = theme;
        let row_width_cb = row_width;
        let mut loading_label_cb = loading_label.clone();

        tabs.set_callback(move |t| {
            if let Some(child) = t.value()
                && child.label() == "Community"
                && !*community_loaded_cb.borrow()
            {
                *community_loaded_cb.borrow_mut() = true;

                // Remove loading label
                loading_label_cb.hide();

                match fetch_community_registry_cached() {
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
                                let row = create_plugin_row(
                                    RowKind::Community {
                                        info: plugin_info,
                                        already_installed,
                                    },
                                    &row_ctx_cb,
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
                        let mut error_label = Frame::default()
                            .with_size(row_width_cb, 60)
                            .with_label(&format!("Failed to fetch community plugins:\n{}", e));
                        error_label.set_label_color(theme_cb.error_color());
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
        let row_ctx_url = row_ctx.clone();
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
                    row_ctx_url.record_install(&name);

                    let desc = metadata
                        .as_ref()
                        .map(|m| m.description.clone())
                        .unwrap_or_default();
                    row_ctx_url.add_installed_row(InstalledPlugin {
                        name: &name,
                        version: &version,
                        description: &desc,
                        enabled: true,
                        tier: PluginTier::Manual,
                    });
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
    theme.apply_titlebar(&dialog);
    super::run_dialog(&dialog);

    result.borrow().clone()
}

// ============ PLUGIN ROWS ============

/// Shared dialog state every plugin row needs.
///
/// The session lists `installed` and `uninstalled` have the same type, so
/// passing them (and the other handles) positionally made a swap at a call site
/// compile silently. Bundling them under named fields makes that impossible.
#[derive(Clone)]
struct PluginRowContext {
    theme: DialogTheme,
    /// Width of every row (and of the Installed tab's pack).
    row_width: i32,
    /// Enable/disable toggles made this session.
    toggles: Rc<RefCell<Vec<(String, bool)>>>,
    /// Plugins installed this session.
    installed: Rc<RefCell<Vec<String>>>,
    /// Plugins uninstalled this session.
    uninstalled: Rc<RefCell<Vec<String>>>,
    /// Official tab install buttons by registry name (reset on uninstall).
    available_buttons: Rc<RefCell<HashMap<String, Button>>>,
    /// Community tab install buttons by registry name (reset on uninstall).
    community_buttons: Rc<RefCell<HashMap<String, Button>>>,
    /// Installed tab rows by plugin name (hidden on update/uninstall).
    installed_rows: Rc<RefCell<HashMap<String, Group>>>,
    /// The Installed tab's row container.
    pack_installed: Rc<RefCell<Pack>>,
    /// The "No plugins installed" label, if the Installed tab started empty.
    empty_installed_label: Rc<RefCell<Option<Frame>>>,
}

impl PluginRowContext {
    /// Session book-keeping after a successful install from any tab.
    fn record_install(&self, name: &str) {
        let norm = normalize_plugin_name(name);
        self.uninstalled
            .borrow_mut()
            .retain(|n| normalize_plugin_name(n) != norm);
        self.installed.borrow_mut().push(name.to_string());
        if let Some(ref mut label) = *self.empty_installed_label.borrow_mut() {
            label.hide();
        }
    }

    /// Add a freshly installed (enabled) plugin's row to the Installed tab.
    fn add_installed_row(&self, plugin: InstalledPlugin<'_>) {
        let name = plugin.name.to_string();
        let new_row = create_plugin_row(RowKind::Installed(plugin), self);
        self.installed_rows
            .borrow_mut()
            .insert(name, new_row.clone());
        self.pack_installed.borrow_mut().add(&new_row);
        adjust_pack_width(&mut self.pack_installed.borrow_mut(), self.row_width);
        self.pack_installed.borrow_mut().redraw();
    }
}

/// Registry names are lowercase-hyphenated ("python-lint") while installed
/// plugins carry display names ("Python Lint"); compare them in this form.
fn normalize_plugin_name(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// An installed plugin as shown on the Installed tab.
struct InstalledPlugin<'a> {
    name: &'a str,
    version: &'a str,
    description: &'a str,
    enabled: bool,
    tier: PluginTier,
}

/// Which tab a row belongs to, with the data that tab's row shows.
enum RowKind<'a> {
    /// Installed tab: Enable/Disable + Uninstall.
    Installed(InstalledPlugin<'a>),
    /// Official tab: Install/Update (+ Details when a README is listed).
    Official {
        info: &'a AvailablePluginInfo,
        already_installed: bool,
        update_available: bool,
    },
    /// Community tab: Install (+ Details when a repo is listed).
    Community {
        info: &'a CommunityPluginInfo,
        already_installed: bool,
    },
}

/// The three text lines of a row plus the colour of its meta line.
#[derive(Debug, PartialEq, Eq)]
struct RowHeader<'a> {
    name: &'a str,
    version: &'a str,
    description: &'a str,
    meta_text: String,
    badge: TrustBadge,
}

impl<'a> RowKind<'a> {
    fn header(&self) -> RowHeader<'a> {
        match self {
            RowKind::Installed(p) => {
                let badge = TrustBadge::for_tier(&p.tier);
                RowHeader {
                    name: p.name,
                    version: p.version,
                    description: p.description,
                    meta_text: installed_meta_text(p.enabled, badge),
                    badge,
                }
            }
            RowKind::Official { info, .. } => {
                let badge = if info.is_verified() {
                    TrustBadge::Verified
                } else {
                    TrustBadge::Unverified
                };
                RowHeader {
                    name: &info.name,
                    version: &info.version,
                    description: &info.description,
                    meta_text: official_meta_text(&info.author, &info.tags, badge),
                    badge,
                }
            }
            RowKind::Community { info, .. } => RowHeader {
                name: &info.name,
                version: &info.version,
                description: &info.description,
                meta_text: community_meta_text(&info.author),
                badge: TrustBadge::Community,
            },
        }
    }
}

/// Trust badge shown on a row's meta line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrustBadge {
    Verified,
    Community,
    Unverified,
}

impl TrustBadge {
    fn for_tier(tier: &PluginTier) -> Self {
        match tier {
            PluginTier::Official => TrustBadge::Verified,
            PluginTier::Community => TrustBadge::Community,
            PluginTier::Manual => TrustBadge::Unverified,
        }
    }

    fn label(self) -> &'static str {
        match self {
            TrustBadge::Verified => "\u{2713} Verified",
            TrustBadge::Community => "\u{2637} Community",
            TrustBadge::Unverified => "\u{26A0} Unverified",
        }
    }

    fn color(self, is_dark: bool) -> Color {
        let (r, g, b) = match (self, is_dark) {
            (TrustBadge::Verified, true) => (100, 180, 100),
            (TrustBadge::Verified, false) => (50, 120, 50),
            (TrustBadge::Community, true) => (100, 150, 200),
            (TrustBadge::Community, false) => (50, 100, 160),
            (TrustBadge::Unverified, true) => (200, 160, 80),
            (TrustBadge::Unverified, false) => (160, 120, 40),
        };
        Color::from_rgb(r, g, b)
    }
}

fn installed_meta_text(enabled: bool, badge: TrustBadge) -> String {
    let status = if enabled { "Enabled" } else { "Disabled" };
    format!("{} | {}", status, badge.label())
}

fn official_meta_text(author: &str, tags: &[String], badge: TrustBadge) -> String {
    if author.is_empty() {
        format!("{} \u{00B7} {}", tags.join(", "), badge.label())
    } else {
        format!(
            "by {} \u{00B7} {} \u{00B7} {}",
            author,
            tags.join(", "),
            badge.label()
        )
    }
}

fn community_meta_text(author: &str) -> String {
    let badge = TrustBadge::Community.label();
    if author.is_empty() {
        badge.to_string()
    } else {
        format!("{} \u{00B7} by {}", badge, author)
    }
}

/// Initial state of an Official/Community install button.
#[derive(Debug, PartialEq, Eq)]
enum InstallButtonState {
    Install,
    Update,
    Installed,
}

fn install_button_state(already_installed: bool, update_available: bool) -> InstallButtonState {
    if update_available {
        InstallButtonState::Update
    } else if already_installed {
        InstallButtonState::Installed
    } else {
        InstallButtonState::Install
    }
}

// Text block shared by every row: title / description / meta lines.
const TITLE_LINE_H: i32 = 18;
const DESC_LINE_H: i32 = 16;
const META_LINE_H: i32 = 14;
const META_LINE_GAP: i32 = 2;

// Action buttons on the right of a row.
const ROW_BTN_WIDTH: i32 = 72;
const ROW_BTN_HEIGHT: i32 = 22;
const DETAILS_BTN_WIDTH: i32 = 60;
const ROW_BTN_GAP: i32 = 8;

/// Y offsets (relative to the row) of the title, description and meta lines.
#[derive(Debug, PartialEq, Eq)]
struct RowTextLayout {
    title_y: i32,
    desc_y: i32,
    meta_y: i32,
}

/// One layout for every tab: the three lines stacked and centred vertically.
fn row_text_layout(row_height: i32) -> RowTextLayout {
    let block_h = TITLE_LINE_H + DESC_LINE_H + META_LINE_GAP + META_LINE_H;
    let title_y = (row_height - block_h) / 2;
    let desc_y = title_y + TITLE_LINE_H;
    let meta_y = desc_y + DESC_LINE_H + META_LINE_GAP;
    RowTextLayout {
        title_y,
        desc_y,
        meta_y,
    }
}

fn style_row_button(btn: &mut Button, theme: &DialogTheme) {
    btn.set_frame(FrameType::RFlatBox);
    btn.set_color(theme.button_bg);
    btn.set_label_color(theme.text);
    btn.set_label_size(11);
}

/// Create a plugin row (VSCode-style layout) for any of the three tabs.
fn create_plugin_row(kind: RowKind<'_>, ctx: &PluginRowContext) -> Group {
    let theme = &ctx.theme;
    let is_dark = theme.is_dark();
    let row_width = ctx.row_width;
    let header = kind.header();

    let mut row = Group::default().with_size(row_width, PLUGIN_ROW_HEIGHT);
    row.set_frame(FrameType::FlatBox);
    row.set_color(theme.row_bg);

    // Layout: [margin 8] [icon 32] [margin 8] [content...] [buttons 80] [margin 8]
    let x = ICON_MARGIN;

    // Icon with first letter
    let first_letter = header.name.chars().next().unwrap_or('?');
    let mut icon = Frame::default()
        .with_pos(x, (PLUGIN_ROW_HEIGHT - ICON_SIZE) / 2)
        .with_size(ICON_SIZE, ICON_SIZE)
        .with_label(&first_letter.to_uppercase().to_string());
    icon.set_frame(FrameType::FlatBox);
    icon.set_color(icon_color_for_letter(first_letter, is_dark));
    icon.set_label_color(Color::White);
    icon.set_label_font(Font::HelveticaBold);
    icon.set_label_size(16);

    let content_x = x + ICON_SIZE + ICON_MARGIN;
    let content_width = row_width - content_x - ACTION_BUTTON_WIDTH - ICON_MARGIN;
    let layout = row_text_layout(PLUGIN_ROW_HEIGHT);

    // Title: name + version (line 1)
    let mut title = Frame::default()
        .with_pos(content_x, layout.title_y)
        .with_size(content_width, TITLE_LINE_H)
        .with_label(&format!("{}  v{}", header.name, header.version));
    title.set_label_color(theme.text);
    title.set_align(Align::Left | Align::Inside);

    // Description (line 2) - truncated
    let mut desc = Frame::default()
        .with_pos(content_x, layout.desc_y)
        .with_size(content_width, DESC_LINE_H)
        .with_label(&truncate_text(header.description, DESC_MAX_CHARS));
    desc.set_label_color(theme.text_dim);
    desc.set_align(Align::Left | Align::Inside);
    desc.set_label_size(11);

    // Meta line (line 3) - status/author/tags + trust badge
    let mut meta = Frame::default()
        .with_pos(content_x, layout.meta_y)
        .with_size(content_width, META_LINE_H)
        .with_label(&header.meta_text);
    meta.set_label_color(header.badge.color(is_dark));
    meta.set_align(Align::Left | Align::Inside);
    meta.set_label_size(10);

    let btn_x = row_width - ROW_BTN_WIDTH - ICON_MARGIN;
    match kind {
        RowKind::Installed(plugin) => {
            add_installed_actions(&row, meta, plugin.name, plugin.enabled, header.badge, ctx);
        }
        RowKind::Official {
            info,
            already_installed,
            update_available,
        } => {
            add_details_button(info.readme_url.as_deref(), btn_x, theme);
            let state = install_button_state(already_installed, update_available);
            add_official_install_button(info, state, btn_x, ctx);
        }
        RowKind::Community {
            info,
            already_installed,
        } => {
            let repo = Some(info.repo.as_str()).filter(|r| !r.is_empty());
            add_details_button(repo, btn_x, theme);
            let state = install_button_state(already_installed, false);
            add_community_install_button(info, state, btn_x, ctx);
        }
    }

    row.end();
    row
}

/// Installed tab: stacked Disable/Enable and Uninstall buttons.
fn add_installed_actions(
    row: &Group,
    meta: Frame,
    name: &str,
    enabled: bool,
    badge: TrustBadge,
    ctx: &PluginRowContext,
) {
    let btn_x = ctx.row_width - ROW_BTN_WIDTH - ICON_MARGIN;

    // Disable/Enable button (top)
    let mut toggle_btn = Button::default()
        .with_pos(btn_x, 12)
        .with_size(ROW_BTN_WIDTH, ROW_BTN_HEIGHT)
        .with_label(if enabled { "Disable" } else { "Enable" });
    style_row_button(&mut toggle_btn, &ctx.theme);

    // Uninstall button (bottom)
    let mut uninstall_btn = Button::default()
        .with_pos(btn_x, 12 + ROW_BTN_HEIGHT + 6)
        .with_size(ROW_BTN_WIDTH, ROW_BTN_HEIGHT)
        .with_label("Uninstall");
    style_row_button(&mut uninstall_btn, &ctx.theme);

    // Toggle callback - switches between Disable/Enable
    let plugin_name = name.to_string();
    let mut meta_clone = meta;
    let mut is_enabled = enabled;
    let toggles = ctx.toggles.clone();
    toggle_btn.set_callback(move |btn| {
        is_enabled = !is_enabled;
        btn.set_label(if is_enabled { "Disable" } else { "Enable" });
        meta_clone.set_label(&installed_meta_text(is_enabled, badge));

        // Track the toggle
        let mut t = toggles.borrow_mut();
        t.retain(|(n, _)| n != &plugin_name);
        t.push((plugin_name.clone(), is_enabled));

        btn.redraw();
        meta_clone.redraw();
    });

    // Uninstall callback - clone row to hide it after confirmation
    let mut row_to_hide = row.clone();
    let plugin_name_uninstall = name.to_string();
    let ctx = ctx.clone();
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
            ctx.uninstalled
                .borrow_mut()
                .push(plugin_name_uninstall.clone());
            // Remove from installed list (install-then-uninstall in same session)
            let registry_name = normalize_plugin_name(&plugin_name_uninstall);
            ctx.installed
                .borrow_mut()
                .retain(|n| normalize_plugin_name(n) != registry_name);
            // Hide the entire row immediately
            row_to_hide.hide();
            // Adjust pack width after hiding
            adjust_pack_width(&mut ctx.pack_installed.borrow_mut(), ctx.row_width);
            // Trigger parent redraw
            if let Some(mut parent) = row_to_hide.parent() {
                parent.redraw();
            }

            // Remove from installed_rows tracking
            ctx.installed_rows
                .borrow_mut()
                .remove(&plugin_name_uninstall);

            // Cross-tab sync: reset the Official/Community tab buttons to "Install"
            // (registry names are lowercase-hyphenated, see normalize_plugin_name)
            if let Some(btn) = ctx.available_buttons.borrow_mut().get_mut(&registry_name) {
                btn.set_label("Install");
                btn.activate();
            }
            if let Some(btn) = ctx.community_buttons.borrow_mut().get_mut(&registry_name) {
                btn.set_label("Install");
                btn.activate();
            }
        }
    });
}

/// "Details" button left of the install button; opens `url` after confirmation.
fn add_details_button(url: Option<&str>, btn_x: i32, theme: &DialogTheme) {
    let Some(url) = url else {
        return;
    };
    let mut details_btn = Button::default()
        .with_pos(
            btn_x - DETAILS_BTN_WIDTH - ROW_BTN_GAP,
            (PLUGIN_ROW_HEIGHT - ROW_BTN_HEIGHT) / 2,
        )
        .with_size(DETAILS_BTN_WIDTH, ROW_BTN_HEIGHT)
        .with_label("Details");
    style_row_button(&mut details_btn, theme);

    let url = url.to_string();
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

/// Install/Update button (right side, vertically centred) in its initial state.
fn create_install_button(state: &InstallButtonState, btn_x: i32, theme: &DialogTheme) -> Button {
    let mut install_btn = Button::default()
        .with_pos(btn_x, (PLUGIN_ROW_HEIGHT - ROW_BTN_HEIGHT) / 2)
        .with_size(ROW_BTN_WIDTH, ROW_BTN_HEIGHT);
    style_row_button(&mut install_btn, theme);
    match state {
        InstallButtonState::Installed => {
            install_btn.set_label("Installed");
            install_btn.deactivate();
        }
        InstallButtonState::Update => {
            install_btn.set_label("Update");
            // Green accent for update
            install_btn.set_label_color(if theme.is_dark() {
                Color::from_rgb(100, 200, 100)
            } else {
                Color::from_rgb(40, 140, 40)
            });
        }
        InstallButtonState::Install => install_btn.set_label("Install"),
    }
    install_btn
}

/// Official tab: Install/Update button wired to the signed registry install.
fn add_official_install_button(
    info: &AvailablePluginInfo,
    state: InstallButtonState,
    btn_x: i32,
    ctx: &PluginRowContext,
) {
    let mut install_btn = create_install_button(&state, btn_x, &ctx.theme);

    // Store button reference for cross-tab sync (uninstall -> re-enable Install button)
    ctx.available_buttons
        .borrow_mut()
        .insert(info.name.clone(), install_btn.clone());

    let info = info.clone();
    let ctx = ctx.clone();
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
                        ctx.record_install(&info.name);

                        // Cross-tab sync: hide old row if this is an update
                        // Try multiple key formats: registry name uses "python-lint",
                        // but installed plugins use display name "Python Lint"
                        let display_name = plugin_display_name(&info.name);

                        // Try display name first, then registry name
                        // Must split borrows to avoid RefCell double-borrow panic
                        let old_row = {
                            let mut rows = ctx.installed_rows.borrow_mut();
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
                        ctx.add_installed_row(InstalledPlugin {
                            name: &info.name,
                            version: &info.version,
                            description: &info.description,
                            enabled: true,
                            tier: PluginTier::Official,
                        });
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
}

/// Community tab: Install button wired to the review-then-install flow.
fn add_community_install_button(
    info: &CommunityPluginInfo,
    state: InstallButtonState,
    btn_x: i32,
    ctx: &PluginRowContext,
) {
    let mut install_btn = create_install_button(&state, btn_x, &ctx.theme);

    // Store button reference for cross-tab sync
    ctx.community_buttons
        .borrow_mut()
        .insert(info.name.clone(), install_btn.clone());

    let info = info.clone();
    let ctx = ctx.clone();
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

        if !show_community_install_dialog(&review, &ctx.theme) {
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
                ctx.record_install(&info.name);

                // Add row to Installed tab
                let desc = metadata
                    .as_ref()
                    .map(|m| m.description.clone())
                    .unwrap_or_default();
                ctx.add_installed_row(InstalledPlugin {
                    name: &info.name,
                    version: &info.version,
                    description: &desc,
                    enabled: true,
                    tier: PluginTier::Community,
                });
            }
            Err(e) => {
                btn.set_label("Error");
                btn.activate();
                fltk::dialog::alert_default(&format!("Failed to install {}:\n{}", info.name, e));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_text_layout_is_centred_and_non_overlapping() {
        let layout = row_text_layout(PLUGIN_ROW_HEIGHT);
        // The Installed tab's original offsets, now shared by all three tabs.
        assert_eq!(
            layout,
            RowTextLayout {
                title_y: 10,
                desc_y: 28,
                meta_y: 46,
            }
        );
        assert!(layout.desc_y >= layout.title_y + TITLE_LINE_H);
        assert!(layout.meta_y >= layout.desc_y + DESC_LINE_H);
        let bottom_margin = PLUGIN_ROW_HEIGHT - (layout.meta_y + META_LINE_H);
        assert_eq!(layout.title_y, bottom_margin);
    }

    #[test]
    fn trust_badge_follows_tier() {
        assert_eq!(
            TrustBadge::for_tier(&PluginTier::Official),
            TrustBadge::Verified
        );
        assert_eq!(
            TrustBadge::for_tier(&PluginTier::Community),
            TrustBadge::Community
        );
        assert_eq!(
            TrustBadge::for_tier(&PluginTier::Manual),
            TrustBadge::Unverified
        );
    }

    #[test]
    fn installed_meta_text_shows_status_and_badge() {
        assert_eq!(
            installed_meta_text(true, TrustBadge::Verified),
            "Enabled | \u{2713} Verified"
        );
        assert_eq!(
            installed_meta_text(false, TrustBadge::Unverified),
            "Disabled | \u{26A0} Unverified"
        );
    }

    #[test]
    fn official_meta_text_with_and_without_author() {
        let tags = vec!["lint".to_string(), "python".to_string()];
        assert_eq!(
            official_meta_text("", &tags, TrustBadge::Verified),
            "lint, python \u{00B7} \u{2713} Verified"
        );
        assert_eq!(
            official_meta_text("ann", &tags, TrustBadge::Unverified),
            "by ann \u{00B7} lint, python \u{00B7} \u{26A0} Unverified"
        );
    }

    #[test]
    fn community_meta_text_with_and_without_author() {
        assert_eq!(community_meta_text(""), "\u{2637} Community");
        assert_eq!(
            community_meta_text("bob"),
            "\u{2637} Community \u{00B7} by bob"
        );
    }

    #[test]
    fn install_button_state_prefers_update_over_installed() {
        assert_eq!(
            install_button_state(false, false),
            InstallButtonState::Install
        );
        assert_eq!(
            install_button_state(true, false),
            InstallButtonState::Installed
        );
        assert_eq!(install_button_state(true, true), InstallButtonState::Update);
    }

    #[test]
    fn installed_header_keeps_name_version_description_apart() {
        let kind = RowKind::Installed(InstalledPlugin {
            name: "Python Lint",
            version: "1.2.0",
            description: "Lints Python",
            enabled: false,
            tier: PluginTier::Community,
        });
        let header = kind.header();
        assert_eq!(header.name, "Python Lint");
        assert_eq!(header.version, "1.2.0");
        assert_eq!(header.description, "Lints Python");
        assert_eq!(header.meta_text, "Disabled | \u{2637} Community");
        assert_eq!(header.badge, TrustBadge::Community);
    }

    #[test]
    fn normalize_plugin_name_matches_registry_form() {
        assert_eq!(normalize_plugin_name("Python Lint"), "python-lint");
        assert_eq!(normalize_plugin_name("python-lint"), "python-lint");
    }
}
