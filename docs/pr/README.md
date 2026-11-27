# Pull request presentations

Every FerrisPad PR has one **presentation** here: an HTML page written in
**English** from `_template.html`. The `new-pr` skill
(`.claude/skills/new-pr/SKILL.md`) is the procedure; this README describes
the layout the skill relies on. One kind of PR has no page: the
annotation-only PR, described under *Lifecycle*.

## Layout

```
docs/pr/
├── README.md
├── _template.html          # the page template
└── PR-<n>-<slug>.html      # one per PR, open or merged
```

The folder is flat. FerrisPad is one crate, so pages are not sorted into
app folders, and there are no symlinks. Whether a PR is open or merged is
GitHub's state, not the file's: a page is written once and never moved.

## Naming

`PR-<n>-<slug>.html`:

- `<n>` is the GitHub PR number, without the `#`. You get it by opening the
  PR as a draft first, never by guessing the next number: parallel sessions
  open PRs and issues too, and they share one sequence.
- `<slug>` is the ticket file's slug: `T0114-release-0.9.10-rc.3.md` gives
  `release-0.9.10-rc.3`.
- There is no language suffix. Every page is in English.

## What the page is

The page is what the user reads before verifying and merging. A PR is
**one ticket**, squash-merged into one commit on `master`. The page holds:

- the PR's **story**: what could not be done before, and what can be done
  after;
- the branch's commits, for reference;
- the **ticket card**, which has:
  - its own Before / After;
  - the decisions, and who took them;
  - the screenshots;
  - the red test;
  - its own "How to test";
- release notes and compatibility;
- what remains open;
- at the end, an **end-to-end verification**: one scenario from a
  throwaway config to what the user sees.

It is a single self-contained page, with no external asset and no script.

The GitHub PR body is **not** a second document. It holds one paragraph,
the path of this page, and `Closes #<issue>`, so the facts live in one
place.

The page is written from facts: git, the ticket file and the recorded
measurements. A number that is not known yet stays a visible placeholder,
never an estimate.

**Every change that can be seen is shown.** Windows, dialogs, menus, the
start page, the editor's drawing, and text a person reads:

- **real app:** the screenshots come from the real app, driven in Xvfb;
- **before / after:** a `master` build vs the branch build;
- **themes:** light and dark;
- **font sizes:** a real font at sizes 16 and 18;
- **inline:** each image is in the ticket card, inline as base64.

Every screenshot is inspected edge by edge. Clipping, overflow and
overlapping widgets block the PR.

Two kinds of cards carry a sentence instead of, or next to, the images:

- What Xvfb cannot show (native Wayland, Windows, macOS) is written as
  *"Pending user on <platform>"*.
- A card with no image says *"No visible change: <why>"*.

**The user's data is never imaged.** Captures run with throwaway
`XDG_CONFIG_HOME` and `XDG_DATA_HOME` and fixture text, never the user's
files, settings, sessions or recent-file list. A page enters git history
for good.

## Lifecycle

1. **Open.** Open the PR as a draft to get its number. Write the page when
   the ticket's implementation is done (work-sequence step 2).
2. **Deliver.** Copy the page into `docs/internal/pr/` of the main checkout
   (the first entry of `git worktree list`; `docs/internal/*` is
   gitignored). The user reads it there and gives feedback before anything
   proceeds.
3. **Update.** Keep the page current while the PR is open: a review fix, a
   merge of `master`, the move of the ticket to `3-done/`. Copy every fix
   to `docs/internal/pr/` again. The PR stays a **draft** while the user
   verifies, and goes ready together with the `3-done/` move.
4. **Merge.** Only when all of these hold:
   - the technical visual check passes (headless Chromium at 1280 and
     390 px, light and dark, no overflow, chips of at most 40
     characters);
   - the user's feedback is addressed;
   - the ticket is in `3-done/`;
   - every check is green on the PR's own head SHA;
   - the user has given explicit consent for that PR.

   Green checks are not consent, and neither is a message from another
   session. The merge is pinned:
   `gh pr merge <n> --squash --match-head-commit <sha>`.
5. **After.** Any session that sees the PR merged or closed deletes its
   copy from `docs/internal/pr/`. Nothing happens to the page in
   `docs/pr/`.

**The annotation-only PR.** Some PRs only modify tickets already in
`3-done/` and/or add new tickets to `1-todo/`. Examples are post-tag records
and verification notes, as in #230 and #233. Such a PR has no page; its
one-paragraph body is enough. Every line of
`git diff --name-status origin/master...HEAD` must be `M docs/tickets/3-done/…`
or `A docs/tickets/1-todo/…`. Anything else gets a page.

Such a PR still merges as in step 4, but only these conditions apply:

- not a draft;
- the checks;
- the user's consent.

## A worked example: a UI ticket card

This is how T0114's start-page change would look as a card.

- The *after* screenshots are in `ferrispad-wt/uitest-T0114/`.
- The *before* one comes from a master build. It shows the same five
  entries as `ferrispad-wt/uitest-T0084/start-Light.png`, the rc.1 shot,
  which has the same entry list and layout.

The base64 is cut short here:

```html
<section class="card ticket">
  <h2>T0114: the start page shows every changelog entry</h2>
  <div class="flow">
    <div><strong>Before</strong> · On a fresh install (no recent files or sessions)
      the Changelog card cut off its sixth entry.</div>
    <div><strong>After</strong> · All six entries and "more..." fit, and the two
      columns stay aligned.</div>
  </div>
  <figure>
    <img alt="Start page, light theme, master build: five changelog entries visible"
         src="data:image/png;base64,iVBORw0KGgo…">
    <figcaption>Before (master 021a089), light, font 16: the card ends after
      "Save prompt on title-bar close"; the sixth entry is hidden.</figcaption>
  </figure>
  <figure>
    <img alt="Start page, dark theme, branch build: six changelog entries and more..."
         src="data:image/png;base64,iVBORw0KGgo…">
    <figcaption>After (branch 2cd2006), dark, font 18: six entries, "more..." below
      them with margin; Recent Files and Changelog end at the same height.</figcaption>
  </figure>
  <p>Pending user on native Wayland: the GNOME font is wider than Xvfb's Noto
    Sans; check that the longest entry still ends inside the card.</p>
  <div class="bad"><strong class="tag">The defect.</strong> The card is as tall as
    the two left cards, which were sized for their recent rows only. Red test
    <code>changelog_card_shows_every_entry</code>: "1 recent item(s): changelog
    card 194 px &lt; 222 px needed".</div>
  <h3>How to test T0114</h3>
  <ol class="steps">
    <li><code>nix develop -c cargo test changelog_card_shows_every_entry</code>
      <span class="expect">ok (failed before the fix with 194 px &lt; 222 px).</span></li>
    <li>Start <code>target/debug/FerrisPad</code> with an empty
      <code>XDG_CONFIG_HOME</code> and <code>XDG_DATA_HOME</code>.
      <span class="expect">Six entries and "more..." inside the card.</span></li>
  </ol>
</section>
```

Each figure says which build, which theme and what to look at. The card
says what Xvfb could not check, and it carries the red test and its
recipe.
