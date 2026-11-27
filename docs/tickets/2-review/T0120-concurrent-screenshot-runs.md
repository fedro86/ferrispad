---
id: T0120
title: compare.sh and shoot.sh runs are safe to run concurrently
status: review          # todo | review | done
created: 2026-11-27
severity: minor
area: process
depends-on: []
issue: 242
---

## Goal

`.claude/skills/new-pr/compare.sh` builds the "before" binary in a
throwaway worktree at a fixed path, `ferrispad-wt/compare-<before sha>`.
Two sessions that compare against the same base share that path, and they
race. On 2026-11-27 two concurrent runs failed:
- one run's `EXIT` trap ran `git worktree remove --force` on the path while
  the other run was still building in it;
- what was left was a directory holding only `target/`. The next run saw
  that the directory existed, skipped `git worktree add`, and failed with
  `could not find a flake.nix`, with no hint about why.

`shoot.sh` has the same class of race. It picks its Xvfb display by testing
`/tmp/.X$n-lock` and `/tmp/.X11-unix/X$n`, and only later starts
`xvfb-run -n $n`: check, then start. On 2026-11-27 (found by ferrispad-3b)
two runs took `:91`. One run's shots captured the other run's app, and its
input crossed over. The header's claim that the second run "fails safe"
did not hold.

Several sessions open PRs at once, so both tools must be safe to run
concurrently.

## In scope

- Every run makes its own worktree at a unique path (a `mktemp -d` under
  `ferrispad-wt/`, named after the base sha). Its `EXIT` trap removes only
  that worktree.
- `compare.sh` never reuses an existing directory as the "before"
  worktree. If `git worktree add` fails, the run stops with a message that
  names the path.
- Optional: a shared cargo target directory per base sha, so a second run
  does not rebuild every dependency. Only if it is safe: the build and the
  copy of the binary into the run's own directory happen under one lock.
  Cargo's own lock covers only the build, and a run must never execute a
  binary that another run is relinking.
- `shoot.sh` gets its display atomically, either from Xvfb itself
  (`-displayfd`) or through a reservation no other run can take
  (`mkdir`/`flock`). It checks that the display's server is the Xvfb this
  run started, and that every window it shoots or types into belongs to
  this run's app pid. A selftest case covers the reservation if feasible.
- `docs/pr/README.md` or `SKILL.md`: one line saying that concurrent runs
  are safe.

## What was done

- **`compare.sh`: a worktree per run.**
  - "before" is built in `mktemp -d ferrispad-wt/compare-<sha>.XXXXXX`,
    never in an existing directory;
  - a failed `git worktree add` stops the run, naming the path;
  - the `EXIT` trap removes only that worktree (unless `--keep`).
- **`compare.sh`: a shared target, under one lock.**
  - "before" builds with `CARGO_TARGET_DIR=ferrispad-wt/compare-target`.
  - The build, the copy of the binary into the run's own worktree
    (`FerrisPad-before`, with `cp -p`, so the stale-build check still
    sees the link time) and `cargo clean -p FerrisPad` all run under
    `flock compare-target.lock`.
  - A run never executes a binary that another run is relinking. Only
    dependencies stay in the shared target: 890 MB, with no FerrisPad
    artifacts left after a run.
- **`shoot.sh`: an atomic display reservation.**
  - The outer half takes `:N` only with `flock -n` on
    `/tmp/ferrispad-shoot-display-N.lock`, and only if `:N` has no X lock
    or socket.
  - The lock fd stays open in `xvfb-run` and Xvfb, so the kernel releases
    it when the run ends, even on kill -9. The inner half closes it, so the
    app does not hold it.
- **`shoot.sh`: the X server must be this run's own.**
  - The inner half refuses a display whose server (the pid in
    `/tmp/.X<n>-lock`) does not have as its parent one of the inner
    shell's ancestors below the outer shell (`SHOOT_OUTER_PID`).
  - The walk stops there on purpose. `systemd --user` is an ancestor of
    every run, and of other Xvfbs too: a first version that walked to pid 1
    accepted `:98`, and the selftest caught it.
  - Window lookups were already filtered by the app's pid.
- **Selftest**, three new checks:
  - the server on `DISPLAY` is this run's own;
  - the server check refuses `:98` and `:0`;
  - this run holds its display reservation.
- **`SKILL.md`**: one paragraph saying concurrent runs are safe, and how.
- `shellcheck`: two new `info` findings, of kinds already present. One is
  SC2016, the selftest's single-quoted `eval` strings. The other is
  SC2329, the trap's function. No warnings.

## Out of scope

- `shoot.sh`'s other barriers (private D-Bus, XDG dirs, shims): each run
  already has its own.
- The "after" build: it runs in the caller's own worktree, which no other
  session uses.
- Cleaning up stale `compare-*` directories that runs from before this fix
  left behind (only a note in the ticket).

## How to test

### Red repro (on master)

Two scripted cases, run from a ticket worktree with the `start-page.sh`
scenario. Each one is noted before → after. They use `--before 0.9.10-rc.1`
(sha `c0e189e`, a ref no other session compares against),
so the shared directory is `compare-<rc.1 sha>` and real runs of other
sessions against master are not disturbed.

1. **A leftover directory.** `mkdir -p ../compare-$(git rev-parse --short=7 '0.9.10-rc.1^{commit}')/target`,
   then `.claude/skills/new-pr/compare.sh docs/pr/scenarios/start-page.sh ~/Code/personal/ferrispad-wt/uitest-T0120 --before 0.9.10-rc.1`.
   - Before: the run skips `git worktree add`, and the build fails with
     `could not find a flake.nix`.
   - After: the leftover directory is ignored (or the run fails loudly,
     naming it), and the run succeeds in its own worktree.
2. **Two concurrent runs.** Start the same `compare.sh … --before 0.9.10-rc.1`
   command twice at once, with two outdirs (`… uitest-T0120-a` and `… uitest-T0120-b`), and
   wait for both.
   - Before: at least one run fails, because the other run's trap removed
     the shared worktree mid-build, or the two `cargo build`s and the trap
     interleave.
   - After: both exit 0, with identical `before` shots.

3. **Two `shoot.sh` runs at once.** Run `shoot.sh` twice at the same
   moment on two outdirs. For a display nobody else uses, the repro first
   holds the lower displays with its own `Xvfb` processes, which it kills
   afterwards.
   - Before: both runs pick the same `:9N`. One run's shots show the other
     run's app, or that run fails.
   - After: two distinct displays; each run's log names its own, and each
     shot shows its own app.

The exact failing output of every case is recorded here before the fix.

#### Red runs on master (818970b's `compare.sh`), 2026-11-27/28

- **Case 1**, a leftover `compare-c0e189e/` holding only `target/`:
  ```text
  before: 0.9.10-rc.1 (c0e189e) in /home/fedro/Code/personal/ferrispad-wt/compare-c0e189e
  path "/home/fedro/Code/personal/ferrispad-wt/compare-c0e189e" does not contain a 'flake.nix', searching up
  error: could not find a flake.nix file
  compare.sh: the build failed in /home/fedro/Code/personal/ferrispad-wt/compare-c0e189e
  ```
  Exit 1. The trap's `git worktree remove` failed quietly, so the leftover
  directory stayed.
- **Case 2**, two concurrent runs. The timing matters: two runs started 2 s
  apart, or B started when A reached `after:`, both finished green, because
  B's "before" shots were done before A's trap fired. B started when A had
  taken its last shot (A was writing its montages):
  ```text
  A exit=0
  B: realpath: /home/fedro/Code/personal/ferrispad-wt/compare-c0e189e/target/debug/FerrisPad: No such file or directory
  B: realpath: /home/fedro/Code/personal/ferrispad-wt/compare-c0e189e/target/debug/FerrisPad: No such file or directory
  B: no after shot for *
  B exit=1
  ```
  A's `EXIT` trap removed the shared worktree under B. The driver script
  starts B when A's log shows `ok: start-page (dark) -> …/after`.
- **Case 3**, two `shoot.sh` runs at the same instant (`editor.sh`). The
  copy of master's `shoot.sh` starts its search at `:150` instead of
  `:90`, so no other session's display is involved:
  ```text
  A: display: :150 ... X I/O error
  A: shoot.sh: the app exited during the scenario
  B: display: :150
  B: shoot.sh: no window matching 'Find' for shot find-dialog
  A exit=1, B exit=1
  ```
  Both took `:150`. B's second Xvfb did not start, and `xvfb-run` ran B's
  scenario on A's server: B's Ctrl+F opened the Find dialog in A's app.
  When B ended, A's app lost its display.

#### After the fix (same drivers, 2026-11-28)

- Case 1: the leftover `compare-c0e189e/` is ignored. The run builds in
  `compare-c0e189e.WeoWqQ` and exits 0.
- Case 2: A and B each have their own worktree (`….yWfXIb`, `….yj0fJs`).
  Both exit 0, and their `before` shots are byte-identical. No `compare-*`
  worktree or directory is left; only `compare-target/` and its lock.
- Case 3: A gets `:150`, B gets `:151`, both exit 0, and each shot its own
  Find dialog.
- `shoot.sh --selftest`: every check ok, including the three new ones.

### Checks after the fix

- `.claude/skills/new-pr/shoot.sh --selftest <dir>` still passes. This is
  a guard: `shoot.sh` itself is not changed.
- After both runs, `git worktree list` shows no `compare-*` entry, and
  `ls ../ | grep compare-` lists no worktree directory (a shared target
  directory may remain if it is kept).
- Leftover check on the host:
  `flatpak-spawn --host bash -c 'pgrep -af "zenity|Xvfb|dbus-daemon --config-file" | grep -v pgrep || echo clean'`
  prints `clean`. The pre-existing Xvfb `:98` is known and not ours.
- `bash -n` and `shellcheck` on `compare.sh` are clean.
- Gates: `nix develop -c cargo test`, clippy `-D warnings` and fmt stay
  green. No Rust changes are expected.

## Acceptance criteria

- [ ] Two concurrent `compare.sh` runs against the same base both succeed.
- [ ] A leftover `compare-<sha>` directory is never reused as a worktree.
- [ ] Each run removes only its own worktree.
- [ ] Two concurrent `shoot.sh` runs never share a display, and a run never
      shoots or types into a window that is not its own app's.
- [ ] Red repro recorded: both cases fail on master, for the reasons in the
      Goal.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.
- [ ] Pending user verification.

## Affected files

- `.claude/skills/new-pr/compare.sh`: the worktree path, the trap, the
  build (and the shared target, if kept).
- `.claude/skills/new-pr/shoot.sh`: the display reservation, the server
  and window ownership checks, the header comment, the selftest.
- `.claude/skills/new-pr/SKILL.md` or `docs/pr/README.md`: one line.

## Notes

- The `shoot.sh` display race was added to this ticket by the orchestrator
  (ferrispad-03) on 2026-11-27, from ferrispad-3b's report: the same class
  of bug, tool runs that are not safe concurrently.
- Reported by the reviewer (ferrispad-90) on 2026-11-27, while reviewing
  PR #215 concurrently with another session's run. The orchestrator
  (ferrispad-03) asked for this ticket, target 0.9.11.
- A per-run worktree means a cold build of the base (about 1.5 min) unless
  the target directory is shared. That is why the shared target is in
  scope, but only behind a lock.
