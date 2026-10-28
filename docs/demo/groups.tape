# Regenerates docs/images/groups.gif for the README's Groups section.
#
# Prerequisites:
#   - Run from the repo root: vhs docs/demo/groups.tape
#   - `rolomux` on PATH (the real binary, not a dev build)
#
# Shares seed-demo.sh's layout with the other tapes, on its own isolated
# socket. Story: opens attached to `api`, `g` raises the cursor to group
# altitude (sessions dim underneath), `n` creates and names a new group
# (it lands directly above DEV), `c` cycles its color, `g` descends back
# to `api` and `⇧K` crosses it into the new group so the color lights up,
# then `g` raises again and `⇧K` twice reorders the group to the top
# before descending back to `api`.

Output "docs/images/groups.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 1000
Set Theme "Tomorrow Night"

Hide
Type "unset TMUX; export XDG_CONFIG_HOME=$(mktemp -d); ROLOMUX_REPO=$PWD; $ROLOMUX_REPO/docs/demo/seed-demo.sh rolomux-demo-groups-gif $XDG_CONFIG_HOME $ROLOMUX_REPO/docs/demo/demo.tmux.conf; tmux -L rolomux-demo-groups-gif set-environment -g XDG_CONFIG_HOME $XDG_CONFIG_HOME; tmux -L rolomux-demo-groups-gif attach -t api"
Enter
Sleep 2s
Show

Sleep 600ms
Ctrl+B
Type "s"
Sleep 1200ms

Type "g"
Sleep 1200ms

Type "n"
Sleep 600ms
Type@120ms "OPS"
Sleep 500ms
Enter
Sleep 1000ms

Type "c"
Sleep 600ms
Type "c"
Sleep 1000ms

Type "g"
Sleep 800ms
Type "K"
Sleep 1200ms

Type "g"
Sleep 800ms
Type "K"
Sleep 600ms
Type "K"
Sleep 1000ms

Type "g"
Sleep 2000ms

Hide
Type "q"
Sleep 300ms
Type "exit"
Enter
Sleep 300ms
Type "tmux -L rolomux-demo-groups-gif kill-server"
Enter
Sleep 300ms
