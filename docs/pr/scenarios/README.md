# Screenshot scenarios

A scenario drives FerrisPad for the screenshots of a PR page. The `new-pr`
skill runs scenarios with two tools:

- **`.claude/skills/new-pr/shoot.sh`** runs one scenario on one binary.
- **`.claude/skills/new-pr/compare.sh`** runs one scenario on a base ref
  and on the branch, light and dark, and writes labelled `before | after`
  montages.

Both start the app in an isolated Xvfb, with a private D-Bus, no
compositor, shimmed openers and throwaway config/data dirs. See the
header of `shoot.sh`. Never drive FerrisPad with a hand-written script.

```bash
.claude/skills/new-pr/compare.sh docs/pr/scenarios/start-page.sh \
    ~/Code/personal/ferrispad-wt/uitest-T0114 --before 0.9.10-rc.2
```

## Format

A scenario is a bash file that `shoot.sh` sources:

```bash
# What this scenario shows, and for which ticket.
SETTINGS_EXTRA='"word_wrap_enabled": true'  # optional: extra settings.json members
OPEN_FILES=(hello.rs)                        # optional: fixtures opened on the command line

scenario() {
  shot editor                  # the main window
  menu View                    # open a menu of the menu bar
  shot view-menu
  key Escape
  key ctrl+f                   # a shortcut
  shot find-dialog 'Find'      # a dialog, matched by its window title
}
```

`shoot.sh` gives every run a fresh `settings.json` with:

- the theme (`--theme light|dark`);
- the editor font (`--font N`, default 16);
- the window size (`--size WxH`, default 1100x740) at position 0,0;
- tabs on;
- update checks off.

`SETTINGS_EXTRA` adds members to that file.

### The helpers

Coordinates are relative to the main window.

| Helper | What it does |
|---|---|
| `click X Y`, `dclick X Y` | Click, or double-click. FLTK needs the pointer to arrive first, so the helper moves the pointer, pauses, then clicks. |
| `key KEYS…` | Press xdotool key names: `ctrl+t`, `Escape`, `Return`, `Down`. It refuses `ctrl+o`, `ctrl+s` and `ctrl+shift+s`. |
| `type "text"` | Type into the focused widget. |
| `menu File\|Edit\|View\|Format\|Plugins\|Help` | Open a menu. There is no window manager, so Alt shortcuts do not open menus. |
| `wait_idle [S]` | Wait until the app uses no CPU. Every helper already does this. |
| `shot NAME [REGEX]` | PNG of the main window, or of the window whose title matches `REGEX`. At most 1600 px wide. |
| `shot_screen NAME` | PNG of the whole virtual screen. |

### Rules

- **Never open a file dialog**, and never click File → Open…, Save As…,
  the start page's Open File and Wiki buttons, or Details links. Open
  fixtures with `OPEN_FILES`. File dialogs and browsers are what escaped
  to the user's desktop on 2026-11-27, and the shims now make them fail
  the run.
- **Fixtures only.** Use the files in `fixtures/`, written for screenshots.
  Never use a user's real file, setting or session.
- **Look at every shot**, edge by edge. Clipping, overflow and overlap are
  blocking.

## The scenarios here

| Scenario | Shows | Model for |
|---|---|---|
| `start-page.sh` | the start page of a fresh install | a page with no interaction (T0114) |
| `editor.sh` | `fixtures/hello.rs` in the editor, the View menu, the Find dialog | the editor, a menu, a dialog |
| `tab-labels.sh` | the file `fixtures/Untitled` next to a new untitled tab (T0112) | the tab bar after a shortcut |

Commit a ticket's scenario with its PR. The next change to that UI can
then be compared against it.
