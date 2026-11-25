---
id: T0077
title: Remember the main window's size, position and maximized state across restarts
status: todo            # todo | review | done
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
small on restart. The window should reopen the way the user left it.

## In scope

- Save width, height, position and maximized state to `settings.json` when the
  window closes.
- Restore them on startup, within the platform limits below:
  - **macOS and Windows:** full restore of size, position and maximized state
    is feasible and required.
  - **Linux with Wayland:** the client cannot freely position its own window,
    because the compositor decides where it goes, so restoring the position is
    not guaranteed. Size and maximized state must still be restored. The
    position is still saved, but only applied where the system allows it (for
    example on X11), and silently ignored elsewhere, with no errors or
    warnings.
- Move the window back onto a visible screen if the restored geometry would
  otherwise end up off-screen.
- Keep 640×480 as the default for a fresh install and for an older
  `settings.json` without these fields.

## Out of scope

- Per-session or per-window geometry.
- Any workaround specific to a single Wayland compositor.

## How to test

### Unit / integration tests

Add these (names are the ones the reviewer will run):

```bash
nix develop -c cargo test --test settings_persistence window_geometry
```

- `test_window_geometry_roundtrip` (in `tests/settings_persistence.rs`):
  set width, height, x, y and maximized on `AppSettings`, serialize to JSON,
  deserialize, and assert every field is identical.
- `test_window_geometry_backward_compat` (same file): deserialize a
  `settings.json` string with none of the new fields; it loads without error
  and yields the 640×480 default and `maximized == false`.
- A unit test next to the clamping helper (e.g.
  `window_geometry_clamped_onto_screen`): given a saved rect that lies wholly
  or partly outside a supplied screen work area, the helper returns a rect
  that is fully visible on that screen; a rect already on-screen is returned
  unchanged. Keep the helper pure (screen rects passed in) so it is testable
  without FLTK.

All three pass after the change; the full `nix develop -c cargo test` stays
green.

### Manual check (run on every available platform; record results here)

For Linux, record whether the session was **Wayland or X11**
(`echo $XDG_SESSION_TYPE`).

1. Start from a clean config (move `~/.config/ferrispad/settings.json` aside,
   or the platform equivalent) and run `nix develop -c cargo run`.
   Expect: the window opens at 640×480.
2. Resize the window to roughly 1100×750, move it to a clearly different spot,
   then close FerrisPad and run it again.
   - macOS / Windows / Linux X11: expect the same size and position, within a
     few pixels.
   - Linux Wayland: expect the same size; the position may be chosen by the
     compositor — this is not a defect. Expect no error or warning on stderr.
3. Maximize the window, close FerrisPad, run it again. Expect: it reopens
   maximized (all platforms). Un-maximize: it returns to a sensible normal
   size, not 0×0.
4. Off-screen: close FerrisPad, edit `settings.json` so the saved x/y are far
   outside any monitor (e.g. `x: 20000, y: 20000`), run again. Expect: the
   window appears fully on a visible screen. (Alternatively, on a multi-monitor
   setup, close the window on a secondary monitor, disconnect it, reopen.)
5. Backward compatibility: restore the old `settings.json` from step 1 (no
   geometry fields) and run. Expect: it loads with no error and the window is
   640×480; all other settings are preserved.

## Acceptance criteria

- [ ] On macOS and Windows, after resizing, moving, closing and reopening, the
      window has the same size and position, within a few pixels of
      tolerance.
- [ ] On Linux with Wayland, after resizing, closing and reopening, the window
      has the same size; the position may be chosen by the compositor, and
      this is not a defect.
- [ ] On all platforms, a window closed maximized reopens maximized.
- [ ] A `settings.json` without the new fields loads without errors.
- [ ] Off-screen geometry is corrected.
- [ ] A test covers saving and reloading the geometry.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features` and
      `cargo fmt --check` pass with zero warnings.
- [ ] The ticket documents the manual check on each available platform,
      stating for Linux whether the session was Wayland or X11.

## Affected files

- `src/ui/main_window.rs:134` — `Window::new(100, 100, 640, 480, title)`:
  take the restored geometry instead of the hard-coded rect (and the matching
  `Flex::new(0, 0, 640, 480, …)` at line 156).
- `src/app/domain/settings.rs:176` — `AppSettings`: new geometry fields with
  `#[serde(default)]` (or a defaulted nested struct) so old files still load.
- `src/main.rs:210`, `:239` — settings are loaded before
  `build_main_window`; pass the geometry through and apply maximized state
  after `show()`.
- `src/app/state.rs:703` (`file_quit`) / `src/dispatch.rs:85`
  (`FileQuit | WindowClose`) — capture geometry and maximized state and save
  settings on close.
- `tests/settings_persistence.rs` — roundtrip and backward-compat tests.

## Notes

- Origin: user request, GitHub issue #151.
- Wayland position handling is a platform limitation, not a bug: save the
  position everywhere, apply it only where the windowing system honours it,
  and never log a warning about it.
- Saving happens once, on close — no polling or resize-event timer (0% CPU
  idle rule). Writing on every resize event is not required.
- When the window is maximized, save the *normal* (un-maximized) size and
  position too, so un-maximizing after restore lands somewhere sensible.
