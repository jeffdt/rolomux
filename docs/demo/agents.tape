# Regenerates docs/images/agents.gif for the README's agent status section.
#
# Prerequisites:
#   - Run from the repo root: vhs docs/demo/agents.tape
#   - `cargo build --release` first (PATH is pinned to target/release)
#
# Shares seed-demo.sh's layout, then seed-agents.sh sets agent state on five
# panes and starts a simulated agent in two of them. HOME is pointed at a
# throwaway dir inside the tmux server so agent paths read as ~/code/<name>.
#
# Story: opens in api's server window, where an agent is working. `prefix +
# s` shows the tree with its agent badges; `a` flips to the agent view,
# where the dot marks the api row you came from and the first row, a
# blocked agent, shows its question below the list. Moving down shows the
# note for a done agent, then for a done agent that still has background
# tasks (listed under WORKING with ↻2). `g` shows the flat list and back;
# `1` jumps to the blocked agent, which is asking the same question.

Output "docs/images/agents.gif"

Set Shell zsh
Set FontSize 14
Set Width 1100
Set Height 1000
Set Theme "Tomorrow Night"

Hide
Type "unset TMUX; export XDG_CONFIG_HOME=$(mktemp -d); FAKE_HOME=$(cd $(mktemp -d) && pwd -P); ROLOMUX_REPO=$PWD; export PATH=$ROLOMUX_REPO/target/release:$PATH; $ROLOMUX_REPO/docs/demo/seed-demo.sh rolomux-demo-agents-gif $XDG_CONFIG_HOME $ROLOMUX_REPO/docs/demo/demo.tmux.conf; tmux -L rolomux-demo-agents-gif set-environment -g XDG_CONFIG_HOME $XDG_CONFIG_HOME; tmux -L rolomux-demo-agents-gif set-environment -g PATH $PATH; tmux -L rolomux-demo-agents-gif set-environment -g HOME $FAKE_HOME; $ROLOMUX_REPO/docs/demo/seed-agents.sh rolomux-demo-agents-gif $FAKE_HOME; tmux -L rolomux-demo-agents-gif attach -t api:server"
Enter
Sleep 2s
Show

Sleep 1500ms
Ctrl+B
Type "s"
Sleep 2200ms

Type "a"
Sleep 2500ms

Down
Sleep 1500ms
Down
Sleep 400ms
Down
Sleep 400ms
Down
Sleep 2000ms

Type "g"
Sleep 2000ms
Type "g"
Sleep 1000ms

Type "1"
Sleep 3000ms

Hide
Type "tmux -L rolomux-demo-agents-gif kill-server"
Enter
Sleep 300ms
