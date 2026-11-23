#!/bin/sh
# Reports this Claude Code session's state to rolomux through tmux pane options.
[ -n "$TMUX_PANE" ] || exit 0
input=$(cat)
field() { printf '%s' "$input" | jq -r "$1" 2>/dev/null; }
opt() { tmux set -p -t "$TMUX_PANE" "$1" "$2"; }
unset_opt() { tmux set -pu -t "$TMUX_PANE" "$1"; }
# What rolomux shows: the state, plus whether background tasks are running.
shown() { tmux display -p -t "$TMUX_PANE" '#{@agent_state}#{?@agent_bg,+,}'; }
questions() { field '[.tool_input.questions[]?.question] | join(" · ") | gsub("\\s+"; " ")'; }
before=$(shown)
case "$1" in
  start|end)
    for o in @agent_state @agent_note @agent_bg @agent_since; do unset_opt "$o"; done
    exit 0 ;;
  prompt) opt @agent_state working; opt @agent_note "$(field '.prompt | gsub("\\s+"; " ")')"; unset_opt @agent_bg ;;
  tool) opt @agent_state working ;;
  ask) opt @agent_state waiting; opt @agent_note "$(questions)" ;;
  permission)
    opt @agent_state waiting
    # AskUserQuestion can also raise a permission request; keep its questions.
    if [ "$(field .tool_name)" = AskUserQuestion ]; then opt @agent_note "$(questions)"
    else opt @agent_note "wants $(field .tool_name)"; fi ;;
  notify) opt @agent_state waiting ;;
  stop)
    opt @agent_state done
    opt @agent_note "$(field '.last_assistant_message // "" | gsub("\\s+"; " ")')"
    n=$(field '.background_tasks | length')
    if [ "${n:-0}" -gt 0 ] 2>/dev/null; then opt @agent_bg "$n"; else unset_opt @agent_bg; fi ;;
esac
[ "$(shown)" = "$before" ] || opt @agent_since "$(date +%s)"
exit 0
