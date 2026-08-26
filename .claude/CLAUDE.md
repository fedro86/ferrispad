# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

FerrisPad is a single-binary, FLTK-based text editor written in Rust (2024 edition). The crate builds both a library (`ferris_pad`, `src/lib.rs`) and a binary (`FerrisPad`, `src/main.rs`); integration tests and `dispatch.rs` consume the library crate.

## Work sequence — every change lands through a ticket

Implementation changes land **through a ticket** that moves
`docs/tickets/1-todo/ → 2-review/ → 3-done/`, and **no commit happens until the
user has verified the in-review ticket.** The binding rules load from
`.claude/rules/`:

- `work-sequence.md` — the ticket state machine, the red-test-first bugfix loop,
  the commit gate.
- `engineering-standards.md` — the design constraints (below) and the cargo
  quality gates.

Open new tickets with the `new-ticket` skill (`.claude/skills/new-ticket/`).
The ticket workspace (`docs/tickets/`) and this `.claude/` overlay are **tracked
in git**: FerrisPad is an AI-assisted project and this workflow is part of the
repo. Only personal overrides stay gitignored — `.claude/settings.local.json`, a
root `CLAUDE.md`, and `.mcp.json`. Each worked ticket also gets a GitHub issue
(see `.claude/rules/work-sequence.md` → *GitHub issue tracking*).

The gates (must be green before a commit; `cargo` is reached via the Nix dev
shell since it is not on the VSCode Flatpak-terminal PATH):

```bash
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

The pre-commit hook (`.claude/hooks/pre-commit-gates.sh`, wired in
`.claude/settings.json`) runs these automatically and blocks the commit on any
failure.

## Commands

```bash
cargo build --release          # Release build → target/release/FerrisPad
cargo run                      # Debug run
cargo test                     # All unit + integration tests
cargo test --test session_roundtrip          # One integration test file (see tests/)
cargo test <name>              # Tests matching a substring
cargo clippy --all-targets --all-features     # Lint — must be warning-free
cargo fmt --check              # Format check
```

Before submitting changes, `cargo test`, `cargo clippy`, and `cargo fmt --check` must all pass with **zero warnings**.

**Linux build deps:** `libfltk1.3-dev libfontconfig1-dev libxext-dev libxft-dev libxinerama-dev libxcursor-dev libxrender-dev libxfixes-dev libpango1.0-dev libgl1-mesa-dev libglu1-mesa-dev`. A `shell.nix` / `.envrc` is present for Nix users. Local distribution packaging for testing is driven by `./scripts/build-releases.sh`; see `docs/guides/BUILD_GUIDE.md`.

## Releases

Releases are **fully automated via GitHub Actions** (`.github/workflows/release.yml`) — pushing a version tag builds, signs, and publishes everything. Full details in `docs/guides/RELEASE_PROCESS.md`. The normal flow:

```bash
# 1. Edit CHANGELOG.md FIRST — add a new ## [X.Y.Z] section (CI extracts release notes from it)
# 2. Bump version everywhere + auto-commit (updates Cargo.toml, docs/, README, build script)
./scripts/bump-version.sh X.Y.Z        # add -y to skip the prompt
# 3. Tag, push, and sync the website
./scripts/release.sh
```

Tag/version conventions:
- **Tags have NO `v` prefix** (e.g. `0.9.5`, not `v0.9.5`) — matches the existing tagging convention.
- Tags containing `-alpha`, `-beta`, or `-rc` are auto-marked as GitHub pre-releases.
- Manual one-liner if not using `release.sh`: `VERSION="X.Y.Z" && git tag -a "$VERSION" -m "Release $VERSION" && git push origin "$VERSION"`.

What CI does on a pushed tag: builds native binaries (Linux `.deb`+raw, Windows `.zip`+`.exe`, macOS universal `.dmg`+raw), **signs each binary**, extracts release notes from `CHANGELOG.md`, and publishes the GitHub Release with all binaries + `.sig` files attached.

**Signing:** ed25519 signatures are produced **in CI, not locally**. The `sign-binaries` job checks out the `fedro86/ferrispad-plugins` repo, builds its `tools/signer` (`plugin-signer`), and signs using the base64-encoded `SIGNING_KEY` GitHub Actions secret. The auto-updater refuses unsigned binaries, so verify `.sig` files are attached after a release. Manual signing is only a fallback (see RELEASE_PROCESS.md). Platform identifiers must be exact: `linux-amd64`, `macos-universal`, `windows-x64.exe`.

## Architecture

FerrisPad is a **message-passing, event-driven** app with Clean-Architecture layering under `src/app/`. Understanding three things explains most of the codebase:

**1. The Message loop.** `main.rs` owns the single FLTK event loop. Every UI action (menu item, key binding, plugin callback, MCP request, background thread) sends a `Message` (≈100-variant enum in `src/app/domain/messages.rs`) over an `fltk::app::channel`. The loop in `main.rs` matches each variant to a grouped handler in `src/dispatch.rs` (`handle_file`, `handle_tab`, `handle_edit`, `handle_view`, `handle_plugin`, `handle_terminal_view`, …). `dispatch.rs` keeps `main.rs` thin — handlers translate a Message into controller calls. Menu items and widget handlers are deliberately one-line `sender.send(...)` calls; the real work lives behind the Message. To add a feature: add a Message variant → route it in the `main.rs` match → handle it in the right `dispatch.rs` function.

**2. AppState as mediator.** `src/app/state.rs` (`AppState`) is the central coordinator holding the editor widget, `TabManager`, settings (`Rc<RefCell<AppSettings>>`), and ~10 controllers (`src/app/controllers/`): `file`, `highlight`, `tabs`, `view`, `session`, `plugin`, `widget`, `preview`, `update`, plus `hook_dispatch`. Controllers own a slice of domain logic and **return action enums** rather than mutating UI directly — e.g. `FileController` returns `Vec<FileAction>` which `AppState::dispatch_file_actions` then applies. This keeps side effects funnelled through `AppState`. Dispatch handlers in `dispatch.rs` typically: call a controller, then hand its returned actions back to `AppState`.

**3. Services vs controllers.** `src/app/services/` holds reusable, mostly UI-agnostic business logic (`syntax/` chunked highlighter, `terminal/` PTY+VTE, `session.rs` persistence, `shortcut_registry.rs`, `updater.rs`, `file_size.rs`, `font_catalog.rs`, `plugin_*`). Controllers orchestrate services in response to Messages.

Other big-picture pieces:
- **Syntax highlighting** (`services/syntax/`, `controllers/highlight.rs`): a 3-tier engine using `syntect`. Highlighting is chunked and incremental with sparse checkpoints; large files (threshold in `highlight.rs` / `file_size.rs`) are handled specially. Highlighting runs via `DoRehighlight`/`ContinueHighlight` Messages, never on a background timer (see Philosophy).
- **Plugin system** (`src/app/plugins/`): Lua 5.4 via `mlua`, statically linked. `loader.rs` discovers plugins from `~/.config/ferrispad/plugins/`; `runtime.rs` runs the VM; `security.rs` does static source analysis + sandbox enforcement; `api/` exposes the editor/filesystem/command APIs; 11 event hooks live in `hooks.rs`. Plugins are **sandboxed by default** and signature/checksum-verified (`services/plugin_verify.rs`).
- **MCP server** (`src/app/mcp/`): JSON-RPC over TCP plus a stdio bridge (`FerrisPad --mcp-server`). Lets external AI agents read editor context and propose edits (surfaced as diff/split-view reviews; see `pending_diff_reviews` in `AppState`).
- **Deferred work** (`infrastructure/defer.rs`, `Deferred*` Messages): session restore and CLI file opens are deferred via `defer_send` so the window paints instantly. The status bar and editor-context file are updated every loop iteration (not just on Messages) because mouse selection generates no Message.

UI widgets live under `src/ui/` (`main_window.rs` layout, `tab_bar.rs`, `terminal_panel.rs`, `dialogs/`, `theme.rs`, …) and are passed into dispatch handlers as `LayoutWidgets` / `HighlightWidgets` borrow structs.

## Design constraints (PHILOSOPHY.md — treat as hard requirements)

These are non-negotiable and shape what code is acceptable:
- **0% CPU when idle.** No background indexers, file watchers, daemons, polling timers, or LSP. Features must be reactive to user action, not proactive. Update/plugin-update checks run **once at startup** on a thread, then terminate.
- **Single self-contained binary, zero runtime deps.** Scripting (Lua) is statically linked. No Node/Python/JVM.
- **No telemetry** of any kind.
- **Memory frugality.** jemalloc on Linux/macOS is configured with `dirty_decay_ms`/`muzzy_decay_ms = 0` (in `main.rs`) to return freed pages to the OS immediately; there is a `MallocTrim` Message path.
- Minimize `unsafe`; treat all external input as untrusted.

When evaluating a feature or change, check it against these before implementing.

## Notable constraints / gotchas

- **Max editable file size ≈1.9 GiB.** FLTK's `Fl_Text_Buffer` uses 32-bit positions; FerrisPad hard-caps below 2 GiB. Larger files open read-only (memory-mapped) or in tail/chunk mode.
- Use `buffer_text_no_leak` (`infrastructure/buffer.rs`) instead of FLTK's buffer text getter — the raw FFI getter leaks.
- `AppSettings` is shared as `Rc<RefCell<AppSettings>>`; borrow it narrowly and `drop` the borrow before sending Messages or calling controllers that may re-borrow.
- Release signing happens in CI (see Releases), not locally. The signing key (also used for plugins) lives at `~/.config/ferrispad/signing/plugin_signing_key.bin` and is stored base64-encoded as the `SIGNING_KEY` GitHub Actions secret.
