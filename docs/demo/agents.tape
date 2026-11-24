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
# a` opens straight into the agent view (`rolomux --agents`), where the dot
# marks the api row you came from. Two agents are blocked; the newest, a
# permission prompt, is selected first. One step down selects the other,
# whose question shows below the list, and `↵` switches to it, where it is
# asking that same question. The cursor only moves forward, so it never
# looks like the popup was opened from the row it lands on. The flat
# layout is shown as a still instead (agents-layouts.tape).

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

Sleep 1200ms
Ctrl+B
Type "a"
Sleep 2500ms

Down
Sleep 2500ms

Enter
Sleep 3000ms

Hide
Type "tmux -L rolomux-demo-agents-gif kill-server"
Enter
Sleep 300ms
