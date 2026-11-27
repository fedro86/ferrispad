---
id: T0100
title: A settings.json missing a key gets that key's default (one rule for every field)
status: done            # todo | review | done
created: 2026-11-26
severity: minor
area: robustness
depends-on: []
issue: 188
---

## Goal

A `settings.json` without `"tabs_enabled"` turns tabs **off**, while a fresh
install (no settings file) has tabs **on**. Any file that lacks the key, hand
written or left by a version from before tabs existed, starts FerrisPad in a
different mode from a new user's. Found while setting up the T0091 Xvfb
config.

## Cause

- `AppSettings` gets defaults for missing keys from field-level serde
  attributes. `#[serde(default)]` on a field uses the *field type's*
  `Default` (`false` for a bool), not `AppSettings::default()`.
- `tabs_enabled` has `#[serde(default)]` (→ `false`), while
  `AppSettings::default()` says `true`. The two diverged in e6cb050
  (Feb 2026), which changed the fresh-install value to `true` but not the
  serde fallback.
- Every other field agrees today, each by hand. Nothing stops the next
  field from diverging the same way.

## In scope

- One rule: `#[serde(default)]` on the **struct**, so serde fills every
  missing key from `AppSettings::default()`. The per-field `default`
  attributes are removed, because each one is a second place that can
  disagree. The lenient `deserialize_with` theme and window fields keep
  their deserializers.
- A test that parses `{}` and compares it field by field with
  `AppSettings::default()`, so a future field cannot diverge unnoticed.

## Out of scope

- Changing any default value. The values stay those of
  `AppSettings::default()`.
- Migrating existing settings files. A key that is present is read as
  before.

## Behaviour change (intended)

Only a `settings.json` that has **no** `tabs_enabled` key changes: it now
gets tabs on, like a fresh install. Every save writes all keys, so a file
lacks the key only if it was written by hand or by a version from before
tabs, and never saved since. A file with `"tabs_enabled": false` keeps tabs
off.

## How to test

### Regression test

```bash
nix develop -c cargo test settings_missing_keys
```

- `domain::settings::tests::settings_missing_keys_take_the_defaults`:
  `AppSettings::from_json_or_default("{}")` serialised equals
  `AppSettings::default()` serialised; the failure message lists every
  mismatched field.
  - Before the fix: `tabs_enabled: missing -> false, default true`, the
    only mismatch.
- The existing settings tests (`tests/settings_persistence.rs`, the
  theme-slot leniency tests) stay green: present keys and malformed values
  behave as before.

### Manual (Xvfb by Claude, then the user)

1. Fresh config dir with `settings.json` = `{"highlighting_enabled":true}`.
2. Start FerrisPad. Expect the start page and the tab bar, as with no file
   at all. Before: no tabs.
3. With `{"tabs_enabled":false}`: no tabs, as before.

Red run, before the fix (the test on the unchanged struct):

```text
test app::domain::settings::tests::settings_missing_keys_take_the_defaults ... FAILED
tabs_enabled: missing -> false, default true
```

After the fix it passes. All 25 settings unit tests and
`tests/settings_persistence.rs` stay green. Gates: 659 passed, clippy
`-D warnings` and fmt clean.

### Xvfb results (driven by Claude, debug build; "before" = `master` 7ea980d release)

| settings.json | before | after |
|---|---|---|
| `{"highlighting_enabled":true}` (no tabs key) | editor without tabs | **start page with tabs** (as a fresh install) |
| `{"tabs_enabled":false}` | no tabs | no tabs (unchanged) |

## What was done

- `#[serde(default)]` on `AppSettings`, with a comment giving the rule.
- Every field-level `default` / `default = "…"` attribute was removed (30
  attributes), including the `default` inside the `deserialize_with`
  attributes of the two theme slots and the window geometry. Those keep
  their lenient deserializers.
- The `default_*` functions stay: `AppSettings::default()` uses them, so
  there is one source of each value.
- **The same rule for the nested settings types** (review note from
  ferrispad-a3, generalised by the orchestrator):
  - `PluginApprovals` and `PluginConfig` get a container
    `#[serde(default)]`; their field-level defaults are removed and the
    derived `Default` matches.
  - `ShortcutOverride` was a latent trap. Its derived `Default` had
    `enabled = false`, while a missing key gave `true`
    (`default = "default_true"`). And `{}` did not parse at all
    (`shortcut` was required), so one malformed override reset the *whole*
    settings file to defaults.
  - It now has a container default and a hand-written `Default` (unbound,
    enabled). `default_true` is gone.
  - `WindowGeometry` is **deliberately strict**: no container default,
    because a geometry without coordinates is meaningless. A partial
    object is dropped by the lenient `window` deserializer (the window opens
    at 640×480) rather than defaulted to 0×0. Only `maximized` may be
    missing. Pinned by `settings_window_geometry_is_strict` and documented
    on the struct.
  - The test now runs the `{}` == `Default` comparison on `AppSettings`,
    `PluginApprovals`, `ShortcutOverride` and `PluginConfig`. Red on the
    old code:
    `ShortcutOverride: {} does not parse: missing field shortcut`.
- New test `settings_missing_keys_take_the_defaults` compares all fields.

- **One invalid key never resets the others** (orchestrator condition on
  the strict `WindowGeometry`, generalised to every field).
  - Before, *any* value that failed to parse reset the whole file to
    defaults: an unknown enum (`"theme_mode": "Purple"`), a wrong type
    (`"tab_size": "wide"`), or a `ShortcutOverride` without `shortcut`.
  - `from_json_or_default` now falls back, only on that error path, to
    `salvage()`. It starts from the defaults, adds the user's keys one at a
    time, keeps each one that still parses, and drops (and logs) each one
    that does not.
  - This needs no per-field annotation, so a new field is covered
    automatically. A file that is not JSON, or not an object, still yields
    all defaults.
  - Where a dropped value goes: invalid keys are logged once at load, and
    the next settings save writes the file without them (normalised to the
    default).
  - Test `settings_one_bad_key_keeps_the_others`. Red:
    `valid keys kept  left: 16  right: 17` (the whole file had been reset).
- **A shortcut override without a "shortcut" key is ignored** (orchestrator
  decision on ferrispad-a3's note).
  - With the container default it would have parsed as `""`, i.e.
    *unbound*, silently removing a working shortcut.
  - `shortcut_overrides` now goes through a lenient map deserializer that
    drops such entries, and entries that fail to parse, with a log line. The
    built-in shortcut stays.
  - Explicit unbinding is still `"shortcut": ""`.
  - Test `settings_shortcut_override_without_key_is_ignored`: File/Save
    without the key keeps Ctrl+S, File/Open… with `""` is unbound, and the
    other keys of the file are kept. Red: the entry was kept.
- **Proposed CHANGELOG line** for the release that ships this: "Settings:
  one invalid or incomplete value in settings.json (e.g. a shortcut
  override without its key) no longer resets every setting to its default;
  only that value is ignored."

## Acceptance criteria

- [x] `{}` parses to exactly `AppSettings::default()` (test).
- [x] No field-level serde default remains on `AppSettings`.
- [x] A present key is read as before (existing tests green).
- [x] Regression test added; it failed before the fix.
- [x] Gates pass with zero warnings; Xvfb check of steps 1–3.
- [x] User verification.

## Affected files

- `src/app/domain/settings.rs` — `#[serde(default)]` on `AppSettings`,
  `PluginApprovals`, `ShortcutOverride` (with a hand-written `Default`) and
  `PluginConfig`; field attributes removed; `WindowGeometry` documented as
  strict; tests

## Notes

- Assigned by the orchestrator (ferrispad-5a) from the T0091 neighbour
  check.

## User verification

2026-11-26, by the user on Fedora GNOME (Wayland) with the rc.3 integration
build `verify/rc3` (checklist `VERIFICA-RC3.md`).
