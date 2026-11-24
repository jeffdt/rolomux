#!/usr/bin/env bash
# Draws a static, generic CLI-agent screen for docs/demo/agents.tape, then
# waits. Not any real agent's UI: just enough to read as "an agent working"
# or "an agent asking you something", so the jump at the end of the
# recording lands somewhere that matches the note the agent view showed.
#
# Usage: fake-agent.sh work|ask
set -u

dim=$'\033[2m'
bold=$'\033[1m'
cyan=$'\033[36m'
green=$'\033[32m'
red=$'\033[31m'
reset=$'\033[0m'

# \033c (RIS) wipes the echoed command line along with everything else;
# \033[?25l hides the cursor so the screen reads as an app, not a shell.
printf '\033c\033[?25l'

case "${1:-}" in
  work)
    cat <<EOF

 ${dim}you${reset}  Add cursor pagination to the /orders endpoint and update the tests

 ${cyan}agent${reset}  Reading src/routes/orders.ts
        Reading tests/orders.test.ts
        Editing src/routes/orders.ts  ${green}+42${reset} ${red}-9${reset}
        Running npm test -- orders

        ${dim}PASS${reset}  tests/orders.test.ts (11 tests)
        ${dim}RUNS${reset}  tests/pagination.test.ts

 ${green}●${reset} ${dim}working · 40s${reset}
EOF
    ;;
  ask)
    cat <<EOF

 ${dim}you${reset}  Spike a login flow for the admin app. Keep it small.

 ${cyan}agent${reset}  Scaffolded the /login route and a session middleware stub.
        One decision before I wire up storage:

 ${bold}Which session store should the auth spike use?${reset}

   ${cyan}❯ 1. Redis${reset}            ${dim}shared across instances, survives restarts${reset}
     2. Signed cookies   ${dim}no server state, simplest to ship${reset}

 ${red}●${reset} ${dim}waiting for you · ↑↓ choose · ↵ confirm${reset}
EOF
    ;;
esac

# Hold the screen without echoing keystrokes.
stty -echo 2>/dev/null
while :; do sleep 3600; done
