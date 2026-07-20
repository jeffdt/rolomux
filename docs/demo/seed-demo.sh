#!/usr/bin/env bash
# Seeds an isolated tmux server plus a matching scratch rolomux config for
# docs/demo tapes. Everything here is throwaway: a fresh isolated tmux
# socket and a fresh XDG_CONFIG_HOME are created per recording, so there's
# no persistent sandbox to keep in sync (contrast boomerang's seed-issues.sh,
# which resets state in a real, shared GitHub repo).
#
# Usage: seed-demo.sh <isolated-tmux-socket-name> <xdg-config-home> <tmux-conf-path>
#
# The tmux config (rolomux's `bind s ...`) only loads when a server first
# starts, not on a later `attach` -- so the very first tmux command against
# a fresh socket must carry `-f <tmux-conf-path>`, here on the first
# new-session call.
#
# DEV sits directly above INBOX in the group order (not PERSONAL) so
# `⇧K` on `db-migration` eventually crosses into DEV, where it actually
# belongs. `db-migration` is the *last* member of INBOX (freshly created,
# not yet sorted) rather than the first -- crossing into the group above
# only fires from the top of a group's block (see AGENTS.md's Numbering
# philosophy / group-crossing rule), so a recording moving it out of
# INBOX needs several `⇧K` presses to bubble it to the top first, then
# one more to cross.
#
# `performance-review` gets a staged, fake `hyperfine` transcript via
# printf so it doesn't look like a blank shell when a recording lands
# there -- deterministic output, not a real timed command. `db-migration`
# gets a similar staged `docker compose`/`docker ps` transcript in its
# `docker-config` window -- but `alembic`, not `docker-config`, is left
# as the *active* window (the one marked with rolomux's ● dot), so a
# recording can show the window-level jump feature for real: expanding
# db-migration surfaces alembic as active, then explicitly selecting the
# docker-config row and pressing ↵ jumps straight to it (real
# `switch-client` + `select-window`, not just landing on whatever the
# session's active window already was) and reveals the staged transcript.
set -euo pipefail

sock="$1"
xdg="$2"
conf="$3"

tmux -L "$sock" kill-server >/dev/null 2>&1 || true

tmux -L "$sock" -f "$conf" new-session -d -s pr-reviews -n main "zsh -f"
tmux -L "$sock" new-session -d -s claude-config -n main "zsh -f"

tmux -L "$sock" new-session -d -s dotfiles -n main "zsh -f"
tmux -L "$sock" new-session -d -s notes -n main "zsh -f"

tmux -L "$sock" new-session -d -s api -n server "zsh -f"
tmux -L "$sock" new-window -t api -n tests "zsh -f"

tmux -L "$sock" new-session -d -s web -n dev "zsh -f"
tmux -L "$sock" new-window -t web -n build "zsh -f"

tmux -L "$sock" new-session -d -s spike-auth -n main "zsh -f"
tmux -L "$sock" new-session -d -s performance-review -n main "zsh -f"
tmux -L "$sock" new-session -d -s investigate-performance-issues -n main "zsh -f"

tmux -L "$sock" new-session -d -s db-migration -n alembic "zsh -f"
tmux -L "$sock" new-window -t db-migration -n docker-config "zsh -f"
tmux -L "$sock" new-window -t db-migration -n pgsql "zsh -f"
# Creating a window auto-selects it, so pgsql would otherwise be left as
# db-migration's active window; explicitly re-select alembic instead, so
# it's the one marked active (the ● dot) when a recording expands the
# tree -- leaving docker-config as a deliberate, real jump target rather
# than the session's already-active window.
tmux -L "$sock" select-window -t db-migration:alembic

sleep 0.3
# \033c (RIS) clears the whole screen as the command's first byte of output,
# wiping the echoed `printf '...'` invocation itself along with it -- only
# the fake transcript below is left on screen once it runs.
tmux -L "$sock" send-keys -t performance-review "printf '\033c\$ hyperfine --warmup 3 ./target/release/rolomux\nBenchmark 1: ./target/release/rolomux\n  Time (mean +/- stddev):   41.8 ms +/-   1.2 ms\n  Range (min ... max):      39.6 ms ... 45.1 ms    62 runs\n\n'" Enter
tmux -L "$sock" send-keys -t db-migration:docker-config "printf '\033c\$ docker compose up -d\n[+] Running 2/2\n \xe2\x9c\x94 Network db-migration_default  Created\n \xe2\x9c\x94 Container db-migration-pg-1   Started\n\$ docker ps\nCONTAINER ID   IMAGE         STATUS         NAMES\na1b2c3d4e5f6   postgres:15   Up 3 seconds   db-migration-pg-1\n\n'" Enter

mkdir -p "$xdg/rolomux"
cat > "$xdg/rolomux/config.toml" <<'EOF'
config_version = 4

[[groups]]
name = "PINNED"
members = ["pr-reviews", "claude-config"]
color = "red"

[[groups]]
name = "PERSONAL"
members = ["dotfiles", "notes"]
color = "yellow"

[[groups]]
name = "DEV"
members = ["api", "web"]
color = "green"

[[groups]]
name = "INBOX"
members = ["spike-auth", "performance-review", "investigate-performance-issues", "db-migration"]
color = "blue"
inbox = true
EOF
