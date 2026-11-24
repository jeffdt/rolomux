# Regenerates docs/images/agents-grouped.png and docs/images/agents-flat.png,
# the side-by-side stills of the agent view's two layouts in the README.
#
# Prerequisites:
#   - Run from the repo root: vhs docs/demo/agents-layouts.tape
#   - `cargo build --release` first (PATH is pinned to target/release)
#
# Same seeding as agents.tape. Opens the agent view, holds on the grouped
# layout, then `g` and holds on the flat one. VHS 0.11's `Screenshot`
# command writes nothing here, so both stills are pulled from the throwaway
# gif and cropped to the popup afterwards:
#   ffmpeg -ss 1.5 -i /tmp/rolomux-agents-layouts-throwaway.gif -frames:v 1 \
#     -vf crop=750:470:166:250 docs/images/agents-grouped.png -y
#   ffmpeg -sseof -0.5 -i /tmp/rolomux-agents-layouts-throwaway.gif -update 1 \
#     -frames:v 1 -vf crop=750:470:166:250 docs/images/agents-flat.png -y

Output "/tmp/rolomux-agents-layouts-throwaway.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 1000
Set Theme "Tomorrow Night"

Hide
Type "unset TMUX; export XDG_CONFIG_HOME=$(mktemp -d); FAKE_HOME=$(cd $(mktemp -d) && pwd -P); ROLOMUX_REPO=$PWD; export PATH=$ROLOMUX_REPO/target/release:$PATH; $ROLOMUX_REPO/docs/demo/seed-demo.sh rolomux-demo-agents-layouts $XDG_CONFIG_HOME $ROLOMUX_REPO/docs/demo/demo.tmux.conf; tmux -L rolomux-demo-agents-layouts set-environment -g XDG_CONFIG_HOME $XDG_CONFIG_HOME; tmux -L rolomux-demo-agents-layouts set-environment -g PATH $PATH; tmux -L rolomux-demo-agents-layouts set-environment -g HOME $FAKE_HOME; $ROLOMUX_REPO/docs/demo/seed-agents.sh rolomux-demo-agents-layouts $FAKE_HOME; tmux -L rolomux-demo-agents-layouts attach -t api:server"
Enter
Sleep 2s
Ctrl+B
Type "a"
Sleep 1s
Show

Sleep 3s

Hide
Type "g"
Sleep 500ms
Show

Sleep 3s

Hide
Type "q"
Sleep 300ms
Type "tmux -L rolomux-demo-agents-layouts kill-server"
Enter
Sleep 300ms
