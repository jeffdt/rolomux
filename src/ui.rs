use crate::model::{Group, Mode, PickerState, Row, Session, SortKey, Window, HEADER_COLORS};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::time::{SystemTime, UNIX_EPOCH};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const DOT: Color = Color::Green;
const SEL_BG: Color = Color::DarkGray;
/// Default column where a session's metadata begins, used when every visible
/// name is short. It is also the floor for the shared metadata column.
const META_COL: usize = 30;
/// Fixed cells preceding a session name: jump number (2) + expand glyph and
/// its trailing space (2).
const SESSION_PREFIX: usize = 4;
/// Minimum gap kept between the longest visible name and its metadata when the
/// shared column is anchored to that name rather than to META_COL.
const META_GAP: usize = 2;
/// Cells reserved at the right so the shared column never pushes metadata off
/// the card; roughly the widest plausible "12 windows · 20s".
const META_BUDGET: usize = 18;
/// Uniform buffer between the picker's border and the popup edge. The popup is
/// launched borderless (`tmux display-popup -B`), so this blank ring is the
/// only separation between smux's frame and the surrounding tmux panes; it
/// keeps the picker from sitting flush against busy content behind the popup.
const POPUP_MARGIN: u16 = 2;

const FOOTER_HINT: &str =
    "/ search · 1-9 jump · ⇧JK move · g groups · s sort · z all · q quit";

const SEARCH_FOOTER_HINT: &str = "↑↓ move · ⌃W word · ⌃U clear · Esc back";

/// Human label for the active sort mode, shown in the picker's title bar.
fn mode_label(sort: SortKey) -> &'static str {
    match sort {
        SortKey::Activity => "recency",
        SortKey::Created => "age",
        SortKey::Manual => "manual",
    }
}

/// Style for secondary text (expand glyph, metadata, tree connectors). On the
/// selected row it drops to the default foreground so it matches the session
/// name and stays visible against the DarkGray selection bar; otherwise it is
/// dimmed.
fn secondary(selected: bool) -> Style {
    if selected {
        Style::default()
    } else {
        Style::default().fg(DIM)
    }
}

/// Format a duration in seconds as a compact human-readable string.
pub fn fmt_age(secs: i64) -> String {
    if secs < 0 {
        return "0s".to_string();
    }
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

fn activity_age(activity: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    fmt_age(now.saturating_sub(activity).max(0))
}

/// Shrink `area` by `margin` cells on every side. The margin is reduced toward
/// zero when the area is too small to inset without collapsing, so a tiny popup
/// still renders a non-empty frame rather than panicking (consistent with the
/// project's graceful-on-degenerate-input stance).
fn inset(area: Rect, margin: u16) -> Rect {
    let mx = margin.min(area.width.saturating_sub(1) / 2);
    let my = margin.min(area.height.saturating_sub(1) / 2);
    Rect {
        x: area.x + mx,
        y: area.y + my,
        width: area.width.saturating_sub(2 * mx),
        height: area.height.saturating_sub(2 * my),
    }
}

pub fn draw(frame: &mut Frame, state: &PickerState) {
    let area = inset(frame.area(), POPUP_MARGIN);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " smux  session picker ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .title(
            Line::from(Span::styled(
                format!(" sort: {} ", mode_label(state.sort)),
                Style::default().fg(DIM),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match state.mode {
        Mode::Command => draw_command(frame, state, inner),
        Mode::Search => draw_search(frame, state, inner),
        Mode::Groups => draw_groups(frame, state, inner),
    }
}

fn draw_command(frame: &mut Frame, state: &PickerState, inner: Rect) {
    // Split inner area: list region on top, 2-row footer at bottom.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(inner);
    let list_area = chunks[0];
    let footer_area = chunks[1];

    let ordered = state.ordered();
    let rows = state.visible_rows();
    let cursor_row = rows.get(state.cursor).copied();

    // Anchor metadata to one shared geometry across every session row, computed
    // from the visible sessions (window rows carry no metadata).
    let session_refs = rows.iter().filter_map(|r| match r {
        Row::Session(si) => Some(ordered[*si]),
        Row::Window(..) => None,
    });
    let meta = MetaLayout::compute(session_refs, list_area.width);

    let group_ids = state.ordered_group_ids();
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_line: Option<usize> = None;
    let mut last_section: Option<Option<usize>> = None;
    // Named groups whose header is already emitted are those with index < this.
    // Empty groups produce no session rows, so we "catch up" and emit their bare
    // (dimmed) headers when we pass their position or at the end of the list.
    let mut next_group: usize = 0;

    for row in rows.iter() {
        match row {
            Row::Session(si) => {
                let sess = ordered[*si];
                let section = group_ids[*si];
                if last_section != Some(section) {
                    let target = match section {
                        Some(gi) => gi,
                        None => state.groups.len(),
                    };
                    while next_group < target {
                        push_empty_group_header(&mut items, &state.groups[next_group].name, list_area.width);
                        next_group += 1;
                    }
                    match section {
                        Some(gi) => {
                            let color = group_color(&state.groups[gi], gi);
                            push_section_header(&mut items, &state.groups[gi].name.to_uppercase(), list_area.width, color);
                            next_group = gi + 1;
                        }
                        None => {
                            push_section_header(&mut items, "SESSIONS", list_area.width, ACCENT);
                        }
                    }
                    last_section = Some(section);
                }
                let selected = Some(*row) == cursor_row;
                if selected {
                    selected_line = Some(items.len());
                }
                // Stable jump number: 1-based position in the session order,
                // for the first 9 sessions. Unaffected by what is expanded.
                let number = if *si < 9 { Some(*si + 1) } else { None };
                items.push(session_item(
                    sess,
                    state.is_expanded(&sess.name),
                    selected,
                    number,
                    meta,
                ));
            }
            Row::Window(si, wi) => {
                let sess = ordered[*si];
                let selected = Some(*row) == cursor_row;
                if selected {
                    selected_line = Some(items.len());
                }
                let last = *wi + 1 == sess.windows.len();
                items.push(window_item(&sess.windows[*wi], last, selected));
            }
        }
    }
    // Trailing empty groups (after the last session row, with no residual below).
    while next_group < state.groups.len() {
        push_empty_group_header(&mut items, &state.groups[next_group].name, list_area.width);
        next_group += 1;
    }

    let list = List::new(items).highlight_style(
        Style::default().bg(SEL_BG).add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    list_state.select(selected_line);
    frame.render_stateful_widget(list, list_area, &mut list_state);

    // Render the divider and hint row inside the footer area.
    let rule = "─".repeat(footer_area.width as usize);
    let footer = Paragraph::new(vec![
        Line::from(Span::styled(rule, Style::default().fg(DIM))),
        Line::from(Span::styled(FOOTER_HINT, Style::default().fg(DIM))),
    ]);
    frame.render_widget(footer, footer_area);
}

fn draw_search(frame: &mut Frame, state: &PickerState, inner: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // query prompt
            Constraint::Min(0),    // results
            Constraint::Length(2), // footer
        ])
        .split(inner);

    let prompt = Line::from(vec![
        Span::styled("search: ", Style::default().fg(DIM)),
        Span::raw(state.query.clone()),
        Span::styled("▏", Style::default().fg(ACCENT)),
    ]);
    frame.render_widget(Paragraph::new(prompt), chunks[0]);

    let results = state.search_results();
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_line: Option<usize> = None;
    if results.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "  no matches",
            Style::default().fg(DIM),
        ))));
    } else {
        let meta = MetaLayout::compute(results.iter().copied(), chunks[1].width);
        for (i, sess) in results.iter().enumerate() {
            let selected = i == state.search_cursor();
            if selected {
                selected_line = Some(items.len());
            }
            // Flat, collapsed, no jump number (None), normal metadata.
            items.push(session_item(sess, false, selected, None, meta));
        }
    }
    let list = List::new(items)
        .highlight_style(Style::default().bg(SEL_BG).add_modifier(Modifier::BOLD));
    let mut list_state = ListState::default();
    list_state.select(selected_line);
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    let rule = "─".repeat(chunks[2].width as usize);
    let footer = Paragraph::new(vec![
        Line::from(Span::styled(rule, Style::default().fg(DIM))),
        Line::from(Span::styled(SEARCH_FOOTER_HINT, Style::default().fg(DIM))),
    ]);
    frame.render_widget(footer, chunks[2]);
}

const GROUP_FOOTER_HINT: &str = "Enter rename · n new · c color · d delete · ⇧JK reorder · Esc back";

fn draw_groups(frame: &mut Frame, state: &PickerState, inner: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(inner);
    let list_area = chunks[0];
    let footer_area = chunks[1];

    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_line: Option<usize> = None;
    for (gi, g) in state.groups.iter().enumerate() {
        let selected = gi == state.group_cursor();
        if selected { selected_line = Some(items.len()); }
        let editing = selected && state.group_editing();
        let line = if editing {
            let buf = state.group_edit_buffer().unwrap_or("");
            Line::from(vec![
                Span::styled(buf.to_uppercase(), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled("▏", Style::default().fg(ACCENT)),
            ])
        } else {
            // Empty groups read as dimmed shelves, matching session mode;
            // populated groups show their (flippable) header color.
            let name_color = if g.members.is_empty() { DIM } else { group_color(g, gi) };
            Line::from(vec![
                Span::styled(g.name.to_uppercase(),
                    Style::default().fg(name_color).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  · {}", g.members.len()), secondary(selected)),
            ])
        };
        items.push(ListItem::new(line));
    }
    // Dimmed, non-editable residual anchor for context.
    items.push(ListItem::new(Line::from(Span::styled(
        format!("SESSIONS  · {}", state.residual_count()),
        Style::default().fg(DIM),
    ))));

    let list = List::new(items)
        .highlight_style(Style::default().bg(SEL_BG).add_modifier(Modifier::BOLD));
    let mut list_state = ListState::default();
    list_state.select(selected_line);
    frame.render_stateful_widget(list, list_area, &mut list_state);

    let rule = "─".repeat(footer_area.width as usize);
    let footer = Paragraph::new(vec![
        Line::from(Span::styled(rule, Style::default().fg(DIM))),
        Line::from(Span::styled(GROUP_FOOTER_HINT, Style::default().fg(DIM))),
    ]);
    frame.render_widget(footer, footer_area);
}

/// Map a `HEADER_COLORS` name to a named ANSI color (never RGB, so headers
/// follow the terminal theme). `magenta` is the Nord purple. Unknown names fall
/// back to the accent so a hand-edited config can never crash the picker.
fn color_from_name(name: &str) -> Color {
    match name {
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "magenta" => Color::Magenta,
        "blue" => Color::Blue,
        "red" => Color::Red,
        _ => Color::Cyan,
    }
}

/// The header color for the group at `index`: its explicit color, or the
/// positional default `HEADER_COLORS[index]` when unset.
fn group_color(group: &Group, index: usize) -> Color {
    let name = if group.color.is_empty() {
        HEADER_COLORS[index % HEADER_COLORS.len()]
    } else {
        group.color.as_str()
    };
    color_from_name(name)
}

/// Push a section header, preceding it with a blank spacer unless it is the very
/// first item in the list.
fn push_section_header(items: &mut Vec<ListItem<'static>>, label: &str, width: u16, color: Color) {
    if !items.is_empty() {
        items.push(ListItem::new(Line::from("")));
    }
    items.push(header_item(label, width, color));
}

/// Push a bare, dimmed header for an empty named group (a labeled shelf to fill).
fn push_empty_group_header(items: &mut Vec<ListItem<'static>>, name: &str, width: u16) {
    push_section_header(items, &name.to_uppercase(), width, DIM);
}

fn header_item(label: &str, width: u16, color: Color) -> ListItem<'static> {
    let rule_len = (width as usize).saturating_sub(label.chars().count() + 2);
    ListItem::new(Line::from(vec![
        Span::styled(
            label.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("─".repeat(rule_len), Style::default().fg(DIM)),
    ]))
}

/// The "{n} window(s)" count token for a session, singular for exactly one.
fn window_count(wins: usize) -> String {
    let label = if wins == 1 { "window" } else { "windows" };
    format!("{wins} {label}")
}

/// Shared geometry for the metadata block, computed once per render so every
/// row aligns to it (issue #3). `col` is the column where metadata begins;
/// `count_width` is the width reserved for the window-count token so the middot
/// and age line up even as "1 window" / "9 windows" / "12 windows" differ.
#[derive(Debug, Clone, Copy)]
struct MetaLayout {
    col: usize,
    count_width: usize,
}

impl MetaLayout {
    /// Derive the layout from the visible sessions and the available width. The
    /// column sits at META_COL by default, advances to META_GAP past the
    /// longest visible name when that name would otherwise overrun, and is
    /// capped so metadata never falls off the card.
    fn compute<'a>(sessions: impl Iterator<Item = &'a Session>, width: u16) -> Self {
        let mut longest_prefix = 0usize;
        let mut count_width = 0usize;
        for s in sessions {
            longest_prefix = longest_prefix.max(SESSION_PREFIX + s.name.chars().count());
            count_width = count_width.max(window_count(s.windows.len()).chars().count());
        }
        let target = META_COL.max(longest_prefix + META_GAP);
        let cap = (width as usize).saturating_sub(META_BUDGET).max(META_COL);
        MetaLayout { col: target.min(cap), count_width }
    }
}

fn session_item(
    sess: &Session,
    expanded: bool,
    selected: bool,
    number: Option<usize>,
    meta: MetaLayout,
) -> ListItem<'static> {
    let glyph = if expanded { "▾" } else { "▸" };
    let num = match number { Some(n) => format!("{n} "), None => "  ".to_string() };
    let name_style = if sess.attached {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let prefix_len = SESSION_PREFIX + sess.name.chars().count(); // num + "glyph " + name
    let pad = meta.col.saturating_sub(prefix_len);
    let count = window_count(sess.windows.len());
    let count_pad = meta.count_width.saturating_sub(count.chars().count());
    let age = activity_age(sess.activity);
    ListItem::new(Line::from(vec![
        Span::styled(num, secondary(selected)),
        Span::styled(format!("{glyph} "), secondary(selected)),
        Span::styled(sess.name.clone(), name_style),
        Span::styled(
            format!("{}{count}{} · {age}", " ".repeat(pad), " ".repeat(count_pad)),
            secondary(selected),
        ),
    ]))
}

fn window_item(win: &Window, last: bool, selected: bool) -> ListItem<'static> {
    // Three leading spaces align under the session's number gutter. No window
    // number is shown: numbers are reserved for things you can jump to, and
    // windows aren't jumpable yet.
    let connector = if last { "   └─ " } else { "   ├─ " };
    let dot = if win.active { "●" } else { " " };
    ListItem::new(Line::from(vec![
        Span::styled(connector.to_string(), secondary(selected)),
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
    ToggleAll,
    Select,
    Switch(usize),
    Focus(usize),
    EnterGroups,
    MoveUp,
    MoveDown,
    CycleSort,
    EnterSearch,
    Quit,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchInput {
    Char(char),
    Backspace,
    DeleteWord,
    Clear,
    Up,
    Down,
    Select,
    Exit,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupInput { Up, Down, MoveUp, MoveDown, New, Rename, CycleColor, Delete, Exit, None }

/// Key mapping for group-management mode while NOT editing a name. During an
/// inline rename the loop routes keys through `map_search_key` instead.
pub fn map_group_key(key: KeyEvent) -> GroupInput {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => GroupInput::Down,
        KeyCode::Char('k') | KeyCode::Up => GroupInput::Up,
        KeyCode::Char('J') if shift => GroupInput::MoveDown,
        KeyCode::Char('K') if shift => GroupInput::MoveUp,
        KeyCode::Char('n') => GroupInput::New,
        KeyCode::Enter | KeyCode::Char('r') => GroupInput::Rename,
        KeyCode::Char('c') => GroupInput::CycleColor,
        KeyCode::Char('d') => GroupInput::Delete,
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('g') => GroupInput::Exit,
        _ => GroupInput::None,
    }
}

/// Key mapping while in search mode. Printable characters (including digits)
/// build the query; movement uses arrows plus the fzf/vim Ctrl pairs.
///
/// Note: under the legacy (non-kitty) encoding some terminals deliver Ctrl-j as
/// Enter, in which case it selects rather than moving down. Arrows, Ctrl-n,
/// Ctrl-p, and Ctrl-k are the reliable movement keys; Ctrl-j is mapped for
/// terminals that can distinguish it.
pub fn map_search_key(key: KeyEvent) -> SearchInput {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Esc => SearchInput::Exit,
        KeyCode::Enter => SearchInput::Select,
        KeyCode::Backspace if alt => SearchInput::DeleteWord,
        KeyCode::Backspace => SearchInput::Backspace,
        KeyCode::Up => SearchInput::Up,
        KeyCode::Down => SearchInput::Down,
        KeyCode::Char('w') if ctrl => SearchInput::DeleteWord,
        KeyCode::Char('u') if ctrl => SearchInput::Clear,
        KeyCode::Char('p') | KeyCode::Char('k') if ctrl => SearchInput::Up,
        KeyCode::Char('n') | KeyCode::Char('j') if ctrl => SearchInput::Down,
        KeyCode::Char(_) if ctrl => SearchInput::None,
        KeyCode::Char(c) => SearchInput::Char(c),
        _ => SearchInput::None,
    }
}

pub fn map_key(key: KeyEvent) -> Input {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Input::Down,
        KeyCode::Char('k') | KeyCode::Up => Input::Up,
        KeyCode::Char('l') | KeyCode::Right => Input::Expand,
        KeyCode::Char('h') | KeyCode::Left => Input::Collapse,
        KeyCode::Char('z') => Input::ToggleAll,
        KeyCode::Enter => Input::Select,
        KeyCode::Char('g') => Input::EnterGroups,
        KeyCode::Char('K') if shift => Input::MoveUp,
        KeyCode::Char('J') if shift => Input::MoveDown,
        KeyCode::Char('s') => Input::CycleSort,
        KeyCode::Char('/') => Input::EnterSearch,
        KeyCode::Char(c @ '1'..='9') if key.modifiers.contains(KeyModifiers::ALT) => {
            Input::Focus(c as usize - '0' as usize)
        }
        KeyCode::Char(c @ '1'..='9') => Input::Switch(c as usize - '0' as usize),
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
    fn alt(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT)
    }
    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    use crate::model::{Group, PickerState, Session, SortKey, Window};
    use crate::store::Config;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_to_string(state: &PickerState) -> String {
        let backend = TestBackend::new(80, 20);
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
        let cfg = Config {
            groups: vec![Group { name: "PINNED".into(), members: vec!["pr-review".into()], color: String::new() }],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
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
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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
                // The glyph cells of the selected row carry the bar background,
                // now offset right by the popup margin + border.
                for x in (POPUP_MARGIN + 1)..(POPUP_MARGIN + 5) {
                    if buf[(x, y)].style().bg == Some(ratatui::style::Color::DarkGray) {
                        found = true;
                    }
                }
            }
        }
        assert!(found, "cursor row should have a DarkGray background bar");
    }

    #[test]
    fn selected_row_has_no_invisible_dark_on_dark_cells() {
        // The expand glyph / metadata are dim (DarkGray) on unselected rows, but
        // the selection bar is also DarkGray. On the selected row, secondary text
        // must brighten so nothing renders DarkGray-on-DarkGray (invisible).
        let sessions = vec![
            Session { name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg); // cursor on alpha (row 0)

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        for y in 0..buf.area.height {
            let mut line = String::new();
            for x in 0..buf.area.width {
                line.push_str(buf[(x, y)].symbol());
            }
            if line.contains("alpha") {
                for x in 0..buf.area.width {
                    let st = buf[(x, y)].style();
                    let invisible = st.bg == Some(Color::DarkGray)
                        && st.fg == Some(Color::DarkGray);
                    assert!(
                        !invisible,
                        "selected row has DarkGray-on-DarkGray (invisible) cell at x={x}"
                    );
                }
            }
        }
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
        assert_eq!(map_key(key(KeyCode::Char('g'))), Input::EnterGroups);
        assert_eq!(map_key(key(KeyCode::Char('p'))), Input::None);
        assert_eq!(map_key(key(KeyCode::Char('q'))), Input::Quit);
        assert_eq!(map_key(key(KeyCode::Esc)), Input::Quit);
        assert_eq!(map_key(shift(KeyCode::Char('K'))), Input::MoveUp);
        assert_eq!(map_key(shift(KeyCode::Char('J'))), Input::MoveDown);
        assert_eq!(map_key(key(KeyCode::Char('z'))), Input::ToggleAll);
        assert_eq!(map_key(key(KeyCode::Char('1'))), Input::Switch(1));
        assert_eq!(map_key(key(KeyCode::Char('9'))), Input::Switch(9));
        assert_eq!(map_key(key(KeyCode::Char('0'))), Input::None);
        assert_eq!(map_key(key(KeyCode::Char('x'))), Input::None);
        // Option/Alt+digit focuses (moves highlight) instead of switching.
        assert_eq!(map_key(alt(KeyCode::Char('1'))), Input::Focus(1));
        assert_eq!(map_key(alt(KeyCode::Char('9'))), Input::Focus(9));
        assert_eq!(map_key(alt(KeyCode::Char('0'))), Input::None);
    }

    #[test]
    fn maps_cycle_sort_key() {
        assert_eq!(map_key(key(KeyCode::Char('s'))), Input::CycleSort);
    }

    #[test]
    fn draw_shows_active_sort_mode() {
        let mode_text = |sort| {
            let sessions = vec![Session {
                name: "main".into(),
                activity: 100,
                created: 1,
                attached: false,
                windows: vec![Window { index: 0, name: "w".into(), active: true }],
            }];
            let cfg = Config { groups: vec![], manual_order: vec![], sort };
            render_to_string(&PickerState::build(sessions, &cfg))
        };
        assert!(mode_text(SortKey::Activity).contains("recency"), "recency label");
        assert!(mode_text(SortKey::Created).contains("age"), "age label");
        assert!(mode_text(SortKey::Manual).contains("manual"), "manual label");
    }

    #[test]
    fn fmt_age_formats_durations() {
        assert_eq!(fmt_age(0), "0s");
        assert_eq!(fmt_age(30), "30s");
        assert_eq!(fmt_age(59), "59s");
        assert_eq!(fmt_age(120), "2m");
        assert_eq!(fmt_age(7200), "2h");
        assert_eq!(fmt_age(172800), "2d");
        assert_eq!(fmt_age(-1), "0s");
        assert_eq!(fmt_age(-100), "0s");
    }

    #[test]
    fn draw_no_longer_renders_pin_star() {
        let sessions = vec![
            Session { name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![Group { name: "G".into(), members: vec!["claude".into()], color: String::new() }],
                           manual_order: vec![], sort: SortKey::Activity };
        let text = render_to_string(&PickerState::build(sessions, &cfg));
        assert!(!text.contains('★'), "pin star retired");
    }

    #[test]
    fn draw_shows_multiple_group_headers_in_order() {
        let sessions = vec![
            Session { name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "tent".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "ticket".into(), activity: 10, created: 3, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![
                Group { name: "config".into(), members: vec!["claude".into()], color: String::new() },
                Group { name: "tools".into(), members: vec!["tent".into()], color: String::new() },
            ],
            manual_order: vec![], sort: SortKey::Activity,
        };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("CONFIG"), "group name uppercased");
        assert!(text.contains("TOOLS"));
        assert!(text.contains("SESSIONS"));
        let (c, t, s) = (text.find("CONFIG"), text.find("TOOLS"), text.find("SESSIONS"));
        assert!(c < t && t < s, "sections render top-to-bottom");
    }

    #[test]
    fn draw_shows_empty_group_header_dimmed_in_session_mode() {
        // A named group with no members must still render its header (a shelf to
        // fill), grayed out, and it sits in config order between the group above
        // and the residual SESSIONS below.
        let sessions = vec![
            Session { name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "loose".into(), activity: 10, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![
                Group { name: "config".into(), members: vec!["claude".into()], color: String::new() },
                Group { name: "tools".into(), members: vec![], color: String::new() }, // empty shelf
            ],
            manual_order: vec![], sort: SortKey::Activity,
        };
        let state = PickerState::build(sessions, &cfg);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut text = String::new();
        let mut tools_dim = false;
        let mut config_accent = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            text.push_str(&line);
            text.push('\n');
            let first_letter = |needle: &str| line.find(needle).map(|i| i as u16);
            if let Some(x) = first_letter("TOOLS") {
                tools_dim = buf[(x, y)].style().fg == Some(Color::DarkGray);
            }
            if let Some(x) = first_letter("CONFIG") {
                config_accent = buf[(x, y)].style().fg == Some(ACCENT);
            }
        }
        // Order: CONFIG (populated) then TOOLS (empty) then SESSIONS (residual).
        let (c, t, s) = (text.find("CONFIG"), text.find("TOOLS"), text.find("SESSIONS"));
        assert!(c < t && t < s, "empty group header sits in order: got {c:?} {t:?} {s:?}");
        assert!(tools_dim, "empty group header renders dimmed (gray)");
        assert!(config_accent, "populated group header keeps the accent color");
    }

    #[test]
    fn draw_shows_footer_hints() {
        let sessions = vec![
            Session { name: "main".into(), activity: 100, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("search"), "footer hint: search present");
        assert!(text.contains("groups"), "footer hint: groups present");
        assert!(text.contains("sort"), "footer hint: sort present");
        assert!(text.contains("quit"), "footer hint: quit present");
    }

    #[test]
    fn draw_numbers_sessions_in_left_gutter() {
        let sessions = vec![
            Session { name: "main".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "other".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg); // main #1, other #2

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        // Inner content (excluding the popup margin and left border) per row.
        let inner_line = |y: u16| -> String {
            ((POPUP_MARGIN + 1)..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
        };
        for y in 0..buf.area.height {
            let line = inner_line(y);
            if line.contains("main") {
                assert!(line.starts_with("1 "), "main row gutter: got {line:?}");
            }
            if line.contains("other") {
                assert!(line.starts_with("2 "), "other row gutter: got {line:?}");
            }
        }
    }

    /// Column (x) of the metadata middot on every row that shows a session
    /// name, so alignment across rows can be asserted directly.
    fn metadata_dot_columns(buf: &ratatui::buffer::Buffer, names: &[&str]) -> Vec<u16> {
        let mut cols = Vec::new();
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if names.iter().any(|n| line.contains(n)) {
                for x in 0..buf.area.width {
                    if buf[(x, y)].symbol() == "·" {
                        cols.push(x);
                        break;
                    }
                }
            }
        }
        cols
    }

    #[test]
    fn metadata_shares_one_column_across_long_and_short_names() {
        // A single long name must shift every row's metadata together, not just
        // its own, so the middot separators stay vertically aligned (issue #3).
        let sessions = vec![
            Session { name: "a-very-long-session-name-here".into(), activity: 30, created: 1,
                      attached: false, windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "short".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let cols = metadata_dot_columns(&buf, &["a-very-long-session-name-here", "short"]);
        assert_eq!(cols.len(), 2, "both session rows should carry metadata, got {cols:?}");
        assert_eq!(cols[0], cols[1], "metadata middots must align across rows");
        // The long name (prefix 6 + 29 = 35) must push the shared column past
        // the default META_COL, taking the short row's metadata with it.
        assert!(cols[0] as usize > META_COL, "long name should advance the shared column");
    }

    #[test]
    fn metadata_middot_aligns_across_singular_and_plural_counts() {
        // "9 windows" is wider than "1 window"; the count field must be padded
        // to a uniform width so the middot and age stay aligned (issue #3).
        let many: Vec<Window> = (0..9)
            .map(|i| Window { index: i, name: "w".into(), active: i == 0 })
            .collect();
        let sessions = vec![
            Session { name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: many },
            Session { name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let cols = metadata_dot_columns(&buf, &["alpha", "beta"]);
        assert_eq!(cols.len(), 2, "both rows present, got {cols:?}");
        assert_eq!(cols[0], cols[1], "middot must align across 9-window and 1-window rows");
    }

    #[test]
    fn metadata_stays_at_default_column_for_short_names() {
        // With only short names, the shared column collapses back to META_COL,
        // preserving the original compact layout.
        let sessions = vec![
            Session { name: "main".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "other".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let cols = metadata_dot_columns(&buf, &["main", "other"]);
        assert_eq!(cols.len(), 2, "both rows present");
        assert_eq!(cols[0], cols[1], "short rows already align");
        // Content starts at POPUP_MARGIN + 1 (margin + left border). Metadata
        // begins at META_COL; the middot follows the "1 window " token (9 cells).
        let content_start = (POPUP_MARGIN + 1) as usize;
        assert_eq!(cols[0] as usize, content_start + META_COL + 9,
                   "default column unchanged: got {}", cols[0]);
    }

    #[test]
    fn draw_insets_frame_by_popup_margin() {
        let sessions = vec![
            Session { name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);

        let (w, h) = (60u16, 20u16);
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        // Rounded border corners are inset by the margin, not flush to the edge.
        assert_eq!(buf[(POPUP_MARGIN, POPUP_MARGIN)].symbol(), "╭", "top-left inset");
        assert_eq!(buf[(w - 1 - POPUP_MARGIN, POPUP_MARGIN)].symbol(), "╮", "top-right inset");
        assert_eq!(buf[(POPUP_MARGIN, h - 1 - POPUP_MARGIN)].symbol(), "╰", "bottom-left inset");
        assert_eq!(buf[(w - 1 - POPUP_MARGIN, h - 1 - POPUP_MARGIN)].symbol(), "╯", "bottom-right inset");

        // The buffer ring (outer `margin` cells on every side) stays blank.
        for y in 0..h {
            for x in 0..w {
                let in_ring = x < POPUP_MARGIN
                    || y < POPUP_MARGIN
                    || x >= w - POPUP_MARGIN
                    || y >= h - POPUP_MARGIN;
                if in_ring {
                    assert_eq!(buf[(x, y)].symbol(), " ", "ring cell ({x},{y}) blank");
                }
            }
        }
    }

    #[test]
    fn slash_enters_search_in_command_mode() {
        assert_eq!(map_key(key(KeyCode::Char('/'))), Input::EnterSearch);
    }

    #[test]
    fn search_keys_map_to_query_edits_and_nav() {
        assert_eq!(map_search_key(key(KeyCode::Char('a'))), SearchInput::Char('a'));
        assert_eq!(map_search_key(key(KeyCode::Char('1'))), SearchInput::Char('1'));
        assert_eq!(map_search_key(shift(KeyCode::Char('A'))), SearchInput::Char('A'));
        assert_eq!(map_search_key(key(KeyCode::Backspace)), SearchInput::Backspace);
        assert_eq!(map_search_key(key(KeyCode::Enter)), SearchInput::Select);
        assert_eq!(map_search_key(key(KeyCode::Esc)), SearchInput::Exit);
        assert_eq!(map_search_key(key(KeyCode::Up)), SearchInput::Up);
        assert_eq!(map_search_key(key(KeyCode::Down)), SearchInput::Down);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('p'))), SearchInput::Up);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('k'))), SearchInput::Up);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('n'))), SearchInput::Down);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('j'))), SearchInput::Down);
        // Bulk deletes: Ctrl-W / Alt-Backspace delete a word, Ctrl-U clears.
        assert_eq!(map_search_key(ctrl(KeyCode::Char('w'))), SearchInput::DeleteWord);
        assert_eq!(map_search_key(alt(KeyCode::Backspace)), SearchInput::DeleteWord);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('u'))), SearchInput::Clear);
        // Plain Backspace still deletes a single char.
        assert_eq!(map_search_key(key(KeyCode::Backspace)), SearchInput::Backspace);
        // Ctrl-modified letters are nav/no-op, never query text.
        assert_eq!(map_search_key(ctrl(KeyCode::Char('a'))), SearchInput::None);
    }

    fn searching_state(query: &str) -> PickerState {
        let sessions = vec![
            Session { name: "pr-review".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "scratch".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![Group { name: "PINNED".into(), members: vec!["pr-review".into()], color: String::new() }],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        for c in query.chars() {
            state.search_push(c);
        }
        state
    }

    #[test]
    fn draw_search_shows_prompt_and_filters() {
        let text = render_to_string(&searching_state("pr"));
        assert!(text.contains("search:"), "search prompt present");
        assert!(text.contains("pr-review"), "match shown");
        assert!(!text.contains("scratch"), "non-match hidden");
    }

    #[test]
    fn draw_search_hides_headers_and_numbers() {
        let text = render_to_string(&searching_state("pr"));
        assert!(!text.contains("PINNED"), "no section headers in search");
        assert!(!text.contains("SESSIONS"), "no section headers in search");
        // No jump-number gutter: the pr-review row must not start with "1 ".
        for line in text.lines() {
            if line.contains("pr-review") {
                assert!(!line.trim_start().starts_with("1 "), "no jump number: {line:?}");
            }
        }
    }

    #[test]
    fn draw_search_shows_no_matches_and_search_footer() {
        let text = render_to_string(&searching_state("zzzzz"));
        assert!(text.contains("no matches"), "empty-state line present");
        assert!(text.contains("Esc"), "search footer present");
    }

    #[test]
    fn group_keys_map_to_ops() {
        assert_eq!(map_group_key(key(KeyCode::Char('j'))), GroupInput::Down);
        assert_eq!(map_group_key(key(KeyCode::Char('k'))), GroupInput::Up);
        assert_eq!(map_group_key(shift(KeyCode::Char('J'))), GroupInput::MoveDown);
        assert_eq!(map_group_key(shift(KeyCode::Char('K'))), GroupInput::MoveUp);
        assert_eq!(map_group_key(key(KeyCode::Char('n'))), GroupInput::New);
        assert_eq!(map_group_key(key(KeyCode::Enter)), GroupInput::Rename);
        assert_eq!(map_group_key(key(KeyCode::Char('r'))), GroupInput::Rename);
        assert_eq!(map_group_key(key(KeyCode::Char('c'))), GroupInput::CycleColor);
        assert_eq!(map_group_key(key(KeyCode::Char('d'))), GroupInput::Delete);
        assert_eq!(map_group_key(key(KeyCode::Esc)), GroupInput::Exit);
        assert_eq!(map_group_key(key(KeyCode::Char('q'))), GroupInput::Exit);
        assert_eq!(map_group_key(key(KeyCode::Char('g'))), GroupInput::Exit);
        assert_eq!(map_group_key(key(KeyCode::Char('x'))), GroupInput::None);
    }

    #[test]
    fn draw_colors_group_header_by_its_color_in_session_mode() {
        // An explicit group color paints its header; a color-less group falls
        // back to the positional default (HEADER_COLORS[0] == cyan == ACCENT).
        let sessions = vec![
            Session { name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "tent".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![
                Group { name: "config".into(), members: vec!["claude".into()], color: String::new() },
                Group { name: "tools".into(), members: vec!["tent".into()], color: "magenta".into() },
            ],
            manual_order: vec![], sort: SortKey::Activity,
        };
        let state = PickerState::build(sessions, &cfg);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut config_cyan = false;
        let mut tools_magenta = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(i) = line.find("CONFIG") {
                config_cyan = buf[(i as u16, y)].style().fg == Some(Color::Cyan);
            }
            if let Some(i) = line.find("TOOLS") {
                tools_magenta = buf[(i as u16, y)].style().fg == Some(Color::Magenta);
            }
        }
        assert!(config_cyan, "color-less group uses positional default (cyan)");
        assert!(tools_magenta, "explicit magenta group header is purple");
    }

    fn groups_view(edit: bool) -> PickerState {
        let sessions = vec![
            Session { name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "ticket".into(), activity: 10, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![Group { name: "config".into(), members: vec!["claude".into()], color: String::new() }],
                           manual_order: vec![], sort: SortKey::Activity };
        let mut st = PickerState::build(sessions, &cfg);
        st.enter_groups();
        if edit { st.group_start_rename(); }
        st
    }

    #[test]
    fn draw_groups_lists_group_with_count_and_residual_anchor() {
        let text = render_to_string(&groups_view(false));
        assert!(text.contains("CONFIG"), "group header");
        assert!(text.contains("· 1"), "member count");
        assert!(text.contains("SESSIONS"), "residual anchor");
        assert!(text.contains("Enter rename"), "group footer");
    }

    #[test]
    fn draw_groups_shows_inline_rename_field() {
        let mut st = groups_view(true);
        st.group_edit_clear();
        for c in "misc".chars() { st.group_edit_push(c); }
        let text = render_to_string(&st);
        assert!(text.contains("MISC"), "inline buffer uppercased");
    }

    #[test]
    fn draw_is_graceful_on_tiny_popup() {
        let sessions = vec![
            Session { name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);

        // Smaller than 2*margin+1: must not panic and must keep its size.
        let backend = TestBackend::new(3, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        assert_eq!(buf.area.width, 3);
        assert_eq!(buf.area.height, 3);
    }
}
