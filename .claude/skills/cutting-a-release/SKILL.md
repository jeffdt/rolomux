---
name: cutting-a-release
description: Use when shipping any change to this app that changes user-facing behavior (not docs/tests/CI-only) and it's time to bump the version, tag a release, and update the Homebrew tap. Triggers on "cut a release", "release this", "bump the version", or when a PR merges to main and the change needs to reach Homebrew users.
---

**Every push to `main` that changes shipped behavior must also cut a release.**
Users install via Homebrew, which only ever sees tagged release binaries, never
`main`. A commit on `main` with no accompanying release is invisible to anyone
who runs `brew upgrade`: the code is "shipped" in git but not to users. So
unless a change is purely internal (docs, tests, CI, scratch under `specs/` or
`plans/`), finish the job by running the steps below in the same session: bump,
tag, wait for CI, and update the tap. Don't leave `main` ahead of the latest
release.

Shipped changes reach `main` via PR (see this repo's AGENTS.md/CLAUDE.md for
its own PR conventions), and the version bump rides in that PR. Once it has
merged, cut the tag and update the tap. The tap is a separate repo,
`jeffdt/homebrew-tap`; clone it if it isn't already checked out.
`.claude/skills/cutting-a-release/release.sh` expects it at
`~/code/homebrew-tap`; override with `<FORMULA>_TAP_DIR` if it lives
elsewhere (see the script's header comment for the exact env var name).

`release.sh` automates the mechanical steps:

1. On the feature branch, before opening the PR: `release.sh bump
   <patch|minor|major>`. Reads the current version from `Cargo.toml`, applies
   the bump, refreshes `Cargo.lock` (`cargo build --release`), and commits.
   That commit rides in the PR as usual. Picking `patch` vs `minor` vs `major`
   is the one call the script doesn't make for you -- same judgment as always
   (a bug fix is patch, new user-facing behavior like a setting is minor).
2. After the PR merges: `git checkout main && git pull`, then `release.sh
   cut`. It reads the version already on `main` (no bump decision left --
   that was step 1), tags and pushes `vX.Y.Z`, waits for `release.yml`
   (which builds and attaches a single asset to the GitHub Release, named
   after this app's package name plus `-aarch64-apple-darwin`), downloads
   and hashes that asset, updates and validates
   `jeffdt/homebrew-tap`'s formula, pushes the tap, and runs `brew update &&
   brew upgrade jeffdt/tap/<formula>` locally, ending on a confirmed
   `<binary> --version`. It refuses to run off `main`, with a dirty tree, or
   against a tag that already exists, rather than guessing.

Asset name, formula name, and the binary invoked at the end all default to
this app's `Cargo.toml` package name. If any of them differ (e.g. a package
named `tp-core` that ships as Homebrew formula `tp`), the script reads
overrides from a `[package.metadata.tui-utils]` table in `Cargo.toml` --
check there first if release.sh's behavior doesn't match what you expect for
this app.

The formula carries `depends_on arch: :arm64` and `depends_on :macos` and a
top-level `url` (the version is scanned from the URL, e.g.
`.../download/vX.Y.Z/<asset>`; there is no separate `version` line) so the
tap's `brew test-bot` CI passes -- keep that shape by hand if editing the
formula outside the script (a nested `on_macos`/`version`-line formula fails
`readall`/`audit`). `release.sh cut` only ever rewrites the `url` and `sha256`
lines; it won't touch the `caveats` block or anything else app-specific, so
update those by hand if they changed.

Two things the script doesn't cover -- finish these by hand after `cut`
succeeds:

- If a local dotfile or config (e.g. `~/.tmux.conf`) was temporarily pointed
  at a dev build (`target/release/<binary>`) for testing, revert it to the
  installed binary and re-source/reload as needed.
- If this was the final PR for the work (no agreed-upon follow-up or
  multi-PR split), clean up rather than leaving the worktree lying around:
  confirm the linked issue actually closed (`Closes #N` closes it on merge,
  but check `gh issue view N --json state,closed` rather than assuming; `gh
  issue close N` by hand if it didn't), then run `wt remove` from inside the
  feature worktree (it deletes the worktree and the now-merged branch, and
  switches the shell back to the `main` worktree on its own). Offer to `git
  pull` the merge into that `main` worktree rather than doing it silently.

Currently Apple Silicon only across these apps. Supporting Intel means
adding `x86_64-apple-darwin` to the release matrix, an Intel branch in the
formula, and updating `release.sh`'s asset handling.
