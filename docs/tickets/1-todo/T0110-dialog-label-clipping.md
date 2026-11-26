---
id: T0110
title: Fix clipped labels and overlaps in the dialogs T0106 made testable
status: todo            # todo | review | done
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
