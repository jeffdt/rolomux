---
name: live-preview
description: Use once a feature in one of these TUI apps is implemented and tests pass, to launch the freshly built binary in a real tmux window so the change is on screen rather than just green test output. Triggers on finishing a feature, "leave a preview", or before reporting a feature done.
---

Launch the freshly built binary in a new window of the *current* tmux session
so the change is waiting on screen as a real running app, not just green
test output. Use the `mux` wrapper (`~/.claude/scripts/mux`; see the `tmux`
skill) rather than raw `tmux`:

`cargo build --release` then determine the binary path. The package name in
`Cargo.toml` usually matches the built binary name; if the app overrides
this (see the `cutting-a-release` skill's `[package.metadata.tui-utils]`
convention), use that override's `asset_name` instead. Then:

`tab=$(mux spawn --workspace caller --cwd "$PWD" --cmd "$PWD/target/release/<binary>" --title <binary>-preview)`
then `tmux set-window-option -t "$tab" remain-on-exit on` (`mux` has no
set-option verb, so that one step stays raw tmux, targeted by the exact tab
token `mux spawn` printed, never ambiguous). `--workspace caller` targets
the calling pane's own session robustly; omit `--focus` so the new window
doesn't steal focus. Do NOT hand `mux spawn --cmd` a command wrapped in
`exec`: these apps exit on any selection/quit keypress, and an `exec`'d
window vanishes with the process, so the preview disappears the moment it's
touched; `--cmd` alone runs it as a plain foreground command in a fresh
shell, so `remain-on-exit` can keep the window open afterward. This is for
unattended runs: the app sits at its prompt waiting for input, so when the
user returns to the session the feature is previewable straight from the
command line. Launching via a plain `mux spawn` window works regardless of
whether the app's normal, real-world invocation is a plain pane or a tmux
popup -- live-preview isn't trying to replicate the exact launch context,
just get the running binary on screen. These apps detect the current
session normally as long as `$TMUX` is set in the spawned pane, which `mux
spawn` guarantees.

When the feature lives in a `wt switch --create` worktree, pin the absolute
worktree path first instead of trusting the agent shell's current directory
or any tool-side cwd override. `wt` can report `Cannot change directory`,
leaving `$PWD` in the original checkout. Use the same absolute path for both
`mux spawn --cwd` and `--cmd`, e.g. `preview_dir=/path/to/worktree; tab=$(mux
spawn --workspace caller --cwd "$preview_dir" --cmd
"$preview_dir/target/release/<binary>" --title <binary>-preview)`. After
spawning, verify the tab is live and pointed at the intended worktree with
`tmux list-windows -a -F '#{window_id} #{window_name} #{pane_current_command}
#{pane_current_path}'`.

If the feature touches anything that persists to config (a settings file,
saved state, a dormant/pinned list), isolate the preview from the real
config by pointing `XDG_CONFIG_HOME` at a scratch directory inside the
worktree, folded into the same `--cmd` string: `--cmd
"XDG_CONFIG_HOME=$preview_dir/.preview-config
$preview_dir/target/release/<binary>"`. Check this app's actual config path
first (grep for `XDG_CONFIG_HOME` or a hardcoded `~/.config/<name>` in its
source) -- without this isolation, any poking during verification (toggling
a row, editing an entry) silently mutates the user's real config. Skip it
only for apps/changes with no config-writing surface at all.

Once the preview window is confirmed live, report back a one-sentence
summary of the problem being solved and a one-sentence description of how
to test it (which keys to press, what to look for) -- so the user can jump
straight to trying it without re-deriving context from the conversation.

A prior version of this note told agents to run bare
`tmux split-window`/`new-window` with no `-t`. Don't: an agent's Bash tool
runs as a detached subprocess with no controlling tty, so that resolves
"current window" against whatever window is currently active in the
session, not the window this session is actually attached to, and the
preview can silently land somewhere else. `mux spawn --workspace caller`
avoids the whole class of bug by resolving the session robustly and always
creating a fresh window rather than depending on tmux's ambient "current
window."
