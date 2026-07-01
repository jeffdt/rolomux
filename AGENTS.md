# AGENTS.md

Orientation for agents and humans working on smux. This file holds durable
intent and conventions, not a file-by-file map (that goes stale). Read the
source for current structure.

## What this is

smux is a fast terminal UI that replaces tmux's built-in `prefix + s` session
picker. It is a standalone compiled binary that tmux launches on demand via
`tmux popup -E`; it is not a tmux plugin and runs no background process.

## Goals

These are the reasons the project exists. Changes should preserve them.

- **Fast and on-demand.** Opens in well under 100ms. Gathers all state in a
  single tmux subprocess call, renders, and exits. No daemon, no caching layer.
- **Named groups first, then sorted.** Sessions are curated into an arbitrary
  number of durable, user-named groups that stay on top in a user-defined order;
  everything else falls into the residual `SESSIONS` bucket, which follows the
  active sort mode. Three modes cycle in the picker (the `s` key): recency, age
  (creation), and manual; the mode governs only `SESSIONS` (named groups are
  always manual order). In `SESSIONS` manual mode the order is user-defined and
  reordered with the same `⇧J/⇧K` keys; new/unlisted sessions sink to the bottom.
  Groups, their order, the active mode, and the manual order all persist across
  tmux restarts. Groups are durable: they survive empty and vanish only via an
  explicit delete (there is intentionally no auto-prune). A legacy single
  `pinned` list migrates to one group named `PINNED`.
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
  testable without a live tmux; the sort algorithm sits behind an enum so it
  can be swapped. Keep new I/O behind seams like these.
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
  mirror the existing `INITIAL_FOCUS`/`SortKey` seams and are the hook for a
  future `default_mode` config key (deferred, not shipped). During search,
  section headers and 1-9 jump numbers are suppressed by design (digits are
  query text; numbers cannot be stable when results re-rank on every keystroke).
  Window-name matching is intentionally reachable via the `session_haystack`
  seam in `src/model.rs` but is not built.

## Configuration

User config persists to `$XDG_CONFIG_HOME/smux/config.toml` (else
`~/.config/smux/config.toml`): a `[[groups]]` array (each with a `name`, an
ordered `members` list, and an optional `color` from the named palette in
`HEADER_COLORS`; empty/absent means the positional default, and `c` in group
mode flips it), a `manual_order` list (the user-defined order for the
`SESSIONS` manual sort mode), and a `sort` key (`activity`, `created`, or
`manual`). A legacy top-level `pinned` list is still read and migrates to a
single group named `PINNED`. Users normally never edit it by hand; the picker
writes it on group/membership/reorder/sort-cycle. Groups are never auto-pruned;
`reconcile` drops dead members but keeps the group.

## Packaging and distribution

smux ships as a prebuilt binary through a personal Homebrew tap, mirroring the
`jeffdt/teleport` pattern:

- A `v*` git tag triggers `release.yml`, which builds the
  `aarch64-apple-darwin` binary and attaches it to the GitHub Release.
- `jeffdt/homebrew-tap` carries `Formula/smux.rb`, a binary formula that
  downloads that asset by pinned `sha256`. Install with
  `brew install jeffdt/tap/smux`.
- **The tmux keybind is not part of the package.** It lives in the user's
  dotfiles (`~/.tmux.conf`), e.g.
  `bind S display-popup -E -B -w 84 -h 60% "exec smux"`. Distribution ships the
  binary; the bind travels with the user's config. The popup is launched
  borderless (`-B`) at a fixed 84-column width; smux draws its own framed card
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
already checked out.

1. Make sure the `version` bump in `Cargo.toml` and the refreshed `Cargo.lock`
   (any `cargo build`) are part of the feature PR. After it merges, `git
   checkout main && git pull` so the tag points at the merged commit.
2. Tag and push: `git tag -a vX.Y.Z -m "Release X.Y.Z" && git push origin
   vX.Y.Z`. The `v*` tag triggers `release.yml`, which builds and attaches a
   single asset named **`smux-aarch64-apple-darwin`** to the GitHub Release.
3. Wait for the build, then download the asset and hash it:

   ```sh
   gh run watch <run-id> --exit-status
   gh release download vX.Y.Z -R jeffdt/smux -p smux-aarch64-apple-darwin -D /tmp/r
   shasum -a 256 /tmp/r/smux-aarch64-apple-darwin
   ```

4. In `jeffdt/homebrew-tap`'s `Formula/smux.rb`, bump the version in the `url`
   (the full URL is hardcoded, e.g. `.../download/vX.Y.Z/smux-...`; there is no
   separate `version` line, brew scans it from the URL) and update `sha256`.
   Also update the example keybind in the `caveats` block if it changed. The
   formula carries `depends_on arch: :arm64` and `depends_on :macos` and a
   top-level `url` so the tap's `brew test-bot` CI passes; keep that shape (a
   nested `on_macos`/`version`-line formula fails `readall`/`audit`). Validate
   before pushing with `brew style jeffdt/tap`, `brew readall --aliases
   --os=all --arch=all jeffdt/tap`, and `brew audit --except=installed
   --tap=jeffdt/tap`. Commit and push the tap.
5. Pick up the build locally: `brew update && brew upgrade jeffdt/tap/smux`,
   then confirm `smux --version`. If `~/.tmux.conf`'s `bind S` was temporarily
   pointed at a dev build (`target/release/smux`) for testing, revert its `exec`
   to `exec smux` and `tmux source-file ~/.tmux.conf`.

Currently Apple Silicon only. Supporting Intel means adding
`x86_64-apple-darwin` to the release matrix and an Intel branch in the formula.

## Working in this repo

- Build/test loop: `RUSTFLAGS="-D warnings" cargo test`, then
  `cargo build --release`.
- **Leave a live preview when a feature is done.** Once a feature is
  implemented and tests pass, launch the freshly built binary in a new pane or
  window of the *current* tmux session so the change is waiting on screen as a
  real running picker, not just green test output. Run the raw build artifact
  directly rather than through the `tmux popup` keybind:
  `cargo build --release` then
  `tmux split-window -h "exec $PWD/target/release/smux"` (use
  `tmux new-window "exec $PWD/target/release/smux"` if you want it full width).
  This is for unattended runs: the picker sits at its prompt waiting for input,
  so when Jeff returns to the session the feature is previewable straight from
  the command line. smux detects the current session normally in a plain pane
  (`$TMUX` is set), so no popup is required. Do NOT launch it with `exec`: smux
  exits on any selection/quit keypress, and an `exec`'d window vanishes with the
  process, so the preview disappears the moment it's touched. Run it as a plain
  command and set `tmux set-window-option -t <win> remain-on-exit on` so the
  pane survives exit and shows what happened instead of closing.
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
