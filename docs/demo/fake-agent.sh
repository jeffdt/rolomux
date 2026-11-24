#!/usr/bin/env bash
# Draws a static agent screen for docs/demo/agents.tape, then waits: just
# enough to read as "an agent working" or "an agent asking you something",
# so the jump at the end of the recording lands somewhere that matches the
# note the agent view showed. Shaped like a Claude Code transcript (prompt
# bar, ● replies, dim tool summaries, a ✻ status line) without its logo or
# other product chrome.
#
# Usage: fake-agent.sh work|ask
set -u

mode="${1:-}"
dim=$'\033[2m'
bold=$'\033[1m'
cyan=$'\033[36m'
bar=$'\033[100m'
eol=$'\033[K'
reset=$'\033[0m'

# Redrawn on every resize: the panes start at tmux's detached default size
# and only reach full width once the recording's client attaches.
draw() {
  local rule
  rule=$(printf '%*s' "$(tput cols)" '' | sed 's/ /─/g')
  # \033c (RIS) wipes the echoed command line along with everything else;
  # \033[?25l hides the cursor so the screen reads as an app, not a shell.
  printf '\033c\033[?25l'
  case "$mode" in
    work)
      cat <<EOF

${bar}❯ Add cursor pagination to the /orders endpoint and update the tests${eol}${reset}

● I'll add a cursor to the orders query, then update the route and its tests.

  ${dim}Read ${reset}${bold}2${reset}${dim} files, edited ${reset}${bold}1${reset}${dim} file${reset}

● The route now takes ${cyan}cursor${reset} and ${cyan}limit${reset} and returns ${cyan}next_cursor${reset}. Running
  the tests.

  ${dim}Ran ${reset}${bold}1${reset}${dim} shell command${reset}

${dim}✻ Working… (40s · esc to interrupt)${reset}
EOF
      ;;
    ask)
      cat <<EOF

${bar}❯ Spike a login flow for the admin app. Keep it small.${eol}${reset}

● Scaffolded the ${cyan}/login${reset} route and a session middleware stub. One decision
  before I wire up storage.

  ${dim}Read ${reset}${bold}3${reset}${dim} files, wrote ${reset}${bold}2${reset}${dim} files${reset}

${dim}${rule}${reset}
 ${bold}Which session store should the auth spike use?${reset}

 ${cyan}❯ 1. Redis${reset}
      ${dim}Shared across instances, survives restarts${reset}
   2. Signed cookies
      ${dim}No server state, simplest to ship${reset}

 ${dim}Enter to select · ↑/↓ to navigate · Esc to cancel${reset}
EOF
      ;;
  esac
}

trap draw WINCH
draw
stty -echo 2>/dev/null
# Backgrounded sleep + wait so the WINCH trap runs as soon as the size changes.
while :; do sleep 3600 & wait $!; done
