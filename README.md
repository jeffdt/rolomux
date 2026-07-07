# rolomux

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white)
![TUI](https://img.shields.io/badge/TUI-ratatui-1f6feb)
![License](https://img.shields.io/badge/license-MIT-green)
![Platform](https://img.shields.io/badge/platform-macOS%20(Apple%20Silicon)-lightgrey)
![Vibe coded](https://img.shields.io/badge/vibe%20coded-100%25-ff69b4)

A slick tmux session picker that sorts your sessions into color-coded groups to bring some zen to tmux.

Set up the groups that match how you work: a CONFIG group for the tools you're tweaking, a DEV group for what you're building, throwaway groups for whatever projects you're working on right now.
Anything you haven't sorted yet just sinks to the bottom, out of the way but never lost.

It's a productivity tool first, so it's designed to feel intuitive the moment you launch it, but it bends to fit your workflow.

![rolomux session picker](docs/images/screenshot.png)

## Install

```sh
brew install jeffdt/tap/rolomux
```

Then add a keybind to `~/.tmux.conf` to pop it open:

```tmux
bind s display-popup -E -B -w 84 -h 60% "exec rolomux"
```

Reload tmux and press `prefix + s`.

## How it works

- **Create your groups.** Press `g` to jump into group management mode, where you can create, rename and color code your groups.
- **Sort your sessions.** Move your sessions between groups with `⇧J`/`⇧K`. Once sorted, they stay there, in that order.
  New sessions drop in at the bottom of a designated catchall group, waiting to be sorted (think of it like a triage queue).
  Groups and their ordering persist across tmux restarts.
  Groups will stick around (even when empty) until you delete them.
- **Expandable tree.** Each session can be expanded to peek at the list of windows inside it.
- **Color-coded gutter.** Every session (and its windows, once expanded) shows a thin colored bar matching its group's header color, so it's visually obvious which group a row belongs to even when you've scrolled past the header.
- **On demand, no daemon.** tmux launches it via `tmux popup -E`; it makes one tmux query, renders, and exits.
  Its own overhead is a couple of milliseconds, so it opens about as fast as tmux can answer.
- **Fuzzy search built in.** Press `/` to filter sessions by name; matching is in-process with no extra runtime dependency. If this is your preferred way of working, tweak the settings to always launch in search mode.
- **Dim or hide the sessions you're not using.** Press `d` to mark a session dormant; it stays in place but renders in a dimmed state to indicate that it's on the back burner. Press `h` to hide dormant sessions entirely, and `h` again to show them. Dormant sessions are still fully usable when shown, but reduced visual noise helps you stay laser focused on the sessions that matter right now.
- **Tune the colors.** Press `,` to open Settings and tune the color of the application border, palette used for group headers, and more. Uses your terminal's ANSI colors to ensure it harmonizes with your existing terminal themes.

**Note:** rolomux depends on (and promotes) good tmux hygiene.
Get in the habit of naming your sessions and windows so you can make sense of them later.
You can reinforce this habit with your binds for creating sessions and windows, immediately prompting you for the name at creation time:
```tmux
bind c new-window -c "#{pane_current_path}" \; command-prompt -I "" "rename-window '%%'"
bind C new-session \; command-prompt -I "" "rename-session '%%'" \; command-prompt -I "" "rename-window '%%'"
```

## Keys

| Key | Action |
| --- | --- |
| `↵` | Switch to the selected session/window and close |
| `1`-`9` | Switch to that session immediately |
| `M-1`-`M-9` | Highlight and expand that session (Option/Alt) |
| `j` / `k` | Move the cursor (also `↓` / `↑`) |
| `l` / `→` | Expand a session's window tree |
| `←` | Collapse a session's window tree |
| `z` | Expand or collapse window trees for all sessions |
| `⇧J` / `⇧K` | Move the selected session up or down within its group, or into the neighboring group (also `⇧↓` / `⇧↑`) |
| `g` | Open group-management mode |
| `,` | Open settings |
| `d` | Toggle dormant (dim) on the selected session |
| `h` | Hide or show dormant sessions |
| `/` | Enter search mode (type to filter, `↵` switch, `Esc` back) |
| `q` / `Esc` | Quit |

`M-` is Meta (Option on macOS).
Your terminal must send Option as Meta: in Ghostty set `macos-option-as-alt = true` (iTerm2: "Left Option key → Esc+"; Terminal.app: "Use Option as Meta key").
On Linux it is automatic.

When a session is at the top of its group, `⇧K` jumps it up to the group above it; when it's at the bottom, `⇧J` drops it into the group below it.

### Groups

Press `g` to open group-management mode, a full-screen view of your current groups.
Once inside:

| Key | Action |
| --- | --- |
| `j` / `k` | Navigate between groups (also `↓` / `↑`) |
| `↵` / `r` | Rename the selected group |
| `n` | Create a new group and name it |
| `d` | Delete the selected group (its sessions fall back to SESSIONS) |
| `c` | Cycle the selected group's header color |
| `⇧J` / `⇧K` | Reorder the selected group down / up (also `⇧↓` / `⇧↑`) |
| `Esc` / `q` / `g` | Back to the picker |

As you create groups, they'll be assigned a color from your terminal theme (cyan, green, yellow, magenta, blue, red); new groups rotate through them, `c` flips a group's color, and empty groups show grayed out until you fill them.

### Search

Press `/` to enter search mode.
Type any part of a session name; results are ranked best-match-first, with the top result auto-selected as you type.
`Enter` switches to the highlighted session; `Esc` returns to command mode with the cursor left on the match.
Move within results with `↑`/`↓` (or `Ctrl-n`/`Ctrl-p`, `Ctrl-j`/`Ctrl-k`).
`Backspace` deletes the last character, `Ctrl-W` (or `Option/Alt` + `Backspace`) deletes the last word, and `Ctrl-U` clears the query.

While searching, section headers and jump numbers (1-9) are hidden; the list is flat and collapsed.
If dormant sessions are hidden, search results exclude them and the footer shows how many are currently hidden.

### Dormant sessions

Press `d` to mark the selected session dormant; it renders dimmed in place as a "not in active rotation" cue.
When dormant sessions are shown, they keep their jump number, group membership, and position, and nothing else about them changes.
Press `h` to hide dormant sessions entirely; press `h` again to show them.
Hidden dormant sessions are excluded from the normal picker and from search results, and both modes show a reminder such as `8 dormant sessions hidden` while the filter is active.
Press `d` again on a dormant session to undim it.
The dormant set persists across restarts; the hide/show filter is just for the current picker run.
Think of it as one more optional tool in your kit to help you tend your sessions, if you find it helpful.

### Settings

Press `,` to open Settings, a full-screen view of picker-wide preferences:

- **Default mode.** Whether the picker opens in Command mode or straight into Search.
- **Attached session color.** The color used to highlight the session your tmux client is currently attached to.
- **Border color.** rolomux's own border frame color.
- **New group color.** How a newly created group picks its header color: Rotate through the palette in order, pick a Random color each time, or always use one Static color.
- **Color palette.** Which of the 16 colors from your terminal theme are in the rotation for new group headers.

| Key | Action |
| --- | --- |
| `j` / `k` | Move between rows (also `↓` / `↑`) |
| `h` / `l` | Cycle a value / expand-collapse a color picker (also `←` / `→`) |
| `Space` / `Enter` | Toggle or activate the selected row |
| `c` | Cycle the selected color row |
| `Esc` / `q` / `,` | Back to the picker |

## Configuration

Groups, session order, dormant sessions, and settings persist to `~/.config/rolomux/config.toml`:

```toml
manual_order = ["etsy"]
dormant = ["zen-mod"]

[[groups]]
name = "CONFIG"
members = ["workbench", "config-tmux"]

[[groups]]
name = "TOOLS"
members = ["dev-stack"]
color = "magenta"  # optional; omit to use the rotating default

[settings]
default_mode = "command"           # or "search"
new_group_color_policy = "rotate"  # or "random", "static"
attached_color = "cyan"
border_color = "cyan"
```

You normally don't edit this by hand; groups, order, dormant status, and settings all save automatically as you use the picker.

## Motivation

Agent-driven development has stretched our expectations on parallelism/multitasking to the extreme, and the biggest challenge is now context switching and staying organized.
And while there are brand new tools popping up every day to launch and manage agents, tmux is an extremely mature, stable, and full-featured app that solved parallelism decades ago.
It just lacks a few key qualities for keeping things organized.
It isn't going to become vaporware, it's not going to have weird bugs that nobody catches because the userbase is too small, and its APIs are stable and easy for an agent to manipulate.
It makes more sense to solve that organization gap than to throw it away and build some new tool altogether.

## Design philosophy

I want this tool to be fun to use, nice to look at, unobtrusive, and low-commitment.
It places a high value on aesthetics but remembers it is a productivity tool first and foremost.
It doesn't hook into your tmux session and watch for sessions being created or destroyed; it relies on live data available at the time it's invoked.
It stores everything in a simple TOML file you could hand-edit if you desire, but should never have to.
It's minimalist and clean in presentation and prefers to spend its complexity on customization.
As a tool intended to be used all day every day, it should aim for zero friction.
If a workflow takes two keystrokes instead of one, there would ideally be a setting that makes it one keystroke, for the people that plan to invoke that workflow every time they launch it.
It favors your existing terminal palette over complete color customization to ensure it always feels like a "native" tool and not something bolted on later.
Any new features must make the tool easier or more fun to use.
Bloat is an enemy that must be actively kept at bay.

## Disclaimer

This project was fully vibe coded. Use at your own risk.

## License

MIT
