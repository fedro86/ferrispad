---
id: T0106
title: Build seams so the structural theme test covers every dialog
status: done             # todo | review | done
created: 2026-11-26
severity: minor
area: ui
release: 0.9.11         # the planned release
depends-on: [T0105]     # landed on master before this ticket
issue: 205
---

## Goal

T0105's structural theme test (guard (b): `unthemed_inputs` in
`tests/ui_dialogs.rs`) only reaches dialogs that have a `build_*` function
returning their window without running it: the theme manager, color picker,
name prompt, Settings and the Sessions picker. Every other dialog builds its
window and runs its event loop in one function. A themed property one of
them gets wrong (a field without the shared selection, a hand-styled
button) is caught only if someone opens it under Xvfb. The goal is for
guard (b) to walk **every** dialog.

## In scope

- Split each run-only dialog into `build_<dialog>(theme, …) -> <Dialog>Ui`
  (window and the widgets its callbacks need, built but not shown) plus the
  existing `show_*` function, which builds, wires callbacks and runs. No
  behavior change. The dialogs: find, replace, go to line, font picker, new
  session prompt, shortcut dialog, about, update available, large-file
  warning and "file too large", the read-only viewer, plugin
  config / settings / permissions / manager, and community install.
- `tests/ui_dialogs.rs` builds every one of them in dark and light, and runs
  the layout checks and `unthemed_inputs` on each.
- `unthemed_inputs` also checks buttons (frame, color and label color equal
  to a `button()`, `secondary_button()` or `warning_button()`). The lists it
  already checks.

## Out of scope

- Any visual change: proven by a pixel diff per dialog (below).
- Dialogs that FLTK builds itself (the native file chooser, `fltk::dialog`
  alerts).
- The progress window of `load_to_buffer_with_progress`, which drives its
  own loop while it reads.

## How to test

```bash
nix shell nixpkgs#xvfb-run -c xvfb-run -a env FERRISPAD_REQUIRE_UI_TESTS=1 \
  nix develop -c cargo test --test ui_dialogs
```

- Red: with one component mutated (e.g. `style_text_field` not setting the
  cursor color, or `button()` not setting the label color), the test fails
  and names each newly covered dialog that uses that component.
- Pixel diff (Xvfb, Claude): each dialog is opened in the app, from the
  T0105 build (before) and from this branch (after), on a dark and a light
  theme. `compare -metric AE` must report 0 differing pixels per dialog.

## Acceptance criteria

- [x] Every listed dialog has a `build_*` seam, and the structural test
      walks it in dark and light.
- [x] `unthemed_inputs` checks buttons, and fails on a mutated component
      for each newly covered dialog.
- [x] Pixel diff: 0 differing pixels per dialog, before vs after.
- [x] Full gates green.

## Affected files

- `src/ui/dialogs/*.rs`, `src/ui/file_dialogs.rs`: the `build_*` seams.
- `tests/ui_dialogs.rs`, `tests/support/ui_layout.rs`: the extended guard.

## Notes

- Origin: T0105's review (the follow-up text in T0105's Notes); assigned
  by the orchestrator on 2026-11-26, target 0.9.11.

## What was done (2026-11-26)

### Seams

Every listed dialog has a `build_*` function returning a `<Dialog>Ui` struct
(window plus the widgets its callbacks need), built but not shown. Its
`show_*` keeps its signature, calls the builder, then does the rest in the
original order. The theme-managed popups also have
`build_themed_message` / `build_themed_choice` (the large-file warning is a
themed choice).

- **Data passed in, so tests can fake it:**
  - The shortcut dialog takes `PluginShortcuts { name, menu_items }` instead
    of the `PluginManager`, whose plugins hold a Lua table.
  - The Plugin Manager takes `InstalledPluginEntry` / `OfficialPluginEntry`
    lists, which `show_*` computes first (same logic, now before the window
    is built).
  - Plugin config, plugin settings, permissions, community install and the
    update dialog take their plain data.
- **The one reorder:** `make_modal(true)` now runs in `show_*` right after
  the build, before `show()`, instead of right after the window was
  created. It only sets a flag FLTK reads at `show()`.
- **Left in `show_*`, by design:**
  - the font list, filled after `show()`;
  - the read-only viewer's first page, which needs the memory-mapped file;
  - the Plugin Manager's Community tab, which loads lazily from the network.

### The guard

`tests/ui_dialogs.rs` builds every dialog above in dark and light (both
variants where a dialog has optional parts: shortcuts with and without
plugins, update with "Manual Update Only", community install manual and
indexed, Plugin Manager online and offline). It runs `layout_failures` and
`unthemed_inputs` on each.

`unthemed_inputs` now also checks buttons:

- A push button must look like `button()`, `secondary_button()`,
  `warning_button()` or the new `success_button()`: flat, with that color
  and label color.
- A check or radio button must sit on the dialog background with the text
  color.
- `text_dim` is accepted for an input's placeholder text and a dimmed
  check label (a disabled plugin).

**One hand-styled widget found and made a component:** the Plugin Manager's
"Update" button set its green label by hand. It now uses
`DialogTheme::success_text()`, the same green as before (dark
(100,200,100), light (40,140,40)), modelled on `warning_text()`. There is no
pixel change.

**Test harness fixes**, which were false positives:

- text displays and color choosers are checked as one widget (their
  internal scrollbars sit at 0,0 1×1 until drawn);
- `Pack` children are laid out as `Fl_Pack::draw` does;
- a `Tabs`' pages are not checked against each other (they share one
  rectangle, and their labels are the tab captions);
- labels are measured with symbols on, as FLTK draws them ("@<" is an
  arrow).

### Layout defects found (pre-existing): `KNOWN_LAYOUT_DEFECTS`

Reaching these dialogs surfaced 50 layout failures (25 per theme) that
exist on master today. The orchestrator decided (2026-11-26) that T0106
stays pixel-identical. The test subtracts an exact-string
`KNOWN_LAYOUT_DEFECTS` list and fails on any other defect, and also on a
listed one that disappears. **T0110** fixes them and empties the list.

Clipped already at the default font size:

- New Session "Session name:" (105/100 px);
- Update "Skip This Version" (122/112);
- Update "Manual Update Only" (147/113).

The rest are at +2 pt:

- Find/Replace, Go To Line, Font picker;
- Shortcuts "Plugins > General > Run All Checks";
- read-only viewer buttons;
- Update "View on GitHub".

Plus two overlaps: the Permissions message frame runs 3 px into the
buttons, and the Plugin Manager's Official rows' text frames run under
"Details".

### Red (mutated components)

With `flat_button` not setting the label color and `style_text_field` not
setting the cursor color, the test fails with 138 button and 71 cursor
failures. **Every** newly covered dialog is named:

- find 4, replace 7, go to line 3, font picker 4, new session 3, about 1;
- message 1, large-file warning 2, shortcuts 88, update 8, file too
  large 6, read-only viewer 12;
- plugin config 4, plugin settings 3, permissions 2, community install 4,
  plugin manager 19.

### Pixel diff (Xvfb, Claude)

**Builds:** the T0105 build (before) against this branch (after).

**Setup:** a fresh config per run. It has:

- a fixture plugin with a menu item, four config params (number, boolean,
  choice, string with placeholder) and `execute = ["ruff"]`;
- a fresh official registry cache;
- `large_file_warning_mb = 1` and `max_editable_size_mb = 2`, plus 1.5 MB
  and 3 MB text files.

**How each dialog was opened**, on Base16 Ocean Dark and Base16 Ocean Light
(one full 1280×800 screenshot each):

- shortcuts: Ctrl+F, Ctrl+H, Ctrl+G, and Ctrl+G then Go with an empty
  field (the themed message popup);
- Ctrl+Shift+N (new session);
- menus: Edit → Key Shortcuts…, Format → Font → More Fonts…, Help → About;
- Plugins → General → Plugin Manager… (Installed tab, then the Official
  tab), Plugins → General → Settings…, Plugins → T0106 Demo → Settings…;
- at startup: the permission request (command not yet approved);
- opening the 1.5 MB file (large-file warning) and the 3 MB file (File Too
  Large, then "View Read-Only" for the read-only viewer).

**Result:** `compare -metric AE` gives **0 differing pixels in all 34
pairs** (17 dialogs × 2 themes). Each "after" screenshot was checked to
show its dialog.

**Not reachable offline:** the update dialog (it needs a newer release on
GitHub) and the community install review (it fetches `plugin.toml` from
GitHub). Their builders hold the moved code verbatim, apart from
`make_modal` (see Seams), and the guard builds both in each variant.

### Gates

- Full suite under Xvfb (`FERRISPAD_REQUIRE_UI_TESTS=1`): lib 585, every
  binary green.
- fmt and clippy `-D warnings` clean.

## Review round 1 (a3, 2026-11-26): CI fonts

**Blocking:** CI was red. `KNOWN_LAYOUT_DEFECTS` pinned exact pixel strings,
and label widths depend on the installed fonts. CI measures wider than this
desktop:

- "Find what:" needs 89 px there and 86 px here;
- "Session name:" needs 112 px at +0 pt there;
- CI reports defects that don't show here: "Remind Later", "Open Lines:",
  "Install Anyway", "e.g. Ctrl+Shift+L", "(case folding: ASCII only)";
- the two overlaps show here and not on CI.

Fix (orchestrator's option (a)):

- Each known defect is now listed by a **pixel-free identity**: the
  failure minus its theme mode and measurements (`defect_identity`; e.g.
  `find/0:"Find what:": clipped label`, `permissions: "" overlaps "Deny"`).
  The list is the **union** of this desktop's and CI's: 29 identities
  (local 21, CI 25).
- A defect whose identity is not listed fails.
- The "a listed defect is gone" check is dropped. Which listed defects
  reproduce depends on the fonts, so it could not hold on both machines.
  T0110 empties the list anyway.
- Test: `defect_identity_drops_measurements` (a CI string, a local one and
  an overlap).

**Lesson for T0110:** frame widths must be measured at build time, so they
fit any font.

## The merges and the compare.sh pixel proof (2026-11-28, Implementer A)

### Merges of master

- **1d9a6e7, master c3672fe** (T0097 and T0105 as squash commits, T0098's
  `fltk_rgb`). The previous session had left this merge staged with all 20
  conflicts marked resolved, but it **did not compile**: `find.rs` had lost
  its `Button`/`CheckButton`/`Input` imports, and `about.rs`/`update.rs`
  kept a `Color` import that T0098 made unused (a `-D warnings` failure).
  After the fix, the merged diff against c3672fe equals this ticket's own
  diff (c6edf1b..3f53937) line for line, except master's
  `Color::from_rgb` → `fltk_rgb`.
- **6f8e505, master 818970b** (T0111): clean.
- **49f8fc3, master 6d70289** (T0104, T0116, T0112, T0073, T0095, T0122,
  T0121): one conflict, in `docs/pr/scenarios/README.md`; both tables'
  new rows kept. The page moved to `docs/pr/0.9.11/` (T0121).
- **f691881, master 62669e9** (T0120, the screenshot scripts only): clean.
  No app code changed, so the compare.sh proof against 6d70289 holds.

### Pixel proof with compare.sh

The 2026-11-26 pixel diff ran from a hand-written Xvfb script, which the
new-pr skill no longer allows. It is redone with `compare.sh` and four
scenarios committed with this ticket (they serve T0110 too):

- `dialogs.sh`: Find, Replace, Go To Line and its message popup (line 99
  of a 6-line file), New Session, Key Shortcuts, Choose Font, About, Plugin
  Manager (Installed and Official), Plugin Settings ("Run All Checks
  Configuration"), the fixture plugin's settings;
- `dialogs-permission.sh`: the permission request at startup;
- `dialogs-large-file.sh`: the large-file warning (generated 1.5 MB);
- `dialogs-too-large.sh`: "File Too Large", then the read-only viewer
  (generated 3 MB).

The fixture plugin is `docs/pr/scenarios/plugins/dialogs-demo`, and a
registry cache keeps the Official tab offline. `_dialog-helpers.sh` gives
a dialog the keyboard with `xdotool windowfocus` (there is no window
manager). The large fixtures are generated at source time and gitignored.

```bash
.claude/skills/new-pr/compare.sh docs/pr/scenarios/<scenario>.sh <outdir> --before 6d70289
```

`--before 6d70289` is the master this branch was last merged with, so the
pair isolates T0106. Result: **16 dialogs × 2 themes, all 32 pairs AE 0**,
both against 818970b (after 6f8e505) and again against 6d70289 (after
49f8fc3)
(`magick compare -metric AE`). No panic or assert in any log; every run on
its own display (:90), one at a time. Every after shot was read edge by
edge on a contact sheet.

**Pre-existing defect seen (not T0106's):** the permission request clips
a long plugin description at its right edge ("…settings and a command
p"), identical on master. The guard doesn't list it because the test
builds the dialog with a short description. Reported to T0110
(ferrispad-39).

Still not reachable offline: the update dialog and the community-install
review (network). The guard builds both in each variant.

## User verification

The user approved the PR on 2026-11-28, relayed by the orchestrator
(ferrispad-03), after reading the presentation
`docs/pr/0.9.11/PR-208-dialog-build-seams.html`. The reviewer
(ferrispad-90) approved 79f7f6b: gates 790 passed, CI 4/4 green, the merged
code diff identical to the reviewed one except master's `fltk_rgb`, and
their own compare.sh run against 62669e9 at AE 0 on 32 of 33 pairs (the
33rd, too-large-light at AE 38, is the text cursor caught in another blink
phase). This commit only moves the ticket and updates the page.
