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
- **Pinned-first, then sorted.** Pinned sessions stay on top in a user-defined
  order; everything else is sorted by an algorithm (recency or creation). Pins
  and sort order persist across tmux restarts.
- **Collapsible session/window tree.** Sessions expand into their windows, with
  a choose-tree feel but calmer behavior (see "Numbering philosophy").
- **Keyboard-driven, in-picker mutation.** Pin/unpin, reorder pins, expand,
  jump, and focus, all from the picker. Mutations persist immediately.
- **Aesthetics matter.** The picker should be pleasant to open and use. It
  respects the user's terminal theme rather than imposing its own colors.

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
  jumpable, so only sessions are numbered. Numbering is stable, pinned-first,
  continuous, capped at 1-9, and **never renumbers on expand**. This is the
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

## Configuration

User config persists to `$XDG_CONFIG_HOME/smux/config.toml` (else
`~/.config/smux/config.toml`): a `pinned` list and a `sort` key. Users normally
never edit it by hand; the picker writes it on pin/reorder.

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

Everything lands on `main` (see "Working in this repo"). The tap is a separate
repo, `jeffdt/homebrew-tap`; clone it if it isn't already checked out.

1. Bump `version` in `Cargo.toml`, refresh `Cargo.lock` (any `cargo build`),
   commit, and `git push origin main`.
2. Tag and push: `git tag -a vX.Y.Z -m "Release X.Y.Z" && git push origin
   vX.Y.Z`. The `v*` tag triggers `release.yml`, which builds and attaches a
   single asset named **`smux-aarch64-apple-darwin`** to the GitHub Release.
3. Wait for the build, then download the asset and hash it:

   ```sh
   gh run watch <run-id> --exit-status
   gh release download vX.Y.Z -R jeffdt/smux -p smux-aarch64-apple-darwin -D /tmp/r
   shasum -a 256 /tmp/r/smux-aarch64-apple-darwin
   ```

4. In `jeffdt/homebrew-tap`'s `Formula/smux.rb`, update `version` (the download
   URL interpolates `#{version}`, so it follows automatically) and `sha256`.
   Also update the example keybind in the `caveats` block if it changed. Commit
   and push the tap.
5. Pick up the build locally: `brew update && brew upgrade jeffdt/tap/smux`,
   then confirm `smux --version`. If `~/.tmux.conf`'s `bind S` was temporarily
   pointed at a dev build (`target/release/smux`) for testing, revert its `exec`
   to `exec smux` and `tmux source-file ~/.tmux.conf`.

Currently Apple Silicon only. Supporting Intel means adding
`x86_64-apple-darwin` to the release matrix and an Intel branch in the formula.

## Working in this repo

- Build/test loop: `RUSTFLAGS="-D warnings" cargo test`, then
  `cargo build --release`.
- Specs live in `specs/`, plans in `plans/`, the build ledger in
  `.superpowers/`; all three are git-ignored scratch, not part of the package.
- **Commit straight to `main`.** This is a solo project with no PR or
  branch-review gate: ordinary commits, version bumps, and release tags all land
  directly on `main`. Do **not** open a feature branch or pull request here, the
  global `jeffdt/<domain>-<desc>` branch convention does not apply to this repo.
  A git-ignored `.claude/local/commit-to-main` marker makes the git-commit
  workflow honor this without prompting or branching.
