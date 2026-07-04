# AGENTS.md

Orientation for agents and humans working on rolomux. This file holds durable
intent and conventions, not a file-by-file map (that goes stale). Read the
source for current structure.

## What this is

rolomux is a fast terminal UI that replaces tmux's built-in `prefix + s` session
picker. It is a standalone compiled binary that tmux launches on demand via
`tmux popup -E`; it is not a tmux plugin and runs no background process.

## Versioning

There is no dated plan to reach `1.0.0`. It's an open milestone reserved for
a deliberate decision that the feature set and config schema are stable, not
something a routine change (like a rename) should trigger incidentally.

## Goals

These are the reasons the project exists. Changes should preserve them.

- **Fast and on-demand.** Opens in well under 100ms. Gathers all state in a
  single tmux subprocess call, renders, and exits. No daemon, no caching layer.
- **Named groups first, then always manual.** Sessions are curated into an
  arbitrary number of durable, user-named groups that stay on top in a
  user-defined order; everything else falls into the residual `SESSIONS`
  bucket, which is also always in manual order (there is no sort-mode
  cycling — this tool is for people who curate their layout by hand). The
  order is user-defined and reordered with the same `⇧J/⇧K` keys;
  new/unlisted sessions sink to the bottom. Groups, their order, and the
  manual order all persist across tmux restarts. Groups are durable: they
  survive empty and vanish only via an explicit delete (there is
  intentionally no auto-prune). A legacy single `pinned` list migrates to
  one group named `PINNED`.
- **Two altitudes, two modes.** Session mode operates on sessions (switch, jump,
  move a session across group boundaries with `⇧J/⇧K`, search). A dedicated
  full-screen group mode (`g`) operates only on group structure (create, rename,
  delete, reorder) and never shows sessions. Entering group mode costs a
  deliberate `g`, so once inside it is frictionless: no confirmation prompts, and
  create drops straight into inline naming.
- **Collapsible session/window tree.** Sessions expand into their windows, with
  a choose-tree feel but calmer behavior (see "Numbering philosophy").
- **Keyboard-driven, in-picker mutation.** Group membership, group structure,
  reorder, expand, jump, and focus, all from the picker. Mutations persist
  immediately.
- **Aesthetics matter.** The picker should be pleasant to open and use. It
  respects the user's terminal theme rather than imposing its own colors.
- **Type-to-filter search.** Press `/` to enter a read-only fuzzy filter;
  sessions are re-ranked best-match-first with the top result auto-selected.
  `Enter` switches; `Esc` returns to command mode. Search never writes config.

## Tech stack

- **Rust** (edition 2021). Single binary, `cargo build`.
- **ratatui** + **crossterm** for the TUI.
- **serde** + **toml** for the persisted config.
- The only runtime dependency beyond the binary itself is **tmux** on PATH.

## Durable design decisions

These are deliberate and have driven past work. Do not reverse them casually.

- **Named ANSI colors only.** Use the 16 named terminal colors (e.g.
  `Color::Cyan`, `Color::DarkGray`, `Color::Green`), never `Color::Rgb`. This is
  what lets the picker inherit the user's theme (e.g. Nord). A hardcoded RGB
  value is a regression.
- **Numbering philosophy.** Numbers mean "jumpable." Only sessions are
  jumpable, so only sessions are numbered. Numbering is stable, grouped-first
  (named-group members first, then `SESSIONS`), continuous, capped at 1-9, and
  **never renumbers on expand**. This is the
  intentional divergence from tmux choose-tree, which renumbers every visible
  line as the tree opens. Plain digit switches and closes; `Option/Alt + digit`
  focuses and expands a session without switching (uses the legacy ESC-prefix
  Meta encoding crossterm decodes to `KeyModifiers::ALT`; no kitty protocol).
- **Test seams.** tmux access sits behind a trait so the UI and model are
  testable without a live tmux. Keep new I/O behind seams like these.
- **Graceful no-op on tmux failure.** Switch/select actions swallow non-zero
  tmux exit status rather than crashing the popup. This is intentional for a
  transient popup UI.
- **TDD.** Model and UI logic are covered by unit tests (ratatui `TestBackend`
  buffer assertions for rendering). Keep the suite pristine under
  `RUSTFLAGS="-D warnings"` and `cargo clippy --all-targets -- -D warnings`; CI
  enforces both.
- **Fuzzy search is in-process, compile-time only.** The matcher uses the
  `nucleo-matcher` crate; it is a build-time dependency and does not change the
  runtime dep (still just tmux). The `Mode` enum and `DEFAULT_MODE` constant
  mirror the existing `INITIAL_FOCUS` seam and are the hook for a
  future `default_mode` config key (deferred, not shipped). During search,
  section headers and 1-9 jump numbers are suppressed by design (digits are
  query text; numbers cannot be stable when results re-rank on every keystroke).
  Window-name matching is intentionally reachable via the `session_haystack`
  seam in `src/model.rs` but is not built.

## Configuration

User config persists to `$XDG_CONFIG_HOME/rolomux/config.toml` (else
`~/.config/rolomux/config.toml`): a `[[groups]]` array (each with a `name`, an
ordered `members` list, and an optional `color` from the named palette in
`HEADER_COLORS`; empty/absent means the positional default, and `c` in group
mode flips it), and a `manual_order` list (the user-defined order for the
residual `SESSIONS` bucket). A legacy top-level `pinned` list is still read
and migrates to a single group named `PINNED`. Users normally never edit it
by hand; the picker writes it on group/membership/reorder changes. Groups
are never auto-pruned; `reconcile` drops dead members but keeps the group.

### Config migrations

Every saved config carries a `config_version` (see `CONFIG_VERSION` in
`src/store.rs`), stamped on each write. Plain additive fields don't need a
version bump — `serde(default)` already makes them backward- and
forward-compatible. Bump `CONFIG_VERSION` and add a step in `Config::migrate`
only for a rename or a semantic change (the `pinned` → `groups` conversion is
the existing example of both: a version-0 file lacks `config_version` and is
migrated once; a version-1+ file is never re-migrated even if a stale legacy
field is still lying around). **Any change to the config schema in that
category must ship with a matching migration step, a unit test in
`src/store.rs` for that step, and a bump to `CONFIG_VERSION`** — this project
has real users on installed binaries now, so a config file must never fail to
load or silently lose data across a version upgrade. A file with a
`config_version` newer than this binary's `CONFIG_VERSION` (e.g. a colleague
running a newer rolomux) must also load cleanly without misfiring an old
migration — current-shape fields are just read as-is.

## Packaging and distribution

rolomux ships as a prebuilt binary through a personal Homebrew tap, mirroring the
`jeffdt/teleport` pattern:

- A `v*` git tag triggers `release.yml`, which builds the
  `aarch64-apple-darwin` binary and attaches it to the GitHub Release.
- `jeffdt/homebrew-tap` carries `Formula/rolomux.rb`, a binary formula that
  downloads that asset by pinned `sha256`. Install with
  `brew install jeffdt/tap/rolomux`.
- **The tmux keybind is not part of the package.** It lives in the user's
  dotfiles (`~/.tmux.conf`), e.g.
  `bind s display-popup -E -B -w 84 -h 60% "exec rolomux"`. Distribution ships the
  binary; the bind travels with the user's config. The popup is launched
  borderless (`-B`) at a fixed 84-column width; rolomux draws its own framed card
  inset by a 2-cell buffer ring (`POPUP_MARGIN` in `ui.rs`), so the picker reads
  as a compact, evenly-bordered panel rather than filling a large popup.

### Cutting a release

**Every push to `main` that changes shipped behavior must also cut a release.**
Users install via Homebrew, which only ever sees tagged release binaries, never
`main`. A commit on `main` with no accompanying release is invisible to anyone
who runs `brew upgrade`: the code is "shipped" in git but not to users. So
unless a change is purely internal (docs, tests, CI, scratch under `specs/` or
`plans/`), finish the job by running the steps below in the same session: bump,
tag, wait for CI, and update the tap. Don't leave `main` ahead of the latest
release.

Shipped changes reach `main` via PR (see "Working in this repo"), and the
version bump rides in that PR. Once it has merged, cut the tag and update the
tap. The tap is a separate repo, `jeffdt/homebrew-tap`; clone it if it isn't
already checked out. `scripts/release.sh` expects it at `~/code/homebrew-tap`;
set `ROLOMUX_TAP_DIR` if it lives elsewhere.

`scripts/release.sh` automates the mechanical steps:

1. On the feature branch, before opening the PR: `scripts/release.sh bump
   <patch|minor|major>`. Reads the current version from `Cargo.toml`, applies
   the bump, refreshes `Cargo.lock` (`cargo build --release`), and commits.
   That commit rides in the PR as usual. Picking `patch` vs `minor` vs `major`
   is the one call the script doesn't make for you -- same judgment as always
   (a bug fix is patch, new user-facing behavior like a setting is minor).
2. After the PR merges: `git checkout main && git pull`, then
   `scripts/release.sh cut`. It reads the version already on `main` (no bump
   decision left -- that was step 1), tags and pushes `vX.Y.Z`, waits for
   `release.yml` (which builds and attaches a single asset named
   **`rolomux-aarch64-apple-darwin`** to the GitHub Release), downloads and
   hashes that asset, updates and validates `jeffdt/homebrew-tap`'s
   `Formula/rolomux.rb`, pushes the tap, and runs `brew update && brew upgrade
   jeffdt/tap/rolomux` locally, ending on a confirmed `rolomux --version`. It
   refuses to run off `main`, with a dirty tree, or against a tag that
   already exists, rather than guessing.

The formula carries `depends_on arch: :arm64` and `depends_on :macos` and a
top-level `url` (the version is scanned from the URL, e.g.
`.../download/vX.Y.Z/rolomux-...`; there is no separate `version` line) so the
tap's `brew test-bot` CI passes -- keep that shape by hand if editing the
formula outside the script (a nested `on_macos`/`version`-line formula fails
`readall`/`audit`). `release.sh cut` only ever rewrites the `url` and `sha256`
lines; it won't touch the `caveats` block's example keybind, so update that by
hand if it changed.

Two things the script doesn't cover -- finish these by hand after `cut`
succeeds:

- If `~/.tmux.conf`'s `bind s` was temporarily pointed at a dev build
  (`target/release/rolomux`) for testing, revert its `exec` to `exec rolomux` and
  `tmux source-file ~/.tmux.conf`.
- If this was the final PR for the work (no agreed-upon follow-up or
  multi-PR split), clean up rather than leaving the worktree lying around:
  confirm the linked issue actually closed (`Closes #N` closes it on merge,
  but check `gh issue view N --json state,closed` rather than assuming; `gh
  issue close N` by hand if it didn't), then run `wt remove` from inside the
  feature worktree (it deletes the worktree and the now-merged branch, and
  switches the shell back to the `main` worktree on its own). Offer to `git
  pull` the merge into that `main` worktree rather than doing it silently.

Currently Apple Silicon only. Supporting Intel means adding
`x86_64-apple-darwin` to the release matrix, an Intel branch in the formula,
and updating `scripts/release.sh`'s asset handling.

## Working in this repo

- Build/test loop: `RUSTFLAGS="-D warnings" cargo test`, then
  `cargo build --release`.
- **Leave a live preview when a feature is done.** Once a feature is
  implemented and tests pass, launch the freshly built binary in a new window
  of the *current* tmux session so the change is waiting on screen as a real
  running picker, not just green test output. Use the `mux` wrapper
  (`~/.claude/scripts/mux`; see the `tmux` skill) rather than raw `tmux`:
  `cargo build --release` then
  `tab=$(mux spawn --workspace caller --cwd "$PWD" --cmd "$PWD/target/release/rolomux" --title rolomux-preview)`
  then `tmux set-window-option -t "$tab" remain-on-exit on` (`mux` has no
  set-option verb, so that one step stays raw tmux, targeted by the exact tab
  token `mux spawn` printed, never ambiguous). `--workspace caller` targets
  the calling pane's own session robustly; omit `--focus` so the new window
  doesn't steal focus. Do NOT hand `mux spawn --cmd` a command wrapped in
  `exec`: rolomux exits on any selection/quit keypress, and an `exec`'d window
  vanishes with the process, so the preview disappears the moment it's
  touched; `--cmd` alone runs it as a plain foreground command in a fresh
  shell, so `remain-on-exit` can keep the window open afterward. This is for
  unattended runs: the picker sits at its prompt waiting for input, so when
  Jeff returns to the session the feature is previewable straight from the
  command line. rolomux detects the current session normally in a plain pane
  (`$TMUX` is set), so no popup is required.

  A prior version of this note told agents to run bare
  `tmux split-window`/`new-window` with no `-t`. Don't: an agent's Bash tool
  runs as a detached subprocess with no controlling tty, so that resolves
  "current window" against whatever window is currently active in the
  session, not the window this session is actually attached to, and the
  preview can silently land somewhere else. `mux spawn --workspace caller`
  avoids the whole class of bug by resolving the session robustly and always
  creating a fresh window rather than depending on tmux's ambient "current
  window."
- Specs live in `specs/`, plans in `plans/`, the build ledger in
  `.superpowers/`; all three are git-ignored scratch, not part of the package.
- **Changes land via pull request.** Work on a feature branch named
  `jeffdt/<domain>-<brief-kebab-desc>` (the global convention applies here). When
  Jeff clears a change to go live, open a PR and then merge it yourself (squash,
  to keep `main` linear) purely for the audit trail; this is a solo project with
  no human review gate, so the PR exists for history, not approval. Release tags
  are cut on `main` after the merge (see "Cutting a release"). The version bump
  rides in the same PR as the shipped change. If the session was kicked off from
  a GitHub issue on this repo (i.e. an issue number was mentioned in the
  session), reference it in the PR body with `Closes #N` so the issue links and
  auto-closes on merge.
