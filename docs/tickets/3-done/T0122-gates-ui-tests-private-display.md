---
id: T0122
title: The gates run the UI tests on a private display, never on the user's screen
status: done            # todo | review | done
created: 2026-11-28
severity: moderate
area: process
depends-on: []
issue: 244
---

## Goal

The pre-commit hook and the documented gate command run `cargo test` with
the caller's display. From the VSCode Flatpak sandbox, the gates go through
`flatpak-spawn --host bash -lc "… nix develop -c cargo test"`. That host
shell has `DISPLAY=:0` and `WAYLAND_DISPLAY=wayland-0` (checked on
2026-11-27). `tests/support/ui_layout.rs::display_available()` therefore
sees a display and runs the UI construction tests on the user's real
screen: `ui_dialogs`, `start_page_layout`, `fltk_main_thread` and the
others. Windows can flash on the user's desktop at every commit of every
session. This is the same class of leak as the 2026-11-27 incident, when
FerrisPad's file dialogs reached the user's GNOME session.

CI already does it right: `xvfb-run -a cargo test`.

## In scope

- **One gate script**, `scripts/gates.sh`: `cargo fmt --check`, then
  `cargo clippy --all-targets --all-features -- -D warnings`, then
  `cargo test`, run in the Nix dev shell. The tests run under
  `env -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET` and `xvfb-run -a`
  (from nixpkgs), with `FERRISPAD_REQUIRE_UI_TESTS=1`. The UI tests still
  run, as in CI, on a display of their own; a missing Xvfb fails the gate
  instead of skipping the UI tests.
- **The pre-commit hook** (`.claude/hooks/pre-commit-gates.sh`) calls that
  script, through `flatpak-spawn --host` from the sandbox, as today.
- **Its test** (`.claude/hooks/test-pre-commit-gates.sh`) pins it: with
  `DISPLAY`, `WAYLAND_DISPLAY` and `WAYLAND_SOCKET` set by the caller, the
  test step sees none of the caller's values, and its `DISPLAY` is a
  private one.
- **The docs name one gate command** (the script):
  - `.claude/CLAUDE.md`;
  - `.claude/rules/engineering-standards.md`;
  - `.claude/rules/work-sequence.md`;
  - `.claude/skills/new-pr/SKILL.md` §0 and §3.

## What was done

- **`scripts/gates.sh [<checkout>]`** (new). In the Nix dev shell it runs
  fmt, then clippy `-D warnings`. Then it runs `cargo test` as
  `env -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET
  FERRISPAD_REQUIRE_UI_TESTS=1 xvfb-run -a nix develop --command bash -c 'cargo test'`,
  with `xvfb-run` taken from nixpkgs. Two test seams:
  - `FERRISPAD_GATES_CMD` stands in for the cargo gates and runs as the
    test step, inside the private-display environment;
  - `FERRISPAD_GATES_XVFB` stands in for `xvfb-run -a`.
- **The hook** calls the gate script that ships next to it (the main
  checkout's copy), so a worktree on an older branch is gated the same way:
  through `flatpak-spawn --host` from the sandbox, or directly on the host.
  `FERRISPAD_GATES_CMD` keeps the run local, as before, because its
  stand-ins live in the sandbox.
- **The hook test**, case 15: the caller has `DISPLAY=:0`,
  `WAYLAND_DISPLAY=wayland-0` and `WAYLAND_SOCKET=3`. The stand-in gate
  records its environment, and an `xvfb-run` stand-in sets `DISPLAY=:99`.
  The case checks that none of the caller's three values reaches the gate,
  that `DISPLAY=:99` does, and that `FERRISPAD_REQUIRE_UI_TESTS=1` is set.
- **Review fix (ferrispad-90, on e86b8c0): the Wayland path.** fltk is
  built with `use-wayland`. Where its Wayland backend is compiled in, it
  calls `wl_display_connect(NULL)` first when `FLTK_BACKEND` is unset, and
  that falls back to `wayland-0` in `XDG_RUNTIME_DIR`, the user's
  compositor. Removing `WAYLAND_DISPLAY` does not stop that. The test step
  now also gets:
  - `FLTK_BACKEND=x11`;
  - an empty private `XDG_RUNTIME_DIR` (0700, removed on exit);
  - no `DBUS_SESSION_BUS_ADDRESS`.
- **A real-backend proof.** Under `FERRISPAD_REQUIRE_UI_TESTS=1` (the gates
  and CI), the UI tests' display check asserts two things once the display
  is open: in `tests/support/ui_layout.rs::assert_private_display_when_required`
  (shared, through `mod support;`, with `tests/fltk_main_thread.rs`'s own
  check, after the reviewer's nit):
  - FLTK is not on Wayland (`Fl_using_wayland()`);
  - on Linux, the X connection FLTK really opened
    (`XDisplayString(fltk::app::display())`) is `$DISPLAY` and not `:0`,
    `:1` or empty.

  Both print the display, for example
  `FLTK main-thread checks: FLTK on the private X display :99`. A person
  running `cargo test` on their own desktop, without that variable, is not
  affected.
- **The docs** (`.claude/CLAUDE.md`, `engineering-standards.md`,
  `work-sequence.md`, the new-pr skill §0) name the script as the one gate
  command. They say why a bare `nix develop -c cargo test` from the host
  shell is not a gate run: that shell has the user's display.

## Out of scope

- `display_available()`: a person running `cargo test` on their own
  desktop on purpose may keep doing so (the orchestrator's decision,
  2026-11-27).
- CI (`ci.yml`, `macos-gates.yml`): it already runs the tests under Xvfb on
  Linux, and macOS has no X display to leak to.
- `shoot.sh` and `compare.sh`: T0120.
- The PR page template's end-to-end "gates" step (`docs/pr/_template.html`).
  It is a recipe the user runs on their own desktop, on purpose, like the
  `display_available()` case above.
- `xvfb-run -a` picks its display by testing lock files, so it can collide
  with a concurrent Xvfb (T0120's class of race). A collision makes Xvfb
  fail to start, so the gate fails rather than reach the user's display. A
  rerun fixes it.

## How to test

### Red test first

```bash
bash .claude/hooks/test-pre-commit-gates.sh
```

A new case makes the caller's environment carry `DISPLAY=:0`,
`WAYLAND_DISPLAY=wayland-0` and `WAYLAND_SOCKET=3`. A stand-in for the test
step records the environment it sees.
- Before the fix: it FAILS. The test step sees `DISPLAY=:0` and
  `WAYLAND_DISPLAY=wayland-0`. Recorded on master (4f482a1)'s hook:
  ```text
  ok   15. gates pass in a clean worktree, caller on the user's display (exit 0)
  FAIL 15. the gate does not see the caller's DISPLAY: '^DISPLAY=:0$' is present in the gate's environment
  FAIL 15. the gate does not see WAYLAND_DISPLAY: '^WAYLAND_DISPLAY=' is present in the gate's environment
  FAIL 15. the gate does not see WAYLAND_SOCKET: '^WAYLAND_SOCKET=' is present in the gate's environment
  FAIL 15. the gate runs on the private display: '^DISPLAY=:99$' is absent in the gate's environment
  FAIL 15. the UI tests are required: '^FERRISPAD_REQUIRE_UI_TESTS=1$' is absent in the gate's environment
  18 passed, 5 failed
  ```
- After: 23 passed, 0 failed. `shellcheck` is clean on the three scripts.
- Review fix: case 15 also asserts `FLTK_BACKEND=x11`, an
  `XDG_RUNTIME_DIR` that is not the caller's, and no session bus. Red on
  e86b8c0's `gates.sh`:
  ```text
  FAIL 15. FLTK is forced onto X11: '^FLTK_BACKEND=x11$' is absent in the gate's environment
  FAIL 15. the gate does not see the caller's XDG_RUNTIME_DIR: '^XDG_RUNTIME_DIR=/run/user/4242$' is present in the gate's environment
  FAIL 15. the gate does not see the caller's session bus: '^DBUS_SESSION_BUS_ADDRESS=' is present in the gate's environment
  24 passed, 3 failed
  ```
  After: 27 passed, 0 failed.
- The display-name check has no safe red run. Its failure case is FLTK
  opening the user's `:0` or `:1`, and both belong to the user's session on
  this machine (`/tmp/.X11-unix/X0`, `X1`). Showing it would mean opening
  the user's display. It is shown green instead: every gate run prints the
  private display FLTK opened (`:99`).
- After: it passes. The three variables are not the caller's, and
  `DISPLAY` names an Xvfb display (`:99` or another, never `:0`/`:1`).

### The real gate

```bash
DISPLAY=:0 WAYLAND_DISPLAY=wayland-0 scripts/gates.sh
```

Run from a ticket worktree on the host (or through `flatpak-spawn --host`).
Expect fmt, clippy and the full test suite to be green, with the UI test
binaries reporting real passes and no `skipping UI construction tests`
line. Nothing appears on the user's screen.

Done on 2026-11-28, through `flatpak-spawn --host` (caller
`DISPLAY=:0 WAYLAND_DISPLAY=wayland-0`): exit 0, 784 passed, no `skipping`
line. With the real `xvfb-run` and a stand-in test step that prints what it
sees, the result was `inner DISPLAY=:99 WAYLAND_DISPLAY=unset REQUIRE=1`,
and `xdpyinfo` on that display answered `The X.Org Foundation`. A real Xvfb
served it.

After the review fix (2026-11-28): `scripts/gates.sh` through
`flatpak-spawn --host` exited 0 with 784 passed. The main-thread checks
printed `FLTK on the private X display :99`, and a UI binary run with
`--nocapture` printed `UI tests: FLTK on the private X display :99`. No
`ferrispad-gates-runtime.*` directory and no new Xvfb were left behind.

### Leftovers

`pgrep -af Xvfb` after the run lists no new Xvfb. The pre-existing `:98`
is known and not ours.

## Acceptance criteria

- [x] The hook's `cargo test` never sees the caller's `DISPLAY`,
      `WAYLAND_DISPLAY` or `WAYLAND_SOCKET`; the UI tests run on a private
      Xvfb and are required.
- [x] One gate script, called by the hook and named by every doc that lists
      the gates.
- [x] The hook test pins it; it failed before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.
- [x] User verified (2026-11-28).

## User verification

The user approved the PR on 2026-11-28, relayed by the orchestrator
(ferrispad-03), after reading the presentation
`docs/pr/0.9.11/PR-246-gates-ui-tests-private-display.html`. The reviewer
(ferrispad-90) approved 55cbd7b, after the Wayland-path fix and the shared
private-display assertion. This commit only moves the ticket and updates
the page.

## Affected files

- `scripts/gates.sh`: new.
- `.claude/hooks/pre-commit-gates.sh`: the gate command.
- `.claude/hooks/test-pre-commit-gates.sh`: the new case.
- `.claude/CLAUDE.md`, `.claude/rules/engineering-standards.md`,
  `.claude/rules/work-sequence.md`, `.claude/skills/new-pr/SKILL.md`: the
  gate command.

## Open question (from the review)

Why is the local FLTK built without its Wayland backend? `Cargo.toml` asks
for `fltk = { features = ["use-wayland"] }`, and `cargo tree -e features`
shows `fltk-sys` "use-wayland" enabled. Yet in the nix-dev-shell build:
- `target/debug/build/fltk-sys-*/out/include/FL/fl_config.h` has
  `#define FLTK_USE_X11 1` and `/* #undef FLTK_USE_WAYLAND */`;
- the CMake cache has no `FLTK_BACKEND_WAYLAND` entry, although the build
  script defines it under that feature;
- `ldd target/debug/FerrisPad` shows no `libwayland-client` (checked by
  ferrispad-90).

So here the leak was only through `DISPLAY=:0`, XWayland. Whether release
builds (CI, the Flatpak) get Wayland, and whether they should, may be a
packaging issue of its own. It is not answered in this ticket.

## Notes

- Found by ferrispad-39 on 2026-11-27, while preparing the T0108 gates,
  whose UI tests need a display. The orchestrator (ferrispad-03) set the
  scope. Until this lands, every manual gate run uses
  `env -u WAYLAND_DISPLAY -u DISPLAY nix shell nixpkgs#xvfb-run -c xvfb-run -a nix develop -c cargo test`.
- Until then, the pre-commit hook keeps running the UI tests on the real
  display at every commit. The sessions cannot avoid that without
  bypassing the hook, which the rules forbid.
