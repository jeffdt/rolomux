# Regenerates docs/images/focus.gif for the README's dormant sessions /
# focus mode section.
#
# Prerequisites:
#   - Run from the repo root: vhs docs/demo/focus.tape
#   - `cargo build --release` first (PATH is pinned to target/release, not
#     whatever `rolomux` happens to be on the operator's real PATH)
#
# Shares seed-demo.sh's layout with organize.tape and search.tape, but
# passes a trailing list of session names so five of the ten seeded
# sessions -- claude-config, dotfiles, notes, investigate-performance-issues,
# db-migration -- open already marked dormant, as if sorted gradually over
# earlier real work. pr-reviews, api, web, spike-auth, and
# performance-review stay bright. Own isolated socket
# (rolomux-demo-focus-gif) so this never collides with the other two tapes
# if run back to back.
#
# Story: opens attached to `performance-review` -- its staged nvim buffer
# of Q4 self-review notes is already on screen, mid-task, same as
# organize.tape's "already working on something" opening beat (and a
# small joke: the session name is about a career review, not a
# benchmarking run). `prefix + s` pops rolomux
# open, auto-selecting the current session. Cursor moves up to `api` (3x
# Up), the first of three still-bright sessions that aren't today's
# priority, then walks back down marking each dormant in turn -- `d` on
# api, down, `d` on web, down, `d` on spike-auth -- three repetitions so
# the dim-on-`d` effect reads clearly instead of relying on a single,
# subtle before/after. The third `Down` lands back on performance-review,
# untouched throughout, and `f` enters focus mode: PERSONAL and DEV empty
# out entirely and disappear, INBOX collapses to just performance-review,
# and PINNED collapses to just pr-reviews -- a ten-session workspace
# resting at two. The recording holds on that narrowed, quiet view rather
# than reverting or jumping anywhere else; the payoff here is the calm
# end state itself, not a session switch.

Output "docs/images/focus.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 1000
Set Theme "Tomorrow Night"

Hide
Type "unset TMUX; export XDG_CONFIG_HOME=$(mktemp -d); ROLOMUX_REPO=$PWD; export PATH=$ROLOMUX_REPO/target/release:$PATH; $ROLOMUX_REPO/docs/demo/seed-demo.sh rolomux-demo-focus-gif $XDG_CONFIG_HOME $ROLOMUX_REPO/docs/demo/demo.tmux.conf claude-config dotfiles notes investigate-performance-issues db-migration; tmux -L rolomux-demo-focus-gif set-environment -g XDG_CONFIG_HOME $XDG_CONFIG_HOME; tmux -L rolomux-demo-focus-gif set-environment -g PATH $PATH; tmux -L rolomux-demo-focus-gif attach -t performance-review"
Enter
Sleep 2s
Show

Sleep 800ms
Ctrl+B
Type "s"
Sleep 1200ms

Up
Sleep 300ms
Up
Sleep 300ms
Up
Sleep 500ms

Type "d"
Sleep 700ms
Down
Sleep 300ms

Type "d"
Sleep 700ms
Down
Sleep 300ms

Type "d"
Sleep 700ms
Down
Sleep 500ms

Type "f"
Sleep 2200ms

Hide
Type "q"
Sleep 300ms
Escape
Type ":qa!"
Enter
Sleep 300ms
Type "exit"
Enter
Sleep 300ms
Type "tmux -L rolomux-demo-focus-gif kill-server"
Enter
Sleep 300ms
