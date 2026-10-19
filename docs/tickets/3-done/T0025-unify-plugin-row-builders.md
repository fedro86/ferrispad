---
id: T0025
title: Unify the three plugin-row builders (remove the arg-swap hazard)
status: done
created: 2026-08-24
severity: minor
area: ui
depends-on: []
issue: 59
---

## Goal

`plugin_manager.rs` has three ~identical row builders (installed / available /
community, ~700 lines total), each with `#[allow(clippy::too_many_arguments)]`.
They've already drifted (title/description y-offsets differ, so the three tabs
render misaligned), and — worse — their parameter lists are the same set of
`Rc<RefCell<Vec<String>>>` handles in different orders, so swapping
`installed`/`uninstalled` at a call site compiles silently. Collapse to one
builder taking a `RowKind` enum.

## In scope

- One `PluginRow`/`create_plugin_row(kind: RowKind, ...)` builder shared by all
  three tabs; pass typed state (not three same-typed `Vec` handles in ambiguous
  order).
- Fix the vertical-alignment drift as a side effect (single layout).

## Out of scope

- Restyling the plugin manager.

## How to test

### Automated

```bash
nix develop -c cargo test --lib ui::dialogs::plugin_manager
```

This runs the new unit tests for the pure row logic: the shared text layout
(`row_text_layout`: 10/28/46, centred, no overlap), the trust-badge mapping,
the three meta-line formats, the install-button state, the Installed header,
and name normalisation. Full gates:

```bash
nix develop -c cargo fmt --check
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo test
```

### Manual (in the running app)

`nix develop -c cargo run`, then open **Plugins → General → Plugin Manager...**.

1. **Installed** tab: each row shows title / description / status line. Click
   **Disable** on a plugin: the button becomes **Enable** and the status line
   reads `Disabled | <badge>`. Click again to restore.
2. **Official** tab: the title, description and meta lines sit at the same
   height as on the Installed tab (before this change they were 4-6 px higher).
   Plugins already installed show a greyed **Installed** button; outdated ones
   show a green **Update**; plugins with a README also show **Details** (opens
   a browser confirmation).
3. **Community** tab: same row alignment; **Install** opens the review dialog;
   **Details** shows the repo URL confirmation.
4. Install a plugin from Official (or Community): the button turns **Done!**
   and a new enabled row appears on the Installed tab.
5. On the Installed tab, **Uninstall** that plugin and confirm: the row
   disappears and the Official/Community button returns to **Install**.
6. Close the dialog: the toggle/install/uninstall changes are applied as before.

Widget placement and callbacks are UI-only, so they are manual-only checks; the
logic behind them is covered by the unit tests above.

## Acceptance criteria

- [x] One row builder; no `too_many_arguments` allow needed.
- [x] Same-typed handles no longer positionally ambiguous.
- [x] Rows aligned across tabs; actions still work.
- [x] `cargo test` / `clippy` / `fmt` clean.

## Affected files

- `src/ui/dialogs/plugin_manager.rs` — the three row builders replaced by
  `create_plugin_row(RowKind, &PluginRowContext)`; call sites in
  `show_plugin_manager_dialog` updated.

## Notes

- Origin: UI audit (SEVERE — the silent arg-swap is a latent correctness bug,
  not just dead weight).

## Outcome (2-review)

**Was there a real swap?** No. Every call site of the three old builders (the
Installed tab loop, the Official tab loop, the lazy Community loader, the URL
install callback, and the two cross-tab install callbacks) passed `installed`,
`uninstalled` and the other handles in the order the callee expected. So this is
a refactor, not a defect fix: the hazard is removed by construction, not by a
red test.

What changed (all in `src/ui/dialogs/plugin_manager.rs`):

- `PluginRowContext` holds the shared dialog state (theme, row width,
  `toggles`, `installed`, `uninstalled`, the cross-tab button/row maps, the
  Installed pack and its empty label) under **named fields**. It is built once
  in `show_plugin_manager_dialog` and cloned into callbacks, so the two
  `Rc<RefCell<Vec<String>>>` lists can no longer be swapped by position.
- One builder, `create_plugin_row(kind: RowKind, ctx: &PluginRowContext)`,
  draws the icon, title, description and meta line for every tab. `RowKind` is
  `Installed(InstalledPlugin { name, version, description, enabled, tier })`,
  `Official { info, already_installed, update_available }` or
  `Community { info, already_installed }`. These are also named fields, so the
  three `&str` fields of an installed plugin cannot be swapped either. Small
  helpers wire each tab's buttons (`add_installed_actions`,
  `add_details_button`, `add_official_install_button`,
  `add_community_install_button`), each with at most six parameters. All three
  `#[allow(clippy::too_many_arguments)]` are gone.
- One text layout (`row_text_layout`): title/description/meta at y = 10/28/46,
  centred in the 70 px row. This is the Installed tab's old layout; the Official
  and Community tabs used 6/24/40 and now line up with it. This is the only
  visible change.
- The repeated "after install" bookkeeping (drop from `uninstalled`, push to
  `installed`, hide the empty label, add the Installed row, refresh the pack)
  now lives in `PluginRowContext::record_install` / `add_installed_row`, used
  by the Official, Community and URL install paths. Badge text and colour live
  in `TrustBadge`, so the toggle callback no longer repeats the badge `match`.
- Unit tests for the pure helpers listed under *How to test*.

Behaviour is otherwise unchanged: same labels, colours, buttons and cross-tab
sync. One internal detail differs: each Installed tab row used to get its own
`Rc<RefCell<Pack>>` around a clone of the same pack handle; they now share the
context's single `Rc`. It wraps the same FLTK widget, and every borrow of it is
statement-scoped inside a callback, so no borrows overlap.

Scope note: in the URL-install callback only the capture list and the
row-creation code changed (it now calls `record_install` / `add_installed_row`).
Its alert dialogs are untouched, for T0024.
