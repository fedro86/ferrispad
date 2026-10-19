---
id: T0056
title: Upgrade the TLS stack past the rustls-webpki advisories
status: review
created: 2026-10-19
severity: severe
area: security
depends-on: []
issue: 95
---

## Goal

GitHub Dependabot reports 3 open alerts on `master`, all against
`rustls-webpki 0.101.7` in `Cargo.lock`:

| Alert | Severity | Advisory | Fixed in |
|---|---|---|---|
| #3 | high | DoS via panic on a malformed CRL BIT STRING | 0.103.13 |
| #2 | low | name constraints accepted for certificates asserting a wildcard name | 0.103.12 |
| #1 | low | name constraints for URI names incorrectly accepted | 0.103.12 |

`cargo tree -i rustls-webpki` shows `minreq 2.14.1 → rustls 0.21.12 →
rustls-webpki 0.101.7`. `minreq` is FerrisPad's only HTTP client, used by the
updater (`services/updater.rs`) and the plugin registry
(`services/plugin_registry.rs`). Both download over HTTPS and verify signed
binaries and plugins, so certificate validation is security-relevant. These
alerts have to be gone before the 0.9.6 release, which is the first FlatPark
build (T0047).

## In scope

- Find the smallest change that resolves `rustls-webpki` to `>= 0.103.13`, in this
  order of preference:
  1. a newer `minreq` (2.x or 3.x) that uses `rustls 0.23` with webpki 0.103;
  2. otherwise, swap `minreq` for another small blocking client with rustls
     (e.g. `ureq`). Keep the same features: JSON, rustls, no native-tls/OpenSSL
     (single-binary constraint), timeouts, and the streaming size cap from T0035.
- Keep every security property the HTTP code has today: the T0035 streaming
  download cap, timeouts, TLS-only URLs, and the updater's signature check.
- Record the release-binary size before and after.

## Out of scope

- The five open Dependabot version-bump PRs (#22–#26: windows, open, mlua,
  semver, winreg). Each needs its own decision, and `mlua 0.11` changes the
  plugin runtime API.
- Any change to what gets downloaded, or when.

## What was done

Option 1 worked: **`minreq` 2.14.1 → 3.0.0** (published 2026-06-15). With the
same features (`json-using-serde`, `https-rustls`) it depends on `rustls ^0.23.35`,
`rustls-webpki ^0.103.8` and `webpki-roots ^1`. The resolved lock is
`rustls 0.23.46`, `rustls-webpki 0.103.15`, `webpki-roots 1.0.9`. No client swap.

- `Cargo.toml`: `minreq = { version = "3", ... }`. `Cargo.lock` updated with
  `cargo update -p minreq` only.
- No call site had to change. The minreq 3 API changes that touch FerrisPad are
  all source-compatible here: `status_code` is now `u16` (comparisons against
  literals still compile), `ResponseLazy` drops its `Iterator` impl but keeps
  `Read` (T0035's cap already uses `Read::take`), and `with_timeout`,
  `with_max_headers_size`, `with_max_status_line_length`, `send_lazy`, `json`,
  `as_str` and `as_bytes` are unchanged. The one edit in
  `plugin_registry.rs` is a stale comment ("both an `Iterator` and a `Read`").
- Security properties are unchanged: the T0035 streaming cap (and the explicit
  status-line/header limits), every timeout (10 s / 30 s / 60 s / 120 s), the
  hard-coded `https://` URLs, webpki-roots as the trust store (no platform
  verifier, no native-tls/OpenSSL) and the updater's ed25519 signature check.

### Side effects in the lock

- rustls 0.23's default crypto provider is **aws-lc-rs** (`aws-lc-rs 1.18.2`,
  `aws-lc-sys 0.46.0`, a statically linked C library built with `cc`), in place of
  `ring`, which is no longer compiled in. minreq turns on rustls's default
  features, so FerrisPad cannot pick `ring` instead without a client swap (or a
  process-wide `CryptoProvider` install, which is a code change).
- Build-only crates pulled along: `cc 1.2.38 → 1.9.0`, `find-msvc-tools
  0.1.2 → 0.1.15`, `shlex 1.3.0 → 2.0.1`, plus new `jobserver`, `dunce`,
  `fs_extra`. `sct` and `webpki-roots 0.25` are gone; `rustls-pki-types` is new.
- Windows: aws-lc-sys needs NASM for x86-64, but rustls enables
  `aws-lc-rs/prebuilt-nasm`, so the `windows-latest` release job uses the
  prebuilt objects when NASM isn't installed. This was **not** built on Windows or
  macOS locally. The first tag build (or a manual `release.yml` dry run) confirms it.

### Evidence (dependency advisory, so the "red" run is the lock itself)

Before (`origin/master`):

```
$ nix develop -c cargo tree -i rustls-webpki
rustls-webpki v0.101.7
├── minreq v2.14.1
│   └── FerrisPad v0.9.5
└── rustls v0.21.12
    └── minreq v2.14.1 (*)
```

After:

```
$ nix develop -c cargo tree -i rustls-webpki
rustls-webpki v0.103.15
├── minreq v3.0.0
│   └── FerrisPad v0.9.5
└── rustls v0.23.46
    └── minreq v3.0.0 (*)
```

`nix run nixpkgs#cargo-audit -- audit` (cargo-audit 0.22.2, 1338 advisories):

| | Before | After |
|---|---|---|
| Vulnerabilities | **7** | **2** |
| rustls-webpki 0.101.7: RUSTSEC-2026-0098, -0099, -0104 (= Dependabot #1–#3) | present | gone |
| rustls 0.21.12: RUSTSEC-2026-0346 (Debug leaks TLS secrets), RUSTSEC-2026-0349 (ServerHello KEX downgrade), fixed in 0.23.46 | present | gone |
| quick-xml 0.38.4: RUSTSEC-2026-0194, -0195 (via `syntect → plist`) | present | present (out of scope) |

The 7 "allowed warnings" (unmaintained/unsound: bincode, paste, ttf-parser,
unsafe-libyaml, yaml-rust, anyhow, memmap2) are the same before and after.

### Release-binary size (`nix develop -c cargo build --release`, Linux x86-64)

| | `stat -c %s target/release/FerrisPad` |
|---|---|
| Before (minreq 2.14.1, rustls 0.21 + ring) | 12,292,144 bytes |
| After (minreq 3.0.0, rustls 0.23 + aws-lc-rs) | 15,145,736 bytes |
| Delta | **+2,853,592 bytes (+23.2 %)** |

Nearly all of it is aws-lc. This is the price of option 1. If it is too much,
the alternative is option 2 (a client that builds rustls with `ring`, e.g. `ureq`)
in a follow-up ticket.

### Tests run

- `cargo test --lib plugin_registry::tests`: 19 passed, 1 ignored, including
  `oversized_download_aborts_mid_stream` and `download_within_limit_is_returned`
  (T0035 cap guard).
- `cargo test --lib updater`: 11 passed.
- Network (reachable): `cargo test --lib plugin_registry -- --ignored` →
  `fetch_limited_reads_real_registry_and_plugin_files ... ok` (real HTTPS
  through the streaming path, no cache). `cargo test --test plugin_registry_fetch
  -- --ignored` → `test_fetch_official_registry ... ok`,
  `test_fetch_community_registry ... ok`.
- One-off updater check (a scratch integration test, not committed): the real
  `fetch_latest_release` + `download_and_verify` against GitHub fetched 0.9.5,
  followed the asset redirect, downloaded 12,370,120 bytes, fetched the `.sig` and
  reported "Signature verified successfully".
- Gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D
  warnings` and `cargo test` all green.

## How to test

```bash
nix develop -c cargo tree -i rustls-webpki    # every version >= 0.103.13
nix develop -c cargo test --lib plugin_registry::tests
nix develop -c cargo test --lib updater
nix develop -c cargo test --lib plugin_registry -- --ignored      # real network
nix develop -c cargo test --test plugin_registry_fetch -- --ignored
nix run nixpkgs#cargo-audit -- audit         # only the 2 quick-xml advisories remain
```

- The T0035 tests `oversized_download_aborts_mid_stream` and
  `download_within_limit_is_returned` stay green. They are the regression guard
  for the size cap through the new client.
- After the push: `gh api 'repos/{owner}/{repo}/dependabot/alerts?state=open'`
  lists no `rustls-webpki` alerts. Dependabot re-scans `master`, so this can only
  be confirmed after landing; record it then.

### Manual

1. `nix develop -c cargo run`, Help → Check for Updates: the check completes
   with no TLS error. This debug build is 0.9.5, the latest release, so expect
   "up to date".
2. Plugin Manager: the Official and Community lists load. To force a real
   network fetch rather than the 1-hour disk cache, first delete
   `~/.config/ferrispad/cache/plugins.json` and `community-plugins.json`.
   Install or update one plugin: it succeeds and appears in the plugin list.

## Acceptance criteria

- [x] `cargo tree -i rustls-webpki` shows no version below 0.103.13 (0.103.15).
- [x] Size cap, timeouts and signature checks unchanged, and their tests green.
- [x] Binary size delta recorded (+2,853,592 bytes, +23.2 %).
- [x] Gates green.

## Affected files

- `Cargo.toml`, `Cargo.lock`
- `src/app/services/plugin_registry.rs` (one stale comment). `updater.rs` was
  not changed: the minreq 3 API is source-compatible with every call.

## Notes

- Origin: Dependabot alerts #1–#3, planned for 0.9.6 since 2026-08-24.
