---
id: T0110
title: Fix clipped labels and overlaps in the dialogs T0106 made testable
status: review           # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: [T0106]     # stacked on ticket/T0106 (KNOWN_LAYOUT_DEFECTS)
issue: 210
---

## Goal

T0106 made the layout test reach every dialog. It found 50 pre-existing
layout defects (25 per theme), now pinned in `KNOWN_LAYOUT_DEFECTS`
(`tests/ui_dialogs.rs`).

Three labels are clipped at the default font size today:

- New Session "Session name:";
- Update "Skip This Version";
- Update "Manual Update Only".

The rest clip with a desktop font 2 pt larger:

- Find/Replace "Find what:" and "Use regex";
- Go To Line "Line number:";
- Font picker "Search:";
- Shortcuts "Plugins > General > Run All Checks";
- read-only viewer "Go to Page", "Go to Line" and "Next Page";
- Update "View on GitHub".

Two layouts overlap:

- the Permissions message frame runs 3 px into its buttons;
- the Plugin Manager's Official rows put their text frames under the
  "Details" button.

Clipping and overlap are blocking defects (the user's rule).

## In scope

- Widen each clipped label, check box or button to its measured width
  (`label_width` / `max_label_width`, as in T0083 / T0105), and shift only
  the widgets that must move to make room.
- End the Permissions message frame above its buttons.
- Plugin Manager rows: the text frames end left of the row buttons. A long
  name or description is clipped at that edge, as text already is when
  the dialog is narrow (see Notes).
- Empty `KNOWN_LAYOUT_DEFECTS`; the list and its checks are removed.

## Out of scope

- Any change other than geometry: no colors, fonts, frame types or
  wording.
- Dialogs without a defect in the list.

## How to test

```bash
nix shell nixpkgs#xvfb-run -c xvfb-run -a env FERRISPAD_REQUIRE_UI_TESTS=1 \
  nix develop -c cargo test --test ui_dialogs
```

- Red: on T0106 the test passes only because the 50 defects are listed.
  With the list emptied it fails, naming all 50.
- After: it passes with no list.
- Montage (Xvfb, Claude): every affected dialog before / after, at the
  default font size and +2 pt, in light and dark. It is shown to the user
  before review.

## Acceptance criteria

- [ ] `KNOWN_LAYOUT_DEFECTS` is gone, and the layout test passes for every
      dialog in dark and light.
- [ ] Only geometry changed (diff review), and every other dialog is
      pixel-identical (Xvfb).
- [ ] Montage inspected edge by edge; full gates green.

## Affected files

- `src/ui/dialogs/{find,goto_line,font_picker,session_picker,shortcut_dialog,update,readonly_viewer,plugin_permissions,plugin_manager}.rs`.
- `tests/ui_dialogs.rs`: the list removed.

## Notes

- Origin: T0106's review; assigned by the orchestrator on 2026-11-26,
  ahead of T0109, target 0.9.11.
- **The lesson from T0106's CI:** the defects exist because frames had
  fixed widths that fit one font. CI's fonts measure wider (e.g. "Manual
  Update Only" needs 151 px there and 147 here; "Session name:" needs 112
  px there at the default size). So every width touched here is measured
  at build time (`label_width` / `max_label_width`), floored at its old
  value, and the dependent positions and dialog sizes follow it. No
  constant was tuned to this machine.

## What was done (2026-11-26)

### Red

On T0106's code, with `KNOWN_LAYOUT_DEFECTS` emptied, the test fails with
all 50 local defects. These four are clipped at the default size:

- "Session name:";
- "Skip This Version" ×2;
- "Manual Update Only".

CI's run of T0106 reported 70, including "Remind Later", "Open Lines:",
"Install Anyway", "e.g. Ctrl+Shift+L" and the case-folding hint.

### Fixes (geometry only; widths measured at build time, never smaller than before)

- **Find / Replace:**
  - the label column takes the measured "Find what:" / "Replace:" width;
    the inputs, check boxes and hint shift with it;
  - "Use regex" and the case-folding hint are measured;
  - the dialog width follows (406 here, was 400);
  - Close stays right-aligned.
- **Go To Line:** the label is measured (108 here); the input and buttons
  shift; the dialog is 258 wide (was 250).
- **Font picker:** the "Search:" label is measured; the input and the
  Monospace toggle shift.
- **New Session:**
  - the label is measured (117 here); the input, buttons and status line
    shift; the dialog is 317 wide (was 300);
  - `show_*` centres with the built width.
- **Key Shortcuts:**
  - the command column takes the widest command name actually listed
    (242 here, was 220), capped at 440, so a hostile plugin name cannot
    widen the dialog without bound;
  - headers, inputs and Reset buttons shift; the dialog grows only if a
    row no longer fits.
- **Update:**
  - each of the four buttons is sized from every label it can show
    (Install covers "Install Now" / "Manual Update Only");
  - the dialog width follows (588 here, was 500), and `show_*` centres
    with it.
- **Read-only viewer:** "Go to Page", "Go to Line" and Prev/Next Page are
  measured; the toolbar spacers absorb the growth.
- **File Too Large, Plugin Settings, Community Install** (CI-only defects):
  "Open Lines:", the "e.g. Ctrl+Shift+L" hint and "Install Anyway" are
  measured. Here they already fit, so nothing moves locally. On wider
  fonts they grow, and the dialog / button position follows.
- **Permissions:** the content group ends 15 px (`SECTION_GAP`) above the
  buttons (it was 3 px into them). It has no box, so no pixel changes.
- **Plugin Manager rows:**
  - the title and description frames end 8 px left of the row's leftmost
    button (Official / Community rows with Details);
  - test: `row_text_ends_left_of_the_row_buttons`.
- **Plugin Manager, overflow:** text that would not fit is shortened with
  "…" by `fit_text`. It reuses the existing `truncate_text` and removes
  characters until `label_width` fits the frame. Short names are
  unchanged. Descriptions keep their 60-character cap.

`KNOWN_LAYOUT_DEFECTS`, `defect_identity` and its test are removed. The
layout check runs on every dialog with no exceptions.

### Evidence

- `ui_dialogs` passes with no list, locally and on CI (run 45904882138:
  fmt+clippy+test, linux arm64 and windows x64 all green). The defects are
  gone with CI's fonts too.
- **Untouched dialogs stay pixel-identical:** the in-app Xvfb campaign
  (T0106 build vs this branch, dark and light) gives AE = 0 for about,
  large-file warning, file too large, permissions, plugin config,
  settings and manager (Installed and Official). The message popup
  differs by 3–7 px only because the wider Go To Line dialog sits behind
  it.
- **Montage** (offscreen snapshots of each dialog, before = T0106 /
  after = this branch, at the default size and +2 pt, dark and light):
  `ferrispad-wt/uitest-T0110/montage/T0110-<dialog>.png` (local), for
  new-session, update, update-manual, find, replace, goto, font-picker,
  shortcuts, viewer and plugin-manager. Inspected edge by edge: no
  clipping or overlap remains.
- Gates: lib 586 under Xvfb, every binary green; fmt and clippy
  `-D warnings` clean.
