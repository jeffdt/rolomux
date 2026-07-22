---
name: mockup
description: >-
  Use when building a terminal/ANSI mockup for a design discussion, before
  locking in a visual or rendering change (AGENTS.md's "mock up
  visual/rendering changes before writing the spec" step). Triggers include
  "mock this up", "show me a mockup", "render this as ANSI", "let's compare
  a couple of layouts". Do NOT use for the separate live-binary-preview
  workflow (launching the real compiled binary via `mux spawn --cmd
  target/release/<binary>`), since that already runs real code and has no
  quality-consistency problem to fix.
---

# Terminal mockup

Standardizes how fake (not-real-binary) ANSI terminal mockups get built for
design discussions, so they no longer vary in quality by construction
method, window naming, or dimension accuracy.

## 1. Start: task ID and topic

Generate one short ID at the start of the mockup task, reused for every
window/pane spawned during it:

```bash
id=$(date +%H%M%S)
```

Pick a topic: the GitHub issue number if this work is tied to one (e.g.
`70`), else a short kebab-case description (e.g. `settings-rename`). Window
titles are always `<topic>-mockup-<id>` (never spaces, ever).

## 2. Pick a construction method

**Default: Python.** Use for the common case: text/layout mockups that
don't need to prove an exact color match or a genuinely complex multi-panel
widget layout.

**Escalate to ratatui** only when: the mockup needs to verify an exact color
against the app's real palette, or the layout is complex enough that a
hand-approximated widget risks misleading the design discussion. Ratatui
mockups cost meaningfully more (boilerplate widget/layout code, a
`cargo build`/`run` cycle, real risk of a compile-error round-trip), so
don't reach for it by default.

## 3. Default construction: Python (stdlib only)

Write the mockup script to the Claude Code scratchpad directory (never into
the repo). Every mockup uses this helper; never hand-pad a row:

```python
import re

ANSI_FG = {
    'Black': 30, 'Red': 31, 'Green': 32, 'Yellow': 33,
    'Blue': 34, 'Magenta': 35, 'Cyan': 36, 'Gray': 37,
    'DarkGray': 90, 'LightRed': 91, 'LightGreen': 92, 'LightYellow': 93,
    'LightBlue': 94, 'LightMagenta': 95, 'LightCyan': 96, 'White': 97,
}
RESET = "\x1b[0m"

def fg(name: str) -> str:
    return f"\x1b[{ANSI_FG[name]}m"

ANSI_RE = re.compile(r'\x1b\[[0-9;]*m')

def visible_width(s: str) -> int:
    return len(ANSI_RE.sub('', s))

def row(content: str, width: int, left='│', right='│') -> str:
    pad = width - visible_width(content) - visible_width(left) - visible_width(right)
    return f"{left}{content}{' ' * max(pad, 0)}{right}"
```

`visible_width` always measures the ANSI-stripped string, so the right
border lands in the same column on every row regardless of how many color
codes are embedded earlier in that row. `wcwidth` is unnecessary as long as
the project's UI is plain ASCII box-drawing with no wide characters.

Example full mockup (an 84-col card with a 2-cell margin, one colored
header row):

```python
WIDTH = 84
MARGIN = 2
CARD_WIDTH = WIDTH - MARGIN * 2  # 80

def main():
    blank_margin = " " * WIDTH
    print(blank_margin)
    print(" " * MARGIN + "┌" + "─" * (CARD_WIDTH - 2) + "┐" + " " * MARGIN)
    title = f"{fg('Cyan')} PINNED{RESET}"
    print(" " * MARGIN + row(title, CARD_WIDTH) + " " * MARGIN)
    print(" " * MARGIN + row("  1  my-session", CARD_WIDTH) + " " * MARGIN)
    print(" " * MARGIN + "└" + "─" * (CARD_WIDTH - 2) + "┘" + " " * MARGIN)
    print(blank_margin)

if __name__ == "__main__":
    main()
```

## 4. Escalation: ratatui-rendered mockups

Write a throwaway `examples/mockup.rs` in the project repo, always this
exact filename (one standing `.gitignore` entry, never a new name per
mockup). It builds the screen from real `ratatui` widgets using
`CrosstermBackend<Vec<u8>>`, which captures the actual SGR/cursor bytes
crossterm would emit to a real terminal: the same rendering path the
shipped binary uses, so alignment and color output are correct by
construction, not by care.

Use `Viewport::Fixed` explicitly. Without it, `CrosstermBackend::size()`
queries the real attached terminal's size via `crossterm::terminal::size()`,
which is wrong (or outright errors, since the agent's Bash tool has no
controlling tty) when rendering headlessly. A fixed viewport sidesteps this
entirely.

`CrosstermBackend::writer()` is gated behind an unstable ratatui feature and
isn't callable on the pinned version, so don't rely on it to get the bytes
back out. Instead wrap the `Vec<u8>` in `Rc<RefCell<...>>` and keep your own
handle to it:

```rust
use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;

use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal, TerminalOptions, Viewport,
};

const WIDTH: u16 = 84;
const HEIGHT: u16 = 20;
const MARGIN: u16 = 2;

#[derive(Clone)]
struct SharedBuf(Rc<RefCell<Vec<u8>>>);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn main() -> std::io::Result<()> {
    let buf = Rc::new(RefCell::new(Vec::<u8>::new()));
    let backend = CrosstermBackend::new(SharedBuf(buf.clone()));
    let viewport = Viewport::Fixed(Rect::new(0, 0, WIDTH, HEIGHT));
    let mut terminal = Terminal::with_options(backend, TerminalOptions { viewport })?;
    terminal.draw(|frame| {
        let area = Rect::new(0, 0, WIDTH, HEIGHT);
        let inner = Rect::new(
            area.x + MARGIN,
            area.y + MARGIN,
            area.width.saturating_sub(MARGIN * 2),
            area.height.saturating_sub(MARGIN * 2),
        );
        let block = Block::default().borders(Borders::ALL);
        let title = Paragraph::new(Line::from(Span::styled(
            " PINNED",
            Style::default().fg(Color::Cyan),
        )))
        .block(block);
        frame.render_widget(title, inner);
    })?;
    let bytes = buf.borrow();
    print!("{}", String::from_utf8_lossy(&bytes));
    Ok(())
}
```

Run via `cargo run --example mockup --quiet`. If the crate is bin-only (no
lib target), the example uses `ratatui::style::Color`'s named variants
directly rather than importing the app's own internal color constants.

Note: crossterm serializes ratatui's named colors as `\x1b[38;5;0`
through `\x1b[38;5;15` (an indexed SGR form), not the classic
`\x1b[30`-`\x1b[37`/`\x1b[90`-`\x1b[97` form the Python path uses. That's
still the terminal's themed 16-color palette, not true 256-color or RGB;
indices 0-15 are exactly the same 16 named colors, just addressed
differently. Don't mistake a `38;5;N` (N <= 15) match for a color-fidelity
violation when checking ratatui-path output.

## 5. Standards (both methods)

- **Width**: always 84 columns, `POPUP_MARGIN`-inset (2-cell blank margin,
  then an 80-col drawn card). Height has no fixed number: pick whatever's
  reasonable for the content.
- **Colors**: only the 16 named ANSI colors shown in the `ANSI_FG` table
  above (matches `ratatui::style::Color`'s named, non-RGB variants); never
  invent a color the real picker couldn't produce.

## 6. Launching

```bash
tab=$(mux spawn --workspace caller --title "${topic}-mockup-${id}" --json | jq -r .tab)
```

Always prefix the actual render command with `clear &&` when sending it to
the pane: not needed for the Python path (sequential `print()` never uses
absolute cursor positioning), but required for the ratatui path, since
`CrosstermBackend`'s diffing renderer emits absolute `MoveTo(x,y)` codes
that would otherwise land on top of whatever the pane's shell prompt already
printed:

```bash
mux send --tab "$tab" --cmd "clear && python3 /path/to/scratch_mockup.py"
# or, for the ratatui path:
mux send --tab "$tab" --cmd "clear && cargo run --example mockup --quiet"
```

**Comparing 2+ options**: spawn one window, then add a real tmux pane per
extra option (never a separate window, never a virtualized text trick):

```bash
width=$(tmux display-message -p -t "$tab" '#{window_width}')
# N options need N*84 + (N-1) columns to fit true left-right
```

If `$width` is large enough for all options at 84 cols each plus divider
columns, split left-right:

```bash
tmux split-window -h -t "$tab"
```

Otherwise split stacked top-bottom (tmux's default; omit `-h`), which
always preserves each pane's 84-col width regardless of window size:

```bash
tmux split-window -t "$tab"
```

Write one script per option and label each variant inline in its own title
row (e.g. `rolomux -- BEFORE arrow` / `rolomux -- AFTER arrow`) so the two
panes are distinguishable at a glance without relying on pane position or
memory of which command went where.

`mux send --tab` only reaches a tab's *active* pane, so once split, target
each pane directly. Grab their IDs first, then send each variant's command
to its own pane:

```bash
tmux list-panes -t "$tab" -F '#{pane_id}'
tmux send-keys -t "%142" "clear && python3 /path/to/variant_a.py" Enter
tmux send-keys -t "%143" "clear && python3 /path/to/variant_b.py" Enter
```

## 7. Cleanup (mandatory, every time)

Once you've reacted to the mockup (approved it, asked for changes, or
moved on), tear down unconditionally, not just on approval:

```bash
mux close --tab "$tab"
rm -f /path/to/scratch_mockup.py
rm -f examples/mockup.rs   # only if the ratatui path was used
```

Deleting `examples/mockup.rs` isn't just tidiness: it sits under `cargo
clippy --all-targets -- -D warnings`, part of your own local build/test
loop. Leaving it around would break the *next*, unrelated `cargo clippy`
run.
