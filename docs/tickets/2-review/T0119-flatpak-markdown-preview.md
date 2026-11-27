---
id: T0119
title: Markdown preview (Ctrl+M) opens in the browser inside the Flatpak
status: review          # todo | review | done
created: 2026-11-27
severity: moderate
area: robustness
depends-on: []
issue: 239
---

## Goal

In the FlatPark Flatpak, View → Preview in Browser (Ctrl+M) on a Markdown
file does nothing. No browser opens and no error is shown. The user confirmed
this on 2026-11-27 (FlatPark 0.9.10, GNOME).

The cause: `PreviewController` writes the page to
`std::env::temp_dir().join("ferrispad_preview")`. In the sandbox that is
FerrisPad's private `/tmp`, which the host browser cannot read. `open::that`
then hands the path to `xdg-open`, which inside the sandbox is
flatpak-xdg-open, which calls the OpenURI portal. The open fails, and the
user sees nothing.

Repro inside the installed sandbox
(`flatpak run --command=sh com.ferrispad.FerrisPad -c …`):
- `xdg-open /tmp/test1-tmp.html`: exit 0 and nothing else. No chooser, no
  browser, no portal log.
- `xdg-open $XDG_CACHE_HOME/ferrispad_preview/test2-cache.html`: GNOME shows
  its "Open With" chooser, then the browser opens
  `file:///home/fedro/.var/app/com.ferrispad.FerrisPad/cache/ferrispad_preview/test2-cache.html`.
  The sandbox's `$XDG_CACHE_HOME` is the same path on the host.

## In scope

- **Flatpak detection, one rule.** FerrisPad is in a Flatpak when `FLATPAK_ID`
  is set or `/.flatpak-info` exists. Today `services::updater::is_flatpak()`
  checks only the file. Make that one pure, testable rule (env and probe
  injected) and use it for both the updater and the preview, so they cannot
  disagree.
- **Preview directory.** A pure function picks it from that flag, the cache
  dir and the temp dir:
  - in a Flatpak with a cache dir: `$XDG_CACHE_HOME/ferrispad_preview`;
  - otherwise: `temp_dir()/ferrispad_preview`, unchanged.
  `PreviewController` picks the dir once, in `new()`, and stores it.
  `temp_path_for_doc`, `open_in_browser`, `write_html` and `cleanup` all use
  that stored dir, so writing and cleaning can never point at different
  places.
- **Permissions.** Create the preview dir with mode 0700 on Unix
  (`DirBuilder` with `recursive(true)` and `mode(0o700)`), both in `new()`
  and when a write re-creates it. One rule for both dirs: outside the
  Flatpak it only tightens a dir FerrisPad creates, and the browser runs as
  the same user.
- **Cleanup keeps working in both places.** It removes this session's
  preview files, then the dir if it is empty, whichever dir was chosen. The
  live refresh on save (`write_html`, via `FileController::update_preview_file`)
  rewrites the same file in the same dir.
- **Startup sweep of stale previews** (added by the user's decision,
  2026-11-27). `cleanup` runs only on a normal exit, so a crashed or killed
  run leaves its previews behind for good:
  - Fedora clears `/tmp` at reboot;
  - Windows `%TEMP%` and the Flatpak cache dir are never cleared.

  The sweep:
  - runs once in `PreviewController::new()`, with no timer and no thread;
  - removes preview files in the chosen dir whose mtime is older than 24 h,
    leaving newer ones because another FerrisPad window may still use them;
  - only touches regular files named `preview_<16 hex>.html` directly
    inside the dir: no recursion, symlinks skipped via `symlink_metadata`
    and never followed, nothing outside the dir;
  - ignores any read or remove error, with no panic and no popup.

## Out of scope

- **The "Open With" chooser is not attached to FerrisPad's window.** The log
  says "Failed to associate portal window with parent window", so the chooser
  can open behind FerrisPad. The portal does that; `open::that` passes no
  parent window. Recorded here, not fixed.
- **Portal failures stay silent.** `open::that` returns `Ok` once
  `xdg-open` has spawned. Whatever the portal does afterwards is invisible to
  FerrisPad, so this ticket adds no error popup for it.
- The predictable `/tmp/ferrispad_preview` path outside the Flatpak is left
  unchanged. See Notes.
- Bringing the Markdown preview line back on the FlatPark page (T0118). That
  happens after this fix is released.

## What was done

- `src/app/services/flatpak.rs` (new):
  - `detect(flatpak_id, info_file_exists)`: a non-empty `FLATPAK_ID`, or
    `/.flatpak-info` present;
  - `is_flatpak()` reads both.
  `updater::is_flatpak()` now calls it, so its callers are unchanged.
- `src/app/controllers/preview.rs`:
  - `preview_dir_for(in_flatpak, cache_dir, temp_dir)` picks
    `<cache>/ferrispad_preview` in a Flatpak with a cache dir, otherwise
    `<temp>/ferrispad_preview`;
  - `PreviewController` stores the dir it chose in `new()` (`with_dir` takes
    one for tests);
  - `temp_path_for_doc`, `write_html` and `cleanup` use the stored dir;
  - `create_preview_dir` makes it with mode 0700 on Unix;
  - `open_in_browser` is now `write_preview` (write and track) plus
    `open::that`;
  - `sweep_stale_previews(dir, now)` with `is_preview_file_name`, called
    once from `new()`.

## How to test

### Regression test (red first)

```bash
nix develop -c cargo test --lib -- preview_dir flatpak_detection
```

- `preview_dir_in_flatpak_uses_the_cache_dir`: with in-Flatpak = true, cache
  dir `/c` and temp dir `/t`, expects `/c/ferrispad_preview`.
  - **Red:** first refactor today's `preview_dir()` into the pure function
    with its current behaviour, so the test compiles. It then fails with
    `left: "/t/ferrispad_preview"`, `right: "/c/ferrispad_preview"`. That is
    the bug's own failure, not a compile error.
- `preview_dir_outside_flatpak_is_unchanged` (guard; passes before and
  after): with in-Flatpak = false, expects `/t/ferrispad_preview`.
- `preview_dir_in_flatpak_without_cache_dir_falls_back`: with no cache dir,
  expects the temp dir. That is today's behaviour, never a panic.
- `flatpak_detection`: `FLATPAK_ID` set → true; `/.flatpak-info` present →
  true; neither → false; an empty `FLATPAK_ID` → false.
- `cleanup_removes_files_and_dir`: uses a controller built on an injected
  scratch dir. Write a preview without launching a browser, then rewrite it
  through `write_html`, then call `cleanup`. Expect no preview file and no
  dir left. If needed, split `open_in_browser` into a write step and the
  `open::that` call, so the write step can be tested.
  `PreviewController` builds no FLTK object, so this can be a plain unit test
  rather than a `main_thread_checks` one.
- On Unix, the Flatpak dir is created with mode `0o700` (checked on a scratch
  dir).

- `sweep_removes_only_old_preview_files`: a temp dir holds
  - an old and a new `preview_<hex>.html`;
  - an old `notes.html` and an old `preview_0123.html`;
  - a subdir named like a preview, with an old preview inside;
  - (Unix) a symlink named like a preview, pointing to an old preview
    outside the dir.

  mtimes are set with `File::set_modified`. Only the old regular preview
  goes.
  - **Red** (the sweep as an empty stub): `panicked … an old preview file is
    removed`.
- `sweep_of_a_missing_dir_is_a_no_op`.

Then run the full gates.

Red run (today's behaviour behind the pure functions, before the fix):

```text
preview_dir_in_flatpak_uses_the_cache_dir ... FAILED  left: "/t/ferrispad_preview"  right: "/c/ferrispad_preview"
flatpak_detection                         ... FAILED  (flatpak.rs:17, the FLATPAK_ID-only case)
preview_dir_outside_flatpak_is_unchanged  ... ok      (guard)
preview_dir_in_flatpak_without_cache_dir_falls_back ... ok
```

After the fix, all of them pass, as do `cleanup_removes_files_and_dir` and
`preview_dir_is_private_to_the_user`. With the sweep added, the gates give
762 passed, 0 failed; clippy `-D warnings` and fmt are clean.

### Inside the installed Flatpak, headless (done)

The installed FlatPark 0.9.10 sandbox ran this branch's debug binary and,
as "before", its own `/app/extra/FerrisPad`. The run used Xvfb `:98` and a
scratch `XDG_CONFIG_HOME`/`XDG_DATA_HOME`. A fake `xdg-open` on `PATH` logged
the path it was given and did nothing else, so no portal or chooser was
reached.

The scenario: open `notes.md`, press Ctrl+M, edit and save, press Ctrl+M
again, then quit.

| | 0.9.10 (installed) | branch |
|---|---|---|
| path handed to `xdg-open` | `/tmp/ferrispad_preview/preview_e2b4….html` (the sandbox's private `/tmp`) | `~/.var/app/com.ferrispad.FerrisPad/cache/ferrispad_preview/preview_e2b4….html` |
| preview dir mode | 755 | 700 |
| content | | `<h1>`, both `<li>`, the image as an absolute `file:///home/…` URL |
| save without Ctrl+M | | same file rewritten (contains the edit); no new `xdg-open` call |
| second Ctrl+M | | same path opened again |
| after quit | | file and dir removed |

This run predates the sweep, which only removes files older than 24 h and
is covered by its unit test. It was a hand-written harness (`uitest-T0119/run.sh`), from before the
new-pr skill's `shoot.sh` landed. `shoot.sh` cannot replace it here: it
shims `xdg-open` to fail, and it does not run inside the Flatpak.

The real portal step was checked separately with the orchestrator: the user
saw `xdg-open` of a file under `$XDG_CACHE_HOME` open the chooser, then the
browser (see Goal). It is repeated in the manual recipe below.

### Manual (the user's real conditions, no Flatpak rebuild)

Home is mounted in the sandbox, so the branch binary runs inside the
installed one. The binary is built in the Nix shell and loads its libraries
from `/nix/store`, which the sandbox does not mount. `--filesystem=/nix:ro`
adds it for this one run; it is a command-line flag, not a saved override.

1. `nix develop -c cargo build --release` on the ticket branch.
2. `flatpak run --filesystem=/nix:ro --command=$HOME/Code/personal/ferrispad-wt/T0119/target/release/FerrisPad com.ferrispad.FerrisPad /path/to/notes.md`
   (a `.md` file with a heading, a list and a relative image).
3. Press Ctrl+M. Expected: GNOME's "Open With" chooser appears. It may be
   behind FerrisPad's window; see Out of scope. Pick the browser. The page
   opens from
   `~/.var/app/com.ferrispad.FerrisPad/cache/ferrispad_preview/preview_<hash>.html`
   and the relative image shows. Before the fix: nothing happens.
4. **Live refresh:** edit the Markdown and save with Ctrl+S, without pressing
   Ctrl+M. Reload the browser tab: it shows the edit.
5. Edit again and press Ctrl+M: the reloaded or reopened page shows the
   edit.
6. Quit FerrisPad. `ls ~/.var/app/com.ferrispad.FerrisPad/cache/ferrispad_preview`
   shows no preview files, and the dir itself is gone if nothing else is in
   it.
7. Outside the Flatpak (`cargo run --release -- notes.md`), Ctrl+M still
   opens the preview from `/tmp/ferrispad_preview/` as before.

## Acceptance criteria

- [ ] In the Flatpak, Ctrl+M opens the preview in the host browser
      (through the chooser). Pending the user (manual recipe step 3).
- [ ] Saving rewrites that same file, and a reload shows the edit. The
      rewrite is checked headless; the browser reload is pending the user.
- [x] Outside the Flatpak the preview dir and behaviour are unchanged
      (guard test).
- [x] The updater and the preview use the same Flatpak detection.
- [x] Cleanup removes the session's preview files in either dir.
- [x] At startup, preview files older than 24 h are swept from the preview
      dir, and nothing else is touched (unit test).
- [x] Regression tests added; the Flatpak one failed before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.

## Affected files

- `src/app/controllers/preview.rs`:
  - `preview_dir` becomes a pure chooser;
  - the dir is stored in `PreviewController`;
  - the 0700 create;
  - tests.
- `src/app/services/updater.rs`: `is_flatpak()` uses the shared rule (or the
  rule moves to a small shared module both use).
- Callers of `is_flatpak()` (`src/main.rs`, `src/ui/dialogs/update.rs`):
  unchanged signatures.

## Notes

- Reported and confirmed by the user with the orchestrator (ferrispad-5a),
  2026-11-27. Target 0.9.11, which is on hold until 0.9.10 final ships.
- Once this is released, T0118's dropped line, `Markdown preview in your
  browser (Ctrl+M)`, can go back on the FlatPark page, right after "Syntax
  highlighting for 50+ languages".
- Adding `FLATPAK_ID` to the detection also changes the updater. When only
  `FLATPAK_ID` is set, it now treats the run as a Flatpak too (no
  self-update). That only happens inside a sandbox, or with a hand-exported
  variable.
- `dirs::cache_dir()` (crate `dirs` 7, already a dependency) reads
  `$XDG_CACHE_HOME`, which flatpak sets to
  `~/.var/app/com.ferrispad.FerrisPad/cache`.
- Possible follow-up, not this ticket: outside a Flatpak, the preview path
  `/tmp/ferrispad_preview` is predictable inside a world-writable dir.
  Another local user could pre-create it or plant a symlink there. Moving the
  non-Flatpak preview to the user cache dir too would close that, but it
  changes behaviour outside the Flatpak, so it needs its own decision.
