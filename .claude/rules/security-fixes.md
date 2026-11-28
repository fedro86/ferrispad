# Security fixes — private until the release that ships them

A security defect is fixed in private and disclosed with the release that
ships the fix. This rule says what "private" means, so every session handles
a finding the same way, including one that starts after a reset. It works
alongside `work-sequence.md` (tickets, the `master` gate) and
`engineering-standards.md` (gates, red test first): what follows changes
**where** the work lives until disclosure, not **how** it is done.

## Triage: is it a security defect?

It is when input that someone else can shape crosses a trust boundary, or
misleads a trust decision. Such input includes:
- a plugin, its manifest or a registry entry;
- a crafted file, a URL;
- a session or settings file;
- a terminal byte stream.

Typical outcomes are:
- code or file access beyond what the plugin sandbox allows;
- a permission the user did not knowingly give;
- something on screen that misleads a decision about trust.

A crash or a wrong result on ordinary local input (the user's own files, the
user's own actions) is **not** one: it stays a normal public bugfix ticket.

When unsure, treat it as private and ask the orchestrator or the user. A
finding made public too early cannot be taken back.

## The private workspace

- **The finding and its ticket:**
  - write them as `docs/internal/security/S<N>-<slug>.md`, in the main
    checkout;
  - `docs/internal/` is gitignored, so they never reach a commit: check it
    with `git check-ignore -v docs/internal/security/<file>`;
  - use the `_template.md` format, with `id: S<N>` and `issue: ~`;
  - `S` numbers are monotonic within `docs/internal/security/`: take the
    highest one there plus one;
  - the `T` ID is taken only at disclosure, with the `new-ticket` skill, so
    no public number is reserved where other sessions can't see it.
- **The code:**
  - goes on a local branch `sec/<slug>`, in its own worktree, with no
    upstream;
  - local commits are fine (the pre-commit hook runs the gates);
  - branch from `master`, or from another `sec/` branch when the fix reuses
    its helpers, and say which in the ticket.
- **Shared notes:**
  - `docs/internal/security/MERGE-NOTES.md` records the hand-work needed
    when `sec/` branches meet each other or `master`: conflicts, and helpers
    that must be applied only once;
  - several sessions write to it, so **append only** (`>>`) and continue
    from its last number. Never rewrite the file.
- **Supporting files** stay in `docs/internal/security/` too: screenshot
  scenarios, before/after montages, a manual recipe. Red-test and gate
  outputs go to `~/Code/personal/ferrispad-wt/<worktree>.saved/`, next to
  the worktrees and outside the repository; the S ticket names the file.
- **Words that may travel:**
  - commit messages, branch names, test names and peer messages name the
    *area* (for example "session files" or "the file dialog"), never the way
    to exploit it. Peer messages go only to sessions already told the work
    is private (see *Never*);
  - a peer message about a finding says it is private, and gives the
    worktree path and the commit, not a PR.

## The work loop

The usual ticket loop, in the private workspace:

1. **Red test first.** Write the regression test that reproduces the defect
   and run it. It must fail for the defect's reason, and the run is
   saved in `<worktree>.saved/` and named in the S ticket. When the
   failing test has to be committed before its fix, mark it
   `#[ignore = "<why>"]` and remove the mark with the fix.
2. **Fix**, with the gates green: `scripts/gates.sh` (UI tests on a
   private display, T0122).
3. **Visible changes** are shot with the new-pr skill's `compare.sh` /
   `shoot.sh`, using a private scenario in `docs/internal/security/scenarios/`.
   The montage goes to `docs/internal/security/` for the user. A changed
   look needs the user's OK, as for any ticket.
4. **Review** by the reviewer, in the named worktree, by message. Verdicts
   come back by message only: no PR, no review comment.
5. **The user verifies** locally, with a recipe that runs blind. A throwaway
   config (`XDG_CONFIG_HOME` and the other XDG directories in a fresh temp
   folder) keeps the user's real settings out of it.
6. **Integration dry run.** Before the release, all approved `sec/`
   branches are merged together on top of `master`, locally, following
   MERGE-NOTES, and the gates run on the result.

## Disclosure, with the release

**The first push of any fix starts the disclosure.** A pushed branch is
public: its diff, test names, doc comments and commit messages describe the
defect. So nothing is pushed before the release window, and then everything
happens in one sitting, with no pause:

1. **Prepare, still local.** The integration dry run (above) decides the
   order the fixes land in. For each S ticket, take a T ID with the
   `new-ticket` skill, move the ticket to `docs/tickets/3-done/` (the
   user's private verification counts for the `master` gate), and put the
   fix on a fresh `ticket/T<NNNN>` branch as **one commit** whose message
   names the area, not the exploit. The `sec/` branch's own commits are
   never pushed.
2. **Push and land**, in the dry run's order: push the `ticket/` branch,
   open its GitHub issue and its PR with the presentation page (the title
   and the page name the area), wait for CI to be green, and merge.
3. **Release notes on `master`.** One commit adds the `### Security` section
   to that release's `CHANGELOG.md` entry (`release.yml` builds the release
   notes from it, so it must be on `master` before the tag) and a row to
   `SECURITY.md`'s Changelog table. Credit a reporter who asked for it.
4. **Tag** the release.
5. Publishing a GitHub Security Advisory record afterwards is the user's
   call, per release.

## Never

- No public issue, PR, comment, discussion, page or post about a finding
  that is not disclosed yet.
- Never push a `sec/` branch, and never set an upstream for one.
- Never commit anything from `docs/internal/`.
- Never hand a finding to a session that has not been told it is private.
- Never describe the exploit in a commit message, a branch name or a public
  ticket before disclosure.
