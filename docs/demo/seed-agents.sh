#!/usr/bin/env bash
# Layers agent state onto a server seeded by seed-demo.sh, for
# docs/demo/agents.tape. Sets the same @rolomux_agent_* pane options the
# Claude Code hook script sets, so the recording shows the real agent view
# without a real agent (repeatable takes, nothing private on screen).
#
# Agent panes are respawned in directories under a throwaway home, and the
# tape sets HOME to it for rolomux, so paths read as ~/code/<project>
# rather than a temp dir.
#
# Usage: seed-agents.sh <isolated-tmux-socket-name> <fake-home>
set -euo pipefail

sock="$1"
home="$2"
here="$(cd "$(dirname "$0")" && pwd)"
now=$(date +%s)

# agent <target> <project> <state> <seconds in state> <background tasks> <note>
agent() {
  local target="$1" project="$2" state="$3" age="$4" bg="$5" note="$6"
  mkdir -p "$home/code/$project"
  tmux -L "$sock" respawn-pane -k -t "$target" -c "$home/code/$project" "zsh -f"
  tmux -L "$sock" set -p -t "$target" @rolomux_agent_state "$state"
  tmux -L "$sock" set -p -t "$target" @rolomux_agent_since "$((now - age))"
  tmux -L "$sock" set -p -t "$target" @rolomux_agent_note "$note"
  if [ "$bg" -gt 0 ]; then tmux -L "$sock" set -p -t "$target" @rolomux_agent_bg "$bg"; fi
}

agent web:dev web waiting 60 0 "wants Bash"
agent spike-auth:main admin-app waiting 190 0 "Which session store should the auth spike use? Redis · Signed cookies"
agent pr-reviews:main platform 'done' 720 0 "Reviewed all three PRs and drafted comments on each. Want me to post them?"
agent db-migration:alembic billing 'done' 1500 0 "The migration ran cleanly against a copy of prod."
agent api:server api working 40 0 "Add cursor pagination to the /orders endpoint and update the tests"
agent web:build web 'done' 150 2 "Started the production build and the e2e suite in the background; I'll report back when both finish."

sleep 0.3
tmux -L "$sock" send-keys -t api:server "$here/fake-agent.sh work" Enter
tmux -L "$sock" send-keys -t spike-auth:main "$here/fake-agent.sh ask" Enter
