---
name: cutting-a-release
description: Use when shipping any change to rolomux that changes user-facing behavior (not docs/tests/CI-only) and it's time to bump the version, tag a release, and update the Homebrew tap. Triggers on "cut a release", "release this", "bump the version", or when a PR merges to main and the change needs to reach Homebrew users.
---

**Every push to `main` that changes shipped behavior must also cut a release.**
Users install via Homebrew, which only ever sees tagged release binaries, never
`main`. A commit on `main` with no accompanying release is invisible to anyone
who runs `brew upgrade`: the code is "shipped" in git but not to users. So
unless a change is purely internal (docs, tests, CI, scratch under `specs/` or
`plans/`), finish the job by running the steps below in the same session: bump,
tag, wait for CI, and update the tap. Don't leave `main` ahead of the latest
release.

Shipped changes reach `main` via PR (see AGENTS.md's "Working in this repo"),
and the version bump rides in that PR. Once it has merged, cut the tag and
update the tap. The tap is a separate repo, `jeffdt/homebrew-tap`; clone it if
it isn't already checked out. `scripts/release.sh` expects it at
`~/code/homebrew-tap`; set `ROLOMUX_TAP_DIR` if it lives elsewhere.

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
