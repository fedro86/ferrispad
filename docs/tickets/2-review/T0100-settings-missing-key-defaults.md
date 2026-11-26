---
id: T0100
title: A settings.json missing a key gets that key's default (one rule for every field)
status: review          # todo | review | done
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
- New test `settings_missing_keys_take_the_defaults` compares all fields.

## Acceptance criteria

- [x] `{}` parses to exactly `AppSettings::default()` (test).
- [x] No field-level serde default remains on `AppSettings`.
- [x] A present key is read as before (existing tests green).
- [x] Regression test added; it failed before the fix.
- [x] Gates pass with zero warnings; Xvfb check of steps 1–3.
- [ ] Pending user verification.

## Affected files

- `src/app/domain/settings.rs` — `#[serde(default)]` on `AppSettings`;
  field attributes removed; test

## Notes

- Assigned by the orchestrator (ferrispad-5a) from the T0091 neighbour
  check.
