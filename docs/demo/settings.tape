# Regenerates docs/images/settings.png for the README's Settings section.
#
# Prerequisites:
#   - Run from the repo root: vhs docs/demo/settings.tape
#   - `rolomux` on PATH (the real binary, not a dev build)
#
# A still, not a gif: opens Settings over the seeded demo layout, flips
# one setting so its changed-from-default `•` marker shows. VHS 0.11's
# `Screenshot` command writes nothing here, so the frame is pulled from the
# throwaway gif and cropped to the popup afterwards:
#   ffmpeg -sseof -0.5 -i /tmp/rolomux-settings-throwaway.gif -update 1 \
#     -frames:v 1 -vf crop=790:500:150:240 docs/images/settings.png -y

Output "/tmp/rolomux-settings-throwaway.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 1000
Set Theme "Tomorrow Night"

Hide
Type "unset TMUX; export XDG_CONFIG_HOME=$(mktemp -d); ROLOMUX_REPO=$PWD; $ROLOMUX_REPO/docs/demo/seed-demo.sh rolomux-demo-settings $XDG_CONFIG_HOME $ROLOMUX_REPO/docs/demo/demo.tmux.conf; tmux -L rolomux-demo-settings set-environment -g XDG_CONFIG_HOME $XDG_CONFIG_HOME; tmux -L rolomux-demo-settings attach -t api"
Enter
Sleep 2s
Show

Ctrl+B
Type "s"
Sleep 1000ms
Type ","
Sleep 800ms
Type "3"
Sleep 600ms

Hide

Type "q"
Sleep 300ms
Type "q"
Sleep 300ms
Type "exit"
Enter
Sleep 300ms
Type "tmux -L rolomux-demo-settings kill-server"
Enter
Sleep 300ms
