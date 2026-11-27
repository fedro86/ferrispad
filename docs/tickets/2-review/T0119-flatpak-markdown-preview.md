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

- **Flatpak detection: unchanged.** The preview uses the updater's rule,
  `services::updater::is_flatpak()`, which checks `/.flatpak-info` only. That
  function and the updater are untouched (a3's review, the orchestrator's
  decision). `FLATPAK_ID` alone does not count: a host process can inherit
  it, for example from VS Code's Flatpak (`com.visualstudio.code`). A guard
  test pins this.
- **Preview directory.** A pure function picks it from that flag, the cache
  dir and the temp dir:
  - in a Flatpak with a cache dir: `$XDG_CACHE_HOME/ferrispad_preview`;
  - otherwise: `temp_dir()/ferrispad_preview`, unchanged.

  `PreviewController` picks the dir once, in `new()`, and stores it.
  `temp_path_for_doc`, `write_preview`, `write_html` and `cleanup` all use
  that stored dir, so writing and cleaning can never point at different
  places.
- **A safe preview folder** (Unix, both dirs). This fixes a pre-existing
  security issue that a3 found in review; the orchestrator put it in this
  ticket. Before any write or sweep, `prepare_preview_dir`:
  - creates the dir if needed (mode 0700);
  - checks with `symlink_metadata` that it is a real directory, not a
    symlink;
  - checks that it is owned by the current effective user (`geteuid` from
    the `nix` crate, already in the build through portable-pty);
  - sets mode 0700 if it is anything else.

  If any check fails, the dir is refused: no preview is written (the
  existing "Failed to write preview file" notice) and no sweep runs. On
  Windows nothing changes: `%TEMP%` is per user.
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
  - runs once at startup, with no timer and no thread;
  - runs only in a dir that passed the checks above;
  - removes preview files whose mtime is older than 24 h, leaving newer
    ones because another FerrisPad window may still use them;
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
- Moving the non-Flatpak preview out of `/tmp`. With the folder checks, a
  pre-created or symlinked `/tmp/ferrispad_preview` is refused, not used.
- Bringing the Markdown preview line back on the FlatPark page (T0118). That
  happens after this fix is released.

## What was done

- `src/app/controllers/preview.rs`:
  - `preview_dir_for(in_flatpak, cache_dir, temp_dir)` picks
    `<cache>/ferrispad_preview` in a Flatpak with a cache dir, otherwise
    `<temp>/ferrispad_preview`;
  - `PreviewController` stores the dir it chose in `new()`. `with_dir` takes
    one for tests, and it prepares the dir and sweeps it once;
  - `prepare_preview_dir`: create, then real dir, owner and 0700 on Unix, or
    refuse;
  - `open_in_browser` is now `write_preview` (prepare, write, track) plus
    `open::that`. `write_html` also prepares before it rewrites;
  - `sweep_stale_previews(dir, now)` with `is_preview_file_name`.
- `Cargo.toml` (non-Windows): `nix` 0.28 with the `user` feature, for a safe
  `geteuid()`. No new crate: it is the version portable-pty already builds,
  so `Cargo.lock` only gains a dependency edge.
- `services::updater` is unchanged: no diff against master.

## How to test

### Regression tests (red first)

```bash
nix develop -c cargo test --lib -- preview_dir flatpak_id_alone sweep cleanup_removes existing_open_dir symlinked_preview
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
- `flatpak_id_alone_is_not_a_flatpak` (guard): the test re-runs its own
  binary with only `FLATPAK_ID=com.visualstudio.code` set, and expects
  `is_flatpak()` to be false. Inside a real sandbox it has nothing to show,
  so it returns.
  - It passes on master. It failed on the first version of this branch
    (d8816a6): "FLATPAK_ID alone counted as a Flatpak".
- `existing_open_dir_is_made_private` (Unix): a pre-existing 0777 preview
  dir ends 0700.
  - **Red** on d8816a6: `left: 511` (0o777), `right: 448` (0o700).
- `symlinked_preview_dir_is_refused` (Unix): the preview dir is a symlink to
  another dir, which holds a stale preview. Expected:
  - `write_preview` returns "Failed to write preview file…";
  - nothing is written through the link;
  - the stale file is not swept;
  - the link itself is left alone.
  - **Red** on d8816a6: the write succeeded (`unwrap_err` on `Ok`).
- `preview_dir_is_private_to_the_user` (Unix): a new dir is created 0700.
- `cleanup_removes_files_and_dir`: a controller on a scratch dir writes a
  preview (no browser), rewrites it through `write_html`, then cleans up.
  No file and no dir are left.
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

`PreviewController` builds no FLTK object, so these are plain unit tests,
not `main_thread_checks`. Then run the full gates: 765 passed, 0 failed;
clippy `-D warnings` and fmt are clean.

The first version of this branch also had a red test, `flatpak_detection`,
for counting `FLATPAK_ID` as a Flatpak. It tested a heuristic, not the bug:
the bug is the preview's folder. a3's review dropped the heuristic, and the
test is gone.

Not covered by a test: a preview dir owned by another user. Making one needs
root. The check is the `uid` comparison in `prepare_preview_dir`.

### Inside the installed Flatpak, headless (done, first version)

The installed FlatPark 0.9.10 sandbox ran this branch's debug binary at
29fa3a6 and, as "before", its own `/app/extra/FerrisPad`. The run used Xvfb
and a scratch `XDG_CONFIG_HOME`/`XDG_DATA_HOME`. A fake `xdg-open` on `PATH`
logged the path it was given and did nothing else, so no portal or chooser
was reached.

The scenario: open `notes.md`, press Ctrl+M, edit and save, press Ctrl+M
again, then quit.

| | 0.9.10 (installed) | branch 29fa3a6 |
|---|---|---|
| path handed to `xdg-open` | `/tmp/ferrispad_preview/preview_e2b4….html` (the sandbox's private `/tmp`) | `~/.var/app/com.ferrispad.FerrisPad/cache/ferrispad_preview/preview_e2b4….html` |
| preview dir mode | 755 | 700 |
| content | | `<h1>`, both `<li>`, the image as an absolute `file:///home/…` URL |
| save without Ctrl+M | | same file rewritten (contains the edit); no new `xdg-open` call |
| second Ctrl+M | | same path opened again |
| after quit | | file and dir removed |

The run predates the sweep and the folder checks, which are covered by
their unit tests. It was a hand-written harness (`uitest-T0119/run.sh`), from
before the new-pr skill's `shoot.sh` landed. `shoot.sh` cannot replace it:
it shims `xdg-open` to fail, and it does not run inside the Flatpak.

The harness left its Xvfb `:98` running on the host. The orchestrator
noticed it and left it for the user to stop. Since then each gate run
starts and stops its own Xvfb.

The real portal step was checked separately with the orchestrator: the user
saw `xdg-open` of a file under `$XDG_CACHE_HOME` open the chooser, then the
browser (see Goal). It is repeated in the manual recipe below.

### Manual (the user's real conditions, no Flatpak rebuild)

The branch binary runs inside the installed Flatpak, since home is mounted
in the sandbox. Run from the branch's worktree. The config and data dirs are
scratch dirs under the worktree, passed only to the sandbox with `--env`, so
your real Flatpak settings and sessions are not read or written. The Nix
build loads its libraries from `/nix/store`, which the sandbox does not
mount. `--filesystem=/nix:ro` adds it for this one run; it is not a saved
override.

1. `nix develop -c cargo build --release`
2. `S=$(mktemp -d -p $PWD) && mkdir $S/cfg $S/data`, then write
   `$S/notes.md` with a heading, a list and `![img](pic.png)`, and put a
   `pic.png` next to it.
3. `flatpak run --filesystem=/nix:ro --env=XDG_CONFIG_HOME=$S/cfg --env=XDG_DATA_HOME=$S/data --command=$PWD/target/release/FerrisPad com.ferrispad.FerrisPad $S/notes.md`.
   Expected: FerrisPad opens `notes.md`.
4. Press Ctrl+M. Expected:
   - GNOME's "Open With" chooser appears. It may be behind FerrisPad's
     window; see Out of scope.
   - Pick the browser. It shows the heading, the list and the image, from
     `~/.var/app/com.ferrispad.FerrisPad/cache/ferrispad_preview/`.

   Before the fix: nothing happens.
5. **Live refresh:** add a line and press Ctrl+S, without Ctrl+M. Reload
   the browser tab: it shows the line.
6. Add another line and press Ctrl+M: the page shows it too.
7. Quit FerrisPad, then `ls ~/.var/app/com.ferrispad.FerrisPad/cache/`.
   Expected: no `ferrispad_preview` is left.
8. Outside the Flatpak: `nix develop -c cargo run --release -- $S/notes.md`,
   then Ctrl+M. Expected: the browser opens the preview from
   `/tmp/ferrispad_preview/`, as before.
9. `rm -rf "$S"`

## Acceptance criteria

- [ ] In the Flatpak, Ctrl+M opens the preview in the host browser
      (through the chooser). Pending the user (manual step 4).
- [ ] Saving rewrites that same file, and a reload shows the edit. The
      rewrite is checked headless; the browser reload is pending the user.
- [x] Outside the Flatpak the preview dir and behaviour are unchanged
      (guard test).
- [x] Flatpak detection is the updater's, unchanged: `FLATPAK_ID` alone
      does not count (guard test), and `updater.rs` has no diff.
- [x] A pre-existing open preview dir is made 0700. A symlinked or foreign
      one is refused, with no write and no sweep (red tests).
- [x] Cleanup removes the session's preview files in either dir.
- [x] At startup, preview files older than 24 h are swept from the preview
      dir, and nothing else is touched (unit test).
- [x] Regression tests added; they failed before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.

## Affected files

- `src/app/controllers/preview.rs`: the dir chooser, the stored dir,
  `prepare_preview_dir`, `write_preview`, the sweep, and tests.
- `Cargo.toml`, `Cargo.lock`: `nix` (`user` feature) on non-Windows.

## Notes

- Reported and confirmed by the user with the orchestrator (ferrispad-5a),
  2026-11-27. Target 0.9.11.
- Review by a3 on d8816a6; the orchestrator decided:
  - keep the detection as it was;
  - add the folder checks here;
  - approve the sweep as is;
  - fix the recipe nits.
- Once this is released, T0118's dropped line, `Markdown preview in your
  browser (Ctrl+M)`, can go back on the FlatPark page, right after "Syntax
  highlighting for 50+ languages".
- `dirs::cache_dir()` (crate `dirs` 7, already a dependency) reads
  `$XDG_CACHE_HOME`, which flatpak sets to
  `~/.var/app/com.ferrispad.FerrisPad/cache`.
