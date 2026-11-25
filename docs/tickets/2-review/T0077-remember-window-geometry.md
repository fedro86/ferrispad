---
id: T0077
title: Remember the main window's size, position and maximized state across restarts
status: review          # todo | review | done
created: 2026-11-25
severity: ~             # severe | moderate | minor   (for audit-derived tickets)
area: ui                # security | robustness | ui | process
depends-on: []          # other ticket IDs that must land first
issue: 151              # GitHub issue number
---

## Goal

FerrisPad always starts at 640×480 because `src/ui/main_window.rs` hard-codes
`Window::new(100, 100, 640, 480)`. `AppSettings` does not persist window
geometry, so even if the window was closed maximized or resized, it comes back
small on restart (reported on Windows 11). The window should reopen the way the
user left it.

## In scope

- Save the *normal* (un-maximized) width, height and position plus the
  maximized flag when the window closes; restore them on startup.
  - **macOS, Windows, Linux X11:** full restore of size, position and
    maximized state.
  - **Linux Wayland:** the compositor places windows, so the position is
    neither read nor applied there. Size and maximized state are still
    restored. A position saved under X11 is kept untouched (not overwritten
    with Wayland's meaningless `0,0`). No errors or warnings.
- Treat the stored geometry as untrusted input (it is hand-editable): validate
  and clamp it with a pure helper before use — see *Edge cases*.
- Save safely on close: re-read `settings.json`, change only the geometry
  fields, and write it atomically (temp file + rename). If the file on disk
  does not parse, do not write at all.
- Keep 640×480 as the default for a fresh install, for an older
  `settings.json` without these fields, and whenever the stored geometry is
  rejected.

## Out of scope

- Per-session or per-window geometry; tree/terminal panel sizes.
- Any workaround specific to a single Wayland compositor.
- Saving geometry on the update-and-restart path (`src/ui/dialogs/update.rs:261`
  calls `app::quit()` directly). That restart keeps the previously saved
  geometry, which is acceptable.
- Fixing the pre-existing "parse failure → defaults" behaviour of
  `AppSettings::load` in general. This ticket only makes sure the new
  close-time save never turns that into data loss (see *Edge cases*).

## Edge cases (each one is covered by a test or a manual step below)

1. **Minimized at close.** Closing from the taskbar while iconified: Windows
   reports the window at about `-32000,-32000`. If the window is minimized
   (not visible) when it closes, do not save its geometry; keep the stored
   value.
2. **Closed while maximized.** `w()`/`h()` then return the maximized size.
   Track the last normal rect in memory from the window's `resize_callback`
   while `!maximize_active()`. This is event-driven, not a timer, and writes
   nothing to disk. Save that rect plus `maximized = true`. On restore, create
   the window at the normal rect, then call `maximize()` after `show()`, so
   un-maximizing lands on the normal rect, never 0×0 or full-screen-sized.
3. **Negative coordinates are valid.** A monitor left of or above the primary
   one has negative x/y. The clamp helper must not reject negatives. It checks
   the rect against the work areas of the available screens.
4. **Off-screen.** The monitor was disconnected, or the resolution changed.
   - If the rect does not sufficiently overlap any screen's work area, move it
     onto the screen with the largest overlap, or screen 0 if there is none.
   - The title bar must stay grabbable: FLTK's `y` is the client area, so
     account for `decorated_h() - h()` above it.
5. **Too big / too small / nonsense.** A size larger than the target work area
   is shrunk to fit. A width or height below a minimum (e.g. 200×150), zero or
   negative falls back to 640×480.
6. **Malformed field types.** E.g. `"width": "abc"`, `1e20`, `null`. A plain
   `#[serde(default)]` field of the wrong type makes `serde_json` reject the
   **whole** `AppSettings`, and `load()` then resets *every* setting to
   defaults.
   - Store the geometry as one nested `Option<WindowGeometry>` (i32 fields +
     bool), deserialized leniently: any error in that object yields `None`, and
     the rest of the settings load normally.
7. **Multiple instances.** FerrisPad has no single-instance lock. If the close
   path saved the in-memory `AppSettings`, closing instance A would revert any
   setting changed in instance B since A started.
   - The close-time save must therefore re-read the file and patch only the
     geometry (in-scope bullet above). For geometry itself, the last window
     to close wins.
8. **Corrupt or hand-broken `settings.json`.** `load()` falls back to defaults
   on a parse error. A blind save on close would then overwrite the user's file
   with defaults.
   - Re-reading at close and skipping the write when the file does not parse
     prevents this.
   - Atomic write: a crash or power loss mid-write must never leave a
     truncated file.
9. **Quit cancelled.** `FileQuit`/`WindowClose` can be cancelled in the
   unsaved-changes prompt. Save geometry only when `file_quit()` returns true.
10. **HiDPI / per-monitor scaling (Windows).** Values are stored in FLTK units.
    If the scale changes between runs, the clamp in (4)/(5) keeps the window
    on-screen. An exact pixel match is not required.
11. **Wayland detection.** fltk-rs keeps `using_wayland()` crate-private.
    Detect the backend the way FLTK chooses it (`FLTK_BACKEND`, then
    `WAYLAND_DISPLAY`) in a small, testable function that takes the env values
    as parameters.

## How to test

### Unit / integration tests

```bash
nix develop -c cargo test window_geometry
```

To make the save path testable without touching the real config dir, add
path-parameterised `AppSettings::load_from(&Path)` / `save_to(&Path)` and let
`load()` / `save()` delegate to them.

In `tests/settings_persistence.rs` (tempdir-based, like the existing tests):

- `test_window_geometry_roundtrip`: set width, height, negative x/y and
  `maximized = true`; serialize, deserialize, assert identical.
- `test_window_geometry_backward_compat`: a `settings.json` without the field
  loads, and the geometry is `None` → 640×480, not maximized.
- `test_window_geometry_malformed_keeps_other_settings`: a file with
  `"window": {"width": "abc", ...}` (and one with `"width": 1e20`) loads; the
  geometry is `None`, **and a non-default unrelated setting in the same file
  (e.g. `font_size`) is preserved**. This test must fail against a naive
  `#[serde(default)]` implementation.
- `test_window_geometry_save_patches_only_geometry`: write a settings file
  with `font_size = 20`; call the close-time save with an in-memory
  `AppSettings` whose `font_size = 12` and a new geometry. The file now has the
  new geometry **and still `font_size = 20`**. This covers edge case 7.
- `test_window_geometry_save_skips_unparseable_file`: the target file contains
  invalid JSON; the close-time save leaves its bytes unchanged. This covers
  edge case 8.

Unit tests next to the pure helpers:

- `window_geometry_clamped_onto_screen` — table-driven, screens passed in as
  rects, no FLTK:
  - on-screen rect → unchanged;
  - rect on a secondary monitor at negative x → unchanged;
  - rect far off-screen (20000,20000) → moved fully onto screen 0;
  - rect partly off the right edge → shifted inside;
  - rect larger than the work area → shrunk to fit;
  - width/height 0, negative or below the minimum → 640×480 default;
  - title-bar offset: y = work-area top → client y pushed down by the
    decoration height.
- `window_geometry_ignored_when_minimized` — the "what to save" decision
  (pure function of visible/maximized/current/last-normal rect):
  - minimized → keep the stored value;
  - maximized → last normal rect + `maximized = true`;
  - normal → current rect.
- `window_geometry_backend_detection` — `FLTK_BACKEND=x11` with
  `WAYLAND_DISPLAY` set → X11; `WAYLAND_DISPLAY` set alone → Wayland; neither
  → X11; on Wayland the decision keeps the previously stored x/y.

All pass after the change; the full `nix develop -c cargo test` stays green.

### Manual check (run on every available platform; record results here)

For Linux, record whether the session was **Wayland or X11**
(`echo $XDG_SESSION_TYPE`).

1. Clean config (move `~/.config/ferrispad/settings.json` aside, or the
   platform equivalent), `nix develop -c cargo run`. Expect 640×480.
2. Resize to roughly 1100×750, move it, close, run again.
   - macOS / Windows / X11: same size and position, within a few pixels.
   - Wayland: same size; the compositor picks the position. No stderr output.
3. Maximize, close, run again → reopens maximized. Un-maximize → returns to
   the step-2 size, not 0×0 or screen-sized.
4. Minimize, close it from the taskbar or dock, run again → reopens at the
   step-2/3 geometry, not off-screen.
5. Edit `settings.json` to `x: 20000, y: 20000` → appears fully on a visible
   screen, title bar reachable. Then set `"width": "abc"` → starts at 640×480
   **and all other settings (font, theme…) are kept**.
6. Two instances: start A, start B, change the font in B, close B, close A,
   run again → the font from B is still set.
7. Multi-monitor (if available): close on a secondary monitor (including one
   left of the primary), run again → same monitor. Disconnect it, run again →
   on the primary.
8. Backward compatibility: restore the old `settings.json` from step 1 → loads
   with no error, 640×480, other settings preserved.

## Acceptance criteria

- [ ] On macOS, Windows and X11 the window reopens with the same size and
      position (within a few pixels) after resize, move and close.
- [ ] On Wayland the size and maximized state are restored. The position is
      left to the compositor, and a stored X11 position is not overwritten.
- [ ] A window closed maximized reopens maximized and un-maximizes to its
      normal size. A window closed minimized does not corrupt the geometry.
- [ ] Off-screen, oversized, undersized or malformed geometry is corrected or
      ignored without affecting any other setting.
- [ ] Closing never reverts settings changed by another instance and never
      overwrites an unparseable `settings.json`. The write is atomic.
- [x] All tests listed above exist and pass; the malformed-field and
      patch-only-geometry tests were seen failing before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.
- [ ] The ticket records the manual check per available platform (Wayland or
      X11 stated for Linux).

## What was actually done

- `src/app/domain/settings.rs`: `WindowGeometry { x, y, width, height,
  maximized }` and `AppSettings::window: Option<WindowGeometry>`, read by a
  lenient deserializer (anything but a well-formed object → `None`, other
  settings untouched; also rejects the array form serde would otherwise
  accept) and omitted from the JSON while `None`. `load()`/`save()` delegate
  to new `load_from(&Path)`/`save_to(&Path)`; `save_to` now writes atomically
  (temp file `.<name>.<pid>.tmp` in the same dir, `sync_all`, `rename`; temp
  removed on failure). `save_window_geometry_to(&Path, WindowGeometry)`
  re-reads the file, replaces only its `"window"` key on the parsed JSON value
  (unknown keys are kept too), and writes atomically. Invalid JSON or a
  non-object → error, nothing written. Missing file → written from the
  in-memory settings.
- `src/app/services/window_geometry.rs` (new, pure, no FLTK):
  `clamp_onto_screens` (min size 200×150 else 640×480; picks the screen with
  the largest overlap of the *decorated* rect; no overlap → centred on screen
  0; shrink to fit; shift fully inside the work area with the title bar below
  its top edge; i64 arithmetic, no `clamp()` panics), `restore_geometry`,
  `geometry_to_save` (the close decision), `NormalRectHistory` +
  `WindowGeometryTracker`, `is_wayland_backend(FLTK_BACKEND, WAYLAND_DISPLAY)`
  / `running_on_wayland()`.
- X11 detail beyond the spec: the `_NET_WM_STATE` maximized flag can arrive
  *after* the maximizing resize, so that resize is recorded as "normal". The
  history keeps the previous size: if the latest record has the maximized size,
  the previous one is used (test `window_geometry_tracker_late_x11_maximize_flag`).
- `src/ui/main_window.rs`: `build_main_window` takes the validated rect;
  `screen_work_areas()`, `install_geometry_tracking()` (window
  `resize_callback`, records only while `visible() && !maximize_active()`),
  `capture_close_geometry()`.
- Close path: the window's `Event::Close` handler hides the window *before*
  `WindowClose` reaches `file_quit()`, so the snapshot (visible / maximized /
  rect) is taken in that handler, before `hide()`. `FileQuit` (menu/shortcut)
  captures in `dispatch::handle_file`. `AppState::save_window_geometry()` runs
  only when `file_quit()` returns true.
- `src/main.rs`: restore at startup (clamped with a 32-unit title-bar
  estimate); after `show()` re-clamp with the real `decorated_h() - h()` when
  it is taller (not on Wayland); `maximize()` after `show()`.
- Wayland: no warning is printed; x/y of the saved geometry come from the
  stored value (or the 100,100 default), never from Wayland's 0,0.
- Known approximation (as specified in edge case 11): FLTK falls back to
  probing the default `wayland-0` socket when `WAYLAND_DISPLAY` is unset; the
  env check then says X11. Only effect: x/y would be saved from that session.

### Red run before the fix (naive `#[serde(default)]` + whole-struct save)

```
$ nix develop -c cargo test --test settings_persistence window_geometry
---- test_window_geometry_malformed_keeps_other_settings stdout ----
Failed to parse settings: invalid type: string "abc", expected i32 at line 1 column 84. Using defaults.
assertion `left == right` failed: font_size lost for "window": {"x": 10, "y": 10, "width": "abc", ...}
  left: 16
 right: 22
---- test_window_geometry_save_patches_only_geometry stdout ----
assertion `left == right` failed: close-time save reverted another instance's setting
  left: 12
 right: 20
---- test_window_geometry_save_skips_unparseable_file stdout ----
assertion `left == right` failed   (file bytes overwritten with a full default settings.json)
test result: FAILED. 3 passed; 3 failed
```

After the fix: `nix develop -c cargo test window_geometry` → 6 unit + 6
integration tests pass; full `cargo test`, clippy `-D warnings` and
`fmt --check` green.

### Verification recipe

```bash
nix develop -c cargo test window_geometry          # 12 tests
nix develop -c cargo test --test settings_persistence
```

Then the manual steps under *How to test → Manual check*, with an isolated
config dir so the real settings stay untouched on Linux:

```bash
mkdir -p /tmp/fp-cfg && XDG_CONFIG_HOME=/tmp/fp-cfg nix develop -c cargo run
cat /tmp/fp-cfg/ferrispad/settings.json   # "window": {...} after a close
FLTK_BACKEND=x11 XDG_CONFIG_HOME=/tmp/fp-cfg nix develop -c cargo run   # X11 path
```

### Manual check results

- **Linux, Wayland session (GNOME, `XDG_SESSION_TYPE=wayland`)** — automated
  smoke only: started with an isolated config holding
  `{"x":20000,"y":20000,"width":1100,"height":750,"maximized":true}` and
  `font_size: 20`; it ran 8 s with no panic and no stderr output about
  geometry. Same with `FLTK_BACKEND=x11` (XWayland). Closing the window was not
  exercised (the process was killed by `timeout`).
- Steps 1–8 on Linux Wayland and X11: **pending user verification**.
- Windows 11 (the original report): **pending user verification**.
- macOS: **pending user verification**.

## Affected files

- `src/ui/main_window.rs:134` — `Window::new(100, 100, 640, 480, title)` and
  the matching `Flex::new(0, 0, 640, 480, …)` at line 156: take the validated
  geometry; register a `resize_callback` that records the last normal rect.
- `src/app/domain/settings.rs:176` — `AppSettings`: `Option<WindowGeometry>`
  with a lenient deserializer; `load_from` / `save_to`; atomic write in
  `save_to` (temp file in the same dir + `rename`).
- `src/main.rs:210`, `:239` — settings are loaded before `build_main_window`:
  pass the geometry through and call `maximize()` after `show()`.
- `src/app/state.rs:703` (`file_quit`) / `src/dispatch.rs:85`
  (`FileQuit | WindowClose`) — on a confirmed quit, run the close-time
  "re-read, patch geometry, atomic save" path.
- `tests/settings_persistence.rs` — integration tests above.

## Notes

- Origin: user request, GitHub issue #151.
- fltk-rs 1.5 API: `maximize()`, `un_maximize()`, `maximize_active()` (there
  is no `is_maximized`); `app::screen_count()`, `app::screen_work_area(n)`,
  `app::screen_num_inside(rect)`; `decorated_h()` for the title-bar offset.
- Wayland position handling is a platform limitation, not a bug: never log a
  warning about it.
- Saving happens once, on a confirmed close. No polling, no timer, no disk
  write on resize (0% CPU idle rule).
- Restore order: create the window at the normal rect → `show()` → `maximize()`.
  A brief flash at the normal size before maximizing is acceptable.
