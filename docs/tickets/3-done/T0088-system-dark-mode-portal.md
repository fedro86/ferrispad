---
id: T0088
title: Detect the system dark mode via the XDG desktop portal on Linux
status: done            # todo | review | done
created: 2026-11-26
severity: minor
area: ui
depends-on: []
issue: 171
---

## Goal

With **Theme → System default**, FerrisPad stays light on a dark GNOME desktop
whenever `gsettings` cannot see the user's dconf database. Today
`detect_system_dark_mode()` (`src/app/infrastructure/platform.rs`) on Linux
only asks `gsettings get org.gnome.desktop.interface gtk-theme` and
`… color-scheme`. That answer is wrong in at least three common setups:

- inside `nix develop` — nix's `gsettings` falls back to the memory GSettings
  backend and always answers `'default'`;
- under a custom `XDG_CONFIG_HOME` (dconf is looked up elsewhere);
- in sandboxes such as Flatpak.

The XDG desktop portal answers correctly in all of them:
`org.freedesktop.portal.Settings.ReadOne("org.freedesktop.appearance",
"color-scheme")` → `1` = prefer dark, `2` = prefer light, `0` = no preference.
Verified on the user's GNOME machine: inside `nix develop`, `gsettings`
answers `'default'` while the portal answers `(<uint32 1>,)`.

## In scope

- A pure decision function that maps (portal answer, gsettings answer) →
  dark/light, plus a pure parser for the portal reply. Rule: portal `1` →
  dark, portal `2` → light; portal `0`, missing or unreadable → the existing
  `gsettings` checks (unchanged) → otherwise light.
- Linux `detect_system_dark_mode()` asks the portal first: one synchronous
  `gdbus call --session --timeout 1 … Settings.ReadOne` (falls back to the
  older `Settings.Read` only when `ReadOne` is an unknown method, i.e. portal
  interface version 1). `gsettings` is spawned only when the portal is not
  decisive.
- Called at the same two moments as today: startup and Settings → Save. No
  new call sites. The desktop query runs **once** per moment: startup used to
  run it twice (in `main.rs` and again inside `AppState::apply_settings`),
  which doubled the worst case once a portal timeout exists.
- Unit tests for the parser and the decision, including the red test below.

## Out of scope

- **Following the appearance live** (the portal's `SettingChanged` signal).
  That needs a long-lived D-Bus listener, which touches the 0%-idle-CPU rule
  in `PHILOSOPHY.md`. Left as a follow-up for the user to decide.
- Windows and macOS detection (untouched).
- Any new crate. `gdbus` ships in the same glib package as the `gsettings`
  binary this code already spawns (`libglib2.0-bin` on Debian/Ubuntu,
  `glib2` on Fedora, the runtime in Flatpak), so availability is unchanged.
  A pure-Rust D-Bus client (`zbus`) or GIO FFI would add a dependency tree or
  `unsafe` for a one-shot call.

## How to test

### Regression test

```bash
nix develop -c cargo test system_appearance
```

- Red test: `system_appearance_portal_dark_beats_gsettings_default` — portal
  answers prefer-dark (`(<uint32 1>,)`), gsettings answers `'default'` /
  `'Adwaita'`. Before the fix the decision ignores the portal and returns
  **light**; after the fix it returns **dark**.
- The other `system_appearance_*` tests pin the parser (both `ReadOne` and
  the nested `Read` reply shapes, garbage, out-of-range values) and the
  fallback order.

### Manual repro (native GNOME, user)

1. Set the GNOME desktop to dark (Settings → Appearance → Dark).
2. From a terminal: `nix develop -c gsettings get org.gnome.desktop.interface color-scheme`
   → `'default'` (this is the broken input).
3. `nix develop -c cargo run`, then Settings → Theme → **System default** → Save.
4. Expect the editor to turn **dark** (before: it stayed light). Restart from
   `nix develop`: it still starts dark.
5. Switch the desktop to Light, Settings → Save again: the editor turns light.

## Acceptance criteria

- [x] With the portal answering prefer-dark and `gsettings` answering
      `'default'`, System default resolves to dark (startup and Save).
- [x] Portal prefer-light resolves to light; no portal / no preference keeps
      today's `gsettings` behaviour.
- [x] No probe can hang startup or Save: the whole Linux detection (portal
      + gsettings) is bounded by a 1.5 s hard deadline, even with a wedged
      session bus; no child process is left behind; no panic on any reply
      text.
- [x] Regression test added and green; it failed before the fix.
- [x] `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`
      and `cargo fmt --check` pass.
- [x] Settings → System default → Save driven under Xvfb with the real
      session bus (portal dark → dark) and with a bus without a portal
      (→ gsettings fallback).

## Affected files

- `src/app/services/system_appearance.rs` (new) — pure parser + decision.
- `src/app/services/mod.rs`, `src/app/infrastructure/mod.rs` — module registration.
- `src/app/infrastructure/probe.rs` (new) — `run_bounded`, the hard deadline for every external probe.
- `src/app/infrastructure/platform.rs` — Linux branch of
  `detect_system_dark_mode()` asks the portal first.
- `src/app/domain/settings.rs` — `ThemeMode::resolve_dark(detect)`, the one
  place that maps a mode to dark/light (the query runs only for
  System default).
- `src/main.rs`, `src/app/state.rs` — `apply_settings(settings, is_dark)`
  takes the already-resolved flag; startup passes `initial_dark_mode`, Save
  resolves once.

## Notes

- Origin: user verification of the 0.9.10 stack (System default stayed light
  on a dark GNOME desktop when run from `nix develop`).
- Follow-up candidate: live `SettingChanged` tracking (see Out of scope).

## What was done (2026-11-26)

- `src/app/services/system_appearance.rs` (new, pure, unit-tested):
  `ColorScheme`, `parse_portal_reply` (both `ReadOne` `(<uint32 N>,)` and
  `Read` `(<<uint32 N>>,)` shapes; input capped at 256 bytes; undefined
  values → no preference; anything else → `None`), `gsettings_prefers_dark`
  (today's rule, unchanged), `prefers_dark(portal, gsettings)` (portal
  1 → dark, 2 → light, else the lazily-called gsettings closure) and
  `session_bus_reachable`.
- `src/app/infrastructure/platform.rs`, Linux branch only:
  `read_portal_color_scheme()` runs one `gdbus call --session --timeout 1 …
  Settings.ReadOne` (stdin null); on `UnknownMethod` (portal interface v1) it
  retries with `Settings.Read`. It does **not** call `gdbus` at all when there
  is neither `DBUS_SESSION_BUS_ADDRESS` nor `$XDG_RUNTIME_DIR/bus`, because
  GIO would then autolaunch a bus daemon. `gsettings_prefers_dark()` keeps the
  old two `gsettings get` calls and their short-circuit; it only runs when the
  portal is not decisive. Windows/macOS branches untouched; call sites
  (startup, Settings → Save) unchanged.
- `ThemeMode::resolve_dark(detect)` replaces the two duplicated
  `match theme_mode` blocks; `AppState::apply_settings` now takes the resolved
  `is_dark`, so startup queries the desktop once instead of twice (test
  `theme_mode_resolve_dark_detects_only_for_system_default`: explicit modes
  never call the query).

### Red test (before the fix)

`prefers_dark` first carried today's behaviour (ignore the portal):

```
test …::system_appearance_portal_light_is_light_without_asking_gsettings ... FAILED
test …::system_appearance_portal_dark_beats_gsettings_default ... FAILED
panicked at src/app/services/system_appearance.rs:84:9: portal dark → dark
test result: FAILED. 4 passed; 2 failed
```

After the fix: `nix develop -c cargo test system_appearance` → 6 passed.
Full suite under Xvfb (`FERRISPAD_REQUIRE_UI_TESTS=1`): lib 592 passed,
all integration binaries green; clippy `-D warnings` and `fmt --check` clean.

### Xvfb campaign (Claude-driven, the user's setup reproduced)

Run from inside `nix develop` (so `gsettings` answers `'default'` /
`'Adwaita'`) under Xvfb with xdotool and screenshots; the tone is read from
the editor background pixel. "Save" = start Light → File → Settings → System
Default → Save. "Before" = a master build without this change.

| Bus / portal | Build | Startup (System default) | After Save |
|---|---|---|---|
| real session bus, GNOME portal answers `1` | before | light ✗ | light ✗ |
| real session bus, GNOME portal answers `1` | after | **dark** | **dark** |
| private bus, no portal (`ServiceUnknown`) | after | light (gsettings fallback) | light |
| private bus, mock portal `ReadOne` → 1 | after | dark | dark |
| private bus, mock portal `ReadOne` → 2 | after | light | light |
| private bus, mock portal v1 (`Read` only) → 1 | after | dark | dark |
| private bus, mock portal that never answers | after | light after the timeout | — |

With the hung mock, `gdbus call --timeout 1` returns after 1.02 s (rc 1), so
a broken portal costs at most ~1 s at startup / Save. `settings.json` keeps
`"theme_mode": "SystemDefault"` after Save in every case. Screenshots show the
whole window (menu, tab bar, editor, status bar) switching to the dark theme;
no layout changes (no UI was added). Pending user verification: native GNOME
recipe above (dark desktop, run from inside `nix develop`, expect dark).

### Availability of `gdbus`, and silence when it is missing

- Debian/Ubuntu: `gdbus` and `gsettings` are both in `libglib2.0-bin`;
  Fedora: both in `glib2`; nix: both in `glib.bin` (what `nix develop` puts
  on PATH).
- Flatpak (`packaging/flatpark/…yml`: `org.freedesktop.Platform` 26.08, no
  `--talk-name`): checked with `flatpak run --command=sh
  runtime/org.freedesktop.Platform/x86_64/26.08` — `/usr/bin/gdbus` and
  `/usr/bin/gsettings` are both in the runtime; inside that sandbox the portal
  answers `(<uint32 1>,)` (portals are reachable without extra permissions)
  while `gsettings` answers `'default'`, i.e. the same bug this ticket fixes.
- Missing `gdbus`: `Command::output()` fails to spawn → `None` → gsettings →
  light, with nothing printed (both commands' stdout/stderr are captured).
  Checked under Xvfb with `PATH=/var/empty`: no extra stderr line, window up
  in 41 ms.

### Cost on the UI thread (startup and Save)

Detection is synchronous on the UI thread and, at startup, runs **before the
main window is built**, so it delays the first paint by exactly its duration.
Measured under Xvfb, time from exec to the main window being mapped, theme
System default:

| Situation | First window |
|---|---|
| real session bus, GNOME portal (one `gdbus` call ≈ 30–50 ms) | 46–54 ms |
| `gdbus` not on PATH | 41 ms |
| no session bus at all | no `gdbus` spawned (no autolaunch) |
| bus present, portal not activatable (`ServiceUnknown`) | immediate error → gsettings |
| portal hung (mock that never answers) — before the single-query fix | 2067 / 2058 ms |
| portal hung — after (one query) | 1065 / 1042 ms |
| **wedged bus** (socket accepts, never speaks) — 2739695 | **no window after 10 s**, `gdbus` child still alive |
| wedged bus — after `run_bounded` (review fix) | 1278 / 1284 ms, no `gdbus` left |

### Review fix: a hard deadline for every probe

Review (a3) found that `gdbus --timeout 1` only bounds the method call: the
connect / auth / `Hello` handshake has no timeout, so a bus socket that
accepts and never speaks (stopped or wedged dbus-daemon/broker, a stale
forwarded address) made `Command::output()` wait forever — before the window
is built, so FerrisPad never appeared. Reproduced above (red: no window after
10 s with the 2739695 build).

Fix: `src/app/infrastructure/probe.rs::run_bounded(cmd, deadline)` — stdin
null, stdout/stderr piped, `try_wait` polled every 10 ms until the deadline,
then `kill()` + `wait()` (reaped, no zombie); output read only after exit,
capped at 4 KiB (probes answer a few bytes, far below the pipe buffer; a
flooding child blocks and is killed at the deadline). **Every** probe uses it:
both `gdbus` calls and both `gsettings` calls. One budget for the whole
detection: the portal may use the first 1.2 s (`PORTAL_BUDGET`), gsettings
whatever is left of 1.5 s (`DETECT_BUDGET`). gsettings still runs after a
portal timeout because it reads dconf directly and does not need the bus.
Tests: `probe_past_the_deadline_is_killed_and_reaped` (`sleep 5` → `None`
in 1.5 s, its `/proc/<pid>` is gone), `probe_that_floods_its_pipe_is_still_bounded`,
`probe_returns_the_output_of_a_quick_command`, `probe_missing_command_is_none`.

Also from review: an explicit `DBUS_SESSION_BUS_ADDRESS` naming the
`autolaunch:` transport (anywhere in the `;` list) now counts as unreachable,
so the guard never lets GIO spawn a daemon; the `resolve_dark` test moved
below `use super::*;`.

**Worst case** (startup before the first paint, and the Save freeze): 1.5 s,
reached only when the bus or portal is wedged *and* gsettings is slow too;
with a wedged bus and a working gsettings it is ≈1.3 s; normally 40–90 ms.
A portal v1 (`ReadOne` unknown) costs two quick calls inside the same budget
(the `UnknownMethod` error is immediate). Moving detection off the UI thread
would need a deferred re-theme after the first paint (a visible light→dark
flash on every start for the common case), so it is left as is; the user can
decide otherwise.

## User verification

2026-11-26, by the user on Fedora GNOME (Wayland) with the rc.3 integration build `verify/rc3` (checklist VERIFICA-RC3.md, section 3): with the desktop set to dark, System default follows it at startup and on Settings → Save.
