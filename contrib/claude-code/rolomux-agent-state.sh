#!/bin/sh
# Reports this Claude Code session's state to rolomux through tmux pane options.
[ -n "$TMUX_PANE" ] || exit 0
input=$(cat)
field() { printf '%s' "$input" | jq -r "$1" 2>/dev/null; }
opt() { tmux set -p -t "$TMUX_PANE" "$1" "$2"; }
unset_opt() { tmux set -pu -t "$TMUX_PANE" "$1"; }
# Flattens whitespace and keeps the head and tail of long text: tmux rejects
# commands over about 16KB, and the closing question is usually at the end.
text='gsub("\\s+"; " ") | if length > 3000 then .[:1500] + " … " + .[-1500:] else . end'
# What rolomux shows: the state, plus whether background tasks are running.
shown() { tmux display -p -t "$TMUX_PANE" '#{@rolomux_agent_state}#{?@rolomux_agent_bg,+,}'; }
questions() { field "[.tool_input.questions[]?.question] | join(\" · \") | $text"; }
before=$(shown)
case "$1" in
  start|end)
    for o in @rolomux_agent_state @rolomux_agent_note @rolomux_agent_bg @rolomux_agent_since @rolomux_agent_prompt; do unset_opt "$o"; done
    exit 0 ;;
  prompt)
    p=$(field ".prompt | $text")
    opt @rolomux_agent_state working; opt @rolomux_agent_note "$p"; opt @rolomux_agent_prompt "$p"; unset_opt @rolomux_agent_bg ;;
  tool)
    # Back to work after a question or permission: show your prompt again.
    case "$before" in waiting*) opt @rolomux_agent_note "$(tmux show -pqv -t "$TMUX_PANE" @rolomux_agent_prompt)" ;; esac
    opt @rolomux_agent_state working ;;
  ask) opt @rolomux_agent_state waiting; opt @rolomux_agent_note "$(questions)" ;;
  permission)
    opt @rolomux_agent_state waiting
    # AskUserQuestion can also raise a permission request; keep its questions.
    if [ "$(field .tool_name)" = AskUserQuestion ]; then opt @rolomux_agent_note "$(questions)"
    else opt @rolomux_agent_note "wants $(field .tool_name)"; fi ;;
  notify) opt @rolomux_agent_state waiting ;;
  stop)
    opt @rolomux_agent_state 'done'
    opt @rolomux_agent_note "$(field ".last_assistant_message // \"\" | $text")"
    n=$(field '.background_tasks | length')
    if [ "${n:-0}" -gt 0 ] 2>/dev/null; then opt @rolomux_agent_bg "$n"; else unset_opt @rolomux_agent_bg; fi ;;
esac
[ "$(shown)" = "$before" ] || opt @rolomux_agent_since "$(date +%s)"
exit 0
