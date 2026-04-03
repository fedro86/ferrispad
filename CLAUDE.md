# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Git Commands

use gh to do whatever and be sure to be logged as fedro86 account

## Wiki Editing

Both repos have GitHub wikis (git-based, no API):

```bash
# Clone, edit, push — that's it. Always use /tmp as working directory.
git clone https://github.com/fedro86/ferrispad.wiki.git /tmp/ferrispad-wiki
git clone https://github.com/fedro86/ferrispad-plugins.wiki.git /tmp/ferrispad-plugins-wiki

# After editing .md files:
cd /tmp/ferrispad-wiki && git add -A && git commit -m "message" && git push origin master

# /tmp is ephemeral — always reclone before editing, never assume a previous clone exists.
```

Wiki pages are flat `.md` files (no subdirectories). Page name = filename without `.md`. Links use `[[Page Name]]` syntax.

**DRY principle**: A topic can appear in both wikis, but one must be the single source of truth and the other must only reference it with a link. Never duplicate content that would need to be updated in two places.

## Build Commands

```bash
# Development build
cargo build

# Release build
cargo build --release

# Run
./target/release/FerrisPad

# Run tests
cargo test

# Lint
cargo clippy --all-targets --all-features

# Format check
cargo fmt --check

# Create .deb package (Linux)
cargo deb
```

**Linux build dependencies:**
```bash
sudo apt-get install libfltk1.3-dev libfontconfig1-dev libxext-dev libxft-dev \
  libxinerama-dev libxcursor-dev libxrender-dev libxfixes-dev libpango1.0-dev \
  libgl1-mesa-dev libglu1-mesa-dev
```

## Architecture

FerrisPad is a Rust text editor using FLTK for GUI. The architecture follows a **message-passing event-driven pattern** with Clean Architecture layers.

### Core Flow

UI callbacks → `Sender<Message>` channel → `main.rs` event loop → `dispatch.rs` grouped handlers → `AppState` → Controllers

All application interactions are defined as `Message` enum variants (~100 variants) in `src/app/domain/messages.rs`. The main loop dispatches these through grouped handler functions in `dispatch.rs`, which call into `AppState` and its controllers.

### Layer Structure

```
src/
├── main.rs              Entry point, FLTK event loop
├── dispatch.rs          Grouped handler functions (file, tab, edit, view, ...)
├── lib.rs               Library crate (ferris_pad)

src/app/
├── domain/              Core data (Document, Message enum, AppSettings)
├── controllers/         11 controllers (file, highlight, tabs, widget, plugin,
│                          session, view, preview, update, hook_dispatch)
├── services/            Business logic (syntax/, terminal/, session, updater, shortcuts, text_ops)
├── infrastructure/      FFI helpers (buffer leak fix), errors, platform detection
├── plugins/             Lua plugin system (api, hooks, loader, runtime, security, widgets/)
├── mcp/                 MCP server (JSON-RPC over TCP, stdio bridge for Claude Code)
└── state.rs             Central coordinator (mediates controllers, ~1,230 lines)

src/ui/
├── main_window.rs       Widget layout
├── editor_container.rs  Editor widget wrapper
├── tab_bar.rs           Custom tab bar with drag/drop (~2,010 lines)
├── terminal_panel.rs    Embedded terminal widget
├── diagnostic_panel.rs  Lint/diagnostic display panel
├── split_panel.rs       Plugin split view widget
├── tree_panel.rs        Plugin tree view widget
├── status_bar.rs        Cursor position, file path, language
├── menu.rs              Menu bar with shortcut registry
├── file_dialogs.rs      Native file picker helpers
├── toast.rs             Toast notifications
├── dialogs/             Find, settings, goto line, update, about, plugin manager, shortcuts
└── theme.rs             Dark/light themes, platform titlebar (Windows/macOS)
```

### Key Files

- `src/main.rs` - Entry point, FLTK event loop (~700 lines)
- `src/dispatch.rs` - Grouped handler functions (~1,130 lines)
- `src/app/state.rs` - Central `AppState` coordinator (~1,230 lines)
- `src/app/domain/messages.rs` - Message enum defining all events (~100 variants)
- `src/app/controllers/file.rs` - File I/O, returns `Vec<FileAction>` for cross-cutting effects
- `src/ui/tab_bar.rs` - Custom widget with drag/drop, groups, diff mode (~2,010 lines)

### Important Patterns

**Memory Management:**
- Jemalloc with 0ms page decay (non-Windows)
- `buffer_text_no_leak()` in `src/app/infrastructure/buffer.rs` prevents FLTK memory leak

**Syntax Highlighting:**
- Three-tier system: Full/Incremental/Chunked (2000-line chunks)
- Sparse checkpoints every 128 lines in `src/app/services/syntax/checkpoint.rs`
- Non-blocking via FLTK timeouts

**Session Persistence:**
- Auto-saves every 30 seconds
- Three modes: Off, Saved Files Only, Full (including unsaved)

**Dialog Theming:**
- All dialogs use `DialogTheme::from_theme_bg(theme_bg)` in `src/ui/dialogs/mod.rs`
- Derives bg, text, text_dim, input_bg, button_bg, scroll colors from the syntax theme
- `error_color()` provides adaptive error text (light red on dark, dark red on light)
- `apply_titlebar()` sets platform titlebar + FerrisPad icon (call after `dialog.show()`)
- Never use FLTK's `dialog::message_default`/`alert_default` — use themed custom dialogs instead

**Plugin System:**
- Lua 5.4 (statically linked via mlua)
- Plugins in `~/.config/ferrispad/plugins/`
- Hooks: `on_document_open`, `on_document_save`, `on_document_close`, `on_document_lint`, `on_highlight_request`, `on_text_changed`, and more (11 types total)
- Three-tier trust model: Official (signed), Community (checksums), Manual (unverified)
- Community plugins listed in `community-plugins.json`, pinned to git tags with mandatory SHA-256 checksums
- Plugin Manager with search bars in all three tabs (Installed, Official, Community)

## Philosophy & Security

Core principles from [PHILOSOPHY.md](PHILOSOPHY.md):
- **0% CPU when idle** - purely event-driven, no polling
- **Single binary** - no external dependencies at runtime, minimal attack surface
- **Privacy & Security** - no telemetry, memory-safe Rust, treat all input as untrusted
- **Passive features** - user-initiated, not proactive
- **Sandboxed plugins** - no arbitrary file/command access (roadmap)

See [SECURITY.md](SECURITY.md) for security policy and roadmap.

## Cross-Platform

- **Linux:** GTK integration, dark mode via GNOME/GTK
- **macOS:** Universal binary (Intel + Apple Silicon)
- **Windows:** MSVC, DWM titlebar theming, Registry dark mode detection

## Workflow Rules

- **Feature completion**: Only the user determines when a feature is complete (after manual testing). Do not mark tasks as "completed" until the user confirms.
- **Philosophy compliance**: At the end of every plan, verify how well the proposed changes comply with [PHILOSOPHY.md](PHILOSOPHY.md). Flag any violations (e.g., polling instead of event-driven, adding runtime dependencies, telemetry, proactive features) and adjust the plan before presenting it for approval.
- **Zero warnings**: Whenever `cargo build` or `cargo clippy` produces warnings, always fix them — even if they were not generated by changes in the current session. The codebase must stay warning-free at all times.

## Lessons Learned

Reference documentation for patterns and gotchas discovered during development:

- [FLTK Widget Lifecycle](docs/temp/lesson_learned/fltk-widget-lifecycle.md) - Widget creation, parent-child ownership, cloning behavior, memory leak scenarios
- [Memory Optimization](docs/temp/lesson_learned/memory-optimization.md) - Syntax highlighting memory fixes: sparse checkpoints, cached text for chunks, fltk-rs TextBuffer::text() leak workaround
- [Wayland Popup Menus](docs/temp/lesson_learned/wayland-popup-menus.md) - Pre-create MenuButton in widget constructor for proper parent surface; use 1x1 minimum anchor size; drop RefCell borrows before popup()
- [FLTK set_tab_distance Dirty Flag](docs/temp/lesson_learned/fltk-set-tab-distance-dirty-flag.md) - set_tab_distance() fires modify callback; must save/restore dirty state around it and any other non-edit buffer operations

## Plugin Release Procedure

Plugins live in a **separate repository**: `/home/frc/code-folder/continuous_learning/ferrispad-plugins/`
Runtime install location: `~/.config/ferrispad/plugins/`

**Community plugins** live in their own repositories:
- **Claude Code plugin**: `/home/frc/code-folder/continuous_learning/ferrispad-claude-code-plugin/` ([GitHub](https://github.com/fedro86/ferrispad-claude-code-plugin))
- **JS Linter plugin**: `/home/frc/code-folder/continuous_learning/ferrispad-js-linter/` ([GitHub](https://github.com/fedro86/ferrispad-js-linter))

**Always edit the source repo first**, then copy to the install location.

### Key Files

| File | Location | Purpose |
|------|----------|---------|
| `plugins.json` | Repo root | Official plugin registry with versions, checksums, and signatures |
| `community-plugins.json` | Repo root | Community plugin registry with git refs and checksums |
| `CONTRIBUTING.md` | Repo root | Plugin development guidelines, API reference, widget docs |
| `tools/signer/` | Repo root | ed25519 signing tool ([README](../ferrispad-plugins/tools/signer/README.md)) |
| `{plugin}/init.lua` | Plugin dir | Main plugin logic |
| `{plugin}/plugin.toml` | Plugin dir | Metadata, permissions, config params |
| `{plugin}/CHANGELOG.md` | Plugin dir | Version history (Keep a Changelog format) |
| `{plugin}/README.md` | Plugin dir | User documentation |

### Release Steps

1. **Edit source files** in `ferrispad-plugins/{plugin-name}/`
   - Update `init.lua` with code changes
   - Update `plugin.toml` (bump version, add permissions/config if needed)

2. **Bump version in ALL locations** (every release, no exceptions):
   - `{plugin}/init.lua` — header comment AND `version` field in module table
   - `{plugin}/plugin.toml` — `version` field
   - `{plugin}/CHANGELOG.md` — new `[x.y.z]` section
   - `{plugin}/README.md` — version in the Version section
   - `plugins.json` (repo root) — `version` field for this plugin

3. **Update documentation**
   - `CHANGELOG.md` — Add new version section with Added/Changed/Fixed
   - `README.md` — Update features, config table if changed

4. **Copy to install location**
   ```bash
   cp -r ferrispad-plugins/{plugin-name}/ ~/.config/ferrispad/plugins/
   ```

5. **Update `plugins.json`**
   - Update `description` if changed
   - Do NOT manually update `checksums` or `signature` — CI handles this automatically on push

6. **Commit and push** the ferrispad-plugins repo
   - CI automatically computes checksums, signs the plugin, and updates `plugins.json`

### Community Plugin Release

Community plugins live in their own repositories and are listed in `community-plugins.json`.

1. **Tag the release** in the plugin's repo: `git tag v1.0.0 && git push origin v1.0.0`
2. **Update `community-plugins.json`** in ferrispad-plugins repo:
   - Set `git_ref` to the tag (e.g., `"v1.0.0"`)
   - Bump `version`
   - Do NOT manually set `checksums` — CI handles this automatically on push
3. **Commit and push** the ferrispad-plugins repo

Tags are immutable — checksums always match, no race condition between repo updates and registry updates.

### Notes

- If FerrisPad core changes are also needed (e.g., new widget support), commit and push the ferrispad repo separately
- Signing is automated in CI for both the plugins repo and core repo

## Roadmap

See [structured_todo.md](docs/temp/0.9.3/structured_todo.md) for the plugin system roadmap covering security hardening, API expansion, and plugin infrastructure. Check the checkboxes for current implementation status.
