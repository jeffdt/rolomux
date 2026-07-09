# rolomux

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white)
![TUI](https://img.shields.io/badge/TUI-ratatui-1f6feb)
![License](https://img.shields.io/badge/license-MIT-green)
![Platform](https://img.shields.io/badge/platform-macOS%20(Apple%20Silicon)-lightgrey)
![Vibe coded](https://img.shields.io/badge/vibe%20coded-100%25-ff69b4)

A slick tmux session picker that sorts your sessions into color-coded groups to bring you terminal zen.

A tidy workshop is easier to work in. Give every session a home, keep active projects front and center, and leave the rest waiting quietly.

It's useful from the moment you launch it but easily shapes to your workflow.

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

You can use other popup dimensions, but these work well to start.

## Quick start
1. Press `prefix + s` to open rolomux. All of your sessions start out in a group called `INBOX`.
2. Move a few sessions around with `⇧J` and `⇧K`.
3. Press `g` to open Group Management.
4. Press `n` to create a new group. Call it `PROJECTS`, then press `Esc` to return to the picker.
5. Move a session into `PROJECTS` with `⇧J` and `⇧K`.
6. Try creating a few more groups, reordering the groups themselves, and changing their colors.

New sessions arrive in INBOX. Over time, you sort, group, and reorder them until your workspace feels the way you want it to.

## How it works

- **Create your groups.** Press `g` to jump into group management mode, where you can create, rename and color code your groups.
- **Sort your sessions.** Move your sessions between groups with `⇧J`/`⇧K`. Once sorted, they stay there, in that order.
  New sessions collect in an inbox until you're ready to sort them.
  Groups and their ordering persist across tmux restarts, and they stick around until you delete them.
- **Expandable trees.** Each session can be expanded to peek at the list of windows inside it or jump straight to a specific window.
- **Fast.** With practically no overhead of its own, it opens about as fast as tmux can describe its sessions.
- **Fuzzy search built in.** Press `/` to filter sessions by name. If this is your preferred way of working, tweak the settings to always launch in search mode.
- **Dim or hide the sessions you're not using.** Press `d` to mark a session dormant; it stays in place but renders in a dimmed state to indicate that it's on the back burner. Press `h` to hide dormant sessions entirely, and `h` again to show them. Dormant sessions are still fully usable, but help you understand your workspace at a glance.
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
If dormant sessions are hidden, search results exclude them and the footer shows how many are currently hidden.

### Dormant sessions

Press `d` to mark the selected session dormant; it renders dimmed in place as a "not in active rotation" cue.
When dormant sessions are shown, they keep their group membership and position. By default they also keep jump numbers; in Settings, change **Number dormant sessions** to **No** if you want visible dormant sessions to be omitted from jump numbering.
Press `h` to hide dormant sessions entirely; press `h` again to show them.
Hidden dormant sessions are excluded from the normal picker and from search results, and both modes show a reminder such as `8 dormant sessions hidden` while the filter is active.
Press `d` again on a dormant session to undim it.
The dormant set and the hide/show filter both persist across popups, so reopening rolomux keeps your last dormant visibility choice.
Think of it as one more optional tool in your kit to help you tend your sessions, if you find it helpful.

### Settings

Press `,` to open Settings, a full-screen view of picker-wide preferences, grouped into two sections:

**Behavior**

- **Default mode.** Whether the picker opens in Command mode or straight into Search.
- **Number dormant sessions.** Whether visible dormant sessions are included in jump numbering (sessions 1-20).
- **Remember expanded sessions.** Off by default (every popup starts fully collapsed). When on, expanding or collapsing a session's window tree (`l`/`h`/`z`) persists across popups, so the sessions you're actively jumping between stay expanded.

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

```toml
config_version = 3
dormant = ["zen-mod"]
hide_dormant = true

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
