# Regenerates docs/images/organize.gif for the README hero image.
#
# Prerequisites:
#   - Run from the repo root: vhs docs/demo/organize.tape
#   - `rolomux` on PATH (the real binary, not a dev build)
#
# Nests a real, isolated tmux server (its own socket, never the default
# one) so the actual `display-popup` chrome renders, matching how rolomux
# is really launched. docs/demo/seed-demo.sh creates the session/window
# layout and a scratch XDG_CONFIG_HOME with a matching config.toml before
# the recording starts, so playback never depends on Jeff's real
# ~/.config/rolomux/config.toml. The pane runs `zsh -f` (no rc files) so a
# themed prompt doesn't fight `Set Theme`. Height is generous (1000px) so
# all 4 groups plus db-migration's expanded 3-window tree fit in the
# popup without scrolling -- confirmed empirically via a throwaway probe
# tape, not assumed. Group order in seed-demo.sh's config.toml is PINNED,
# PERSONAL, DEV, INBOX -- DEV sits directly above INBOX on purpose, so
# `db-migration` eventually crosses straight into the group it actually
# belongs in.
#
# Story: opens attached to `api`, an existing DEV session, mid-task (a
# command already run, a fresh prompt waiting) -- 99% of real usage is
# "I'm already working on something," not a dedicated organizing moment.
# `prefix + s` pops rolomux open over it, auto-selecting the current
# session (`api`, top of DEV) rather than defaulting to the first row,
# confirmed empirically via a throwaway probe. Cursor scrolls down 5 rows
# to `db-migration` -- the last member of INBOX (freshly created, not yet
# sorted). `⇧K` is pressed four times *before* anything is expanded:
# group-crossing only fires from the top of a group's block (see
# AGENTS.md's Numbering philosophy), so the first three presses bubble
# `db-migration` up past `investigate-performance-issues`,
# `performance-review`, and `spike-auth` to the top of INBOX, and the
# fourth crosses it into DEV, landing at the bottom of that group --
# confirmed empirically, cursor following throughout.
#
# Only once it's sorted does `l` expand it, revealing all three of its
# windows (`alembic`, `docker-config`, `pgsql`) -- seed-demo.sh leaves
# `alembic` as the *active* window (the ● dot), not `docker-config`, on
# purpose: the payoff here is the window-level jump, not just "whichever
# window happened to be active." Cursor moves down twice, off the
# session row onto `alembic` and then onto `docker-config`, and `↵`
# switches directly to that specific window -- real `switch-client` +
# `select-window`, confirmed empirically via src/tmux.rs's
# `Action::SwitchWindow` handling, not just landing wherever the session
# already was. It lands in `docker-config`'s staged, fake (but
# deterministic, see seed-demo.sh) `docker compose up -d` / `docker ps`
# transcript -- reads as picking up real, paused work rather than a
# blank shell.

Output "docs/images/organize.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 1000
Set Theme "GruvboxDark"

Hide
Type "unset TMUX; export XDG_CONFIG_HOME=$(mktemp -d); ROLOMUX_REPO=$PWD; $ROLOMUX_REPO/docs/demo/seed-demo.sh rolomux-demo-gif $XDG_CONFIG_HOME $ROLOMUX_REPO/docs/demo/demo.tmux.conf; tmux -L rolomux-demo-gif set-environment -g XDG_CONFIG_HOME $XDG_CONFIG_HOME; tmux -L rolomux-demo-gif attach -t api"
Enter
Sleep 2s

Type "git log --oneline -3"
Enter
Sleep 800ms
Show

Sleep 600ms
Ctrl+B
Type "s"
Sleep 1000ms

Down
Sleep 250ms
Down
Sleep 250ms
Down
Sleep 250ms
Down
Sleep 250ms
Down
Sleep 500ms

Type "K"
Sleep 150ms
Type "K"
Sleep 150ms
Type "K"
Sleep 150ms
Type "K"
Sleep 1000ms

Type "l"
Sleep 750ms

Down
Sleep 300ms
Down
Sleep 500ms

Enter
Sleep 1500ms

Hide
Type "exit"
Enter
Sleep 300ms
Type "tmux -L rolomux-demo-gif kill-server"
Enter
Sleep 300ms
