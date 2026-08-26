# Ticket workflow (FerrisPad, `docs/tickets/`)

This directory is the **tracked, in-repo** work-tracking workspace for FerrisPad,
an AI-assisted project. It holds the tickets opened from the 2026-08-24 security
+ quality audit and everything since. Each ticket we actively work also gets a
GitHub issue (opened at the start of implementation, closed via `Closes #<n>`
when it lands) — see `.claude/rules/work-sequence.md` → *GitHub issue tracking*.

Every implementation change lands through a ticket that moves through three
states. The state is encoded in the directory the ticket file lives in.

```
docs/tickets/
├── _template.md   # copy this for a new ticket
├── 1-todo/        # specified, not started or in progress
├── 2-review/      # implementation done, tests pass — NOT committed, awaiting human verification
└── 3-done/        # verified by the user, commit landed
```

## Rules (short form — full rules in `.claude/rules/`)

1. **Small.** One ticket = one sitting = one diff. Past ~5 in-scope bullets,
   split into dependent tickets.
2. **Self-contained.** Scope, out-of-scope, and a how-to-test recipe a human
   can run blind. "Run the test suite" alone is not a recipe.
3. **Red test first for defects.** Write the regression test that reproduces
   the bug and watch it FAIL before touching the fix. A bugfix diff whose test
   never failed is not reviewable.
4. **No commit while a ticket is in `1-todo/` or `2-review/`.** Human review is
   non-negotiable. The gates (`cargo test` / `clippy` / `fmt`) are *necessary*;
   the ticket being in `3-done/` is what *authorises* the commit.
5. **One ticket, one diff, one commit** (typically).

## State machine

```
   create               work done                 user verified
∅ ────────▶ 1-todo/ ──────────────▶ 2-review/ ──────────────────▶ 3-done/
              │
              └──▶ removed (re-scoped/abandoned — git history is the audit trail)
```

| From → To | Who | What it means |
|---|---|---|
| ∅ → `1-todo/` | Claude | Ticket specified, not implemented. |
| `1-todo/` → `2-review/` | Claude | Code on disk, tests pass, **not committed**. Ticket updated with what was actually done. |
| `2-review/` → `3-done/` | Claude (after user approval) | User verified via the recipe. Commit can land. |

## Naming

`T<NNNN>-<short-slug>.md` — zero-padded, monotonic ID, never reused or
renumbered. The file keeps its name across all three folders; moving it is the
only thing that changes.

## Origin of these tickets

The `1-todo/` set was generated from two audits + direct verification of the
three headline SEVERE findings (shell injection, sandbox escape, terminal
use-after-free). Each ticket's **Notes** section records the audit finding it
came from. Tickets are ordered by severity: security → robustness → UI bugs →
quality/dedup → process. See `INDEX.md` for the full list.
