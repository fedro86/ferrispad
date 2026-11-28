---
id: T0110
title: Fix clipped labels and overlaps in the dialogs T0106 made testable
status: done             # todo | review | done
created: 2026-11-26
severity: minor
area: ui
release: 0.9.11         # the planned release, X.Y.Z (e.g. 0.9.11); new-pr's page folder
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

- [x] `KNOWN_LAYOUT_DEFECTS` is gone, and the layout test passes for every
      dialog in dark and light.
- [x] Only geometry changed (diff review), and every other dialog is
      pixel-identical (Xvfb).
- [x] Montage inspected edge by edge; full gates green.

## User verification

The user approved the PR on 2026-11-28, relayed by the orchestrator
(ferrispad-03), with the widths as proposed: Update 588, New Session 317,
Go To Line 258, Find/Replace 406. They read the presentation
`docs/pr/0.9.11/PR-212-dialog-label-clipping.html`. The reviewer
(ferrispad-90) approved 208dbe4, and 77fbfb3 only changed the page's size
chip. T0106 landed as 9941c6e (#208). b580aab merged master: the conflicts
came only from T0106's squash, and master's code equals T0106's head
49f8fc3, which this branch already held. So the code files were resolved
to this branch's side, and the stale `2-review/` copy of T0106's ticket
was removed. The diff against master is now this ticket alone: 14 code
files plus the page and the ticket.

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

## Merge of master, rebuilt on T0106 (2026-11-28, ferrispad-39)

Master has T0105 as one squash commit (#194), while this branch carries
T0105's original commits through T0106. A plain merge of master conflicted
in 20 files. The tree was rebuilt in two clean steps instead:
1. `origin/ticket/T0106` (6f8e505, which already holds ferrispad-3b's
   resolution of master at 818970b) merged with master f12fd3c: no
   conflict;
2. this ticket's own delta (`git diff 3f53937 dadf622`) replayed on that
   with `git apply -3`: no conflict.

The check: the replayed delta (T0106+master → result) is **line-identical**
to the original one (3f53937 → dadf622). Both are 14 files, +549 / −205,
and `diff` of their +/− lines is empty. The result is this branch's merge
of master. So T0110 depends on T0106 (`depends-on: [T0106]`): #212 lands
after #208.

Gates on the result, with the UI tests on a private Xvfb (T0122's
environment): 790 passed, clippy `-D warnings` and fmt clean.

### The commits after the rebuild

- **afe13dd:** the rebuilt merge of master f12fd3c above. It also holds
  the Permissions description fix (see *Found by ferrispad-3b* below).
  `fit_text` and `truncate_text` moved to `dialogs/mod.rs`.
- **330c9d4:** merge of master 918471d (T0122), no conflict.
- **e0b907e:** merge of master 62669e9 (T0121, T0120), no conflict.
- **b9fe3d4:** merge of `origin/ticket/T0106` 49f8fc3 (its master merge and
  the #208 page). The six conflicts came only from the old merge base, so
  they were resolved to this branch's side. The check: every file that
  differs from T0106's head is either newer on master or touched by this
  ticket. The diff against T0106 in `src/` and `tests/` is this ticket's
  delta plus the Permissions fix (14 files, +405 / −215). ferrispad-3b's
  scenario files now come from T0106; the stale untracked copies in this
  worktree were set aside.
- **bb51140:** restores `docs/pr/scenarios/plugins/tree-demo/` (T0104, on
  master), which afe13dd had dropped. After it, no file of master is
  missing (`git diff --diff-filter=D origin/master HEAD` is empty).

Every commit was gated by the pre-commit hook through `scripts/gates.sh`
(T0122): tests on a private Xvfb, clippy `-D warnings`, fmt.

### Screens for the page (compare.sh, before = origin/ticket/T0106)

The scenarios are ferrispad-3b's (`docs/pr/scenarios/dialogs*.sh`), copied
byte-identical. Runs used T0120's fixed `compare.sh` / `shoot.sh`, with
fonts 16 and 18, light and dark.
- **Changed** (geometry only): Find, Replace, Go To Line, Font picker, New
  Session, Key Shortcuts, the read-only viewer; and Permissions, see
  below.
- **Pixel-identical** before and after: About, the Go To Line message,
  Plugin Config, Plugin Manager (Installed and Official), Plugin Settings,
  Large File Warning, File Too Large. They were also inspected edge by
  edge after the find below: identical does not mean clean.
- **Font 18 = font 16** for every dialog except the Font picker, which
  shows the chosen size. The app's font setting is the editor's. Dialogs
  use FLTK's default size (`app::set_font_size` is never called).
- **The Update dialog** only appears after a successful network check, and
  the scenarios run without network. It is an **offscreen render of its
  build seam** (`update::build_update_available_dialog`): before on T0106,
  after on T0110, at FLTK's default label size and at +2 pt (the layout
  test's stand-in for a wider desktop font), light and dark, under the
  gates' isolation. The orchestrator approved this on 2026-11-28. The
  render tool was a local test file, not committed and removed after use.
  Width 500 → 588 px. At 500, "Manual Update Only" is cut, and at +2 pt
  the buttons touch.
- Found on the way: `_dialog-helpers.sh`'s `gen_fixture` (`yes | head`)
  failed on its first run under `set -euo pipefail` (SIGPIPE). Reported to
  ferrispad-3b, who fixed it in T0106's copy.

### The user's decision on the dialog widths (2026-11-28)

The measured widths make some dialogs wider. Update grows 500 → 588 px
here, New Session 300 → 317, Go To Line 250 → 258 and Find 400 → 406.
CI's fonts are wider still. The alternative is shorter labels, a wording
change this ticket keeps out of scope. The page showed each dialog before
and after. **The user approved the widths as proposed** (2026-11-28,
relayed by the orchestrator).

### Found by ferrispad-3b: Permissions clips a long plugin description

`dialogs-permission.sh` showed the fixture's description cut at the
window's right edge: "…settings and a command p", in light and dark. It
was the same on master and on T0106, so it is pre-existing. The layout test
missed it because it built the dialog with a short description. A plugin's
description is free text of any length.

- **Red:** `tests/ui_dialogs.rs` also builds the dialog with a long
  description (`permissions, long description`). Before the fix:
  ```text
  permissions, long description dark/…: "Screenshot fixture: a plugin with a menu item, settings and a command permission": label needs 440px at +0pt, widget is 370px
  (and at +2 pt, and in light)
  test new_dialog_windows_build ... FAILED
  ```
- **Fix:** the description is cut with "..." to its frame (`fit_text`),
  with the full text as a tooltip.
- **One helper:** `truncate_text` and `fit_text` moved from the Plugin
  Manager to `dialogs/mod.rs`. The Plugin Manager rows use them as before.
- After: `ui_dialogs` passes. In the app (`dialogs-permission.sh`), the
  description ends in "settings and..." inside the window, in light and
  dark. Gates: 790 passed.
