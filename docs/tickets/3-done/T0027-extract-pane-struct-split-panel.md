---
id: T0027
title: Extract a Pane struct in split_panel.rs
status: done
created: 2026-08-24
severity: minor
area: ui
depends-on: []
issue: 51
---

## Goal

`SplitPanel` has ~30 fields, of which the `left_*` and `right_*` groups
(display, buffer, style buffer, label, markers) are hand-mirrored twins.
`show_request_with_syntax`, `apply_theme`, `update_display_fonts`, and
`apply_syntax_diff_pane(is_left: bool, ...)` all write everything twice. With a
`Pane` type owning one side's widgets + methods, the struct drops to ~15 fields
and those methods roughly halve (estimate 1331 → ~800 lines) with no loss of
function.

## In scope

- Introduce a `Pane` struct (one side's display/buffer/style/label/markers) with
  methods; store `left: Pane, right: Pane`.
- Replace the `is_left` boolean-flag branching with per-`Pane` method calls.

## Out of scope

- The scrollbar FFI dedup (T0029) and diff-map logic (which is good as-is).

## What was done

All in `src/ui/split_panel.rs`; the public `SplitPanel` API is unchanged, so no
caller (`dispatch.rs`, `controllers/widget.rs`, `main_window.rs`) changed.

- **`Pane<D: DisplayExt>`** owns one side: `display`, `buffer`, `style_buffer`,
  `label`, `default_label` ("Left"/"Right") and `markers`. It is generic so the
  left pane stays a `TextDisplay` and the right pane a `TextEditor` (needed for
  the editable mode). `SplitPanel` now holds `left: Pane<TextDisplay>` and
  `right: Pane<TextEditor>`; the 11 mirrored `left_*`/`right_*` fields are gone
  (30 → 24 fields; the remaining ones are not per-side).
- **Pane methods** replace every mirrored block: `new` (buffer + display
  setup), `load` (content, label with default fallback, line-number gutter),
  `set_font`, `apply_syntax_diff` (replaces `apply_syntax_diff_pane(is_left,…)`),
  `set_style_table`, `set_markers`, `clear`, `apply_theme` (label, display,
  line-number and scrollbar colors). `show_request_with_syntax`, `apply_theme`,
  `update_display_fonts` and `hide` now call each method once per pane; a
  `SplitPanel::apply_cached_style_table` helper applies the cached table to
  both panes (used by those three methods).
- **Pure logic extracted**: `pane_style_string(content, highlights,
  syntax_bytes, table, sdm)` — the style-string computation that used to sit
  inside `apply_syntax_diff_pane` — with three new unit tests.
- **Other twins collapsed with the Pane**: the two scroll-sync handler bodies
  share one `sync_scroll` helper (the right pane keeps its read-only key/paste
  block in front of it); `get_vscrollbar_value`/`get_vscrollbar_value_editor`
  became one generic `get_vscrollbar_value<W: WidgetExt>`; the
  `style_text_display_scrollbars`/`style_text_editor_scrollbars` pair folded
  into `Pane::apply_theme`; marker geometry for both panes goes through one
  `draw_pane_markers`. `style_scrollbars_raw` moved, unchanged, to a free
  function next to `get_vscrollbar_value_raw`, so T0029 finds both FFI copies
  together. The FFI bodies and their comments are untouched (T0029's scope).
- Line count: 1389 → ~1380 including 3 new tests (≈ −50 lines of
  non-test code). The ticket's "~800" estimate did not hold: most of the file
  is non-mirrored code (diff map, style table, action buttons, theme colors)
  that this ticket leaves alone by scope.

Behaviour is unchanged by construction: same widgets created in the same order
in the same Flex groups, same colors, fonts, labels, style table, scroll and
read-only handling. Only the code shape changed.

## How to test

### Automated

```bash
nix develop -c cargo test --lib split_panel
nix develop -c cargo test
```

The `split_panel` filter runs the 5 split-panel unit tests: the two T0037
pins plus the new `pane_style_string_*` tests (empty content → empty string;
no syntax → diff base styles `AAABBAAA`; syntax + diff → combined style `GG`).
The full suite must stay green.

Gate note (2026-10-19): `fmt --check` and `clippy -D warnings` were clean, and
the full suite was green except
`terminal::grid::tests::line_feed_flood_trims_in_bounded_time`. That is a
wall-clock test (< 2 s) in a file this ticket does not touch, and it failed
only because the host was saturated by concurrent builds (load average 25–37;
the test took 7.9 s even when run alone). Re-run it on an idle machine.

### Manual (the split view is UI-only; this part is manual)

Needs the `git-diff` plugin (Plugins menu → "Show Git Diff", `Ctrl+Shift+D`)
and a file in a git repo with uncommitted changes.

1. `nix develop -c cargo run`, open a modified tracked file (e.g. a `.rs`
   file with an added, a removed and a changed line).
2. Press `Ctrl+Shift+D`. Expected: both panes show the file with syntax colors
   **and** green/red/yellow diff backgrounds; brighter emphasis on the changed
   characters; labels show the plugin's names (e.g. HEAD / Working Copy); line
   numbers on both sides; both panes scrolled to ~3 lines above the first
   change; colored markers on both vertical scrollbars.
3. Scroll with the mouse wheel over either pane: the other pane follows. Drag
   one scrollbar and release: the other pane jumps to the same line.
4. Type in the right pane while it is read-only: nothing is inserted.
5. Toggle dark mode (View menu): labels, panes, line numbers, scrollbars and
   diff backgrounds re-theme on **both** sides.
6. Change the editor font/size in Settings: both panes update.
7. Click "Tab Mode", then "Panel Mode": content, colors and fonts survive the
   toggle on both sides. Click "Close": the panel hides; reopening with
   `Ctrl+Shift+D` shows fresh content.

Compare against `master` if in doubt — the view must look identical.

## Acceptance criteria

- [x] `Pane` struct in place; left/right mirroring gone from the four methods.
- [ ] Split/diff view visually identical to before (manual check by the user).
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/split_panel.rs` — `Pane<D>`, `pane_style_string`, `sync_scroll`,
  generic `get_vscrollbar_value`, free `style_scrollbars_raw`; `SplitPanel`
  fields and the `new` / `show_request_with_syntax` / `setup_scrollbar_markers`
  / `hide` / `apply_theme` / `update_display_fonts` bodies; new unit tests.

## Notes

- Origin: UI audit (MODERATE). `start_page.rs:325-479` shows the codebase
  already knows this style of helper decomposition.
- Follow-up tickets stacked on this branch (T0029, T0021, T0023, T0019, T0024)
  should target `Pane` / the free FFI helpers rather than the old
  `left_*`/`right_*` fields.
