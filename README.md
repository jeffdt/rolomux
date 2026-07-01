# smux

An alternative to tmux's `prefix + s` session picker that adds named session
groups without giving up the tree view. It stays close to the native picker on
purpose, so it feels familiar rather than like a whole new tool to learn.

**Group the sessions you keep coming back to** so they always sit at the top in
named groups whose order you choose: a CONFIG group for your editor/AI config, a
TOOLS group for local dev stacks, whatever fits how you work. Everything else,
the throwaway sessions you spin up for research or a feature and then abandon,
sorts below under SESSIONS by recency. Each session still expands into its
windows, so you keep the tree view.

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust&logoColor=white)
![TUI](https://img.shields.io/badge/TUI-ratatui-1f6feb)
![License](https://img.shields.io/badge/license-MIT-green)
![Platform](https://img.shields.io/badge/platform-macOS%20(Apple%20Silicon)-lightgrey)
![Vibe coded](https://img.shields.io/badge/vibe%20coded-100%25-ff69b4)

## Install

```sh
brew install jeffdt/tap/smux
```

Then add a keybind to `~/.tmux.conf`:

```tmux
bind S display-popup -E -B -w 84 -h 60% "exec smux"
```

`-B` drops tmux's popup border so smux's own framed card is the only border;
the fixed 84-column width and 60% height keep it a compact, centered panel.

Reload tmux and press `prefix + Shift+S`.

## How it works

- **Grouped first.** Sessions you sort into named groups stay on top in your
  order; everything else sorts below under SESSIONS by recency (or creation).
  Groups, their order, and the sort mode persist across tmux restarts.
- **Group management mode.** Press `g` for a dedicated view to create, rename,
  delete, and reorder groups. Move sessions between groups from the picker with
  `⇧J` / `⇧K`. Groups are durable: they stay even when empty, until you delete
  them.
- **Expandable tree.** Each session expands into its windows, choose-tree style.
- **On demand, no daemon.** tmux launches it via `tmux popup -E`; it makes one
  tmux query, renders, and exits. Its own overhead is a couple of milliseconds,
  so it opens about as fast as tmux can answer.
- **Fuzzy search built in.** Press `/` to filter sessions by name; matching is
  in-process with no extra runtime dependency.

## Keys

| Key | Action |
| --- | --- |
| `↵` | Switch to the selected session/window and close |
| `1`-`9` | Switch to that session immediately |
| `M-1`-`M-9` | Focus and expand that session (Option/Alt) |
| `j` / `k` | Move the cursor (also `↓` / `↑`) |
| `l` / `h` | Expand / collapse a session |
| `z` | Expand or collapse all |
| `⇧J` / `⇧K` | Move the selected session across group boundaries (down / up) |
| `g` | Open group-management mode |
| `s` | Cycle the SESSIONS sort mode (recency, age, manual) |
| `/` | Enter search mode (type to filter, `↵` switch, `Esc` back) |
| `q` / `Esc` | Quit |

`M-` is Meta (Option on macOS). Your terminal must send Option as Meta: in
Ghostty set `macos-option-as-alt = true` (iTerm2: "Left Option key → Esc+";
Terminal.app: "Use Option as Meta key"). On Linux it is automatic.

At the top of its group, `⇧K` lifts a session out to the bottom of the group
above; at the bottom, `⇧J` drops it to the top of the group below. Moving a
session down out of the last group ungroups it back into SESSIONS.

### Groups

Press `g` to open group-management mode, a full-screen view of just your groups
(sessions stay in the picker). It is built to be frictionless once you are in:

| Key | Action |
| --- | --- |
| `j` / `k` | Move between groups (also `↓` / `↑`) |
| `↵` / `r` | Rename the selected group inline |
| `n` | Create a new group and name it |
| `d` | Delete the selected group (its sessions fall back to SESSIONS) |
| `c` | Cycle the selected group's header color |
| `⇧J` / `⇧K` | Reorder the selected group down / up |
| `Esc` / `q` / `g` | Back to the picker |

Named groups are always in the manual order you set; only the residual SESSIONS
bucket follows the `s` sort mode. Each group's header takes a color from your
terminal theme (cyan, green, yellow, magenta, blue, red); new groups rotate
through them, `c` flips a group's color, and empty groups show grayed out until
you fill them.

### Search

Press `/` to enter search mode. Type any part of a session name; results are
re-ranked fuzzy best-match-first with the top result auto-selected as you type.
`Enter` switches to the highlighted session; `Esc` returns to command mode with
the cursor left on the match. Move within results with `↑`/`↓` (or `Ctrl-n`/
`Ctrl-p`, `Ctrl-j`/`Ctrl-k`). `Backspace` deletes the last character.

While searching, section headers and jump numbers (1-9) are hidden; the list is
flat and collapsed. Search is read-only: it never groups, reorders, or writes
config.

## Configuration

Groups and sort order persist to `~/.config/smux/config.toml`:

```toml
sort = "activity"  # "activity", "created", or "manual"

[[groups]]
name = "CONFIG"
members = ["workbench", "config-tmux"]

[[groups]]
name = "TOOLS"
members = ["dev-stack"]
color = "magenta"  # optional; omit to use the rotating default
```

You normally don't edit this by hand; create groups and reorder from the picker
and it saves automatically. An older `pinned = [...]` config still loads: its
entries migrate into a single group named PINNED.

## Disclaimer

This project was fully vibe coded. Use at your own risk.

## License

MIT
