---
id: T0053
title: Theme the large-file loading progress dialog
status: review
created: 2026-10-19
severity: minor
area: ui
depends-on: [T0024, T0039]
issue: 78
---

## Goal

The "Loading File" progress dialog shown while a large file streams in is the
last unthemed modal. It still shows FLTK's default grey look, including the
Cancel button added by T0039. It should match every other dialog in both themes.

## In scope

- Pass the theme into `load_to_buffer_with_progress`
  (`src/ui/dialogs/large_file.rs`). Style the window, labels, progress bar and
  Cancel button with `DialogTheme` and the `ThemeRgb` API (T0030). Use the
  same secondary-button look as other Cancel buttons
  (`DialogTheme::secondary_button_bg`).
- Keep its own polling loop and the `cancel_stream` single exit. It stays the
  documented exception to `run_dialog` (T0024), so only its colours change, not
  its event loop.

## Out of scope

- Making the dialog non-modal, or quitting while it is up (T0039 findings).

## What was done

- `load_to_buffer_with_progress(path, size, theme_bg)` now takes the theme
  background, like `show_large_file_warning` and `show_file_too_large_dialog`
  in the same file. Its only caller, `FileController::open_file`
  (`src/app/controllers/file.rs`), already had `theme_bg` and passes it through.
- The dialog builds a `DialogTheme::from_theme_bg(theme_bg)` and uses only its
  existing fields (no new colour math):
  - window: `theme.bg`;
  - "Loading ..." label: `theme.text`; byte-count label: `theme.text_dim`
    (as the update dialog's status line);
  - progress bar: `FrameType::FlatBox` (the default `FL_DOWN_BOX` bevel is drawn
    from FLTK's fixed grey ramp, so it would stay grey in any theme),
    `theme.input_bg` track, `theme.button_bg` fill, `theme.text` label — the
    same colours as the update dialog's progress bar;
  - Cancel: `RFlatBox`, `theme.secondary_button_bg`, `theme.text_dim` label —
    identical to the Cancel button of the "File Too Large" dialog;
  - `theme.apply_titlebar(&dialog)` after `show()`, as every other dialog does
    (icon; themed title bar on Windows).
- Event loop, Cancel callback and `cancel_stream` are untouched; the doc
  comment notes why this dialog keeps its own loop instead of `run_dialog`.
- No pure helper was extracted (the colours are read straight from
  `DialogTheme`, whose derivation is already unit-tested), so verification is
  manual only. The existing T0039 tests still pin the cancel behaviour.

## How to test

(a) Automated (regression only — cancel path and reader thread from T0039):

```bash
nix develop -c cargo test --lib large_file
```

(b) Manual only (the dialog needs a display and a real streaming load):

1. `yes "line" | head -c 1500M > /tmp/big.txt`. In Settings set Max editable
   size to 1800. Open `/tmp/big.txt` and choose Open in the Large File Warning.
2. With a dark theme: the "Loading File" dialog has the app's dark background,
   light label text, a dimmer byte-count line, a flat progress bar (input-bg
   track, button-bg fill) and a flat, secondary-styled Cancel — no grey FLTK
   box or bevel anywhere. Repeat with a light theme: same layout in light colours.
3. Cancel, the title-bar X and Escape each close the dialog and no tab opens;
   letting it finish opens the file in a tab.
4. Clean up: `rm /tmp/big.txt`, set Max editable size back to 150.

## Acceptance criteria

- [ ] No grey FLTK box in either theme; cancel behaviour unchanged.
- [x] Gates green.

## Affected files

- `src/ui/dialogs/large_file.rs`
- `src/app/controllers/file.rs` (passes `theme_bg`)

## Notes

- Origin: T0024 report, 2026-10-19 (documented exception).
