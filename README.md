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

You can use other dimensions, but these work well for me. `-h` also accepts a fixed line count (e.g. `-h 30`) instead of a percentage.

## How it works

- **Create your groups.** Press `g` to jump into group management mode, where you can create, rename and color code your groups. If you have not created any groups yet, the picker prompts you with the `g` then `n` flow.
- **Sort your sessions.** Move your sessions between groups with `⇧J`/`⇧K`. Once sorted, they stay there, in that order.
  New sessions drop in at the bottom of a designated catchall group, waiting to be sorted (think of it like a triage queue).
  Groups and their ordering persist across tmux restarts.
  Groups will stick around (even when empty) until you delete them.
- **Expandable tree.** Each session can be expanded to peek at the list of windows inside it.
- **Color-coded gutter.** Every session (and its windows, once expanded) shows a thin colored bar matching its group's header color, so it's visually obvious which group a row belongs to even when you've scrolled past the header.
- **On demand, no daemon.** tmux launches it via `tmux popup -E`; it makes one tmux query, renders, and exits.
  Its own overhead is a couple of milliseconds, so it opens about as fast as tmux can answer.
- **Fuzzy search built in.** Press `/` to filter sessions by name; matching is in-process with no extra runtime dependency. If this is your preferred way of working, tweak the settings to always launch in search mode.
- **Dim sessions, then focus past them.** Press `d` to mark a session dormant; it stays in place but renders in a dimmed state to indicate that it's on the back burner. Press `f` to enter focus mode, hiding dormant sessions and any group left with nothing visible in it; press `f` again to show everything. The focus choice persists across popups. Dormant sessions are still fully usable when shown, and Settings lets you choose whether they keep or skip jump numbers.
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
| `1`-`9`, `0` | Switch to session 1-10 immediately (`0` = the 10th) |
| `M-1`-`M-9`, `M-0` | Switch to session 11-20 immediately (Option/Alt; `M-1` = 11th ... `M-0` = 20th) |
| `j` / `k` | Move the cursor, wrapping between the top and bottom (also `↓` / `↑`) |
| `l` / `→` | Expand a session's window tree |
| `←` | Collapse a session's window tree |
| `z` | Expand or collapse window trees for all sessions |
| `⇧J` / `⇧K` | Move the selected session up or down within its group, or into the neighboring group (also `⇧↓` / `⇧↑`) |
| `R` | Rename the selected session or window |
| `g` | Open group-management mode |
| `,` | Open settings |
| `d` | Toggle dormant (dim) on the selected session |
| `f` | Toggle focus mode (hide dormant sessions and empty groups) |
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
| `j` / `k` | Navigate between groups, wrapping between the first and last group (also `↓` / `↑`) |
| `↵` / `r` | Rename the selected group |
| `n` | Create a new group and name it |
| `d` | Delete the selected group (its sessions fall back to the inbox group) |
| `c` | Cycle the selected group's header color |
| `⇧J` / `⇧K` | Reorder the selected group down / up (also `⇧↓` / `⇧↑`) |
| `Esc` / `q` / `g` | Back to the picker |

As you create groups, they'll be assigned a color from your terminal theme (cyan, green, yellow, magenta, blue, red); new groups rotate through them, `c` flips a group's color, and empty groups show grayed out until you fill them.

### Search

Press `/` to enter search mode.
Type any part of a session name; results are ranked best-match-first, with the top result auto-selected as you type.
`Enter` switches to the highlighted session; `Esc` returns to command mode with the cursor left on the match.
Move within results with `↑`/`↓` (or `Ctrl-n`/`Ctrl-p`, `Ctrl-j`/`Ctrl-k`), wrapping between the first and last match.
`Backspace` deletes the last character, `Ctrl-W` (or `Option/Alt` + `Backspace`) deletes the last word, and `Ctrl-U` clears the query.

While searching, section headers and jump numbers (sessions 1-20) are hidden; the list is flat and collapsed.
If focus mode is on, search results exclude dormant sessions, and the footer shows how many are currently hidden.

### Focus mode

Press `d` to mark the selected session dormant; it renders dimmed in place as a "not in active rotation" cue.
When dormant sessions are shown, they keep their group membership and position. By default they also keep jump numbers; in Settings, change **Number dormant sessions** to **No** if you want visible dormant sessions to be omitted from jump numbering.
Press `f` to enter focus mode, hiding dormant sessions entirely; press `f` again to show everything.
Hidden dormant sessions are excluded from the normal picker and from search results, and both modes show a reminder such as `8 dormant sessions hidden` while the filter is active.
Focus mode also hides any group left with nothing visible in it, whether it's genuinely empty or every member just went dormant, so a hard focus session doesn't leave empty shelves cluttering the screen. A group reappears the moment something in it becomes visible again.
Press `d` again on a dormant session to undim it.
The dormant set and the focus-mode choice both persist across popups, so reopening rolomux keeps your last focus preference.
Think of it as one more optional tool in your kit to help you tend your sessions, if you find it helpful.

### Settings

Press `,` to open Settings, a full-screen view of picker-wide preferences, grouped into two sections. A description of the currently selected setting is always shown at the bottom.

**Behavior**

- **Default mode.** Whether the picker opens in Command mode or straight into Search.
- **Number dormant sessions.** Whether visible dormant sessions are included in jump numbering (sessions 1-20).
- **Remember expanded sessions.** Off by default (every popup starts fully collapsed). When on, expanding or collapsing a session's window tree (`l`/`h`/`z`) persists across popups, so the sessions you're actively jumping between stay expanded.
- **Session metadata.** Whether the row's trailing timestamp shows time since last activity (**Recency**, default), time since the session was created (**Age**), or is omitted entirely (**Hidden**).

**Appearance**

- **Attached session color.** The color used to highlight the session your tmux client is currently attached to.
- **Border color.** rolomux's own border frame color.
- **New group color.** How a newly created group picks its header color: Rotate through the palette in order, pick a Random color each time, or always use one Static color.
- **Color palette.** Which of the 16 colors from your terminal theme are in the rotation for new group headers.

| Key | Action |
| --- | --- |
| `j` / `k` | Move between rows, wrapping between the first and last row (also `↓` / `↑`) |
| `h` / `l` | Cycle a value / expand-collapse a color picker (also `←` / `→`) |
| `Space` / `Enter` | Toggle or activate the selected row |
| `c` | Cycle the selected color row |
| `Esc` / `q` / `,` | Back to the picker |

## Configuration

Groups, session order, dormant sessions, and settings persist to `~/.config/rolomux/config.toml`:

rolomux also remembers each tracked session's tmux session id in a
`session_ids` table, so a plain `tmux rename-session` (or `prefix ,`) keeps
that session's group, dormant, and expanded state instead of losing it.

```toml
config_version = 4
dormant = ["zen-mod"]
focus_mode = true

[[groups]]
name = "CONFIG"
members = ["workbench", "config-tmux"]

[[groups]]
name = "TOOLS"
members = ["dev-stack"]
color = "magenta"  # optional; omit to use the rotating default

[[groups]]
name = "INBOX"
members = ["etsy"]
inbox = true

[settings]
default_mode = "command"           # or "search"
number_dormant_sessions = true      # false skips visible dormant sessions in jump numbering
session_metric = "recency"         # or "age", "hidden"
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
