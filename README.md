# smux

A fast [tmux](https://github.com/tmux/tmux) session picker, built to replace
`prefix + s`. Pinned sessions stay on top in an order you control; everything
else sorts by recency. Sessions expand into a collapsible window tree, and you
can pin, reorder, and jump entirely from the keyboard. It opens in well under
100ms via `tmux popup -E` and respects your terminal's color theme (named ANSI
only, no hardcoded RGB).

## Install

```sh
brew install jeffdt/tap/smux
```

Then bind it in `~/.tmux.conf`:

```tmux
bind S display-popup -E -w 80% -h 80% "exec smux"
```

Reload tmux (`prefix + :source-file ~/.tmux.conf`) and press `prefix + Shift+S`.

## Keys

| Key | Action |
| --- | --- |
| `↵` | Switch to the selected session/window and close |
| `1`-`9` | Switch to that session immediately |
| `M-1`-`M-9` | Focus that session and expand it (Option/Alt) |
| `j` / `k` (or `↓` / `↑`) | Move the cursor |
| `l` / `h` | Expand / collapse a session |
| `z` | Expand or collapse all |
| `p` | Pin / unpin the selected session |
| `⇧J` / `⇧K` | Reorder a pinned session down / up |
| `q` / `Esc` | Quit |

`M-` is Meta (Option on macOS). On macOS your terminal must send Option as
Meta: in Ghostty set `macos-option-as-alt = true` (iTerm2: "Left Option key →
Esc+"; Terminal.app: "Use Option as Meta key"). On Linux this is automatic.

## Configuration

Pins and sort order persist to `~/.config/smux/config.toml` (or
`$XDG_CONFIG_HOME/smux/config.toml`):

```toml
pinned = ["workbench", "config-tmux"]
sort = "activity"  # or "created"
```

You normally don't edit this by hand: pin/unpin/reorder from the picker and it
saves automatically.

## License

MIT
