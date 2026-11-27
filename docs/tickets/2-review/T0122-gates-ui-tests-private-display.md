---
id: T0122
title: The gates run the UI tests on a private display, never on the user's screen
status: review          # todo | review | done
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

### Leftovers

`pgrep -af Xvfb` after the run lists no new Xvfb. The pre-existing `:98`
is known and not ours.

## Acceptance criteria

- [ ] The hook's `cargo test` never sees the caller's `DISPLAY`,
      `WAYLAND_DISPLAY` or `WAYLAND_SOCKET`; the UI tests run on a private
      Xvfb and are required.
- [ ] One gate script, called by the hook and named by every doc that lists
      the gates.
- [ ] The hook test pins it; it failed before the fix.
- [ ] `cargo test`, `cargo clippy --all-targets --all-features`, and
      `cargo fmt --check` all pass with zero warnings.
- [ ] Pending user verification.

## Affected files

- `scripts/gates.sh`: new.
- `.claude/hooks/pre-commit-gates.sh`: the gate command.
- `.claude/hooks/test-pre-commit-gates.sh`: the new case.
- `.claude/CLAUDE.md`, `.claude/rules/engineering-standards.md`,
  `.claude/rules/work-sequence.md`, `.claude/skills/new-pr/SKILL.md`: the
  gate command.

## Notes

- Found by ferrispad-39 on 2026-11-27, while preparing the T0108 gates,
  whose UI tests need a display. The orchestrator (ferrispad-03) set the
  scope. Until this lands, every manual gate run uses
  `env -u WAYLAND_DISPLAY -u DISPLAY nix shell nixpkgs#xvfb-run -c xvfb-run -a nix develop -c cargo test`.
- Until then, the pre-commit hook keeps running the UI tests on the real
  display at every commit. The sessions cannot avoid that without
  bypassing the hook, which the rules forbid.
