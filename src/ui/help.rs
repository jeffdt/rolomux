//! The `?` shortcuts overlay: a centered panel listing every keybinding for
//! the current mode. Drawn on top of the mode's normal content by `ui::draw`
//! when `PickerState::help_visible()` is true; closed by `Esc`, `q`, or `?`
//! (see `input::is_help_dismiss_key`). Search mode never opens this overlay
//! (`?` stays literal query text there), so its branch below is unreachable
//! through normal input handling -- kept only so the match stays exhaustive.

use super::*;
use ratatui::widgets::Clear;

const COMMAND_SHORTCUTS: &[(&str, &str)] = &[
    ("j/k, ↑/↓", "move cursor"),
    ("l/→, ←", "expand / collapse session"),
    ("Enter", "switch to selected session/window"),
    ("1-9,0 / Alt+1-9,0", "jump to session N (1-20)"),
    ("/", "search"),
    ("g", "group altitude"),
    ("⇧N", "new group around session"),
    (",", "settings"),
    ("z", "expand/collapse all"),
    ("f", "toggle focus mode (hide dormant)"),
    ("d", "toggle dormant"),
    ("Ctrl-d", "clear dormant for this session"),
    ("⇧D", "clear dormant for everything"),
    ("r", "rename session/window"),
    ("⇧J/⇧K", "move window to adjacent session"),
    ("x", "kill session/window (press twice)"),
    ("q / Esc", "quit"),
];

const ALTITUDE_SHORTCUTS: &[(&str, &str)] = &[
    ("j/k, ↑/↓", "move cursor"),
    ("⇧J/⇧K", "reorder group"),
    ("n", "new group"),
    ("r", "rename group"),
    ("c", "cycle color"),
    ("x", "delete group (press twice)"),
    ("Enter", "open group"),
    ("/", "search"),
    ("1-9,0 / Alt+1-9,0", "jump to session N (1-20)"),
    ("Esc / g", "back to session altitude"),
    ("q", "quit"),
];

const SETTINGS_SHORTCUTS: &[(&str, &str)] = &[
    ("j/k, ↑/↓", "move cursor"),
    ("h/l, ←/→", "cycle value / collapse / expand"),
    ("Enter / Space", "activate row"),
    ("1-9,0 / Alt+1-9,0", "jump to row N (1-20)"),
    ("c", "cycle color swatch"),
    ("Esc / q / ,", "back to command mode"),
];

fn shortcuts_for_mode(mode: Mode) -> (&'static str, &'static [(&'static str, &'static str)]) {
    match mode {
        Mode::Command => ("Command Shortcuts", COMMAND_SHORTCUTS),
        Mode::Groups => ("Group Altitude Shortcuts", ALTITUDE_SHORTCUTS),
        Mode::Settings => ("Settings Shortcuts", SETTINGS_SHORTCUTS),
        Mode::Search => ("Search Shortcuts", &[]),
    }
}

const PANEL_WIDTH: u16 = 56;
const CLOSE_HINT: &str = "Esc / q / ? close";

/// Centers a `width` x `height` box inside `area`, clamping both dimensions
/// down to fit when `area` is smaller than requested. The first use of
/// `ratatui::widgets::Clear` (and this helper) in rolomux -- every other
/// mode renders straight into its content chunk with no floating layer.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect { x, y, width, height }
}

pub(super) fn draw_help_overlay(frame: &mut Frame, state: &PickerState, area: Rect) {
    let (title, shortcuts) = shortcuts_for_mode(state.mode);

    // Reserve 2 rows for the panel's own border plus 2 for the trailing
    // blank/close-hint lines; whatever's left is how many shortcut rows
    // actually fit. A static list truncates with a "+N more" marker rather
    // than introducing scroll state -- see the design spec.
    let overhead = 4u16;
    let max_rows = area.height.saturating_sub(overhead) as usize;
    let (shown, hidden_count) = if max_rows == 0 {
        (&shortcuts[..0], shortcuts.len())
    } else if shortcuts.len() > max_rows {
        (&shortcuts[..max_rows - 1], shortcuts.len() - (max_rows - 1))
    } else {
        (shortcuts, 0)
    };

    let key_color = color_from_name(&state.shortcut_color);
    let mut lines: Vec<Line> = shown
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(key.to_string(), Style::default().fg(key_color)),
                Span::styled(format!(" {desc}"), Style::default().fg(DIM)),
            ])
        })
        .collect();
    if hidden_count > 0 {
        lines.push(Line::from(Span::styled(format!("+{hidden_count} more"), Style::default().fg(DIM))));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(CLOSE_HINT, Style::default().fg(DIM))));

    let panel_height = (lines.len() as u16 + 2).min(area.height);
    let panel = centered_rect(PANEL_WIDTH, panel_height, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(key_color))
        .title(Span::styled(format!(" {title} "), Style::default().add_modifier(Modifier::BOLD)));

    frame.render_widget(Clear, panel);
    frame.render_widget(Paragraph::new(lines).block(block), panel);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Config;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_help_to_string(state: &PickerState) -> String {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw_help_overlay(f, state, f.area())).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn centered_rect_centers_within_a_larger_area() {
        let area = Rect { x: 10, y: 5, width: 80, height: 40 };
        let r = centered_rect(56, 20, area);
        assert_eq!(r.width, 56);
        assert_eq!(r.height, 20);
        assert_eq!(r.x, 10 + (80 - 56) / 2);
        assert_eq!(r.y, 5 + (40 - 20) / 2);
    }

    #[test]
    fn centered_rect_clamps_to_a_smaller_area() {
        let area = Rect { x: 0, y: 0, width: 30, height: 10 };
        let r = centered_rect(56, 20, area);
        assert_eq!(r.width, 30);
        assert_eq!(r.height, 10);
        assert_eq!(r.x, 0);
        assert_eq!(r.y, 0);
    }

    #[test]
    fn shortcuts_for_mode_covers_command_groups_and_settings() {
        assert_eq!(shortcuts_for_mode(Mode::Command).0, "Command Shortcuts");
        assert!(!shortcuts_for_mode(Mode::Command).1.is_empty());
        assert_eq!(shortcuts_for_mode(Mode::Groups).0, "Group Altitude Shortcuts");
        assert!(!shortcuts_for_mode(Mode::Groups).1.is_empty());
        assert_eq!(shortcuts_for_mode(Mode::Settings).0, "Settings Shortcuts");
        assert!(!shortcuts_for_mode(Mode::Settings).1.is_empty());
    }

    #[test]
    fn command_mode_help_contains_new_group_and_group_altitude() {
        let mut state = PickerState::build(vec![], &Config::default());
        state.mode = Mode::Command;
        let rendered = render_help_to_string(&state);
        assert!(
            rendered.contains("⇧N"),
            "Command mode help should contain ⇧N key for new group around session"
        );
        assert!(
            rendered.contains("group altitude"),
            "Command mode help should describe g as 'group altitude'"
        );
    }

    #[test]
    fn altitude_mode_help_does_not_contain_stale_shift_r() {
        let mut state = PickerState::build(vec![], &Config::default());
        state.mode = Mode::Groups;
        let rendered = render_help_to_string(&state);
        assert!(
            !rendered.contains("⇧R"),
            "Group altitude mode help should not contain stale ⇧R text"
        );
    }

    #[test]
    fn altitude_mode_help_shows_q_as_quit_not_back() {
        let mut state = PickerState::build(vec![], &Config::default());
        state.mode = Mode::Groups;
        let rendered = render_help_to_string(&state);
        assert!(
            !rendered.contains("Esc / q / g"),
            "q no longer shares a 'back to command mode' entry with Esc/g at group altitude"
        );
        assert!(
            rendered.contains("back to session altitude"),
            "Esc/g should still be documented as returning to session altitude"
        );
        let q_quit_line = rendered.lines().any(|l| l.contains('q') && l.contains("quit"));
        assert!(q_quit_line, "q should be documented as quitting the picker at group altitude");
    }
}
