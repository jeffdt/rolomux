# smux

An alternative to tmux's `prefix + s` session picker that adds pinned sessions
without giving up the tree view. It stays close to the native picker on purpose,
so it feels familiar rather than like a whole new tool to learn.

**Pin the sessions you keep coming back to** so they always sit at the top in an
order you choose: a session for PR reviews, one for your editor/AI config, one
for your local dev stack. Everything else, the throwaway sessions you spin up for
research or a feature and then abandon, sorts below by recency. Each session
still expands into its windows, so you keep the tree view.

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

- **Pinned first.** Pinned sessions stay on top in your order; everything else
  sorts by recency (or creation). Both persist across tmux restarts.
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
| `p` | Pin / unpin the selected session |
| `⇧J` / `⇧K` | Reorder a pinned session down / up |
| `/` | Enter search mode (type to filter, `↵` switch, `Esc` back) |
| `q` / `Esc` | Quit |

`M-` is Meta (Option on macOS). Your terminal must send Option as Meta: in
Ghostty set `macos-option-as-alt = true` (iTerm2: "Left Option key → Esc+";
Terminal.app: "Use Option as Meta key"). On Linux it is automatic.

### Search

Press `/` to enter search mode. Type any part of a session name; results are
re-ranked fuzzy best-match-first with the top result auto-selected as you type.
`Enter` switches to the highlighted session; `Esc` returns to command mode with
the cursor left on the match. Move within results with `↑`/`↓` (or `Ctrl-n`/
`Ctrl-p`, `Ctrl-j`/`Ctrl-k`). `Backspace` deletes the last character.

While searching, section headers and jump numbers (1-9) are hidden; the list is
flat and collapsed. The pinned star marker still shows. Search is read-only: it
never pins, reorders, or writes config.

## Configuration

Pins and sort order persist to `~/.config/smux/config.toml`:

```toml
pinned = ["workbench", "config-tmux"]
sort = "activity"  # or "created"
```

You normally don't edit this by hand; pin and reorder from the picker and it
saves automatically.

## Disclaimer

This project was fully vibe coded. Use at your own risk.

## License

MIT
