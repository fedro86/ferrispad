# Engineering standards — constraints, testing, gates

Source of truth: `.claude/CLAUDE.md` and `PHILOSOPHY.md`. These are enforced,
not aspirational.

## Design constraints (from PHILOSOPHY.md — hard requirements)

- **0% CPU when idle.** No background indexers, file watchers, daemons, polling
  timers, or LSP. Features are reactive to user action. Update/plugin-update
  checks run **once at startup** on a thread, then terminate.
- **Single self-contained binary, zero runtime deps.** Lua is statically
  linked. No Node/Python/JVM.
- **No telemetry** of any kind.
- **Memory frugality.** jemalloc tuned to return freed pages immediately.
- **Minimize `unsafe`; treat all external input as untrusted** — this includes
  plugin source, plugin hook return values, terminal byte streams, session
  files, and registry JSON. Most audit findings live exactly here.

Check any change against these before implementing.

## Rust quality bars

- **Zero warnings.** `cargo clippy --all-targets --all-features` and
  `cargo fmt --check` must be clean. Do not silence a lint with `#[allow(...)]`
  as the fix — restructure. Reach for `#[allow]` only with a one-line comment
  justifying why the lint is a false positive here.
- **No panics on untrusted or ordinary user input.** No raw slicing of `&str`
  by byte offsets without a char-boundary guard (`floor_char_boundary` exists
  in `services/text_ops.rs` — reuse it). No `a - 1` where `a` can be zero — use
  `saturating_sub`. No unbounded recursion on externally-shaped data — bound
  the depth.
- **`.unwrap()`/`.expect()`** only where infallibility is local and obvious;
  otherwise `?`, `let-else`, or `match`. A `.unwrap()` on a lock, a `RefCell`
  borrow, a Lua conversion, or an FLTK widget handle reachable from a callback
  is a bug, not a shortcut.
- **`Rc<RefCell<AppSettings>>`**: borrow narrowly and `drop` the borrow before
  sending Messages or calling controllers that may re-borrow.
- **Security boundaries fail closed.** A validator that can't prove a path /
  command / plugin is safe rejects it. Canonicalize before comparing; never
  hand a `..`-containing path to `create_dir_all`/`fs::write`.

## Testing

- **`cargo test` is the runner of record.** Unit tests in `#[cfg(test)]`
  modules next to the code; integration tests in `tests/`.
- **Regression test first for every defect** (see `work-sequence.md`). The test
  must fail before the fix, for the bug's reason.
- Prefer testing the pure/business logic (controllers return action enums,
  services are UI-agnostic) over driving FLTK widgets. For UI-only bugs where a
  unit test is impractical, document a concrete manual repro in the ticket and
  say so explicitly.

## Gates (all green before a commit)

```bash
nix develop -c cargo test
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
nix develop -c cargo fmt --check
```

There is currently **no CI job** running these on push/PR (only `release.yml`
and `build-deps-check.yml`). Ticket T0036 adds one; until then the pre-commit
hook is the only gate, so do not bypass it.
