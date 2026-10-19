---
id: T0052
title: Live theme preview in Settings re-themes every panel
status: review
created: 2026-10-19
severity: minor
area: ui
depends-on: [T0019]
issue: 80
---

## Goal

While the Settings dialog is open, changing the syntax theme or dark mode
previews the new colours on the editor only. The tab bar, status bar, terminal,
split panel, tree and diagnostics change only after OK. The preview should
show the whole window. If Cancel is pressed, everything goes back to how it was.

## In scope

- Route `Message::PreviewSyntaxTheme` (`dispatch.rs`, `AppState::preview_syntax_theme`)
  through the single re-theme entry point added by T0019
  (`dispatch::retheme_panels`, `Themeable`).
- Cancel restores the previous theme on every panel through the same path.
- Extend T0019's source-level test `every_theme_change_path_rethemes_every_panel`
  to cover `PreviewSyntaxTheme`, or replace it with a better seam.

## Out of scope

- New theme options, or persisting a preview without OK.
- A live preview of the dark-mode radio buttons (the dialog never had one; only
  the syntax-theme choice for the current mode previews).

## What was actually done

Investigating the bug showed a second, deeper cause: the Settings dialog is
modal (`run_dialog` spins `app::wait()`), and FLTK's `app::channel` is only
drained by the main loop in `main.rs`, which is blocked inside
`OpenSettings` until the dialog closes. So `PreviewSyntaxTheme` messages sent by
the dialog were **queued, not applied live**: even the editor changed only after
the dialog closed, and a queued preview could run *after* Save's
`apply_settings` (e.g. pick a light theme, switch to Dark, Save: the stale
light-theme preview then overrode the saved dark theme on the editor).

- `dispatch::handle_settings` now owns the `Message::PreviewSyntaxTheme` arm:
  `state.preview_syntax_theme(theme)` then `retheme_panels(state, lw)`, so the
  preview goes through the single T0019 entry point (T0055 can keep extending
  `retheme_panels` alone). `main.rs` routes the variant to `handle_settings`
  (moved out of the "Background updates" group / `handle_update`).
- `AppState::preview_syntax_theme` re-themes everything `AppState` owns
  (`apply_theme_to_owned_widgets`: editor, window, menu bar, title bar, tab bar)
  instead of only the editor colours.
- `AppState::open_settings` takes an `on_preview(&mut AppState, SyntaxTheme)`
  hook; the `OpenSettings` arm passes
  `|state, theme| handle_settings(Message::PreviewSyntaxTheme(theme), state, lw)`.
- `show_settings_dialog` no longer takes a `Sender`: the theme choices record
  the picked theme in a `Cell`, and the dialog loop (new
  `dialogs::run_dialog_with(dialog, after_event)`, which `run_dialog` now
  delegates to) hands it to the preview hook after each event, i.e. live.
  Cancel and the window close button (X / Escape) queue the original theme the
  same way, only if a preview happened, so the revert also goes through the
  `PreviewSyntaxTheme` arm and re-themes every panel.
- Tests (`src/dispatch.rs`): `Message::PreviewSyntaxTheme` added to
  `THEME_CHANGE_ARMS`; `arm_body` now also finds tuple-variant arms
  (`Variant(binding) =>`). New `settings_preview_and_cancel_go_through_the_preview_arm`
  guards the hook wiring and that the dialog no longer queues previews on the
  channel.

Known limit: files above the large-file highlight threshold are re-highlighted
in chunks via `ContinueHighlight` messages, which still wait for the dialog to
close; their background/foreground colours preview live, token colours follow
after OK/Cancel.

### Red run (before the fix)

```
running 1 test
test dispatch::tests::every_theme_change_path_rethemes_every_panel ... FAILED

---- dispatch::tests::every_theme_change_path_rethemes_every_panel stdout ----
thread 'dispatch::tests::every_theme_change_path_rethemes_every_panel' panicked at src/dispatch.rs:1459:13:
Message::PreviewSyntaxTheme changes the theme but does not re-theme ["tree_panel", "split_panel", "terminal_panel", "diagnostic_panel", "status_bar", "toast"]

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

After the fix both `dispatch::tests` pass.

## How to test

```bash
nix develop -c cargo test --bin FerrisPad dispatch::tests
```

Expect `every_theme_change_path_rethemes_every_panel` and
`settings_preview_and_cancel_go_through_the_preview_arm` to pass.

Manual (UI-only, the live behaviour inside the modal cannot be unit-tested):

1. Start FerrisPad, open a folder tree, open the terminal (and a diagnostics or
   split/diff panel if convenient), keep two tabs open.
2. Edit > Settings. Change the syntax theme of the current mode (e.g. in light
   mode, "Syntax Theme (Light)") to a very different theme. **Without pressing
   OK**, the editor, tab bar, status bar, tree, terminal and other open panels
   change colour immediately, while the dialog is still open.
3. Pick another theme: everything follows again.
4. Press Cancel: every panel returns to the original theme. Repeat steps 2-3
   and close the dialog with the window X (or Escape): same result.
5. Pick a theme and press OK: the new theme stays on every panel.
6. Regression check: pick a different light theme, switch Theme Mode to Dark,
   press OK. The editor must show the dark syntax theme (before the fix, the
   stale light preview could override it after OK).

## Acceptance criteria

- [x] Preview and Cancel re-theme every panel; the red test fails before the fix.
- [x] Gates green.

## Affected files

- `src/dispatch.rs` (`PreviewSyntaxTheme` arm, `OpenSettings` hook, tests)
- `src/app/state.rs` (`open_settings`, `preview_syntax_theme`)
- `src/ui/dialogs/settings_dialog.rs` (preview + Cancel path)
- `src/ui/dialogs/mod.rs` (`run_dialog_with`)
- `src/main.rs` (routing), `src/app/domain/messages.rs` (doc comment)

## Notes

- Origin: T0019 report "left out of scope", 2026-10-19.
