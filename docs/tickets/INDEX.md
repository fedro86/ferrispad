# Ticket index — 2026-08-24 audit

38 tickets in `1-todo/`, ordered by priority. Severity is the audit's; **[V]**
marks the three headline findings I verified directly in the source. Suggested
landing order top-to-bottom (security first, then data-safety, then UI bugs,
then quality/dedup, then process — but **T0036 (CI gates) is worth landing
first** so every other fix is protected).

## Security — SEVERE

| ID | Title | |
|----|-------|--|
| T0001 | Validate terminal_view args & working_dir (shell injection → RCE) | **[V]** |
| T0002 | Canonicalize sandbox write paths (path-traversal escape) | **[V]** |
| T0004 | Bound recursion in Lua/YAML parsers (stack-overflow abort) | |
| T0005 | Make plugin instruction/memory limit unbypassable | |

## Robustness — SEVERE (crash / data loss on ordinary input)

| ID | Title | |
|----|-------|--|
| T0003 | Terminal PtySession use-after-free after close() | **[V]** |
| T0012 | UTF-8 slice panic in YAML value truncation | |
| T0013 | Case-insensitive find/replace offset mismatch (wrong replace / panic) | |
| T0014 | read_tail/save_partial CRLF offset → silent data loss | |

## Security — MODERATE

| ID | Title |
|----|-------|
| T0006 | Rethink the Lua static analyzer (bypasses + false positives) |
| T0007 | Sanitize registry-controlled install paths |
| T0008 | Bound Rust-side plugin work (diff size cap + symlink-safe scan) |
| T0009 | Isolate plugins per Lua environment |
| T0010 | Bound terminal escape repeats + real ring-buffer scrollback |
| T0011 | Time-box synchronous plugin hooks/commands (UI freeze) |
| T0038 | Harden editor-context temp file (perms + cleanup) |

## UI bugs — SEVERE / MODERATE

| ID | Title |
|----|-------|
| T0017 | Fix two plugin-dialog hangs (missing quit check) |
| T0018 | Terminal divider .unwrap() panic + restore drag guard |
| T0019 | Re-theme all panels on settings-driven theme change |
| T0020 | Tab drag/release panic-by-indexing on stale indices |
| T0021 | Preserve split panel dragged height across mode toggle |
| T0022 | tree_panel "none" icon drift when filtering |

## Robustness — MODERATE

| ID | Title |
|----|-------|
| T0015 | save_session wipes another instance's session when empty |
| T0016 | Cross-platform session locking (no /proc off Linux) |

## UI quality / dedup — MINOR (tech debt)

| ID | Title |
|----|-------|
| T0023 | Unify the three divider drag handlers |
| T0024 | Route dialogs through themed helper + one modal loop |
| T0025 | Unify the three plugin-row builders (arg-swap hazard) |
| T0026 | menu.rs → add_emit + single shortcut table |
| T0027 | Extract a Pane struct in split_panel.rs |
| T0028 | Collapse tab_bar hover fields into hover: HitResult |
| T0029 | Consolidate scrollbar FFI helper + fix SAFETY docs |
| T0030 | Unify color-math APIs + fix diverged factors |
| T0031 | Deduplicate find_next/find_prev via Direction enum |

## Robustness — MINOR (latent)

| ID | Title |
|----|-------|
| T0032 | saturating_sub for terminal grid row/col math |
| T0033 | Guard syntax theme lookup against missing key |
| T0034 | Use semver crate for registry version comparison |
| T0035 | Registry download size limits (stream + official path) |
| T0037 | Guard u8 underflow on syntax style byte path |

## Process

| ID | Title |
|----|-------|
| T0036 | Add cargo fmt/clippy/test gates to CI (land first) |

---

**Provenance.** T0001/T0002/T0003 verified by reading the source this session.
The rest come from two audit passes (plugins+services; UI layer) and are
tagged in each ticket's Notes with the originating finding. They are
well-localized (file:line) but not each independently re-verified — treat the
non-[V] ones as "high-confidence, confirm the red test fails before fixing"
(which the workflow enforces anyway).
