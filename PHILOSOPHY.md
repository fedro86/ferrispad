# FerrisPad Philosophy

This document defines the core pillars of FerrisPad. These principles are immutable and serve as the yardstick for any future feature requests.

## 1. 0% CPU Idle & RAM Frugality
**The foundational rules.** 
- **CPU:** If the user is not interacting with the editor, CPU usage must be 0%. We strictly reject background noise—this includes indexers, file watchers, or daemons that run without user intent.
- **RAM:** We strive for a minimal memory footprint. We use specialized allocation strategies and active memory management to ensure that system resources are returned to the OS as soon as they are no longer needed.

## 2. True Event-Driven Design
Features must be reactive, not proactive.
- **Passive Aids (Yes):** Syntax highlighting, line numbers, and UI organization (Tabs/Groups).
- **Active Processes (No):** Language Server Protocol (LSP), background code analysis, or constant disk searching.

## 3. Instant Utility
Speed is the primary feature. FerrisPad must open instantly. We reject any architectural change that introduces splash screens, loading delays, or runtime initialization lag.

## 4. Single Self-Contained Binary
FerrisPad is a **zero runtime dependency** tool. While it leverages the power of the Rust ecosystem at compile-time, the resulting binary is a single, auditable executable with no needs for external environment setup.
- No external runtimes required (Node.js, Python, JVM). 
- Any scripting capabilities (like **Lua**) must be statically linked into the main executable.

## 5. Code Extensions (Plugins & Contributions)
We welcome code extensions, whether they are direct contributions to the core or runtime plugins. However, both must respect the "Event-Driven" core.
- **Accepted:** Features that are **Passive/Reactive** (e.g., formatting text on request or on save).
- **Rejected:** Features that are **Active/Proactive** (e.g., background "crawling", constant disk indexing, or maintaining persistent background logic).

## 6. Privacy is Binary
We do not collect telemetry. There are no "anonymous pings" or usage stats. We rely exclusively on active user feedback provided via GitHub.
- **Auto-Updates:** These are implemented as **Startup Lifecycle Events**. They check for a new version once at launch and then terminate, preserving the 0% CPU rule.

## 7. Digital Ergonomics
The editor must "feel" right. We prioritize stability, predictability, and a clean interface that minimizes cognitive load. We focus on features that help the user organize their mind, not features that try to "understand" their code.

---

*Built for speed, reliability, and peace of mind.*
