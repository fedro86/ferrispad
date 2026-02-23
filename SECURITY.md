# Security Policy

FerrisPad is designed with security as a core principle. This document outlines our security model, what we protect against, and our roadmap.

## Current Status

| Area | Status | Notes |
|------|--------|-------|
| Core Editor (Rust) | ✅ Stable | Memory-safe by design |
| Plugin System (Lua) | ⚠️ In Development | Sandbox not yet hardened |
| Auto-Updates | ✅ Stable | HTTPS only, no auto-install |
| Plugin Downloads | ❌ Not Implemented | Planned with signatures |

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

### Current State (v0.9.x)

The plugin system is **in development**. Current plugins have access to:
- `io.popen()` - Can execute external commands
- `io.open()` - Can read/write files

**This will change.** See roadmap below.

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

#### Phase 1: Sandbox Hardening (Planned)

- [ ] Block dangerous Lua libraries: `os`, `io`, `debug`, `package`
- [ ] Replace `io.popen()` with controlled `api.run_command()`
- [ ] Replace `io.open()` with `api.read_file()` / `api.write_file()`
- [ ] Path canonicalization to prevent traversal
- [ ] Instruction limits to prevent infinite loops
- [ ] Memory limits per plugin

#### Phase 2: Permission System (Planned)

Plugins declare required permissions in `manifest.json`:

```json
{
  "name": "python-lint",
  "version": "1.0.0",
  "permissions": [
    { "type": "execute", "command": "ruff" },
    { "type": "execute", "command": "mypy" }
  ]
}
```

On install, user sees: "This plugin wants to execute: ruff, mypy. Allow?"

#### Phase 3: Signed Plugins (Planned)

- [ ] Embed ed25519 public key in FerrisPad binary
- [ ] All downloaded plugins require valid signature
- [ ] SHA-256 checksums in manifest
- [ ] HTTPS with certificate verification

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
| (future) | Lua sandbox hardening, permission system |

---

*Last updated: 2025-02*
