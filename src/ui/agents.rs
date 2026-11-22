//! Agent-view rendering: every agent pane grouped under a header per state.
//! Shared helpers and palette constants come from the parent `ui` module.

use super::*;
use crate::model::AgentRow;

const MAX_WINDOW_WIDTH: usize = 24;
const AGENTS_FOOTER_HINT: &str = "1-9 jump · ↵ switch · a tree · q quit";

/// Header text for a state's section.
fn section_title(state: AgentState) -> &'static str {
    match state {
        AgentState::Waiting => "NEEDS YOU",
        AgentState::Done => "DONE",
        AgentState::Working => "WORKING",
    }
}

/// Replaces a leading `home` directory in `path` with `~`.
fn tilde_home(path: &str, home: Option<&str>) -> String {
    match home.map(|h| h.trim_end_matches('/')).filter(|h| !h.is_empty()) {
        Some(h) => match path.strip_prefix(h) {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
            _ => path.to_string(),
        },
        None => path.to_string(),
    }
}

/// Cuts `text` to at most `max` chars, ending in `…` when it was shortened.
fn truncate_end(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// One agent row: jump number, state glyph, session, window and path.
fn agent_item(row: &AgentRow, number: usize, selected: bool, name_width: usize, window_width: usize, width: usize, home: Option<&str>) -> ListItem<'static> {
    let color = agent_color(row.state);
    let label = if number <= 9 { jump_label(number) } else { "  ".to_string() };
    let name = format!("{:<name_width$}", row.session);
    let window = format!("{:<window_width$}", truncate_end(&row.window_name, window_width));
    let prefix_width = 2 + 2 + 1 + 1 + name_width + 3 + window_width + 1;
    let path_room = width.saturating_sub(prefix_width);
    let path = truncate_end(&tilde_home(&row.path, home), path_room);
    let dim = secondary(selected);
    ListItem::new(Line::from(vec![
        Span::styled("│ ", Style::default().fg(color)),
        Span::styled(label, dim),
        Span::styled(AGENT_GLYPH, Style::default().fg(color)),
        Span::raw(" "),
        Span::raw(name),
        Span::styled(" › ", dim),
        Span::raw(window),
        Span::raw(" "),
        Span::styled(path, dim),
    ]))
}

/// Draws the agent view: rows grouped under per-state headers, plus the footer.
pub(super) fn draw_agents(frame: &mut Frame, state: &PickerState, inner: Rect) {
    draw_agents_with_home(frame, state, inner, std::env::var("HOME").ok().as_deref());
}

fn draw_agents_with_home(frame: &mut Frame, state: &PickerState, inner: Rect, home: Option<&str>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(inner);

    let rows = state.agent_rows();
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_line: Option<usize> = None;
    if rows.is_empty() {
        for text in [" no agents reporting state", " agents appear once their hooks set @agent_state (see README)"] {
            items.push(ListItem::new(Line::from(Span::styled(text, Style::default().fg(DIM)))));
        }
    } else {
        let name_width = rows.iter().map(|r| r.session.chars().count()).max().unwrap_or(0);
        let window_width = rows.iter().map(|r| r.window_name.chars().count()).max().unwrap_or(0).min(MAX_WINDOW_WIDTH);
        let mut current: Option<AgentState> = None;
        for (i, row) in rows.iter().enumerate() {
            if current != Some(row.state) {
                if current.is_some() {
                    items.push(ListItem::new(Line::from("")));
                }
                current = Some(row.state);
                items.push(ListItem::new(Line::from(Span::styled(
                    format!(" {}", section_title(row.state)),
                    Style::default().fg(agent_color(row.state)).add_modifier(Modifier::BOLD),
                ))));
            }
            let selected = i == state.agent_cursor();
            if selected {
                selected_line = Some(items.len());
            }
            items.push(agent_item(row, i + 1, selected, name_width, window_width, chunks[0].width as usize, home));
        }
    }
    let list = List::new(items).highlight_style(Style::default().bg(SEL_BG).add_modifier(Modifier::BOLD));
    let mut list_state = ListState::default();
    list_state.select(selected_line);
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    let footer = Paragraph::new(vec![
        Line::from(Span::styled("─".repeat(chunks[1].width as usize), Style::default().fg(DIM))),
        styled_hint(AGENTS_FOOTER_HINT, color_from_name(&state.shortcut_color)),
    ]);
    frame.render_widget(footer, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentPane, Window};
    use crate::store::Config;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    fn pane(id: &str, state: AgentState, path: &str) -> AgentPane {
        AgentPane { pane_id: id.into(), state, path: path.into() }
    }

    fn state_with(agents: Vec<(&str, Vec<AgentPane>)>) -> PickerState {
        let sessions = agents
            .iter()
            .enumerate()
            .map(|(i, (name, _))| Session {
                id: format!("${i}"),
                name: (*name).into(),
                activity: 30,
                created: 1,
                attached: false,
                windows: vec![Window { id: format!("@{i}"), index: 0, name: format!("win-{name}"), active: false }],
            })
            .collect();
        let mut st = PickerState::build(sessions, &Config { groups: vec![], ..Default::default() });
        st.set_agents(agents.into_iter().enumerate().map(|(i, (_, p))| (format!("@{i}"), p)).collect());
        st.enter_agents();
        st
    }

    fn render(st: &PickerState, home: Option<&str>) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal
            .draw(|f| {
                let area = inset(f.area(), POPUP_MARGIN);
                draw_agents_with_home(f, st, area, home);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn text_of(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect()
    }

    fn row_with(buf: &Buffer, needle: &str) -> u16 {
        text_of(buf).iter().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("no row with {needle:?}")) as u16
    }

    fn mixed() -> PickerState {
        state_with(vec![
            ("alpha", vec![pane("%1", AgentState::Working, "/w/alpha")]),
            ("beta", vec![pane("%2", AgentState::Waiting, "/home/me/code/beta")]),
            ("gamma", vec![pane("%3", AgentState::Done, "/g")]),
        ])
    }

    #[test]
    fn headers_appear_in_urgency_order_and_empty_sections_are_hidden() {
        let buf = render(&mixed(), None);
        let (n, d, w) = (row_with(&buf, "NEEDS YOU"), row_with(&buf, "DONE"), row_with(&buf, "WORKING"));
        assert!(n < d && d < w);
        let st = state_with(vec![("alpha", vec![pane("%1", AgentState::Done, "/a")])]);
        let text = text_of(&render(&st, None)).join("\n");
        assert!(text.contains("DONE") && !text.contains("NEEDS YOU") && !text.contains("WORKING"));
    }

    #[test]
    fn rows_are_numbered_in_display_order_and_overflow_is_blank() {
        let buf = render(&mixed(), None);
        assert!(text_of(&buf)[row_with(&buf, "beta") as usize].contains("│ 1 ✻ beta"));
        assert!(text_of(&buf)[row_with(&buf, "gamma") as usize].contains("│ 2 ✻ gamma"));
        assert!(text_of(&buf)[row_with(&buf, "alpha") as usize].contains("│ 3 ✻ alpha"));
        let many = state_with((0..11).map(|i| (Box::leak(format!("s{i:02}").into_boxed_str()) as &str, vec![pane("%1", AgentState::Working, "/p")])).collect());
        let buf = render_sized(&many, 40);
        assert!(text_of(&buf)[row_with(&buf, "s08") as usize].contains("│ 9 ✻"));
        assert!(text_of(&buf)[row_with(&buf, "s09") as usize].contains("│   ✻"));
    }

    fn render_sized(st: &PickerState, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(80, height)).unwrap();
        terminal.draw(|f| draw_agents_with_home(f, st, f.area(), None)).unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn header_gutter_and_glyph_use_the_state_color() {
        let buf = render(&mixed(), None);
        for (needle, color) in [("NEEDS YOU", Color::Red), ("DONE", Color::Yellow), ("WORKING", Color::Green)] {
            let y = row_with(&buf, needle);
            let x = text_of(&buf)[y as usize].chars().position(|c| c == needle.chars().next().unwrap()).unwrap() as u16;
            assert_eq!(buf[(x, y)].fg, color, "{needle} header");
            assert!(buf[(x, y)].modifier.contains(Modifier::BOLD));
        }
        let y = row_with(&buf, "gamma");
        for symbol in ["│", "✻"] {
            let x = (0..buf.area.width).find(|x| buf[(*x, y)].symbol() == symbol).unwrap();
            assert_eq!(buf[(x, y)].fg, Color::Yellow, "{symbol}");
        }
    }

    #[test]
    fn path_replaces_home_with_tilde() {
        let buf = render(&mixed(), Some("/home/me"));
        assert!(text_of(&buf)[row_with(&buf, "beta") as usize].contains("~/code/beta"));
        assert_eq!(tilde_home("/home/mex/a", Some("/home/me")), "/home/mex/a");
        assert_eq!(tilde_home("/home/me/x", Some("/home/me/")), "~/x");
    }

    #[test]
    fn paths_align_across_different_window_name_lengths() {
        let st = state_with(vec![
            ("alpha", vec![pane("%1", AgentState::Done, "/pa")]),
            ("beta", vec![pane("%2", AgentState::Done, "/pb")]),
        ]);
        let buf = render(&st, None);
        let text = text_of(&buf);
        let col = |needle: &str| text[row_with(&buf, needle) as usize].find(needle).unwrap();
        assert_eq!(col("/pa"), col("/pb"));
    }

    #[test]
    fn long_path_is_truncated_to_fit() {
        let st = state_with(vec![("alpha", vec![pane("%1", AgentState::Done, &"/x".repeat(60))])]);
        let buf = render(&st, None);
        let line = &text_of(&buf)[row_with(&buf, "alpha") as usize];
        assert!(line.contains('…'));
        assert!(line.trim_end().chars().count() <= 80 - POPUP_MARGIN as usize);
    }

    #[test]
    fn empty_state_shows_two_dim_lines() {
        let st = state_with(vec![("alpha", vec![])]);
        let buf = render(&st, None);
        let y = row_with(&buf, "no agents reporting state");
        assert!(text_of(&buf)[y as usize + 1].contains("agents appear once their hooks set @agent_state (see README)"));
        let x = (0..buf.area.width).find(|x| buf[(*x, y)].symbol() == "n").unwrap();
        assert_eq!(buf[(x, y)].fg, DIM);
    }

    #[test]
    fn selected_row_is_highlighted_and_follows_the_cursor() {
        let mut st = mixed();
        let buf = render(&st, None);
        let y = row_with(&buf, "beta");
        assert_eq!(buf[(POPUP_MARGIN + 3, y)].bg, SEL_BG);
        assert_ne!(buf[(POPUP_MARGIN + 3, row_with(&buf, "gamma"))].bg, SEL_BG);
        st.agent_move(1);
        let buf = render(&st, None);
        assert_eq!(buf[(POPUP_MARGIN + 3, row_with(&buf, "gamma"))].bg, SEL_BG);
    }

    #[test]
    fn footer_lists_agent_view_keys() {
        let text = text_of(&render(&mixed(), None)).join("\n");
        assert!(text.contains("1-9 jump · ↵ switch · a tree · q quit"));
    }
}
