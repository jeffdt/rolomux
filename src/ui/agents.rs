//! Agent-view rendering: every agent pane grouped under a header per state.
//! Shared helpers and palette constants come from the parent `ui` module.

use super::*;
use crate::model::note::{condense, wrap_note};
use crate::model::AgentRow;

const MAX_WINDOW_WIDTH: usize = 24;
const MAX_SESSION_WIDTH: usize = 24;
const DETAIL_INDENT: usize = 7;
/// Wide enough that the path reads as its own column, apart from the dim session name.
const PATH_GAP: &str = "   ";


/// Header text for a state's section.
fn section_title(state: AgentState) -> &'static str {
    match state {
        AgentState::Waiting => "BLOCKED",
        AgentState::Done => "DONE",
        AgentState::Working => "WORKING",
    }
}

/// A section header styled like the tree's group headers: the bold title
/// flush left, then a rule in the state color running to the edge.
fn section_header(state: AgentState, width: usize) -> ListItem<'static> {
    let title = section_title(state);
    let color = agent_color(state);
    let rule_len = width.saturating_sub(title.chars().count() + 2);
    ListItem::new(Line::from(vec![
        Span::styled(title, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled("─".repeat(rule_len), Style::default().fg(color)),
    ]))
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

/// Time `row` has spent in its state, or empty when the agent never said.
fn state_age(row: &AgentRow) -> String {
    if row.since > 0 { activity_age(row.since) } else { String::new() }
}

/// Which optional leading columns every row reserves.
#[derive(Clone, Copy)]
struct LeadColumns {
    /// The state glyph; only the flat list needs it, sections already say the state.
    glyph: bool,
    /// The `●` slot marking the window the popup was opened from.
    here: bool,
}

/// The row's session and window names, each capped at its own max width.
fn label_parts(row: &AgentRow) -> (String, String) {
    (truncate_end(&row.session, MAX_SESSION_WIDTH), truncate_end(&row.window_name, MAX_WINDOW_WIDTH))
}

/// Width of the joined `session › window` label, so later columns can align past it.
fn label_len(row: &AgentRow) -> usize {
    let (name, window) = label_parts(row);
    name.chars().count() + 3 + window.chars().count()
}

#[allow(clippy::too_many_arguments)]
/// One agent row: jump number, state glyph (flat list only), the `●` for the
/// window you came from (whose session brightens and whose window name takes
/// the state color), the session dimmed, the agent's window name, path, and the time in state right-aligned at the end.
fn agent_item(row: &AgentRow, number: usize, selected: bool, lead: LeadColumns, label_width: usize, badge_width: usize, badge_glyph: &str, age_width: usize, width: usize, home: Option<&str>) -> ListItem<'static> {
    let color = agent_color(row.state);
    let label = if number <= 9 { jump_label(number) } else { "  ".to_string() };
    let (name, window) = label_parts(row);
    let window = format!("{window}{}", " ".repeat(label_width.saturating_sub(label_len(row))));
    let badge_cell = if badge_width == 0 {
        String::new()
    } else if row.background > 0 {
        format!(" {:<badge_width$}", format!("{badge_glyph}{}", row.background))
    } else {
        " ".repeat(badge_width + 1)
    };
    let lead_width = 2 * (lead.glyph as usize + lead.here as usize);
    let prefix_width = 1 + 2 + lead_width + label_width + badge_cell.chars().count() + PATH_GAP.len();
    let age_cell = if age_width == 0 { 0 } else { age_width + 2 };
    let path_room = width.saturating_sub(prefix_width + age_cell);
    let path = truncate_end(&tilde_home(&row.path, home), path_room);
    let (path, age) = if age_cell == 0 { (path, String::new()) } else { (format!("{path:<path_room$}"), format!("  {:>age_width$}", state_age(row))) };
    let dim = secondary(selected);
    let session_style = if row.dormant { dormant_session(selected) } else if row.here { Style::default() } else { dim };
    let window_style = if row.dormant { session_style } else if row.here { Style::default().fg(color).add_modifier(Modifier::BOLD) } else { Style::default() };
    let mut spans = vec![Span::styled("│", Style::default().fg(color)), Span::styled(label, dim)];
    if lead.glyph {
        spans.push(Span::styled(AGENT_GLYPH, Style::default().fg(color)));
        spans.push(Span::raw(" "));
    }
    if lead.here {
        spans.push(Span::styled(if row.here { "● " } else { "  " }, Style::default().fg(color)));
    }
    spans.extend([
        Span::styled(name, session_style),
        Span::styled(" › ", dim),
        Span::styled(window, window_style),
        Span::styled(badge_cell, Style::default().fg(color)),
        Span::raw(PATH_GAP),
        Span::styled(path, dim.add_modifier(Modifier::ITALIC)),
        Span::styled(age, dim),
    ]);
    ListItem::new(Line::from(spans))
}

/// The two-line detail block for the selected row: its condensed note under a
/// label in the state color. Blank lines when there is nothing to show.
fn detail_lines(row: Option<&AgentRow>, width: usize) -> Vec<Line<'static>> {
    let Some(row) = row else { return vec![Line::from(""), Line::from("")] };
    let wrapped = wrap_note(&condense(&row.note), width.saturating_sub(DETAIL_INDENT), 2);
    let label = match row.state {
        AgentState::Waiting => "asks",
        AgentState::Done => "said",
        AgentState::Working if row.background > 0 => "said",
        AgentState::Working => "you",
    };
    let mut lines = wrapped.into_iter();
    let first = match lines.next() {
        Some(text) => Line::from(vec![Span::styled(format!("{label:<5}  "), Style::default().fg(agent_color(row.state))), Span::raw(text)]),
        None => Line::from(""),
    };
    let second = lines.next().map_or_else(|| Line::from(""), |text| Line::from(format!("{}{text}", " ".repeat(DETAIL_INDENT))));
    vec![first, second]
}

/// The footer hint. Toggle keys name what pressing them does: `f` reads `show`
/// while focus mode hides agents (as in the tree), `s` the other sort order,
/// `g` the other layout, `w` whether working agents are hidden or shown, `r` reloads agent state.
fn agents_footer_hint(state: &PickerState) -> String {
    let focus = if state.focus_mode() { "show" } else { "focus" };
    let sort = if state.agents_oldest_first { "newest" } else { "oldest" };
    let layout = if state.agents_flat { "group" } else { "flat" };
    let working = if state.agents_hide_working { "show" } else { "hide" };
    format!("1-9 jump · f {focus} · s {sort} · g {layout} · w {working} · r reload · a tree · q quit")
}

/// Draws the agent view: rows under per-state headers (or one flat list), plus the footer.
pub(super) fn draw_agents(frame: &mut Frame, state: &PickerState, inner: Rect) {
    draw_agents_with_home(frame, state, inner, std::env::var("HOME").ok().as_deref());
}

fn draw_agents_with_home(frame: &mut Frame, state: &PickerState, inner: Rect, home: Option<&str>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2), Constraint::Length(2)])
        .split(inner);

    let rows = state.agent_rows();
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_line: Option<usize> = None;
    if rows.is_empty() {
        let empty: &[&str] = if state.agents_hide_working {
            &[" no blocked or done agents", " working agents are hidden"]
        } else {
            &[" no agents reporting state", " agents appear once their hooks set @rolomux_agent_state (see README)"]
        };
        for &text in empty {
            items.push(ListItem::new(Line::from(Span::styled(text, Style::default().fg(DIM)))));
        }
    } else {
        let label_width = rows.iter().map(label_len).max().unwrap_or(0);
        let badge_width = rows
            .iter()
            .filter(|r| r.background > 0)
            .map(|r| state.background_badge.chars().count() + r.background.to_string().len())
            .max()
            .unwrap_or(0);
        let age_width = rows.iter().map(|r| state_age(r).chars().count()).max().unwrap_or(0);
        let lead = LeadColumns { glyph: state.agents_flat, here: rows.iter().any(|r| r.here) };
        let mut current: Option<AgentState> = None;
        for (i, row) in rows.iter().enumerate() {
            if !state.agents_flat && current != Some(row.state) {
                if current.is_some() {
                    items.push(ListItem::new(Line::from("")));
                }
                current = Some(row.state);
                items.push(section_header(row.state, chunks[0].width as usize));
            }
            let selected = i == state.agent_cursor();
            if selected {
                selected_line = Some(items.len());
            }
            items.push(agent_item(row, i + 1, selected, lead, label_width, badge_width, &state.background_badge, age_width, chunks[0].width as usize, home));
        }
    }
    let list = List::new(items).highlight_style(Style::default().bg(SEL_BG).add_modifier(Modifier::BOLD));
    let mut list_state = ListState::default();
    list_state.select(selected_line);
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    frame.render_widget(Paragraph::new(detail_lines(rows.get(state.agent_cursor()), chunks[1].width as usize)), chunks[1]);

    let footer = Paragraph::new(vec![
        footer_rule_line(chunks[2].width, state),
        styled_hint(&agents_footer_hint(state), color_from_name(&state.shortcut_color)),
    ]);
    frame.render_widget(footer, chunks[2]);
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
        AgentPane { pane_id: id.into(), state, reported_state: state, path: path.into(), background: 0, since: 0, note: String::new(), title: String::new() }
    }

    fn state_with(agents: Vec<(&str, Vec<AgentPane>)>) -> PickerState {
        state_with_active(agents, false)
    }

    /// Like `state_with`; `active` sets whether each session's one window is its active window.
    fn state_with_active(agents: Vec<(&str, Vec<AgentPane>)>, active: bool) -> PickerState {
        let sessions = agents
            .iter()
            .enumerate()
            .map(|(i, (name, _))| Session {
                id: format!("${i}"),
                name: (*name).into(),
                activity: 30,
                created: 1,
                attached: false,
                windows: vec![Window { id: format!("@{i}"), index: 0, name: format!("win-{name}"), active }],
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

    /// `mixed`, opened from gamma's window.
    fn mixed_from_gamma() -> PickerState {
        let mut st = state_with_active(vec![
            ("alpha", vec![pane("%1", AgentState::Working, "/w/alpha")]),
            ("beta", vec![pane("%2", AgentState::Waiting, "/home/me/code/beta")]),
            ("gamma", vec![pane("%3", AgentState::Done, "/g")]),
        ], true);
        st.current_session = Some("gamma".into());
        st
    }

    #[test]
    fn headers_appear_in_urgency_order_and_empty_sections_are_hidden() {
        let buf = render(&mixed(), None);
        let (n, d, w) = (row_with(&buf, "BLOCKED"), row_with(&buf, "DONE"), row_with(&buf, "WORKING"));
        assert!(n < d && d < w);
        let st = state_with(vec![("alpha", vec![pane("%1", AgentState::Done, "/a")])]);
        let text = text_of(&render(&st, None)).join("\n");
        assert!(text.contains("DONE") && !text.contains("BLOCKED") && !text.contains("WORKING"));
    }

    #[test]
    fn rows_are_numbered_in_display_order_and_overflow_is_blank() {
        let buf = render(&mixed(), None);
        assert!(text_of(&buf)[row_with(&buf, "beta") as usize].contains("│1 beta"));
        assert!(text_of(&buf)[row_with(&buf, "gamma") as usize].contains("│2 gamma"));
        assert!(text_of(&buf)[row_with(&buf, "alpha") as usize].contains("│3 alpha"));
        let many = state_with((0..11).map(|i| (Box::leak(format!("s{i:02}").into_boxed_str()) as &str, vec![pane("%1", AgentState::Working, "/p")])).collect());
        let buf = render_sized(&many, 40);
        assert!(text_of(&buf)[row_with(&buf, "s08") as usize].contains("│9 s08"));
        assert!(text_of(&buf)[row_with(&buf, "s09") as usize].contains("│  s09"));
    }

    fn render_sized(st: &PickerState, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(80, height)).unwrap();
        terminal.draw(|f| draw_agents_with_home(f, st, f.area(), None)).unwrap();
        terminal.backend().buffer().clone()
    }

    #[test]
    fn header_gutter_and_glyph_use_the_state_color() {
        let buf = render(&mixed(), None);
        for (needle, color) in [("BLOCKED", Color::Red), ("DONE", Color::Yellow), ("WORKING", Color::Green)] {
            let y = row_with(&buf, needle);
            let x = text_of(&buf)[y as usize].chars().position(|c| c == needle.chars().next().unwrap()).unwrap() as u16;
            assert_eq!(buf[(x, y)].fg, color, "{needle} header");
            assert!(buf[(x, y)].modifier.contains(Modifier::BOLD));
        }
        let y = row_with(&buf, "gamma");
        let x = (0..buf.area.width).find(|x| buf[(*x, y)].symbol() == "│").unwrap();
        assert_eq!(buf[(x, y)].fg, Color::Yellow);
    }

    #[test]
    fn state_glyph_shows_only_in_the_flat_list() {
        let mut st = mixed();
        assert!(!text_of(&render(&st, None)).join("\n").contains(AGENT_GLYPH));
        st.toggle_agent_layout();
        let buf = render(&st, None);
        let y = row_with(&buf, "gamma");
        assert!(text_of(&buf)[y as usize].contains(&format!("{AGENT_GLYPH} gamma")));
        let x = (0..buf.area.width).find(|x| buf[(*x, y)].symbol() == AGENT_GLYPH).unwrap();
        assert_eq!(buf[(x, y)].fg, Color::Yellow);
    }

    #[test]
    fn window_you_came_from_gets_a_dot_a_bright_session_and_its_window_name_in_its_state_color() {
        let mut st = mixed_from_gamma();
        let buf = render(&st, None);
        let y = row_with(&buf, "gamma");
        let line = &text_of(&buf)[y as usize];
        assert!(line.contains("● gamma › win-gamma"));
        let dot = (0..buf.area.width).find(|x| buf[(*x, y)].symbol() == "●").unwrap();
        assert_eq!(buf[(dot, y)].fg, Color::Yellow, "the row's state color, not the attached color");
        let at = |needle: &str| line[..line.find(needle).unwrap()].chars().count() as u16;
        assert!(buf[(at("win-gamma"), y)].modifier.contains(Modifier::BOLD));
        assert_eq!(buf[(at("win-gamma"), y)].fg, Color::Yellow);
        assert_eq!(buf[(at("gamma"), y)].fg, Color::Reset, "the session is undimmed, not colored");
        assert!(!buf[(at("gamma"), y)].modifier.contains(Modifier::BOLD));
        assert!(text_of(&buf)[row_with(&buf, "beta") as usize].contains("│1   beta"), "other rows reserve the dot slot");
        st.toggle_agent_layout();
        let buf = render(&st, None);
        assert!(text_of(&buf)[row_with(&buf, "gamma") as usize].contains(&format!("{AGENT_GLYPH} ● gamma")));
    }

    #[test]
    fn no_dot_slot_when_the_popup_was_opened_elsewhere() {
        let mut st = mixed_from_gamma();
        st.current_session = Some("elsewhere".into());
        let buf = render(&st, None);
        assert!(text_of(&buf)[row_with(&buf, "beta") as usize].contains("│1 beta"));
    }

    #[test]
    fn header_is_flush_left_with_a_rule_in_the_state_color() {
        let buf = render(&mixed(), None);
        let y = row_with(&buf, "DONE");
        let header_x = (0..buf.area.width).find(|x| buf[(*x, y)].symbol() == "D").unwrap();
        let gutter_x = (0..buf.area.width).find(|x| buf[(*x, y + 1)].symbol() == "│" && buf[(*x, y + 1)].fg == Color::Yellow).unwrap();
        assert_eq!(header_x, gutter_x, "header starts in the gutter column");
        assert!(text_of(&buf)[y as usize].contains("DONE ───"));
        assert_eq!(buf[(header_x + 5, y)].fg, Color::Yellow);
    }

    #[test]
    fn session_and_window_join_without_a_gap_while_paths_stay_aligned() {
        let st = state_with(vec![("a", vec![pane("%1", AgentState::Done, "/pa")]), ("longer", vec![pane("%2", AgentState::Done, "/pb")])]);
        let buf = render(&st, None);
        let lines = text_of(&buf);
        let short = &lines[row_with(&buf, "win-a") as usize];
        let long = &lines[row_with(&buf, "win-longer") as usize];
        assert!(short.contains("a › win-a "));
        assert_eq!(short.find("/pa"), long.find("/pb"));
    }

    #[test]
    fn path_is_italic_but_the_age_is_not() {
        let _clock = crate::ui::tests::freeze_clock(10_000);
        let st = state_with(vec![("alpha", vec![AgentPane { since: 10_000 - 720, ..pane("%1", AgentState::Done, "/pa") }])]);
        let buf = render(&st, None);
        let y = row_with(&buf, "alpha");
        let line = &text_of(&buf)[y as usize];
        let at = |needle: &str| line[..line.find(needle).unwrap()].chars().count() as u16;
        assert!(buf[(at("/pa"), y)].modifier.contains(Modifier::ITALIC));
        assert!(!buf[(at("12m"), y)].modifier.contains(Modifier::ITALIC));
        assert!(!buf[(at("alpha"), y)].modifier.contains(Modifier::ITALIC));
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
        assert!(text_of(&buf)[y as usize + 1].contains("agents appear once their hooks set @rolomux_agent_state (see README)"));
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
    fn dormant_agents_dim_and_focus_mode_hides_them_with_footer_status() {
        let mut st = state_with(vec![("alpha", vec![pane("%1", AgentState::Done, "/a")]), ("beta", vec![pane("%2", AgentState::Done, "/b")])]);
        st.focus_session("beta");
        st.toggle_dormant();
        let buf = render(&st, None);
        let y = row_with(&buf, "beta");
        let x = (0..buf.area.width).find(|x| buf[(*x, y)].symbol() == "b").unwrap();
        assert_eq!(buf[(x, y)].fg, DIM);
        st.toggle_focus_mode();
        let text = text_of(&render(&st, None)).join("\n");
        assert!(!text.contains("beta") && text.contains("1 session hidden") && text.contains("f show"));
        assert!(!text.contains("blocked"), "a hidden done agent is not called out");
    }

    #[test]
    fn footer_calls_out_blocked_agents_hidden_by_focus_mode_in_red() {
        let mut st = state_with(vec![("alpha", vec![pane("%1", AgentState::Done, "/a")]), ("beta", vec![pane("%2", AgentState::Waiting, "/b")])]);
        st.focus_session("beta");
        st.toggle_dormant();
        st.toggle_focus_mode();
        let buf = render(&st, None);
        let y = row_with(&buf, "1 blocked");
        assert!(text_of(&buf)[y as usize].contains("1 session hidden · 1 blocked ─"));
        let x = (0..buf.area.width).find(|x| buf[(*x, y)].symbol() == "b" && buf[(*x + 1, y)].symbol() == "l").unwrap();
        assert_eq!(buf[(x, y)].fg, Color::Red);
        st.toggle_peek();
        assert!(!text_of(&render(&st, None)).join("\n").contains("blocked"), "peeking shows the agent, so no callout");
    }

    #[test]
    fn blocked_callout_stands_alone_when_nothing_else_counts_as_hidden() {
        let mut st = state_with(vec![("alpha", vec![pane("%1", AgentState::Waiting, "/a")])]);
        st.expand_session("alpha");
        st.focus_session("alpha");
        st.move_cursor(1);
        st.toggle_dormant();
        st.collapse_session("alpha");
        st.toggle_focus_mode();
        let text = text_of(&render(&st, None)).join("\n");
        assert!(text.contains("─ 1 blocked ─") && !text.contains("0 sessions"));
    }

    #[test]
    fn footer_lists_agent_view_keys() {
        let text = text_of(&render(&mixed(), None)).join("\n");
        assert!(text.contains("1-9 jump · f focus · s oldest · g flat · w hide · r reload · a tree · q quit"), "{text}");
    }

    #[test]
    fn footer_names_the_other_sort_layout_and_working_visibility() {
        let mut st = mixed();
        st.toggle_agent_sort();
        st.toggle_agent_layout();
        st.toggle_agent_hide_working();
        let text = text_of(&render(&st, None)).join("\n");
        assert!(text.contains("s newest · g group · w show"));
    }

    #[test]
    fn empty_state_says_working_agents_are_hidden() {
        let mut st = state_with(vec![("alpha", vec![pane("%1", AgentState::Working, "/a")])]);
        st.toggle_agent_hide_working();
        let text = text_of(&render(&st, None)).join("\n");
        assert!(text.contains("no blocked or done agents") && text.contains("working agents are hidden"));
    }

    #[test]
    fn flat_layout_drops_section_headers() {
        let mut st = mixed();
        st.toggle_agent_layout();
        let text = text_of(&render(&st, None)).join("\n");
        assert!(!text.contains("BLOCKED") && !text.contains("WORKING") && text.contains("1 "));
    }

    fn noted(id: &str, state: AgentState, path: &str, background: u32, note: &str) -> AgentPane {
        AgentPane { background, note: note.into(), ..pane(id, state, path) }
    }

    fn detail_lines(buf: &Buffer) -> (String, String) {
        let t = text_of(buf);
        let hint = row_with(buf, "1-9 jump") as usize;
        (t[hint - 3].trim().to_string(), t[hint - 2].trim().to_string())
    }

    #[test]
    fn detail_shows_selected_note_with_state_label_and_color() {
        let mut st = state_with(vec![
            ("alpha", vec![noted("%1", AgentState::Waiting, "/a", 0, "Should I proceed? Other text.")]),
            ("beta", vec![noted("%2", AgentState::Done, "/b", 0, "All finished. More.")]),
        ]);
        let buf = render(&st, None);
        assert_eq!(detail_lines(&buf).0, "asks   Should I proceed?");
        let y = row_with(&buf, "asks");
        let x = (0..buf.area.width).find(|x| buf[(*x, y)].symbol() == "a").unwrap();
        assert_eq!(buf[(x, y)].fg, Color::Red);
        st.agent_move(1);
        assert_eq!(detail_lines(&render(&st, None)).0, "said   All finished.");
    }

    #[test]
    fn working_label_is_you_or_said_with_background() {
        let st = state_with(vec![("alpha", vec![noted("%1", AgentState::Working, "/a", 0, "Do it.")])]);
        assert_eq!(detail_lines(&render(&st, None)).0, "you    Do it.");
        let st = state_with(vec![("alpha", vec![noted("%1", AgentState::Working, "/a", 2, "Do it.")])]);
        assert_eq!(detail_lines(&render(&st, None)).0, "said   Do it.");
    }

    #[test]
    fn empty_note_keeps_detail_block_blank_and_footer_in_place() {
        let blank = render(&mixed(), None);
        let noted_st = state_with(vec![("alpha", vec![noted("%1", AgentState::Done, "/a", 0, "Hi.")])]);
        let with_note = render(&noted_st, None);
        assert_eq!(detail_lines(&blank), (String::new(), String::new()));
        assert_eq!(row_with(&blank, "1-9 jump"), row_with(&with_note, "1-9 jump"));
    }

    #[test]
    fn long_note_wraps_to_two_lines_ending_in_ellipsis() {
        let note = "word ".repeat(40);
        let st = state_with(vec![("alpha", vec![noted("%1", AgentState::Done, "/a", 0, &note)])]);
        let (a, b) = detail_lines(&render(&st, None));
        assert!(a.starts_with("said"));
        assert!(b.ends_with('…'));
    }

    #[test]
    fn background_badge_shows_in_row_color_and_paths_stay_aligned() {
        let st = state_with(vec![
            ("alpha", vec![noted("%1", AgentState::Working, "/pa", 2, "")]),
            ("beta", vec![noted("%2", AgentState::Done, "/pb", 0, "")]),
        ]);
        let buf = render(&st, None);
        let text = text_of(&buf);
        let ya = row_with(&buf, "alpha") as usize;
        assert!(text[ya].contains("↻2"));
        let x = (0..buf.area.width).find(|x| buf[(*x, ya as u16)].symbol() == "↻").unwrap();
        assert_eq!(buf[(x, ya as u16)].fg, Color::Green);
        assert_eq!(text[ya].find("/pa").map(|i| text[ya][..i].chars().count()), text[row_with(&buf, "beta") as usize].find("/pb").map(|i| text[row_with(&buf, "beta") as usize][..i].chars().count()));
    }

    #[test]
    fn time_in_state_right_aligns_and_is_blank_when_unknown() {
        let _clock = crate::ui::tests::freeze_clock(10_000);
        let st = state_with(vec![
            ("alpha", vec![AgentPane { since: 10_000 - 720, ..pane("%1", AgentState::Waiting, "/pa") }]),
            ("beta", vec![AgentPane { since: 10_000 - 5, ..pane("%2", AgentState::Waiting, "/pb") }]),
            ("gamma", vec![pane("%3", AgentState::Waiting, "/pc")]),
        ]);
        let buf = render(&st, None);
        let text = text_of(&buf);
        let line = |name| text[row_with(&buf, name) as usize].trim_end_matches(['│', ' ']).to_string();
        assert!(line("alpha").ends_with(" 12m"));
        assert!(line("beta").ends_with("  5s"));
        assert!(line("gamma").ends_with("/pc"));
        assert_eq!(line("alpha").chars().count(), line("beta").chars().count(), "ages end in one column");
    }

    #[test]
    fn long_session_name_is_capped_so_paths_keep_room() {
        let long = "s".repeat(60);
        let st = state_with(vec![(long.as_str(), vec![pane("%1", AgentState::Done, "/the/path")])]);
        let text = text_of(&render(&st, None)).join("\n");
        assert!(text.contains(&format!("{}… ›", "s".repeat(MAX_SESSION_WIDTH - 1))));
        assert!(text.contains("/the/path"));
    }

    #[test]
    fn background_badge_uses_the_configured_glyph() {
        let mut st = state_with(vec![("alpha", vec![noted("%1", AgentState::Working, "/pa", 2, "")])]);
        st.background_badge = "◔".to_string();
        let buf = render(&st, None);
        let line = &text_of(&buf)[row_with(&buf, "alpha") as usize];
        assert!(line.contains("◔2") && !line.contains('↻'));
    }

    #[test]
    fn no_badge_column_when_no_row_has_background_work() {
        let buf = render(&mixed(), None);
        assert!(text_of(&buf)[row_with(&buf, "alpha") as usize].contains("alpha › win-alpha   /w/alpha"));
    }
}
