---
id: T0046
title: Make the fallback file chooser legible and drop its path-segment buttons
status: review
created: 2026-10-19
severity: moderate
area: ui
depends-on: []
issue: 94
---

## Goal

When FLTK cannot load the GTK file dialog, File → Open/Save shows FLTK's own
fallback chooser (`Fl_File_Chooser`). In it the selected row is unreadable:
black text on a dark navy (`FL_SELECTION_COLOR`) bar. The row of small
clickable path-segment buttons drawn above the **Filename** input
(`Fl_File_Input`) also looks broken. The user should get a readable open/save
dialog that fits the app theme, whichever chooser FLTK ends up using.

## In scope

- Find out **why** the fallback is used on a GNOME/Fedora desktop where
  `/usr/lib64/libgtk-3.so.0` and `/usr/bin/zenity` are both installed.
  Working hypothesis: the dev binary built in `nix develop` uses Nix's glibc
  loader (`readelf -l` → `/nix/store/...-glibc-2.42/lib/ld-linux-x86-64.so.2`),
  so its `dlopen("libgtk-3.so.0")` cannot see the system libraries. Confirm it with
  `LD_DEBUG=libs`, and check whether the CI-built release binary
  (`FerrisPad-v*-linux-amd64`) shows the native GTK dialog on the same machine.
  Also check whether fltk-rs 1.5.14 offers a zenity/kdialog path
  (`Fl::OPTION_FNFC_USES_ZENITY` and similar) that should have been taken.
- Make the selected row legible in both light and dark themes: set the global
  selection colour (and the colours `Fl_File_Browser` draws with) from the
  active theme, next to the existing `app::foreground(...)` calls in
  `main.rs` / `dispatch.rs`.
- Get rid of the path-segment button strip above **Filename**, or style it so
  it no longer looks broken, using whatever FLTK lets us reach through
  `NativeFileChooser`.
- If the investigation shows the fallback can be avoided in the realistic
  setups (release binary, Flatpak), prefer that, and keep the theming as the
  safety net.

## Out of scope

- Writing our own file dialog widget.
- Pulling in a new crate (`rfd`, `ashpd` / xdg-desktop-portal) without a
  separate decision: weigh binary size and the "single self-contained binary"
  constraint first.
- Re-theming other dialogs (T0024) or panels (T0019).

## Findings (measured 2026-10-19)

### Why the dev build falls back

FLTK picks the backend in the `Fl_Native_File_Chooser` constructor
(`fltk-sys-1.5.14/cfltk/fltk/src/Fl_Native_File_Chooser_GTK.cxx:924-983`):

1. zenity, only if `Fl::option(OPTION_FNFC_USES_ZENITY)` — **default off**
   (`Fl.cxx:2073`, prefs key `UseZenity`, default 0). Probe:
   `popen("zenity --version")`.
2. kdialog, only if `OPTION_FNFC_USES_KDIALOG` — default off.
3. GTK, if `OPTION_FNFC_USES_GTK` (default on) and `probe_for_GTK_libs()`
   succeeds: `Fl_Posix_System_Driver::probe_for_GTK` →
   `quadruple_dlopen("libgtk-3")` (tries `.so`, `.so.2`, `.so.1`, `.so.0`, with
   `RTLD_LAZY | RTLD_GLOBAL`), then `libgtk-x11-2.0`, then `gtk_init_check`.
4. Otherwise `Fl_Native_File_Chooser_FLTK_Driver` → `Fl_File_Chooser`.

The dev binary (`target/debug/FerrisPad`, built in `nix develop`) has the
interpreter `/nix/store/0d8g8n0a…-glibc-2.42-67/lib/ld-linux-x86-64.so.2` and a
RUNPATH of Nix store paths only. Running it under gdb with `LD_DEBUG=libs` and
calling `dlopen` in FLTK's order inside the process:

```
$1 = (void *) 0x0   "libgtk-3.so: cannot open shared object file: No such file or directory"
$3 = (void *) 0x0   "libgtk-3.so.2: cannot open shared object file: …"
$5 = (void *) 0x0   "libgtk-3.so.1: cannot open shared object file: …"
$7 = (void *) 0x0   "libgtk-3.so.0: cannot open shared object file: …"

find library=libgtk-3.so.0 [0]; searching
 search path=/nix/store/…-fontconfig-2.18.2-lib/lib:…:/nix/store/…-dbus-1.16.2-lib/lib   (RUNPATH)
 search path=/nix/store/…-glibc-2.42-67/lib                                         (system search path)
 search cache=/nix/store/…-glibc-2.42-67/etc/ld.so.cache
 search path=/nix/store/…-glibc-2.42-67/lib:/nix/store/…-xgcc-15.3.0-libgcc/lib     (system search path)
```

`/usr/lib64` is never searched: the Nix loader uses its own `ld.so.cache` and
default dirs. So the GTK probe fails, zenity is off by default, and FLTK falls
back. **Confirmed**, not a guess. The `--release` binary built in the same shell
has the same loader, so it falls back too.

The "installed release build" on this machine is the **Flatpak**
(`com.ferrispad.FerrisPad` 0.9.5, runtime `org.freedesktop.Platform//25.08`;
the `ferrispad` shell alias is `flatpak run com.ferrispad.FerrisPad`). Its
binary uses `/lib64/ld-linux-x86-64.so.2`, which inside the sandbox is the
runtime's; `ldconfig -p` in the sandbox resolves
`libgtk-3.so.0 => /usr/lib/x86_64-linux-gnu/libgtk-3.so.0`. That is why it shows
the GTK dialog. The CI-built raw/.deb binary (Ubuntu toolchain, system loader)
reads the system `ld.so.cache`, which lists `/usr/lib64/libgtk-3.so.0` on
Fedora, so it takes the GTK path too.

### Why the selected row was unreadable

`Fl_File_Browser::item_draw` draws a selected row's text with
`fl_contrast(textcolor(), selection_color())`. The list's `selection_color()` is
FLTK's default `FL_SELECTION_COLOR` (navy, `0x000080`) and its `textcolor()` is
the global foreground, which FerrisPad sets to black on light themes. FerrisPad
also lowers the CIELAB contrast level from 39 to 10 (`main.rs`, for the tree
panel's git colours). Black (L\* 0) vs navy (L\* ≈ 11.8) differ by more than 10,
so `fl_contrast` keeps black: black on navy, 1.31:1. Dark themes draw light text
on navy and were readable.

### The path-segment strip

`Fl_File_Input::draw_buttons` draws one box per path segment in the top
`DIR_HEIGHT` pixels, with `down_box()` (`FL_UP_BOX`) in the hard-coded colour
`FL_GRAY`. The boxes carry no labels by design; clicking one jumps to that
folder. `Fl_File_Chooser::fileName` and `fileList` are **private**, and
`Fl_Native_File_Chooser` keeps its `Fl_File_Chooser` private inside the driver,
so neither the strip nor the list colours can be reached through
`NativeFileChooser`. Changing FLTK's global colours instead would also change
check marks and every other window (see `theme.rs` on why the selection colour
is only set per widget).

### fltk-rs `app::Option` is misnumbered

fltk-rs 1.5.14 `app::Option` omits FLTK 1.4's `OPTION_FNFC_USES_ZENITY` (5) and
`OPTION_FNFC_USES_KDIALOG` (6) (`FL/Fl.H:244-295` vs `fltk/src/app/opts.rs`), so
`app::Option::FnfcUsesZenity` is 7 = FLTK's `OPTION_PRINTER_USES_GTK`. The
variant that reaches the zenity option is `app::Option::PrinterUsesGtk` (5). A
unit test pins this so an upstream fix is noticed.

### Flatpak / FlatPark (T0047)

From the FLTK source: the GTK driver creates the dialog with
`gtk_file_chooser_dialog_new` (`GtkFileChooserDialog`, an in-process widget);
the symbols it loads (`GET_SYM` list, `Fl_Native_File_Chooser_GTK.cxx:868-917`)
contain no `gtk_file_chooser_native_*`, and nothing in `cfltk/fltk/src` talks to
`org.freedesktop.portal` (the only "portal" hit is a comment in the zenity
branch). So **FLTK 1.4 never uses the xdg-desktop-portal FileChooser itself.**
Inside a Flatpak on `org.freedesktop.Platform//25.08`, FLTK **does** find GTK3
(checked above), so it shows the in-process GTK3 dialog, which only sees what the
sandbox sees. Today's manifest grants `--filesystem=host`, so that works. If
T0047 narrows the filesystem permission, the in-process dialog can no longer
reach the user's files; the only portal-capable route FLTK offers is zenity
(the runtime ships `/usr/bin/zenity`; FLTK's own comment credits zenity's
"portal integration"), which this change does not turn on where GTK loads.
That trade-off is T0047's to decide.

## What was done

`src/ui/file_dialogs.rs` now picks the backend itself, once, at the first file
dialog (no startup cost, nothing runs while idle):

1. **GTK loadable** (same `dlopen` names and flags as FLTK, from the same
   binary) → `NativeFileChooser` exactly as before. The GTK dialog, and the
   macOS/Windows dialogs, are unchanged; zenity is not even probed.
2. **No GTK, zenity installed** → turn on FLTK's zenity option and use
   `NativeFileChooser`. This is what the Nix dev build now gets on GNOME: the
   desktop's own file dialog via zenity. The start folder gets a trailing `/`
   so zenity opens inside it rather than in its parent.
3. **Neither** → build the same `Fl_File_Chooser` ourselves with fltk-rs
   `dialog::FileChooser` (FLTK's own widget, not a new one) and style it from
   `DialogTheme`, per widget only: file list `input_bg`, selected row
   `button_bg` with the theme text colour (the convention of the session and
   font pickers), labels/inputs/menus themed, and the path-segment strip drawn
   with `FrameType::NoBox` (invisible, still clickable). No FLTK global colour
   changes.

Supporting changes: the three dialog functions take `theme_bg`, so
`FileController::file_save` / `file_save_as` gained a `theme_bg` parameter
(callers in `state.rs` and `dispatch.rs` pass the highlighter's theme
background, as `file_open` already did). The contrast level moved into
`ui::theme::FLTK_CONTRAST_LEVEL` so the test models the level `main.rs` sets.

One `unsafe` block (the `dlopen` probe), with a SAFETY comment; declared with a
local `extern "C"` like `session.rs`'s `kill`, no new crate.

### Red test evidence

`ui::file_dialogs::tests::fallback_selected_row_is_readable_in_every_theme`
models what FLTK draws (`fl_contrast` CIELAB at level 10) and requires WCAG AA
(4.5:1) for every syntax theme. Run against a helper returning today's pair
(`FL_SELECTION_COLOR`, global foreground) before the fix:

```
---- ui::file_dialogs::tests::fallback_selected_row_is_readable_in_every_theme stdout ----
panicked at src/ui/file_dialogs.rs:132:9:
Base16OceanLight: text (0, 0, 0) on selection (0, 0, 128) has contrast 1.31:1
SolarizedLight: text (0, 0, 0) on selection (0, 0, 128) has contrast 1.31:1
InspiredGitHub: text (0, 0, 0) on selection (0, 0, 128) has contrast 1.31:1
test result: FAILED. 0 passed; 1 failed
```

Green after the fix, with three more tests: `backend_prefers_gtk_then_zenity_then_themed`
(GTK wins and zenity is not probed; zenity next; themed last),
`zenity_option_has_fltks_number`, and `zenity_folder_ends_with_one_slash`.

Also checked by hand with a scratch probe that calls `native_open_dialog` in the
dev build: with `PATH` intact, FLTK ran
`zenity --title 'Open File' --file-selection --filename='/tmp/'`; with
`PATH=/nonexistent` the themed chooser opened, and window captures in a dark
(`#282c34`) and a light (white) theme showed a readable selected row and no
segment strip.

## How to test

### Automated

```bash
nix develop -c cargo test --lib file_dialogs
```

Expect 4 tests to pass. Full gates:

```bash
nix develop -c cargo fmt --check
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo test
```

### Manual (UI-only; manual-only, the dialogs cannot be driven from tests)

A. Dev build uses the desktop dialog through zenity:

1. `nix develop -c cargo run`
2. File → Open. Expect the GNOME file dialog (zenity), opening in the last
   folder you opened from, not the old FLTK chooser.
3. Pick a file → it opens in a tab. File → Save As → the GNOME save dialog;
   save → the file is written where you chose.

B. The themed fallback (simulate a machine with neither GTK nor zenity):

1. `nix develop -c cargo build`
2. `PATH=/nonexistent ./target/debug/FerrisPad` (the dev binary cannot load
   GTK, and with no `PATH` it cannot find zenity).
3. View → dark mode on. File → Open, click any file. Expect the selected row in
   a lighter/darker shade of the dialog colours with light text, clearly
   readable; the list, labels and **Filename** field follow the dark theme; no
   empty button strip above **Filename**. Clicking just above the path text
   still jumps to that folder.
4. View → dark mode off (a light syntax theme, e.g. InspiredGitHub). Repeat:
   expect black text on a grey selection bar, not black on navy.
5. File → Save As: same look; type a name and Save → the file is written.
   Cancel → nothing happens.

C. Installed Flatpak (`ferrispad`): File → Open still shows the GTK dialog,
unchanged.

## Acceptance criteria

- [x] Root cause of the fallback on the dev build is written down here,
      confirmed, not guessed.
- [x] Selected row is readable in the fallback chooser in light and dark themes.
- [x] The path-segment strip is gone or no longer looks broken.
- [x] The native GTK dialog, where it loads, is unchanged.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.

## Affected files

- `src/ui/file_dialogs.rs` — backend choice, zenity option, themed fallback, tests
- `src/ui/theme.rs` — `FLTK_CONTRAST_LEVEL`
- `src/main.rs` — uses `FLTK_CONTRAST_LEVEL`
- `src/app/controllers/file.rs` — `theme_bg` passed to the dialogs
- `src/app/state.rs`, `src/dispatch.rs` — pass `theme_bg` to `file_save` / `file_save_as`

## Notes

- **Partly confirmed (2026-10-19):** the system-installed FerrisPad (official
  release build) opens the native GTK dialog on the same machine. Only the dev
  binary from `nix develop` falls back, which supports the Nix-loader hypothesis.
  So the fallback is a safety net for setups without GTK, not something every
  Fedora user sees. Now confirmed with `LD_DEBUG=libs` (see Findings); the
  installed build is the Flatpak.
- Deviation from "In scope": the selection colour is **not** set globally. The
  global `FL_SELECTION_COLOR` also colours check marks under the gtk+ scheme
  (`Fl_Light_Button::draw`) and every other window, and the fallback's widgets
  are private behind `NativeFileChooser`. Building the `Fl_File_Chooser`
  ourselves (only when no native dialog exists) styles exactly that dialog.
- Not covered: a system where GTK loads but `gtk_init_check` fails would still
  get FLTK's unstyled fallback (FLTK decides that inside its probe).
- Possible follow-up: report the `app::Option` numbering to fltk-rs.
- Scheduling: the user wants T0046 and T0047 done **after** all the other open
  tickets.
- Origin: the user's manual testing of the T0033–T0045 review batch,
  2026-10-19 (not caused by that batch: `file_dialogs.rs` was not touched).
- Matters for packaging too (T0047, FlatPark): see Findings → Flatpak.
