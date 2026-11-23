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
shown() { tmux display -p -t "$TMUX_PANE" '#{@agent_state}#{?@agent_bg,+,}'; }
questions() { field "[.tool_input.questions[]?.question] | join(\" · \") | $text"; }
before=$(shown)
case "$1" in
  start|end)
    for o in @agent_state @agent_note @agent_bg @agent_since @agent_prompt; do unset_opt "$o"; done
    exit 0 ;;
  prompt)
    p=$(field ".prompt | $text")
    opt @agent_state working; opt @agent_note "$p"; opt @agent_prompt "$p"; unset_opt @agent_bg ;;
  tool)
    # Back to work after a question or permission: show your prompt again.
    case "$before" in waiting*) opt @agent_note "$(tmux show -pqv -t "$TMUX_PANE" @agent_prompt)" ;; esac
    opt @agent_state working ;;
  ask) opt @agent_state waiting; opt @agent_note "$(questions)" ;;
  permission)
    opt @agent_state waiting
    # AskUserQuestion can also raise a permission request; keep its questions.
    if [ "$(field .tool_name)" = AskUserQuestion ]; then opt @agent_note "$(questions)"
    else opt @agent_note "wants $(field .tool_name)"; fi ;;
  notify) opt @agent_state waiting ;;
  stop)
    opt @agent_state 'done'
    opt @agent_note "$(field ".last_assistant_message // \"\" | $text")"
    n=$(field '.background_tasks | length')
    if [ "${n:-0}" -gt 0 ] 2>/dev/null; then opt @agent_bg "$n"; else unset_opt @agent_bg; fi ;;
esac
[ "$(shown)" = "$before" ] || opt @agent_since "$(date +%s)"
exit 0
