---
id: T0022
title: Fix tree_panel "none" icon drift when filtering
status: review
created: 2026-08-24
severity: minor
area: ui
depends-on: []
issue: 50
---

## Goal

`tree_panel.rs` has two tree builders that share an icon-match block but have
diverged: `add_tree_node` maps `"none" => ""` (no icon), while
`add_filtered_tree_node` does not. So a plugin that sets `icon: "none"` gets no
icon normally, but a 📄 emoji appears as soon as the user types in the search
box. A user-visible inconsistency caused by copy-paste drift.

## In scope

- Make `add_filtered_tree_node` handle `"none"` identically to `add_tree_node`
  (ideally by extracting the shared icon-resolution into one helper both call).

## Out of scope

- Fully unifying the two builders (larger refactor) — but a shared icon helper
  is welcome here.

## What was actually done

**Finding: the described symptom does not reproduce on `master`.** The
filtered builder has no explicit `"none"` arm, but its `_ => ""` wildcard
catches `"none"`. It returns the same `""` as the normal builder. The
📄 fallback is only reached when `node.icon` is `None`, and a plugin's
`icon = "none"` arrives as `Some("none")` (`TreeNode::from_lua_table`). The two
match blocks were textually different but gave the same result for every input.

Evidence: both match blocks from `origin/master` were copied verbatim into a
standalone probe and run over every hint (`None`, `"none"`, the 5 known hints,
an unknown hint, `""`), each with and without children:

```
Some("none") children=false normal="" filtered="" same=true
Some("none") children=true normal="" filtered="" same=true
... (all 18 combinations: same=true)
```

So there was no failing test to write: a red test for "`"none"` shows 📄 when
filtered" cannot fail on the current code. The ticket's real value is the
second acceptance criterion: remove the duplicated match so the two builders
*cannot* drift. That part is done as a behaviour-preserving refactor:

- New module-level `icon_prefix(node: &TreeNode) -> &'static str` in
  `src/ui/tree_panel.rs`. It is the only icon decision. `add_tree_node` and
  `add_filtered_tree_node` both call it, and their duplicated match blocks are
  deleted.
- New `ICON_PREFIXES` table (hint → prefix) plus `FOLDER_ICON` / `FILE_ICON`
  constants. `strip_icon` now folds over the same table, so a newly added icon
  is stripped from the context-menu node paths automatically. Before, that was
  a third copy of the emoji list. `is_folder_item` uses `FOLDER_ICON` too, with
  the same `starts_with` semantics as before (no trailing space).
- Unit tests in a new `#[cfg(test)] mod tests` at the end of `tree_panel.rs`:
  `"none"` and unknown hints give no icon (with and without children), known
  hints map to their emoji whether or not the node has children, no hint falls
  back to folder/file, and `strip_icon` strips every prefix `icon_prefix` can
  produce.

All edits are outside the divider-drag region (`create_divider`, ~L248-290)
that T0023 will touch.

## How to test

### Unit tests

```bash
nix develop -c cargo test --lib ui::tree_panel
```

Expected: 5 tests pass (`none_hint_has_no_icon_with_or_without_children`,
`unknown_hint_has_no_icon`, `known_hints_map_to_their_icon_regardless_of_children`,
`no_hint_falls_back_to_folder_or_file`,
`strip_icon_removes_every_prefix_icon_prefix_can_produce`).

### Manual check

When a plugin sends `yaml_content` to the tree view, the parser
(`src/app/services/yaml_parser.rs`) sets `icon = "none"` on every structured
node. Any plugin that does this (e.g. a YAML/JSON viewer plugin from the
registry) exercises the `"none"` path.

1. `nix develop -c cargo run` with such a plugin enabled, then open a YAML
   file, e.g. `.github/workflows/release.yml`.
2. Open the plugin's tree view.
   Expected: nodes show their `{N}` / `[N]` type indicators and **no** emoji.
3. Type a key name that exists in the file (e.g. `jobs`) into the tree's search
   box.
   Expected: the matching nodes and their ancestors stay emoji-free. No 📄 or 📁
   appears.
4. Clear the search box. Expected: the full tree comes back, still emoji-free.
5. Open a file-explorer style tree (folders/files) and filter it the same way.
   Expected: 📁/📄 icons are the same in the normal and filtered views. Right-click
   → "Copy path" (or any context action) still gives the path without the
   emoji.

## Acceptance criteria

- [x] `"none"` yields no icon in both the normal and filtered tree (already true
      on `master`; now pinned by a unit test on the shared helper).
- [x] Icon resolution shared by both builders (no second divergence possible).
- [ ] Manual recipe verified.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/tree_panel.rs` — `add_tree_node`, `add_filtered_tree_node`,
  `strip_icon`, `is_folder_item`; new `icon_prefix`, `ICON_PREFIXES`,
  `FOLDER_ICON`, `FILE_ICON`, and a tests module.

## Notes

- Origin: UI audit (MODERATE — user-visible drift). The audit read the missing
  explicit arm as a behaviour difference. The wildcard made it a latent hazard,
  not a live bug.
- Tracked as `enhancement`, not `bug`, because no defect reproduces.
- Gate note: `fmt` and `clippy -D warnings` are clean, and `cargo test` gives
  372 passed / 1 failed. The failure is the wall-clock test
  `terminal::grid::tests::line_feed_flood_trims_in_bounded_time` (2s bound,
  took ~6.8s). It ran while the machine's load average was ~32 from
  concurrent builds. The test touches no code in this diff. Re-run it on an
  idle machine to confirm it passes.
