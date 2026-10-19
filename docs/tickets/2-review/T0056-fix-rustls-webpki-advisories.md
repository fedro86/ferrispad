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

## Decision: ureq 3 + rustls on ring (option 2)

Option 1 was implemented first (commit `1f284a3` on this branch). It worked:
`minreq 3.0.0` resolves to `rustls 0.23.46` and `rustls-webpki 0.103.15` with no
code change. But minreq turns on rustls's default features, so the crypto
provider becomes **aws-lc-rs** (a C library built with `cc`, plus
NASM/prebuilt-NASM on Windows), and FerrisPad cannot choose `ring` instead.
The release binary grew by **+2,853,592 bytes (+23.2 %)**.

**The user chose option 2:** `ureq 3.4` with `default-features = false,
features = ["rustls"]`. That is rustls 0.23 on the **`ring`** provider with the
bundled webpki roots, and no gzip, no aws-lc, no OpenSSL/native-tls, no platform
verifier. It costs **+1,096,592 bytes (+8.9 %)**, and it reuses `ring 0.17.14`,
which master already compiled (via rustls 0.21) on every CI platform, so the
release jobs need no new toolchain.

## What was done

- `Cargo.toml`: `minreq = { version = "2", features = ["json-using-serde",
  "https-rustls"] }` → `ureq = { version = "3.4", default-features = false,
  features = ["rustls"] }`. Resolved: `ureq 3.4.2`, `ureq-proto 0.6.5`,
  `rustls 0.23.46`, `rustls-webpki 0.103.15`, `webpki-roots 1.0.9`, `ring 0.17.14`
  (unchanged). New small crates: `http`, `httparse`, `bytes`, `base64 0.23`,
  `percent-encoding`, `utf8-zero`, `rustls-pki-types`. Gone: `minreq`, `sct`,
  `webpki-roots 0.25`.
- New `src/app/services/http.rs`: one shared `http::get(url, timeout_secs)`.
  It sets `https_only(true)`, `http_status_as_error(false)` (callers keep their
  own status → message mapping, including the 403/429 rate-limit message),
  a whole-request `timeout_global`, and the `User-Agent: FerrisPad` header. A
  `#[cfg(test)]` `get_allow_http` exists only for the loopback servers in the
  T0035 tests.
- `plugin_registry.rs`: every request goes through `http::get` with the same
  timeouts (10 s registry/API, 30 s plugin files). In `fetch_limited`, the T0035 cap
  is unchanged in shape: `response.into_body().into_reader()` (ureq's
  unlimited reader, see below) wrapped in `Read::take(max_bytes + 1)`, then the
  same `> max_bytes` check. The head is capped with
  `max_response_header_size(64 KiB)`. In ureq that limit covers the status line plus
  all headers, which replaces minreq's separate 1 KiB status-line and 64 KiB
  header limits.
- `updater.rs`: release JSON and the `.sig` are read with `read_to_vec` /
  `read_to_string` (ureq's 10 MB default body limit) and parsed with
  `serde_json`. The binary download sets an explicit
  `.with_config().limit(MAX_BINARY_BYTES)` (256 MiB), because the 0.9.5 Linux
  binary is 12,370,120 bytes, above the 10 MB default. Timeouts unchanged
  (10 s / 10 s / 120 s). The ed25519 signature check and the
  "write to disk only after verification" order are untouched.
- The port came from a measurement spike. I ported it by hand and checked it
  against the ureq 3.4.2 source:
  - `Body::into_reader` is documented as "not limited by default", so the only
    limit is our `take`.
  - `https_only` is checked in `call_run` on every redirect hop.
  - The `rustls` feature = `rustls-no-provider` + `_ring` + `rustls-webpki-roots`.
  - `max_response_header_size` covers "the status, including all headers".
  - Defaults are `max_redirects: 10`, `max_redirects_will_error: true` and
    `proxy: Proxy::try_from_env()`.
  The regenerated `Cargo.lock` has exactly the same package set as the spike's.

### Behaviour differences against minreq (all intentional or harmless)

- **https-only is now enforced by the client:** an `http://` URL, and any
  `https → http` redirect, is refused (`RequireHttpsOnly`). Before, only the
  hard-coded `https://` URLs gave TLS, and a redirect to plain HTTP was followed.
- **At most 10 redirects** (minreq allowed 100). GitHub asset downloads need 1.
- **Proxy environment variables are honoured** (`ALL_PROXY` / `HTTPS_PROXY` /
  `HTTP_PROXY` / `NO_PROXY`, via `Proxy::try_from_env`). minreq ignored them.
  TLS still runs end to end to the real host and is verified against webpki-roots.
- **Updater JSON and `.sig` reads are capped at 10 MB** (ureq default). They had
  no cap before. The binary is capped at 256 MiB.
- **Inner error wording changes**, e.g. `Failed to connect to update server:
  <ureq error>`. The FerrisPad-level prefixes and the rate-limit / HTTP-status
  messages are the same.

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
└── rustls v0.23.46
    └── ureq v3.4.2
        └── FerrisPad v0.9.5

$ nix develop -c cargo tree -i aws-lc-sys
error: package ID specification `aws-lc-sys` did not match any packages

$ nix develop -c cargo tree -i ring -e normal
ring v0.17.14
├── rustls v0.23.46
│   └── ureq v3.4.2
│       └── FerrisPad v0.9.5
└── rustls-webpki v0.103.15
    └── rustls v0.23.46 (*)
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

| | `stat -c %s target/release/FerrisPad` | Delta vs master |
|---|---|---|
| master (minreq 2.14.1, rustls 0.21 + ring) | 12,292,144 bytes | — |
| minreq 3.0.0 (rustls 0.23 + aws-lc-rs), rejected | 15,145,736 bytes | +2,853,592 (+23.2 %) |
| **ureq 3.4.2 (rustls 0.23 + ring), chosen** | **13,388,736 bytes** | **+1,096,592 (+8.9 %)** |

### Tests run

- Gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D
  warnings` and the full `cargo test` all green (lib: 469 passed, 1 ignored;
  every integration-test binary green).
- `cargo test --lib plugin_registry::tests`: 19 passed, 1 ignored, including
  `oversized_download_aborts_mid_stream` and `download_within_limit_is_returned`
  (T0035 cap guard, now through ureq's reader).
- `cargo test --lib updater`: 11 passed.
- Network (reachable):
  - `cargo test --lib plugin_registry -- --ignored` →
    `fetch_limited_reads_real_registry_and_plugin_files ... ok` (real HTTPS
    through the streaming path, no disk cache).
  - `cargo test --test plugin_registry_fetch -- --ignored` →
    `test_fetch_official_registry ... ok`, `test_fetch_community_registry ... ok`.
    These two go through the 1-hour disk cache, so they may not touch the network.
- One-off updater check (a scratch integration test, not committed): the real
  `fetch_latest_release` on both the Beta and Stable channels returned 0.9.5.
  `download_and_verify` followed GitHub's asset redirect, downloaded
  12,370,120 bytes (above ureq's 10 MB default, so the explicit limit is what
  makes it pass), fetched the `.sig` and reported "Signature verified successfully".

## How to test

```bash
nix develop -c cargo tree -i rustls-webpki    # every version >= 0.103.13
nix develop -c cargo tree -i aws-lc-sys       # no match: ring is the only provider
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
- [x] Binary size delta recorded (+1,096,592 bytes, +8.9 %).
- [x] Gates green.

## Affected files

- `Cargo.toml`, `Cargo.lock`
- `src/app/services/http.rs` (new), `src/app/services/mod.rs`
- `src/app/services/updater.rs`, `src/app/services/plugin_registry.rs`

## Notes

- Origin: Dependabot alerts #1–#3, planned for 0.9.6 since 2026-08-24.
- Windows/macOS release builds were not run locally. `ring` was already built
  there for rustls 0.21, so no new toolchain requirement is expected.
