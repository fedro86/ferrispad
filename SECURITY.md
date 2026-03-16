# Security Policy

FerrisPad is designed with security as a core principle. This document outlines our security model, what we protect against, and our roadmap.

## Current Status

| Area | Status | Notes |
|------|--------|-------|
| Core Editor (Rust) | ✅ Stable | Memory-safe by design |
| Plugin System (Lua) | ✅ Sandboxed | Stdlib blocked, path sandbox, instruction/memory limits |
| Auto-Updates | ✅ Stable | HTTPS only, no auto-install |
| Plugin Downloads | ✅ Stable | HTTPS + ed25519 signature verification |

## Core Principles

1. **No Telemetry** - Zero data collection, no phone-home
2. **Single Binary** - No external runtime dependencies to compromise
3. **Memory Safety** - Rust prevents buffer overflows, use-after-free
4. **Minimal Privileges** - No elevated permissions required

## Rust Codebase Security

### What We Do

- **No `unsafe` in parsing code** - File content treated as untrusted
- **Input validation** - UTF-8 verification, bounds checking
- **Dependency auditing** - Regular `cargo audit` checks
- **HTTPS strict** - Certificate verification for update checks

### Dependencies

We minimize external dependencies. Key crates:
- `fltk` - GUI framework
- `syntect` - Syntax highlighting
- `mlua` - Lua runtime (statically linked)
- `serde` / `serde_json` - Settings/session serialization

Run `cargo audit` to check for known vulnerabilities.

## Plugin System Security

### Current State (v0.9.2+)

The plugin sandbox is **hardened**:
- Dangerous Lua standard libraries (`os`, `io`, `debug`, `package`) are blocked
- File access uses `api:read_file()` / `api:write_file()` sandboxed to project root
- Command execution uses `api:run_command()` with a whitelist and timeout
- Path traversal prevented via `fs::canonicalize()` + root check
- Instruction limit (1M per hook) prevents infinite loops
- Memory limit (16 MB per plugin) prevents exhaustion

### Threat Model

**We protect against:**
- Malicious plugins executing arbitrary shell commands
- Plugins accessing files outside project scope
- Path traversal attacks (`../../etc/passwd`)
- Injection via command arguments
- Infinite loops / memory exhaustion (DoS)

**We do NOT protect against (OS responsibility):**
- Compromised system binaries (e.g., malware replaced `/usr/bin/ruff`)
- System-level keyloggers or rootkits
- User voluntarily installing malware

> If an attacker can modify binaries in the user's PATH, the system is already compromised. This is the same trust model as VSCode, Neovim, and Sublime Text.

### Plugin Security Roadmap

#### Phase 1: Sandbox Hardening (Done — v0.9.2)

- [x] Block dangerous Lua libraries: `os`, `io`, `debug`, `package`
- [x] Replace `io.popen()` with controlled `api.run_command()`
- [x] Replace `io.open()` with `api.read_file()` / `api.write_file()`
- [x] Path canonicalization to prevent traversal
- [x] Instruction limits to prevent infinite loops
- [x] Memory limits per plugin

#### Phase 2: Permission System (Done — v0.9.2)

Plugins declare required permissions in `plugin.toml`:

```toml
[permissions]
execute = ["ruff", "mypy"]
```

Command execution permissions are enforced at runtime — plugins can only run whitelisted commands declared in their manifest. On first load, the user sees a per-command approval dialog ("This plugin wants to execute: ruff, mypy. Allow?"). Approvals are persisted to settings so the prompt only appears once per plugin.

#### Phase 3: Signed Plugins (Done — v0.9.2)

- [x] Embed ed25519 public key in FerrisPad binary (`src/app/services/plugin_verify.rs`)
- [x] All downloaded plugins require valid signature — installation blocked if verification fails
- [x] SHA-256 checksums for `init.lua` and `plugin.toml` in registry manifest
- [x] HTTPS with certificate verification via rustls

## Reporting Vulnerabilities

If you discover a security vulnerability:

1. **Do NOT open a public issue**
2. Email: [security contact to be added]
3. Include: Description, reproduction steps, potential impact
4. We aim to respond within 48 hours

## Safe Usage Guidelines

### For Users

- Download FerrisPad only from official sources (ferrispad.com, GitHub releases)
- Verify checksums when available
- Only install plugins from trusted sources
- Keep your system and tools (ruff, mypy, etc.) updated

### For Plugin Developers

- Never execute user input as shell commands
- Validate all file paths
- Use the provided API functions, not raw Lua `io`/`os`
- Document what external tools your plugin requires
- Don't bundle binaries - rely on user-installed tools

## Changelog

| Version | Security Changes |
|---------|------------------|
| 0.9.0 | Initial plugin system (sandbox not hardened) |
| 0.9.1 | Added diagnostic panel, plugin API expansion |
| 0.9.2 | Lua sandbox hardened: stdlib blocked, path sandbox, instruction/memory limits, command whitelist. Permission prompts on plugin load with persistent approvals. Signed plugin downloads with ed25519 verification. Plugin Manager UI. Runtime cleanup on global disable |

---

*Last updated: 2026-03*
