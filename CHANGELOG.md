# Changelog

All notable changes to FerrisPad will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.4-rc.1] - 2026-03-28

### Added
- **Lazy-Loading Tree View**: On-demand expansion replaces full recursive scan, improving startup for large directories.
- **Relative File Path in Status Bar**: Status bar now shows the file path relative to the workspace root.
- **Structured CLI Argument Parsing**: `--help`, `--version`, `--line`, and multi-file arguments with proper parsing.
- **Disambiguated Tab Names**: Same-named files show parent directory to distinguish them; workspace root detection fixed.
- **File Reload Shortcuts & Focus Detection**: Reload files on focus return and via keyboard shortcuts when external changes are detected.
- **Deleted File Handling**: Gracefully handles deleted files and refreshes tree on focus.

### Fixed
- **Dialog Theming**: Applied `DialogTheme` to Find, Replace, Go To Line, and shortcut dialogs.
- **Selection Colors**: Plugin config and settings dialogs now use theme-aware selection colors.
- **Tab Group Collapse**: Auto-expand collapsed group when switching to a tab inside it; removed 2px gap below collapsed group chip.
- **Plugin Lint Results**: Propagate `had_lint_results` in the selected-plugins code path.
- **`use_spaces` Setting**: Added missing `use_spaces` editor setting.
- **Registry Rate Limits**: Plugin registry now caches responses and handles HTTP 429 gracefully.

### Security
- **Hardened Plugin Sandbox**: Restricted `terminal_view` and `_G` access in Lua plugins.
- **Preview URL Whitelist**: Added URL scheme whitelist and Content Security Policy to prevent protocol-handler RCE.

## [0.9.3] - 2026-03-23

### Added
- **Embedded Terminal Panel**: PTY-backed terminal with VTE parsing, SGR colors, reverse video support, and automatic window resize when panel opens.
- **MCP Server**: AI tool integration via stdio transport (`--mcp-server` flag) with `refresh_tree` tool for file explorer updates.
- **`setup_mcp_config` Plugin API**: Plugins can write `.mcp.json` project configs; auto-appends to `.gitignore`.
- **Community Plugins**: Three-tier trust model (Official, Community, Manual) with git tag pinning and mandatory SHA-256 checksums.
- **Community Plugin Install Dialog**: Checksum-verified installation flow for community plugins.
- **Plugin Manager Search**: Search bars in all three tabs (Installed, Official, Community).
- **Status Bar**: Displays cursor position and file information.
- **Editor Context Service**: Provides structured editor state for MCP tools and plugins.
- **`git_status` API Enhancement**: Plugin API now includes gitignored files.

### Fixed
- **Plugin Manager**: Install-then-uninstall in same session now works correctly.
- **Plugin Permissions**: Permissions checked immediately after installing from plugin manager.
- **macOS**: Plugin manager tabs no longer show empty.
- **Windows**: Centered plus sign in new tab button; removed black crab emoji from title bar; rounded window icon.
- **Windows Titlebar**: Guard `set_windows_titlebar_theme` against pre-show null handle.
- **Theme-Aware Dropdowns**: Syntax theme dropdowns use theme-aware selection color.
- **Window Icon**: Use pre-rendered 32x32 icon instead of decompressing 1024x1024 source.
- **Search Input Cursor**: Preserve cursor position in search inputs using `super_draw` for placeholder.
- **Terminal Resize**: Terminal panel resizes correctly on window resize.

### Changed
- Centralized divider width and color in `theme.rs`.
- Applied rustfmt formatting across codebase.
- Fixed clippy warnings.

## [0.9.2] - 2026-03-16

### Added
- **OnTextChanged Hook**: New plugin hook with 300ms debounce for reacting to text edits.
- **SplitPane Read-Only Mode**: Split view panels can now be set as read-only.
- **Integration Test Suite**: 8 new integration test suites (diff/highlights, Lua sandbox, plugin loading/security/verify chains, session roundtrip, settings persistence, shortcut registry).
- **Library Crate**: Exposed `ferris_pad` lib crate for integration testing.

### Fixed
- **Plugin Menu Entries on Global Disable**: Menu entries now properly disappear when plugins are globally disabled.
- **Format Menu Visual Update**: Font and size changes from the Format menu now visually update highlighted text immediately.

### Changed
- **Controller Extraction**: Split `AppState` from 2,799 → 1,053 lines (~62% reduction) by extracting FileController, HighlightController, WidgetController, PluginController, SessionController, and ViewController.
- **Dispatch Refactor**: Extracted grouped handler functions from `main.rs` into `dispatch.rs`, eliminating ~1,195 net lines of dead/duplicated code.
- **Lazy Syntect Loading**: Deferred syntax set initialization reduces idle RSS by ~10 MB (39 → 29 MB).
- **Plugin API Modularization**: Split `plugins/api.rs` into sub-modules (commands, editor, filesystem, sandbox).
- **Security Audit**: Audited plugin API against PHILOSOPHY.md and SECURITY.md; added SAFETY comments to all unsafe blocks.
- **Updated SECURITY.md**: Reflects v0.9.2 status — permission system, signed plugin downloads, and plugin manager all marked as done.
- Fixed all clippy warnings across the codebase.

## [0.9.1] - 2026-03-05

### Added
- **Lua Plugin System**: Full Lua 5.4 scripting with statically linked mlua, 8 synchronous hooks (`on_document_open`, `on_document_save`, `on_document_close`, `on_document_lint`, `on_highlight_request`, `on_widget_action`, `init`, `shutdown`).
- **Plugin Security Sandbox**: Path validation against project root, command approval flow, instruction counter limits, Lua garbage collection monitoring, and timeout enforcement.
- **Plugin Signature Verification**: ed25519 cryptographic signing and verification for plugins via `plugin-signer` tool.
- **Plugin Manager Dialog**: Browse, install, enable/disable, and configure plugins with cross-tab sync between Installed and Available tabs.
- **Plugin Configuration System**: Per-plugin settings with `string`, `number`, `boolean`, and `choice` types, validation rules, and live config dialogs.
- **Plugin Permission Approval Dialog**: Users explicitly approve plugin permissions (e.g., `run_command`) before granting access.
- **Plugin Auto-Update Checking**: Background version checks against remote plugin registry.
- **Plugin Menu Items**: Plugins can register menu actions with keyboard shortcuts.
- **Widget API for Plugins**: Split view (diff/suggestions with intraline highlighting) and tree view (file browser, YAML viewer) widgets created from Lua.
- **Plugin-Defined Context Menus**: Plugins can add context menu items to tree panel nodes.
- **Tree Panel**: File explorer with drag-and-drop file moves, search filtering, click-to-line, git status indicators (modified/added/conflict colors), and type indicators for YAML/JSON ({N} and [N] counts).
- **Split Panel**: Two-pane diff view with syntax highlighting, font matching, and resizable divider.
- **Diagnostic Panel**: Hover tooltips with fix suggestions, double-click to open docs URL, toast notifications for diagnostic events.
- **Plugin API**: `run_command`, `open_file`, `goto_line`, clipboard access, venv detection, cross-platform filesystem API, inline highlighting, custom RGB colors, line annotations, and gutter marks.
- **Key Shortcuts Dialog**: Centralized keyboard shortcut viewer replacing per-plugin shortcut fields.
- **Large File Handling**: Size validation to prevent crashes, tail mode for files >1.8GB, read-only viewer, progress dialog for 100MB+ files, memory-optimized loading.
- **Toast Notifications**: Non-blocking notifications for plugin events and diagnostics.
- **TOML Syntax Highlighting**: Added TOML to the supported syntax highlighting languages.
- **Flat Themed Scrollbars**: Applied to main editor and all dialogs.
- **Syntax Highlighting for Split View**: Diffs display with proper syntax coloring and font matching.
- **Independent Syntax Theme Selection**: Choose syntax theme independently with live preview.
- **Configurable Tab Size**: User-configurable tab width setting.
- **External Browser Markdown Preview**: Stable file naming for browser preview.
- **Persistent Tree View Flag**: File explorer stays open across tab switches.
- **Tree View Auto-Reopen**: Tree view reopens on tab switch with caching and deferred hooks.
- **Configurable Large File Thresholds**: Adjust large file size limits in Settings dialog.
- **"All Checks Passed" Feedback**: Manual lint triggers now show success confirmation.

### Fixed
- **Session Merge Resurrects Closed Tabs**: Tagging session.json with process ID prevents closed tabs from reappearing on next launch.
- **Clipboard Copy Path on Wayland**: Fixed Copy Path not working on Wayland sessions.
- **Plugin Shortcuts After Session Restore**: Shortcuts now work correctly after restoring a session.
- **Wayland Popup Menu Positioning**: Fixed RefCell panic in tab bar and proper popup anchoring.
- **Linting Fired for Any File Type**: Now correctly scopes linting to relevant file types.
- **Tab Bar Layout on Tree Panel Toggle**: Recalculates layout when tree panel shows/hides.
- **Tree Panel Themes**: Correct text color, flat selection style, and theme-derived selection color for dark/light modes.
- **Tree Panel Layout**: Fixed gaps, scrollbar style, header height, and nested folder duplication.
- **Tab Drag Between Groups**: Correctly joins target group when dragging tabs.
- **Tab Bar Overflow**: Added scroll arrows for overflow handling.
- **Temp File Duplicate Bug**: Fixed duplicate temp file creation.
- **Dark Mode Text Color**: Improved UI theming consistency.
- **Diagnostic Tooltip Updates**: Tooltip refreshes after clicking diagnostic item.
- **Dirty Flag on Tab Switch**: Fixed incorrect dirty flag state.
- **Plugin Manager Cross-Tab Sync**: Correct registry name format on uninstall, no ghost rows on update.
- **Success Diagnostic Bar Auto-Dismiss**: Diagnostic bar auto-dismisses and coexists with tree view.
- **Themed Scrollbar Corner Square**: Scrollbar corner now uses theme colors instead of FLTK default gray.

### Changed
- **Clean Architecture Reorganization**: Moved `src/app/` into domain/controllers/services/infrastructure/plugins layers.
- **Dialog Redesign**: Theme-derived colors with VSCode-style layout across all dialogs.
- **Flat Menu Bar**: Dynamic syntax theme colors applied to menu bar.
- **Plugin Manager UX**: Flat buttons, toggle switches, details button, per-action shortcuts.
- **Replaced reqwest with minreq**: Lighter HTTP client with rustls (no OpenSSL dependency).
- System font size used instead of hardcoded 14px.
- Disabled dotted visible focus indicator globally.

## [0.9.0] - 2026-02-19

### Added
- **Markdown Preview**: Live side-by-side preview for `.md` files with async image resizing. Toggle via View menu or settings.
- **Tab Groups**: Colored tab groups (Red, Orange, Yellow, Green, Blue, Purple, Grey) with group labels, collapse/expand into compact chips, right-click context menu for group management (create, rename, recolor, ungroup, close).
- **Drag-to-Group**: Drag a tab onto the center of another tab to group them. Drag to the edges to reorder. Visual feedback: 50% blended highlight for grouping, vertical insertion line for reordering.
- **Draggable Collapsed Groups**: Collapsed group chips can be dragged to reorder entire groups. Click-without-drag still toggles expand/collapse.
- **Group Reorder Protection**: Ungrouped tabs cannot be inserted between tabs of the same group; insertion point snaps to group boundary.
- **Crash-Safe Session Auto-Save**: Session auto-saves every 30 seconds so tabs survive task manager kills, crashes, and power loss.
- **"+" New Tab Button**: Quick new tab creation button in the tab bar.
- **Syntax Highlighting**: 50+ languages via syntect with oniguruma regex backend. Chunked non-blocking processing for large files with sparse checkpoints every 128 lines.
- **Tabbed Editing**: Custom-drawn tab bar with rounded top corners, shrink-to-fit sizing, per-tab close buttons, hover highlighting, middle-click close, and dark/light theme support.
- **Session Restore**: Three modes (Off, Saved Files Only, Full including unsaved content) with multi-instance session merging and schema versioning.
- **Select All** (Ctrl+A), **Find Previous** (Ctrl+Shift+G), **Go To Line** (Ctrl+G).
- **Dynamic Line Number Gutter**: Auto-sizes based on document line count.
- **macOS Dark Mode Detection** via `AppleInterfaceStyle` defaults key.
- **Windows Executable Metadata**: Version, description, and copyright via embedded resource manifest.

### Fixed
- **FLTK Widget Lifecycle Leaks**: Fixed modify callback, tile widget, and shared image cache leaks via direct FFI.
- **glibc Memory Not Returned**: Added targeted `malloc_trim(0)` calls after tab close and preview hide.
- **TextBuffer::text() Memory Leak**: Plugged 3.8MB/call leak via `buffer_text_no_leak()` helper.
- **Find No Longer Marks Document as Dirty**: Dirty flag only sets on actual text changes, not selection changes.
- **Memory Leaks in Syntax Highlighting**: Reduced highlighting memory from 290MB to 63MB on large files.
- **Native File Dialogs Restored**: Fixed regression that broke native file chooser on Linux.
- **Allow Closing App While Dialog is Open**: `run_dialog` checks `app::should_program_quit()`.

### Changed
- **Major Architecture Refactoring**: Decomposed monolithic `main.rs` into `src/app/` (business logic) and `src/ui/` (presentation) with message-passing event-driven architecture. ~5,200 lines across 28 files.
- Extracted `HighlightController`, `UpdateController`, `PreviewController`, `EditorContainer`, and `TabManager` from AppState.
- Added `thiserror`-based `AppError` enum for consistent error handling.
- Configured jemalloc dirty/muzzy decay to 0ms at startup for immediate page return.
- Updated README and website to reflect v0.9.0 feature set.

## [0.1.8] - 2026-02-12

### Added
- **Direct In-App Updates**: Download and install updates directly without visiting GitHub, with automatic backup and restart.
- **Edit Menu Enhancements**: Added Undo, Redo, Cut, Copy, and Paste with standard keyboard shortcuts.
- **Pre-release Channel Toggle**: Opt-in to beta/RC updates directly from the Settings dialog.
- **Native Wayland Support**: Enabled FLTK's Wayland backend for correct keyboard layout handling on Linux Wayland sessions.
- **Linux "Open With" Support**: Added `%f` to desktop entry so FerrisPad appears in the file manager's "Open With" dialog.
- **macOS File Association**: Associated FerrisPad with text files (.txt, .md, .rs, etc.) in Finder.
- **Website Enhancements**: "Feeling brave?" button for unstable releases with dual-track versioning.

### Fixed
- **Keyboard Layout on Wayland**: Resolved incorrect shortcut mapping when OS keyboard layout differed from hardware keyboard (e.g. AZERTY hardware with QWERTY configured).
- **Update Download Freeze**: Fixed silent error handling and thread-safety issues that caused the updater to freeze.
- **Update Banner Visibility**: Improved readability of the update notification in Dark Mode.
- **Dual Icon Bug**: Resolved issue where two icons appeared in the title bar on macOS and Windows.
- **Edit Menu Layout**: Removed extra vertical space and cleaned up item alignment.
- **Version Bump Script**: Hardened script to handle stable and unstable tracks independently.

### Technical
- Implemented robust self-replacement update strategy with automatic backup and restart.
- Switched to `app::awake_callback()` for thread-safe FLTK UI updates from background threads.
- Enhanced `Info.plist` generation in the macOS build process for better system integration.
- Improved CLI argument parser to reliably handle "Open With" file paths.

## [0.1.7] - 2026-02-09

### Added
- **Command-Line Argument Support**: Open files directly from the terminal or via OS "Open With"
  - Pass a file path as the first argument to FerrisPad to load it on startup
  - Automatic loading of files when double-clicked in file managers (macOS/Linux/Windows)
- **Robust Path Handling**: Improved safety when handling file paths
  - Safe conversion of non-UTF-8 paths to prevent application panics
  - Uses `to_string_lossy()` for handling paths from various character sets

### Technical
- Refactored file opening logic into a shared `open_file` helper function
- Modularized state management for easier extension of startup parameters

## [0.1.6] - 2025-10-03

### Added
- **Windows Dark Mode Detection**: Automatic system theme detection on Windows
  - Reads Windows Registry `AppsUseLightTheme` value
  - Respects Windows system dark/light mode preference
  - Content (editor, menus) automatically themes on startup
- **Windows Title Bar Theming**: Dark/light title bar support
  - Uses Windows DWM (Desktop Window Manager) API
  - Title bar matches application theme
  - Supports Windows 10 build 1809+ and Windows 11
  - Dual attribute ID support (19 and 20) for cross-version compatibility
  - Updates dynamically when theme changes via menu or settings

### Changed
- **Default Font**: Changed from Screen Bold to Courier for better readability
- Improved bump-version script to handle pre-release versions (e.g., 0.1.5-rc.1)
- Enhanced Windows security documentation with Defender exclusion instructions

### Fixed
- Version bump script now correctly updates download URLs for RC versions
- Windows Registry dark mode detection properly isolated to Windows platform
- Windows Defender quarantine workaround documented in README and website

### Technical
- Added platform-specific Windows dependencies:
  - `winreg` 0.52 (Windows Registry access)
  - `windows` 0.58 with `Win32_Foundation` and `Win32_Graphics_Dwm` features
- Implemented `set_windows_titlebar_theme()` function (Windows only)
  - Called after `window.show()` to ensure valid HWND
  - Uses `DwmSetWindowAttribute()` with both attribute IDs (19 and 20)
  - Graceful degradation on unsupported Windows versions
- Platform-conditional compilation with `#[cfg(target_os = "windows")]`
- Updated `detect_system_dark_mode()` with platform-specific blocks
- Cross-platform verification (builds on Windows and Linux targets)
- Comprehensive debugging documentation (32KB journey document)

### Platform Support
- Windows 11: Full support (dark mode + title bar)
- Windows 10 2004+: Full support (dark mode + title bar)
- Windows 10 1809-1903: Full support (dark mode + title bar via attribute 19)
- Windows 10 <1809: Dark mode detection only (no title bar theming)
- Linux: Unchanged (GNOME/GTK dark mode detection)
- macOS: Unchanged (defaults to light mode, detection planned for future)

## [0.1.5] - 2025-10-02

### Added
- **In-App Update Checker**: Privacy-first update notification system
  - Manual update check via Help → Check for Updates menu
  - Auto-check on startup (once per 24 hours, fully optional)
  - Notification banner when updates are available
  - Settings toggle to enable/disable auto-updates
  - GitHub API integration for release checking
  - Semantic version comparison
  - Background thread for non-blocking checks
  - No telemetry or tracking - only checks GitHub public API
- **About Dialog**: Standard application information
  - Help → About FerrisPad menu item
  - Displays version, copyright, license information
  - Project website and GitHub links
- **Privacy-First Design**: Complete transparency and user control
  - Update checker can be completely disabled
  - No personal data collection
  - No usage tracking or analytics
  - Open source and auditable code

### Changed
- Extended Settings dialog with Updates section
- Enhanced Help menu with About and Check for Updates items
- Updated website with privacy section and new features
- Improved README with privacy commitments and update checker documentation

### Fixed
- Settings dialog close button no longer closes entire application
- Proper event handling for modal dialogs

### Technical
- Added `src/updater.rs` module (267 lines)
  - GitHub API integration
  - Semantic versioning with semver crate
  - Background update checking
  - Thread-safe state management with Arc<Mutex<T>>
  - Comprehensive test coverage (12 unit tests)
- Extended `src/settings.rs` with update preferences
  - auto_check_updates field
  - update_channel (Stable/Beta)
  - last_update_check timestamp
  - skipped_versions list
  - Backward compatibility with old config files
- Enhanced `src/main.rs` (+288 lines)
  - Background update check on startup
  - Update notification banner
  - About dialog implementation
  - Manual check dialog
- Added dependencies:
  - reqwest (with rustls-tls for better portability)
  - semver (for version comparison)
  - open (for opening URLs in browser)
- Total test count: 18 unit tests (all passing)
- Comprehensive documentation (15 files, ~170 KB in docs/temp/0.1.5/)

## [0.1.4] - 2025-10-02

### Added
- **Find & Replace**: Full text search and replace functionality
  - Find dialog (Ctrl+F) with case-sensitive option
  - Find & Replace dialog (Ctrl+H) with Replace and Replace All
  - Automatic text highlighting and scroll to match
  - Wrap-around search when reaching end of document
- **Settings Persistence**: Save user preferences across sessions
  - Settings dialog accessible from File menu
  - Configure theme (Light/Dark/System Default)
  - Choose font (Screen Bold/Courier/Helvetica Mono)
  - Set font size (Small 12/Medium 16/Large 20)
  - Toggle line numbers and word wrap
  - Settings saved to JSON at cross-platform config location
  - Settings sync with View menu checkboxes
- **Multi-Format File Support**: Save files in various formats
  - Support for .txt, .md, .rs, .py, .json, .xml, .yaml, .toml, .ini, .cfg, .log
  - Clean dropdown menu with format categories
  - "All Files" option for any extension
- **Smart Save Functionality**:
  - Quick Save (Ctrl+S) - saves to existing file without dialog
  - Save As (Ctrl+Shift+S) - opens dialog for new name/location
  - Shows confirmation dialog for unsaved changes on quit

### Fixed
- File dialogs now properly display .txt files on all platforms
- File dialog filters use FLTK-compatible format for cross-platform consistency

### Changed
- Improved file dialog UI with multi-line format dropdown
- Refactored save logic to eliminate code duplication
- Enhanced keyboard shortcuts for better workflow

### Technical
- Added `src/settings.rs` module for settings management
- Added serde and serde_json dependencies for JSON serialization
- Added dirs dependency for cross-platform config paths
- Comprehensive test coverage (22 tests)
- All features follow TDD (Test-Driven Development) principles

## [0.1.3] - 2025-10-01

### Added
- Professional icons for all platforms (Windows, macOS, Linux)
- Windows executable with embedded icon
- Desktop integration improvements

### Changed
- Updated to Rust edition 2024
- Improved build and release workflows

### Fixed
- macOS universal binary build
- Download URLs for release artifacts
- GitHub Actions workflow permissions

## [0.1.2] - 2025-10-01

### Fixed
- GitHub Actions workflow to support tags without 'v' prefix

## [0.1.1] - 2025-10-01

### Added
- MIT License
- GitHub Releases integration
- Build guides and documentation

### Fixed
- Binary file distribution issues
- Download links to use GitHub releases

## [0.1.0] - 2025-09-30

### Added
- Initial release
- Basic text editing functionality
- Native file dialogs for Open and Save
- Clean, minimal interface
- Cross-platform support (Linux, macOS, Windows)
- FLTK-based GUI
- Rust implementation for speed and safety

[0.9.2]: https://github.com/fedro86/ferrispad/compare/0.9.1...0.9.2
[0.9.1]: https://github.com/fedro86/ferrispad/compare/0.9.0...0.9.1
[0.9.0]: https://github.com/fedro86/ferrispad/compare/0.1.8...0.9.0
[0.1.8]: https://github.com/fedro86/ferrispad/compare/0.1.7...0.1.8
[0.1.7]: https://github.com/fedro86/ferrispad/compare/0.1.6...0.1.7
[0.1.6]: https://github.com/fedro86/ferrispad/compare/0.1.5...0.1.6
[0.1.5]: https://github.com/fedro86/ferrispad/compare/0.1.4...0.1.5
[0.1.4]: https://github.com/fedro86/ferrispad/compare/0.1.3...0.1.4
[0.1.3]: https://github.com/fedro86/ferrispad/compare/0.1.2...0.1.3
[0.1.2]: https://github.com/fedro86/ferrispad/compare/0.1.1...0.1.2
[0.1.1]: https://github.com/fedro86/ferrispad/compare/0.1.0...0.1.1
[0.1.0]: https://github.com/fedro86/ferrispad/releases/tag/0.1.0
