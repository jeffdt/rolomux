use crate::model::{PickerState, Row, Session, Window};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState};
use ratatui::Frame;

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const DOT: Color = Color::Green;
const SEL_BG: Color = Color::DarkGray;
const META_COL: usize = 30;

pub fn draw(frame: &mut Frame, state: &PickerState) {
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " smux  session picker ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let ordered = state.ordered();
    let rows = state.visible_rows();
    let cursor_row = rows.get(state.cursor).copied();

    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_line: Option<usize> = None;
    let mut emitted_pinned_header = false;
    let mut emitted_sessions_header = false;

    for row in rows.iter() {
        match row {
            Row::Session(si) => {
                let sess = ordered[*si];
                let pinned = state.is_pinned(&sess.name);
                if pinned && !emitted_pinned_header {
                    items.push(header_item("PINNED", inner.width));
                    emitted_pinned_header = true;
                }
                if !pinned && !emitted_sessions_header {
                    items.push(header_item("SESSIONS", inner.width));
                    emitted_sessions_header = true;
                }
                if Some(*row) == cursor_row {
                    selected_line = Some(items.len());
                }
                items.push(session_item(sess, pinned, state.is_expanded(&sess.name)));
            }
            Row::Window(si, wi) => {
                let sess = ordered[*si];
                if Some(*row) == cursor_row {
                    selected_line = Some(items.len());
                }
                let last = *wi + 1 == sess.windows.len();
                items.push(window_item(&sess.windows[*wi], last));
            }
        }
    }

    let list = List::new(items).highlight_style(
        Style::default().bg(SEL_BG).add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    list_state.select(selected_line);
    frame.render_stateful_widget(list, inner, &mut list_state);
}

fn header_item(label: &str, width: u16) -> ListItem<'static> {
    let rule_len = (width as usize).saturating_sub(label.chars().count() + 2);
    ListItem::new(Line::from(vec![
        Span::styled(
            label.to_string(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("─".repeat(rule_len), Style::default().fg(DIM)),
    ]))
}

fn session_item(sess: &Session, pinned: bool, expanded: bool) -> ListItem<'static> {
    let glyph = if expanded { "▾" } else { "▸" };
    let pin = if pinned { "★ " } else { "  " };
    let name_style = if sess.attached {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let prefix_len = 2 + 2 + sess.name.chars().count(); // pin + "glyph " + name
    let pad = META_COL.saturating_sub(prefix_len);
    let wins = sess.windows.len();
    let label = if wins == 1 { "window" } else { "windows" };
    ListItem::new(Line::from(vec![
        Span::styled(pin.to_string(), Style::default().fg(ACCENT)),
        Span::styled(format!("{glyph} "), Style::default().fg(DIM)),
        Span::styled(sess.name.clone(), name_style),
        Span::styled(
            format!("{}{wins} {label}", " ".repeat(pad)),
            Style::default().fg(DIM),
        ),
    ]))
}

fn window_item(win: &Window, last: bool) -> ListItem<'static> {
    let connector = if last { "   └─ " } else { "   ├─ " };
    let dot = if win.active { "●" } else { " " };
    ListItem::new(Line::from(vec![
        Span::styled(connector.to_string(), Style::default().fg(DIM)),
        Span::styled(format!("{} ", win.index), Style::default().fg(DIM)),
        Span::styled(format!("{dot} "), Style::default().fg(DOT)),
        Span::raw(win.name.clone()),
    ]))
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Up,
    Down,
    Expand,
    Collapse,
    Select,
    Pin,
    MoveUp,
    MoveDown,
    Quit,
    None,
}

pub fn map_key(key: KeyEvent) -> Input {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Input::Down,
        KeyCode::Char('k') | KeyCode::Up => Input::Up,
        KeyCode::Char('l') | KeyCode::Right => Input::Expand,
        KeyCode::Char('h') | KeyCode::Left => Input::Collapse,
        KeyCode::Enter => Input::Select,
        KeyCode::Char('p') => Input::Pin,
        KeyCode::Char('K') if shift => Input::MoveUp,
        KeyCode::Char('J') if shift => Input::MoveDown,
        KeyCode::Char('q') | KeyCode::Esc => Input::Quit,
        _ => Input::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }

    use crate::model::PickerState;
    use crate::model::{Session, SortKey, Window};
    use crate::store::Config;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_to_string(state: &PickerState) -> String {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, state)).unwrap();
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
    fn draw_shows_headers_and_session_names() {
        let sessions = vec![
            Session { name: "pr-review".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "scratch".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { pinned: vec!["pr-review".into()], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("smux"), "title present");
        assert!(text.contains("PINNED"), "pinned header present");
        assert!(text.contains("SESSIONS"), "sessions header present");
        assert!(text.contains("pr-review"), "pinned session present");
        assert!(text.contains("scratch"), "unpinned session present");
    }

    #[test]
    fn draw_marks_cursor_row_with_background() {
        let sessions = vec![
            Session { name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { pinned: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        // Find a cell on the "alpha" row and assert its bg is DarkGray.
        let mut found = false;
        for y in 0..buf.area.height {
            let mut line = String::new();
            for x in 0..buf.area.width {
                line.push_str(buf[(x, y)].symbol());
            }
            if line.contains("alpha") {
                // The glyph cells of the selected row carry the bar background.
                for x in 2..6 {
                    if buf[(x, y)].style().bg == Some(ratatui::style::Color::DarkGray) {
                        found = true;
                    }
                }
            }
        }
        assert!(found, "cursor row should have a DarkGray background bar");
    }

    #[test]
    fn maps_navigation_and_commands() {
        assert_eq!(map_key(key(KeyCode::Char('j'))), Input::Down);
        assert_eq!(map_key(key(KeyCode::Down)), Input::Down);
        assert_eq!(map_key(key(KeyCode::Char('k'))), Input::Up);
        assert_eq!(map_key(key(KeyCode::Char('l'))), Input::Expand);
        assert_eq!(map_key(key(KeyCode::Right)), Input::Expand);
        assert_eq!(map_key(key(KeyCode::Char('h'))), Input::Collapse);
        assert_eq!(map_key(key(KeyCode::Enter)), Input::Select);
        assert_eq!(map_key(key(KeyCode::Char('p'))), Input::Pin);
        assert_eq!(map_key(key(KeyCode::Char('q'))), Input::Quit);
        assert_eq!(map_key(key(KeyCode::Esc)), Input::Quit);
        assert_eq!(map_key(shift(KeyCode::Char('K'))), Input::MoveUp);
        assert_eq!(map_key(shift(KeyCode::Char('J'))), Input::MoveDown);
        assert_eq!(map_key(key(KeyCode::Char('x'))), Input::None);
    }
}
