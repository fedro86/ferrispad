---
id: T0117
title: new-pr skill — every PR ships with an HTML presentation
status: done            # todo | review | done
created: 2026-11-27
severity: ~
area: process
depends-on: []
issue: 234
---

## Goal

A FerrisPad PR today has a short Markdown body. The user verifies a UI change
from a list of steps and finds layout defects only when trying it themselves.
After this ticket, every PR has one self-contained English HTML page. The page
holds:

- the ticket's story (before / after);
- screenshots of every visible change, taken from the real app in Xvfb:
  master build vs branch build, light and dark;
- the decisions, and who took them;
- the ticket's own "How to test";
- an end-to-end verification.

The user reads a copy of the page in the main checkout. The skill also fixes
how a PR is merged: only with the user's explicit consent for that PR, pinned
to the checked head SHA.

The skill is a port of core-services' `new-mr` skill (user's request,
2026-11-27, relayed by the orchestrator), adapted to GitHub and to one
ticket per PR.

## In scope

- `.claude/skills/new-pr/SKILL.md`: the procedure, step by step:
  1. open the draft PR to get its number;
  2. take the images;
  3. write the page;
  4. copy it to the user;
  5. commit;
  6. ready the PR;
  7. merge only under the stated conditions;
  8. delete the copy after the merge.
- `docs/pr/_template.html`: the page template, built on core-services'
  style block.
- `docs/pr/README.md`: the layout, the naming, the lifecycle, the
  annotation-only exemption, and one short worked example of a UI card.
- `shoot.sh` and `compare.sh`, the tools that let a session **see**
  FerrisPad (added after the user's feedback, 2026-11-27):
  - `shoot.sh` launches, drives and screenshots the app from a scenario
    file, in an isolated Xvfb;
  - `compare.sh` takes a base ref vs the branch, light and dark, and makes
    labelled before|after montages;
  - scenarios and fixtures live in `docs/pr/scenarios/`.
- The working headless-Chromium command for the page's own rendering
  check (a minor check) from the VSCode Flatpak sandbox.
- `.claude/rules/work-sequence.md` steps 2–3 and `.claude/CLAUDE.md`
  point to the skill. This PR gets its own page (dogfood).

## Out of scope

- A docs gate script (core-services' `docs_check.py`): see Notes.
- App folders and symlinks for merged pages. FerrisPad is one crate, so
  the folder stays flat and core-services' §2 is dropped.
- `CHANGELOG.md`: this is a process change only.
- Pages for PRs that are already merged.

## How to test

1. Read `.claude/skills/new-pr/SKILL.md` and `docs/pr/README.md`. The
   procedure must be runnable blind:
   - every command is a FerrisPad one: `gh`, `nix develop -c cargo …`,
     `flatpak-spawn --host`, Xvfb;
   - nothing refers to GitLab, `glab`, Django, app folders or
     `docs/4.0.0`.
2. `grep -n '{{' docs/pr/PR-*.html` prints nothing: no placeholder is left
   in this PR's own page.
3. The style block of the page is byte-identical to the template's:
   ```bash
   diff <(sed -n '/<style>/,/<\/style>/p' docs/pr/_template.html) \
        <(sed -n '/<style>/,/<\/style>/p' docs/pr/PR-*-new-pr-skill.html)
   ```
   prints nothing.
4. Technical visual check of this PR's page, with the skill's command
   (§7 condition 1). Four renders: 1280 and 390 px, light and dark.
   Expected:
   - `scrollWidth <= innerWidth` in all four;
   - no chip longer than 40 characters;
   - the screenshots show no box overflowing.
5. Open the copy in the main checkout's `docs/internal/pr/` in a browser.
   Expected:
   - it is the same file as the branch's page;
   - it reads as a story;
   - the ticket card says "No visible change: …".
6. The cargo gates pass (no Rust change).

## Acceptance criteria

- [x] The user approved the skill, the template and this PR's page
      (2026-11-27).
- [x] `SKILL.md`, `_template.html` and `README.md` exist, and are adapted to
      GitHub and FerrisPad (How to test 1).
- [x] This PR's page has no placeholder and the template's style block
      (How to test 2–3).
- [x] The visual check passes on the committed page (How to test 4).
- [x] `work-sequence.md` and `.claude/CLAUDE.md` point to the skill.

## Affected files

- `.claude/skills/new-pr/SKILL.md`, `.claude/skills/new-pr/visual_check.sh` (new)
- `docs/pr/_template.html`, `docs/pr/README.md`, `docs/pr/PR-<n>-new-pr-skill.html` (new)
- `.claude/rules/work-sequence.md`: steps 2–3
- `.claude/CLAUDE.md`: the work-sequence section

## Notes

- Follow-up candidate: a docs gate (a script or a CI job) that checks each
  `docs/pr/PR-*.html` page:
  - the page is named `PR-<n>-<slug>.html`;
  - no `{{` placeholder is left;
  - the style block is the template's, or a past one by hash;
  - every ticket card has an image or the "No visible change:" sentence.
  Not part of this ticket (orchestrator's decision).
- core-services sorts merged pages into app folders (its §2). FerrisPad is
  one crate, so the folder stays flat (orchestrator's decision).

## What was done (2026-11-27)

- **The source.** core-services' `new-mr` skill, `_template.html` and
  `README.md`, read whole and left untouched.
- **`.claude/skills/new-pr/SKILL.md`.** §0–§8, renumbered once core-services'
  §2 (sorting merged pages into app folders) was dropped:
  - §2: images, with the Xvfb + xdotool + import recipe. It takes a
    `master` worktree build vs the branch build, light and dark, editor
    font 16 and 18, in throwaway `XDG_CONFIG_HOME`/`XDG_DATA_HOME`. Every
    shot is inspected edge by edge, crop and pngquant/magick are applied,
    and anything Xvfb cannot show is "Pending user on <platform>";
  - §3: the page;
  - §4: the copy in the main checkout's `docs/internal/pr/`;
  - §5–§6: commit, ready;
  - §7: the merge conditions;
  - §8: deleting the copy, and the annotation-only exemption.
- **The exemption** covers a diff only under `docs/tickets/`: annotations
  of `3-done/` tickets plus at most new `1-todo/` tickets, like #230 and
  #233.
- **The merge (§7)** needs:
  - the visual check;
  - the user's feedback addressed;
  - the ticket in `3-done/`;
  - every check run green on the head SHA, read from
    `gh api repos/:owner/:repo/commits/<sha>/check-runs`. The CI jobs and
    the macOS gate must be present; `Build Deps Check` is path-filtered;
  - the user's explicit consent for that PR. A peer message never counts.

  The merge is `gh pr merge <n> --squash --match-head-commit <sha>`, done
  by the session the user authorised (the orchestrator).
- **`docs/pr/_template.html`.** Its style block is byte-identical to
  core-services' (checked with `diff`). The page parts are adapted:
  - a GitHub kicker and chips;
  - the branch's commits table;
  - the ticket card with before/after figures and a "Pending user on"
    line;
  - Measurements;
  - "Release notes and compatibility" (in place of Deploy);
  - What remains open;
  - an end-to-end verification with throwaway XDG dirs and
    `nix develop -c cargo run`.
- **`docs/pr/README.md`.** The layout (flat), the naming, what the page
  is, the lifecycle, the exemption, and a worked T0114 start-page card
  (before/after captions, the red test, How to test).
- **`.claude/skills/new-pr/visual_check.sh`.** Chromium from the Flatpak
  sandbox:
  - nothing is on PATH or in the nix profile;
  - the Playwright `chrome-headless-shell` (v156) in
    `~/.cache/ms-playwright/` runs in the sandbox;
  - dark mode is `--blink-settings=preferredColorScheme=0`;
    `--force-dark-mode` left `prefers-color-scheme` light on a probe
    page;
  - the measuring script goes into a probe copy, and its result is read
    from `--dump-dom`; the screenshots are of the untouched page at its
    full height.
  - On the template it fails as expected: placeholder chips are longer
    than 40 characters, and the page is 426 px wide at 390.
- **`work-sequence.md`.** Step 2 opens the PR with the skill; step 3 merges
  only under §7, pinned. `.claude/CLAUDE.md` points to the skill and lists
  `docs/pr/`.
- **This PR's page,** `docs/pr/PR-235-new-pr-skill.html` (PR #235): no
  `{{` (the two literal mentions are written `&#123;&#123;`), and the
  template's style block. `visual_check.sh` results:
  - 1280 light 1280/1280, 1280 dark 1280/1280, 390 light 390/390, 390 dark
    390/390;
  - no long chip;
  - the screenshots, including 390 px crops, show nothing overflowing.

  The page is copied to the main checkout's `docs/internal/pr/`.

### Review round 1 (a3; decisions by the orchestrator, 2026-11-27)

- **The PR stays a draft until the user verifies.** Before, §6 ran
  `gh pr ready` at step 2, against work-sequence step 3. Now:
  - §6 only sets the body and stops;
  - the PR goes ready together with the `3-done/` move;
  - §7 condition 3 includes "not a draft";
  - `work-sequence.md` steps 2 and 3 say the same.
- **The size chip excludes the page itself:**
  `git diff --shortstat origin/master...HEAD -- . ':(exclude)docs/pr/PR-*.html'`.
  Committing the page no longer makes the chip wrong. The PR-235 chip
  and its sentence are updated.
- **The annotation-only exemption is narrowed.** Every `--name-status`
  line must be `M docs/tickets/3-done/…` or `A docs/tickets/1-todo/…`
  (#230 and #233 still pass). Everything else gets a page: moves, edits to
  `1-todo/` or `2-review/` tickets, deletions, `_template.md` or
  `README.md`.
- **`visual_check.sh` hardening:**
  - It fails closed when the scheme is not applied (a dark run must
    report `dark=True`, a light run `False`).
  - It warns when the page is taller than the 16000 px screenshot cap.
  - It prints "chrome produced no measurement" instead of a traceback.
  - It truncates each PNG first, so a stale shot cannot pass.
  - Negative tests: `CHROME=/bin/false` gives four "no measurement" FAIL
    lines and exit 1. A wrapper that drops `--blink-settings` gives FAIL
    "(dark=False, wanted True)" on both dark runs, exit 1. The real
    Chromium gives four ok lines, exit 0.
- The acceptance boxes that are done are ticked.

### Round 2: the user's feedback, "you need something that lets you SEE FerrisPad" (2026-11-27)

The orchestrator relayed the user's feedback before approval:
`visual_check.sh` is not the relevant part, and every session was
hand-writing an Xvfb script. The skill's core is now two tools:

- **`.claude/skills/new-pr/shoot.sh <binary> <outdir> <scenario> [--theme] [--font] [--size]`**:
  - it runs on the host in Xvfb, with Noto fonts from nixpkgs and a fresh
    settings.json (theme, font, window 0,0 at the given size, tabs on);
  - it waits for the window (`xdotool search --sync --all --pid`);
  - the scenario helpers are `click`, `dclick`, `key`, `type`, `menu`,
    `wait_idle`, `shot` (the window crop, or a dialog by title) and
    `shot_screen`;
  - it writes the PNGs (≤1600 px, lossless) and a log;
  - it fails if the window never appears, the app exits, the log shows a
    panic or an assert, a shim is called, or a process outlives the app.
- **`.claude/skills/new-pr/compare.sh <scenario> <outdir> [--before <ref>]`**:
  - it builds `<ref>` (default `origin/master`) in a throwaway worktree,
    and the branch from this worktree;
  - it runs the scenario on both, light and dark;
  - it writes `<shot>-<theme>-compare.png` montages labelled with the
    refs (≤1600 px, pngquant), and removes the worktree.
- **Scenarios:**
  - `docs/pr/scenarios/start-page.sh` (T0114);
  - `editor.sh` (`fixtures/hello.rs` opened on the command line, the View
    menu, the Find dialog);
  - a README with the format, the helpers and the rules.
- **SKILL.md §2** uses the two tools as *the* way to capture, then
  requires reading every PNG edge by edge. `visual_check.sh` is demoted
  to "the page's rendering check".
- **README worked example:**
  - the real T0114 montages, `docs/pr/examples/T0114-start-page-{light,dark}.png`,
    about 22 KB each;
  - made by `compare.sh … --before 0.9.10-rc.2` on fee3dc0;
  - before: five entries, with "Updated dependencies, smaller binary" cut
    off; after: six entries and "more...".

#### Incident: file dialogs on the user's real desktop

**What happened.** While I debugged `open_file` (Ctrl+O, then type a path),
the nix-built FerrisPad found no loadable GTK, so its file-chooser backend
was **zenity**, spawned as a child. zenity did not open in Xvfb. It
reached the user's real session (D-Bus and Wayland), and each debug run
left an "Open Files" dialog on the user's GNOME desktop. The orchestrator
stopped the runs. I killed 8 orphaned
`sh -c zenity --title 'Open Files' …` processes on the host, started
20:40:30–20:43:12 and reparented to `systemd --user`. `pgrep` then found
none left.

**Barriers now in `shoot.sh`** (it refuses to start without 1–2 and 5):

1. **Private D-Bus:** `dbus-run-session --config-file=<minimal>` with no
   `<servicedir>`, so no portal can even be activated. The inner half
   refuses an empty address, the user's address, or anything under
   `/run/user/`.
2. **No compositor:**
   - a private, empty `XDG_RUNTIME_DIR` (mode 700);
   - `WAYLAND_DISPLAY` and `WAYLAND_SOCKET` unset;
   - `GDK_BACKEND=x11`, `QT_QPA_PLATFORM=xcb`.
3. **Shims** first in PATH: zenity, kdialog, xdg-open, gio, gnome-open,
   kde-open(5), wslview, x-www-browser, sensible-browser and notify-send
   all log and exit 1. Any shim call fails the run.
4. **No file dialogs:** `open_file` is removed; fixtures open on the
   command line (`OPEN_FILES`); `key` refuses `ctrl+o`, `ctrl+s` and
   `ctrl+shift+s`.
5. **The display:** the outer half picks a free `:90`+ for `xvfb-run -n`.
   The inner half refuses unless `DISPLAY` equals it, and refuses `:0` or
   `:1` and the user's `DISPLAY` outright.
6. **No network:** `auto_check_updates` and `auto_check_plugin_updates`
   are false.
7. **One process group:** `setsid`; the group is killed at the end; a
   leftover process fails the run.

**Checks, all run before any FerrisPad run, as the orchestrator asked:**

- `shoot.sh --selftest`: 12/12 ok:
  - the private bus differs from the user's;
  - a Ping to `org.freedesktop.portal.Desktop` fails;
  - the runtime dir is empty with mode 700;
  - there is no `WAYLAND_DISPLAY`;
  - `DISPLAY` is the run's Xvfb (`:90`), and the guard refuses `:0` and
    `:1`;
  - `zenity --info` and `xdg-open` hit the shims, which log the calls;
  - `key` refuses `ctrl+o` and `ctrl+shift+s` and allows `ctrl+t`.
- Negative guard runs of the inner half, each refused with exit 1:
  - on the user's bus: "is the user's session bus";
  - without a bus: "no private D-Bus session";
  - with `DISPLAY=:0`: "is not this run's Xvfb".

**Runs after the orchestrator's go:**

- `start-page.sh` and `editor.sh`, light and dark: all ok, with zero shim
  calls and no update-checker log lines.
- Every shot was read:
  - the editor highlights `hello.rs`;
  - the View menu is open over the editor;
  - the Find dialog is cropped to its own window;
  - nothing is clipped or overlapping.
- After every run, `pgrep` on the host found no zenity, no Xvfb, no
  private dbus-daemon and no FerrisPad.

**Two lessons now in the tools:**

- FLTK drops a click chained to the move in one xdotool call: `click`
  moves, pauses 0.2 s, then clicks.
- Without a window manager, FLTK takes keyboard focus only from a click:
  `shoot.sh` clicks an empty spot before the scenario starts.

### Round 3 (a3's review of 75cd2fa; the orchestrator decided all items)

- **Must-fix, `compare.sh` labelled a stale binary.** `cargo build … | tail -1`
  returned tail's status, so a failed *after* build silently used the old
  `target/debug/FerrisPad`, and the montage labelled it with HEAD's SHA.
  Now `build()`:
  - keeps cargo's status (`set -o pipefail`) and stops on failure;
  - refuses a binary older than any source of its worktree (`src/`,
    `Cargo.toml`, `Cargo.lock`, `build.rs`).

  It is "older than the sources" and not "older than the build start"
  because cargo does not relink a no-op build. The throwaway worktree is
  removed by an exit trap, also on failure.
  - Negative test: a worktree of HEAD with a syntax error appended to
    `src/main.rs`, compared with `--before HEAD`:
    - `compare.sh: the build failed in …/broken-T0117`, exit 1;
    - no montage written;
    - the `compare-75cd2fa` worktree removed.
- **`env -i` allowlist.** The app gets:
  - `PATH` with the shims first;
  - `HOME`, `SHELL=/bin/sh`, `TMPDIR`;
  - `DISPLAY`, `XAUTHORITY`, the private bus, `XDG_RUNTIME_DIR` and the
    XDG config/data/cache dirs;
  - `FONTCONFIG_FILE`, the GDK/QT/FLTK backends, `LANG=C.UTF-8`;
  - `SHOOT_RUN_ID`.

  Nothing else of the session (`AT_SPI_BUS_ADDRESS`, `SESSION_MANAGER`,
  `BROWSER`, …) reaches it.
- **Private HOME, cwd and TMPDIR** inside the run's work dir. The cwd is
  `files/`, which is also the MCP project root. The terminal panel cannot
  source the real `~/.bashrc`, and the Markdown preview writes into the
  run's `tmp/`. The Noto fonts still resolve: `FONTCONFIG_FILE` is
  explicit, and the selftest checks `fc-match sans` = Noto Sans inside the
  app env.
- **Per-run marker.** After the run, `/proc/*/environ` is swept for
  `SHOOT_RUN_ID=<run>`. Any process carrying it is killed and fails the
  run. This catches a child that left the process group with `setsid`
  (e.g. the terminal panel's shell).
- **Selftest: 18/18 ok.** The new cases:
  - the app env drops the session's variables;
  - HOME, TMPDIR and SHELL are private;
  - the shims are first in PATH;
  - the real font resolves with the private HOME;
  - the marker sweep finds a `setsid sleep 60` child started with the app
    env, and is clean after the kill.
- **Header:** two runs started at the same moment may pick the same `:9N`;
  the second Xvfb then fails and that run aborts (fails safe).
- **Runs on ee27c27:**
  - start-page and editor, light and dark, font 18: all ok, shots read,
    host clean;
  - the T0114 compare against 0.9.10-rc.2: 4× ok, 2 montages, worktree
    removed, host clean.

## User verification

2026-11-27: the user approved T0117 and its merge ("mergia 235"). The
approval covers the round-3 state, a3-approved at 7f3d1e6:
- `shoot.sh` and `compare.sh` with their isolation barriers;
- the T0114 montages;
- the page.

This commit also fixes a3's last nit: the header of `shoot.sh` said the app
starts from `$HOME`. It starts in the run's `files/` folder with
`HOME=<run>/home`.
