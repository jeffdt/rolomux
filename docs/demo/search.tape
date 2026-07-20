# Regenerates docs/images/search.gif for the README's Search section.
#
# Prerequisites:
#   - Run from the repo root: vhs docs/demo/search.tape
#   - `rolomux` on PATH (the real binary, not a dev build)
#
# Shares the same seeded layout as organize.tape (see seed-demo.sh) so the
# two GIFs feel like the same workspace, but uses its own isolated socket
# so the two tapes never collide if run back to back. Nests a real tmux
# server for actual `display-popup` chrome, scratch XDG_CONFIG_HOME so
# playback never depends on Jeff's real config.toml, and `zsh -f` so a
# themed prompt doesn't fight `Set Theme`.
#
# Story: unlike organize.tape (which opens on a shell and pops the picker
# on screen), this one opens with the picker already up -- the popup is
# triggered in the Hidden setup, before Show, so playback starts already
# inside session mode rather than re-showing the same prefix+s beat twice
# across both README GIFs. `/` switches to search. `performance-review`
# and `investigate-performance-issues` exist specifically so typing
# doesn't collapse to a single match on the first keystroke; typing the
# whole word would defeat the point of fuzzy search, so only "p" "e" "r"
# "f" get typed, one at a time with a real pause between each, showing
# the two matches re-rank live rather than jumping straight to a single
# result. `↵` switches into the top match (`performance-review`) and
# closes -- that session has a staged, fake `hyperfine` transcript
# already on screen (see seed-demo.sh) so landing there looks like
# resuming real, in-progress work instead of a blank shell.

Output "docs/images/search.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 1000
Set Theme "GruvboxDark"

Hide
Type "unset TMUX; export XDG_CONFIG_HOME=$(mktemp -d); ROLOMUX_REPO=$PWD; $ROLOMUX_REPO/docs/demo/seed-demo.sh rolomux-demo-search-gif $XDG_CONFIG_HOME $ROLOMUX_REPO/docs/demo/demo.tmux.conf; tmux -L rolomux-demo-search-gif set-environment -g XDG_CONFIG_HOME $XDG_CONFIG_HOME; tmux -L rolomux-demo-search-gif attach -t db-migration"
Enter
Sleep 2s
Ctrl+B
Type "s"
Sleep 1200ms
Show

Sleep 600ms
Type "/"
Sleep 700ms

Type "p"
Sleep 400ms
Type "e"
Sleep 400ms
Type "r"
Sleep 400ms
Type "f"
Sleep 1200ms

Enter
Sleep 1800ms

Hide
Type "exit"
Enter
Sleep 300ms
Type "tmux -L rolomux-demo-search-gif kill-server"
Enter
Sleep 300ms
