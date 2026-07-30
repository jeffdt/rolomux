use crate::model::{
    AttachedColorMode, ColorPolicy, CreateStage, DefaultMode, DotColorMode, Group, Mode, PickerState, Row,
    Session, SessionMetric, SettingsRow, StartFocusMode, SwapDirection, Window, ALL_NAMED_COLORS,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::time::{SystemTime, UNIX_EPOCH};

mod help;
mod settings;

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const WARNING: Color = Color::Red;
const SEL_BG: Color = Color::DarkGray;
/// Default column where a session's metadata begins, used when every visible
/// name is short. It is also the floor for the shared metadata column.
const META_COL: usize = 30;
/// Fixed cells preceding a session name: jump number (2) + expand glyph and
/// its trailing space (2).
const SESSION_PREFIX: usize = 6;
/// Minimum gap kept between the longest visible name and its metadata when the
/// shared column is anchored to that name rather than to META_COL.
const META_GAP: usize = 2;
/// Cells reserved at the right so the shared column never pushes metadata off
/// the card; roughly the widest plausible "12 windows · 20s".
const META_BUDGET: usize = 18;
/// Uniform buffer between the picker's border and the popup edge. The popup is
/// launched borderless (`tmux display-popup -B`), so this blank ring is the
/// only separation between rolomux's frame and the surrounding tmux panes; it
/// keeps the picker from sitting flush against busy content behind the popup.
const POPUP_MARGIN: u16 = 2;
/// Rows reserved right under the top border as blank breathing room,
/// separating the title's chrome from the first group header so content
/// doesn't start flush against it. Used to be two rows (a DarkGray divider
/// rule plus this blank row); the rule was removed (issue #81) since it
/// read as visual clutter rather than a useful separator.
const TITLE_CHROME_ROWS: u16 = 1;

const FOOTER_HINT: &str =
    "/ search · r rename · JK mv · x kill · g grp · , cfg · d dim · f foc · q quit";

const CREATE_GROUP_HINT: &str =
    "No groups yet: press g then n to create one, then use ⇧J/⇧K to move sessions.";

const SEARCH_FOOTER_HINT: &str = "↑↓ move · ⌃w word · ⌃u clear · ⌃f focus · Esc back";

/// The footer hint while the cursor is raised to group altitude. Same verbs
/// as `FOOTER_HINT`, one level up: they act on the highlighted group rather
/// than the selected session.
const ALTITUDE_FOOTER_HINT: &str =
    "r rename \u{b7} n new \u{b7} c color \u{b7} x delete \u{b7} \u{21e7}JK move \u{b7} Enter open \u{b7} Esc back";

/// Footer hint while the in-flight create prompt is in its session-name
/// stage (see `CreateStage`).
const CREATE_SESSION_FOOTER_HINT: &str = "Enter next \u{b7} Esc cancel";

/// Footer hint while the in-flight create prompt is in its window-name
/// stage.
const CREATE_WINDOW_FOOTER_HINT: &str = "Enter skip/create \u{b7} Esc back";

/// The running binary's version, as `git describe --tags --dirty` saw it at
/// build time (e.g. `v0.27.0` on a clean tagged release, `v0.27.0-3-gabc1234`
/// on a local dev build, `-dirty` appended if the tree had uncommitted
/// changes). Falls back to `v{CARGO_PKG_VERSION}` if git/tags weren't
/// available at build time (see `build.rs`).
fn app_version() -> &'static str {
    env!("ROLOMUX_VERSION")
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

fn dormant_session(selected: bool) -> Style {
    if selected {
        Style::default().fg(Color::Gray)
    } else {
        Style::default().fg(DIM)
    }
}

/// Resolves the brief post-`⇧J`/`⇧K` directional flash (issue #130) into a
/// glyph and color, or `None` if this row isn't part of the active
/// indicator. The bright stage is `Color::Yellow`, visible on any row --
/// but the dim stage can't use plain `DIM` (`Color::DarkGray`) on a
/// selected row: that's exactly `SEL_BG`, so the glyph would vanish into
/// its own highlight bar. `dormant_session` above already solves this
/// identical contrast problem by swapping to `Color::Gray` when selected;
/// the dim stage reuses that split. Each row kind places the resolved
/// glyph differently (see `session_item`, `window_item`, `header_item`):
/// session rows splice it into the padding just before the shared
/// metadata column, which is already kept aligned across every row and
/// already guarantees room for it; window rows have no such shared column,
/// so they append it right after their own content, and group headers spend
/// two cells of their trailing rule on it.
fn swap_marker_glyph(marker: Option<(SwapDirection, bool)>, selected: bool) -> Option<(&'static str, Color)> {
    let (dir, bright) = marker?;
    let glyph = match dir {
        SwapDirection::Up => "▲",
        SwapDirection::Down => "▼",
    };
    let color = if bright {
        Color::Yellow
    } else if selected {
        Color::Gray
    } else {
        DIM
    };
    Some((glyph, color))
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
    fmt_age(now_unix().saturating_sub(activity).max(0))
}

/// Current unix time, as the sole impure edge `activity_age` depends on.
/// Tests can freeze this via `tests::freeze_clock` so a session's age string
/// is deterministic instead of racing the real wall clock between when the
/// test computes its fixture timestamps and when render reads `now_unix()`.
fn now_unix() -> i64 {
    #[cfg(test)]
    if let Some(frozen) = tests::TEST_CLOCK.with(|c| c.get()) {
        return frozen;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Which `Session` timestamp field feeds the metadata age string, per the
/// current `SessionMetric` setting. `None` for `Hidden`, meaning the row
/// shows no age string (and no trailing separator) at all.
fn session_metric_timestamp(sess: &Session, metric: SessionMetric) -> Option<i64> {
    match metric {
        SessionMetric::Recency => Some(sess.activity),
        SessionMetric::Age => Some(sess.created),
        SessionMetric::Hidden => None,
    }
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
    let border_color = color_from_name(&state.border_color);
    let border_style = Style::default().fg(border_color);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Line::from(vec![
            Span::styled("─", border_style),
            Span::styled("‹ rolomux ›", border_style.add_modifier(Modifier::BOLD | Modifier::ITALIC)),
        ]));
    if state.mode == Mode::Settings {
        block = block.title_bottom(
            Line::from(vec![
                Span::styled(
                    format!("‹ {} ›", app_version()),
                    border_style.add_modifier(Modifier::BOLD | Modifier::ITALIC),
                ),
                Span::styled("─", border_style),
            ])
            .right_aligned(),
        );
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(TITLE_CHROME_ROWS), Constraint::Min(0)])
        .split(inner);
    let content = chunks[1];

    match state.mode {
        // Group altitude is not a separate screen: it renders the same picker
        // through the same renderer, with the session rows receded and the
        // highlight moved up onto a group header.
        Mode::Command | Mode::Groups => draw_command(frame, state, content),
        Mode::Search => draw_search(frame, state, content),
        Mode::Settings => settings::draw_settings(frame, state, content),
    }

    if state.help_visible() {
        help::draw_help_overlay(frame, state, inner);
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

    // Raised == the cursor is at group altitude. The list is built exactly as
    // at session altitude; only the styling and which line carries the
    // highlight differ.
    let raised = state.mode == Mode::Groups;
    let ordered = state.ordered();
    let rows = state.visible_rows();
    // No session row is selected while raised: the selection bar belongs to a
    // group header, and a row styled as selected without wearing the bar would
    // read as a second, phantom cursor.
    let cursor_row = if raised { None } else { rows.get(state.cursor).copied() };

    // Anchor metadata to one shared geometry across every session row, computed
    // from the visible sessions (window rows carry no metadata).
    let session_refs = rows.iter().filter_map(|r| match r {
        Row::Session(si) => Some(ordered[*si]),
        Row::Window(..) => None,
    });
    // True once 11+ sessions receive a jump number, i.e. some row uses the
    // two-character `⌥N` label instead of a single digit -- see
    // `MetaLayout::compute`'s doc comment for why this is a render-wide flag.
    let numbered_session_count = rows.iter().filter(|r| match r {
        Row::Session(si) => state.number_dormant_sessions || !state.is_dormant(&ordered[*si].name),
        Row::Window(..) => false,
    }).count();
    let wide_numbering = numbered_session_count > 10;
    let meta = MetaLayout::compute(session_refs, list_area.width, true, wide_numbering);
    let attached_static_color = color_from_name(&state.attached_color);
    let dot_static_color = color_from_name(&state.dot_color);

    let group_ids = state.ordered_group_ids();
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_line: Option<usize> = None;
    let mut last_section: Option<usize> = None;
    let mut current_gutter_color: Color = ACCENT;
    // Named groups whose header is already emitted are those with index < this.
    // Empty groups produce no session rows, so we "catch up" and emit their bare
    // (dimmed) headers when we pass their position or at the end of the list.
    let mut next_group: usize = 0;
    let mut next_jump_number: usize = 1;
    let show_create_group_hint = !state.groups.iter().any(|g| !g.inbox);
    let quick_create_target = if state.quick_creating() {
        state.cursor_session_name().and_then(|name| state.group_index_of(&name))
    } else {
        None
    };
    // Two signals for the in-flight session-create phantom row, derived
    // purely from `state.mode` plus existing public accessors -- no key can
    // move the cursor while `state.creating()` is true, so these stay in
    // sync with whatever `start_create_here`/`start_create_in_group` picked.
    // `create_above_session` (Command-mode `AboveSelected`) wins when set;
    // otherwise `create_end_target_group` (group-mode `EndOfGroup`, or the
    // empty-picker fallback to the inbox) places the phantom at the end of
    // that group's block, including an empty group's own header slot.
    let create_above_session = if state.creating() && state.mode != Mode::Groups {
        state.cursor_session_name()
    } else {
        None
    };
    let create_end_target_group = if !state.creating() {
        None
    } else if state.mode == Mode::Groups {
        Some(state.group_cursor())
    } else if create_above_session.is_none() {
        state.inbox_index()
    } else {
        None
    };
    let create_buf = state.create_buffer().unwrap_or("");
    let create_placeholder = match state.create_stage() {
        Some(CreateStage::WindowName) => "window name",
        _ => "session name",
    };
    let blink_visible = state.blink_visible();
    let caret = caret_glyph(state);

    for row in rows.iter() {
        match row {
            Row::Session(si) => {
                let sess = ordered[*si];
                let section = group_ids[*si];
                if last_section != Some(section) {
                    if create_end_target_group.is_some() && create_end_target_group == last_section {
                        push_create_phantom_row(&mut items, create_buf, current_gutter_color, wide_numbering, raised, false, create_placeholder, blink_visible, caret);
                    }
                    let target = section;
                    while next_group < target {
                        if push_empty_group_header_unless_focused(&mut items, state, next_group, list_area.width, raised) {
                            note_highlighted_header(&mut selected_line, &items, state, next_group);
                            if create_end_target_group == Some(next_group) {
                                let color = group_color(&state.groups[next_group], next_group, &state.active_palette);
                                push_create_phantom_row(&mut items, create_buf, color, wide_numbering, raised, false, create_placeholder, blink_visible, caret);
                            }
                        }
                        next_group += 1;
                    }
                    let color = group_color(&state.groups[section], section, &state.active_palette);
                    if show_create_group_hint && state.groups[section].inbox {
                        push_create_group_hint(&mut items);
                    }
                    if quick_create_target == Some(section) {
                        push_quick_create_phantom_row(
                            &mut items,
                            state.quick_create_buffer().unwrap_or(""),
                            color_from_name(&state.border_color),
                            blink_visible,
                            caret,
                        );
                        // The anchor session below keeps the cursor (so its
                        // own row still renders with selected styling), but
                        // the selection *bar* moves to this phantom row --
                        // matching the create-session/create-group prompts,
                        // where the input row is always the one that's
                        // highlighted, not a session sitting nearby.
                        selected_line = Some(items.len() - 1);
                    }
                    push_group_header(&mut items, state, section, list_area.width, color);
                    note_highlighted_header(&mut selected_line, &items, state, section);
                    current_gutter_color = color;
                    next_group = section + 1;
                    last_section = Some(section);
                }
                let selected = Some(*row) == cursor_row;
                if selected && !state.quick_creating() {
                    selected_line = Some(items.len());
                }
                let dormant = state.is_dormant(&sess.name);
                let numbered = state.number_dormant_sessions || !dormant;
                // Stable jump number: 1-based position among numbered sessions,
                // for the first 20 sessions. Unaffected by what is expanded.
                let number = if numbered {
                    let n = next_jump_number;
                    next_jump_number += 1;
                    if n <= 20 { Some(n) } else { None }
                } else {
                    None
                };
                let rename_buf = if selected && state.renaming() { state.rename_edit_buffer() } else { None };
                let attached_color = match state.attached_color_mode {
                    AttachedColorMode::Match => current_gutter_color,
                    AttachedColorMode::Static => attached_static_color,
                };
                if create_above_session.as_deref() == Some(sess.name.as_str()) {
                    push_create_phantom_row(&mut items, create_buf, current_gutter_color, wide_numbering, raised, selected, create_placeholder, blink_visible, caret);
                }
                items.push(recede(session_item(
                    sess,
                    state.is_expanded(&sess.name),
                    selected,
                    number,
                    meta,
                    dormant,
                    attached_color,
                    None,
                    Some(current_gutter_color),
                    state.session_metric,
                    rename_buf,
                    false,
                    false,
                    state.session_swap_marker(&sess.name),
                    wide_numbering,
                    caret,
                ), raised));
            }
            Row::Window(si, wi) => {
                let sess = ordered[*si];
                let selected = Some(*row) == cursor_row;
                if selected && !state.quick_creating() {
                    selected_line = Some(items.len());
                }
                let last = *wi + 1 == sess.windows.len();
                let rename_buf = if selected && state.renaming() { state.rename_edit_buffer() } else { None };
                let window_dormant = state.is_window_dormant(&sess.name, sess.windows[*wi].index);
                let dormant = state.is_dormant(&sess.name) || window_dormant;
                let dot_color = match state.dot_color_mode {
                    DotColorMode::Group => current_gutter_color,
                    DotColorMode::Static => dot_static_color,
                };
                items.push(recede(window_item(
                    &sess.windows[*wi],
                    last,
                    selected,
                    Some(current_gutter_color),
                    dormant,
                    window_dormant,
                    rename_buf,
                    dot_color,
                    state.window_swap_marker(&sess.name, sess.windows[*wi].index),
                    wide_numbering,
                    caret,
                ), raised));
            }
        }
    }
    // The target group was the last section with any session rows, so its
    // closing point (mirroring the mid-list check above) never fired inside
    // the loop.
    if create_end_target_group.is_some() && create_end_target_group == last_section {
        push_create_phantom_row(&mut items, create_buf, current_gutter_color, wide_numbering, raised, false, create_placeholder, blink_visible, caret);
    }
    // Trailing empty groups (after the last session row, with no residual below).
    while next_group < state.groups.len() {
        if push_empty_group_header_unless_focused(&mut items, state, next_group, list_area.width, raised) {
            note_highlighted_header(&mut selected_line, &items, state, next_group);
            if create_end_target_group == Some(next_group) {
                let color = group_color(&state.groups[next_group], next_group, &state.active_palette);
                push_create_phantom_row(&mut items, create_buf, color, wide_numbering, raised, false, create_placeholder, blink_visible, caret);
            }
        }
        next_group += 1;
    }

    let list = List::new(items).highlight_style(
        Style::default().bg(SEL_BG).add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    list_state.select(selected_line);
    frame.render_stateful_widget(list, list_area, &mut list_state);

    // Render the divider and hint row inside the footer area. A pending
    // confirm is destructive if missed, so it takes over the hint line in
    // WARNING (red) rather than the normal dim hint color. Each altitude has
    // its own pair, and within a pair the more destructive one wins: a group
    // delete outranks the blocked-inbox-reorder nudge (the two can't be armed
    // at once anyway, since any input clears the other's arm).
    let warning = if raised {
        state
            .pending_group_delete_warning()
            .or_else(|| state.group_reorder_blocked_warning().map(str::to_string))
    } else {
        state
            .pending_kill_warning()
            .or_else(|| state.pending_window_move_warning().map(str::to_string))
    };
    let hint_line = match warning {
        Some(warning) => Line::from(Span::styled(
            warning,
            Style::default().fg(WARNING).add_modifier(Modifier::BOLD),
        )),
        None => shortcut_hint_line(state, &command_footer_hint(state)),
    };
    let footer = Paragraph::new(vec![
        Line::from(Span::styled(footer_rule(footer_area.width, state), Style::default().fg(DIM))),
        hint_line,
    ]);
    frame.render_widget(footer, footer_area);
}

/// The caret glyph to render this frame for an in-flight inline text edit
/// (session/window rename, group rename/create, quick-create, search),
/// or `None` when `caret_blink` is on and this frame falls in the blink
/// clock's hidden half. Independent of the `blink_visible` parameter
/// threaded through `session_item`/`window_item`/`group_edit_line` below,
/// which gates the *empty-buffer placeholder* text instead -- this one
/// gates the *typed* caret.
fn caret_glyph(state: &PickerState) -> Option<&'static str> {
    if state.caret_blink && !state.blink_visible() {
        None
    } else {
        Some(state.caret_style.glyph())
    }
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

    let caret = caret_glyph(state);
    let mut prompt_spans = vec![
        Span::styled("search: ", Style::default().fg(DIM)),
        Span::raw(state.query.clone()),
    ];
    if let Some(c) = caret {
        prompt_spans.push(Span::styled(c, Style::default().fg(color_from_name(&state.border_color))));
    }
    let prompt = Line::from(prompt_spans);
    frame.render_widget(Paragraph::new(prompt), chunks[0]);

    let results = state.search_results();
    let rows = state.search_rows();
    let cursor_row = rows.get(state.search_cursor()).copied();
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_line: Option<usize> = None;
    if results.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "  no matches",
            Style::default().fg(DIM),
        ))));
    } else {
        // Search always suppresses jump numbers (see module doc), so the
        // wide-numbering column is never needed here.
        let meta = MetaLayout::compute(results.iter().copied(), chunks[1].width, false, false);
        let attached_static_color = color_from_name(&state.attached_color);
        let dot_static_color = color_from_name(&state.dot_color);
        // Widest age string among tagged rows, so every tag in this render
        // starts at the same column regardless of its own row's age width.
        // Degenerates to 0 when `session_metric` is Hidden (every row's
        // resolved timestamp is `None`), which the pad math below already
        // handles as a no-op.
        let age_width = results
            .iter()
            .filter(|s| state.group_index_of(&s.name).is_some())
            .filter_map(|s| session_metric_timestamp(s, state.session_metric))
            .map(|ts| activity_age(ts).chars().count())
            .max()
            .unwrap_or(0);
        for row in rows.iter() {
            match row {
                Row::Session(si) => {
                    let sess = results[*si];
                    let selected = Some(*row) == cursor_row;
                    if selected {
                        selected_line = Some(items.len());
                    }
                    let group_tag = state.group_index_of(&sess.name).map(|gi| {
                        let this_age_width = session_metric_timestamp(sess, state.session_metric)
                            .map(|ts| activity_age(ts).chars().count())
                            .unwrap_or(0);
                        let age_pad = age_width.saturating_sub(this_age_width);
                        (state.groups[gi].name.clone(), group_color(&state.groups[gi], gi, &state.active_palette), age_pad)
                    });
                    let attached_color = match state.attached_color_mode {
                        AttachedColorMode::Match => state
                            .group_index_of(&sess.name)
                            .map(|gi| group_color(&state.groups[gi], gi, &state.active_palette))
                            .unwrap_or(attached_static_color),
                        AttachedColorMode::Static => attached_static_color,
                    };
                    items.push(session_item(sess, state.is_expanded(&sess.name), selected, None, meta, state.is_dormant(&sess.name), attached_color, group_tag, None, state.session_metric, None, false, false, None, false, caret));
                }
                Row::Window(si, wi) => {
                    let sess = results[*si];
                    let selected = Some(*row) == cursor_row;
                    if selected {
                        selected_line = Some(items.len());
                    }
                    let last = *wi + 1 == sess.windows.len();
                    let window_dormant = state.is_window_dormant(&sess.name, sess.windows[*wi].index);
                    let dormant = state.is_dormant(&sess.name) || window_dormant;
                    let dot_color = match state.dot_color_mode {
                        DotColorMode::Group => state
                            .group_index_of(&sess.name)
                            .map(|gi| group_color(&state.groups[gi], gi, &state.active_palette))
                            .unwrap_or(dot_static_color),
                        DotColorMode::Static => dot_static_color,
                    };
                    items.push(window_item(&sess.windows[*wi], last, selected, None, dormant, window_dormant, None, dot_color, None, false, caret));
                }
            }
        }
    }
    let list = List::new(items)
        .highlight_style(Style::default().bg(SEL_BG).add_modifier(Modifier::BOLD));
    let mut list_state = ListState::default();
    list_state.select(selected_line);
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    let footer = Paragraph::new(vec![
        Line::from(Span::styled(footer_rule(chunks[2].width, state), Style::default().fg(DIM))),
        shortcut_hint_line(state, &search_footer_hint(state)),
    ]);
    frame.render_widget(footer, chunks[2]);
}

/// Map a named color to its ANSI `Color` (never RGB, so headers follow the
/// terminal theme). `magenta` is the Nord purple. Unknown names fall back to
/// the accent so a hand-edited config can never crash the picker.
fn color_from_name(name: &str) -> Color {
    match name {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" => Color::Gray,
        "darkgray" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        _ => Color::Cyan,
    }
}

/// The header color for the group at `index`: its explicit color, or the
/// positional default (`active_palette[index % len]`). Falls back to the
/// accent on an empty palette rather than dividing by zero (should not
/// happen -- see `PickerState::group_cycle_color`'s guard -- but never
/// panics if it somehow does).
fn group_color(group: &Group, index: usize, active_palette: &[String]) -> Color {
    let name = if group.color.is_empty() {
        if active_palette.is_empty() {
            return ACCENT;
        }
        active_palette[index % active_palette.len()].as_str()
    } else {
        group.color.as_str()
    };
    color_from_name(name)
}

/// Fade a session or window row while the cursor is at group altitude. The
/// modifier lands on the whole row rather than on each span: ratatui patches
/// an item-level style *under* every span's own style, so each cell gains DIM
/// while keeping the fg and modifiers that give the row its identity (attached
/// cyan and bold, dormant DarkGray, the jump number). Recede is a plane
/// transform; dormancy stays a state recolor, and the two never collide.
fn recede(item: ListItem<'static>, raised: bool) -> ListItem<'static> {
    if raised {
        item.style(Style::default().add_modifier(Modifier::DIM))
    } else {
        item
    }
}

/// Point `selected_line` at the header just pushed as `items`' last entry when
/// that header is the one the group cursor is on. At group altitude the
/// selection bar rides a group header instead of a session row, and `List`'s
/// `highlight_style` only knows about item indices.
fn note_highlighted_header(
    selected_line: &mut Option<usize>,
    items: &[ListItem<'static>],
    state: &PickerState,
    gi: usize,
) {
    if state.mode == Mode::Groups && gi == state.group_cursor() {
        *selected_line = Some(items.len() - 1);
    }
}

/// Push a group header, preceding it with a blank spacer unless it is the very
/// first item in the list.
fn push_group_header(items: &mut Vec<ListItem<'static>>, state: &PickerState, gi: usize, width: u16, color: Color) {
    if !items.is_empty() {
        items.push(ListItem::new(Line::from("")));
    }
    items.push(header_item(state, gi, width, color));
}

/// Push the live `⇧N` quick-create row: the typed buffer in the same
/// uppercased-bold-plus-caret style as an inline group rename, preceded by a
/// blank spacer like `push_group_header`, marking where the new group will
/// land once committed. This row always carries the list's selection bar
/// while it's showing (the caller moves `selected_line` onto it, so the
/// anchor session below reads as selected but isn't the highlighted row) --
/// same as `header_item`'s group-editing branch, so its placeholder needs
/// `Color::Gray`, not plain `DIM` (see `group_edit_line`).
fn push_quick_create_phantom_row(items: &mut Vec<ListItem<'static>>, buf: &str, caret_color: Color, blink_visible: bool, caret: Option<&'static str>) {
    if !items.is_empty() {
        items.push(ListItem::new(Line::from("")));
    }
    items.push(ListItem::new(group_edit_line(buf, None, caret_color, "GROUP NAME", Style::default().fg(Color::Gray), blink_visible, caret)));
}

/// Push the live create-session phantom row: the typed buffer in the same
/// caret style as an inline session rename. Reuses `session_item`'s
/// `rename_buf` branch via a placeholder `Session` whose fields are never
/// read on that branch, so no real session's number or metadata is
/// perturbed by pushing this directly into `items`. While `buf` is empty,
/// shows a slow-blinking dim `placeholder` (e.g. "session name") instead of
/// leaving the row blank, so the prompt reads as "type a name here" rather
/// than looking like an unlabeled key hint; `blink_visible` (from
/// `PickerState::blink_visible`) is when this frame shows it. `selected`
/// must be true exactly when this phantom row will inherit the list's
/// selection bar (`SEL_BG`, which is the same color as `DIM`) -- otherwise
/// the placeholder renders invisible on top of it, the same contrast trap
/// `dormant_session`/swap-marker styling already works around elsewhere in
/// this file.
#[allow(clippy::too_many_arguments)]
fn push_create_phantom_row(
    items: &mut Vec<ListItem<'static>>,
    buf: &str,
    gutter_color: Color,
    wide_numbering: bool,
    raised: bool,
    selected: bool,
    placeholder: &str,
    blink_visible: bool,
    caret: Option<&'static str>,
) {
    let sess = Session {
        id: String::new(), name: String::new(), activity: 0, created: 0, attached: false, windows: vec![],
    };
    let meta = MetaLayout { col: 0, count_width: 0 };
    let (rename_buf, dim_buf) = if buf.is_empty() { (placeholder, true) } else { (buf, false) };
    items.push(recede(
        session_item(
            &sess,
            false,
            selected,
            None,
            meta,
            false,
            gutter_color,
            None,
            Some(gutter_color),
            SessionMetric::Hidden,
            Some(rename_buf),
            dim_buf,
            blink_visible,
            None,
            wide_numbering,
            caret,
        ),
        raised,
    ));
}

fn push_create_group_hint(items: &mut Vec<ListItem<'static>>) {
    items.push(ListItem::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(CREATE_GROUP_HINT.to_string(), Style::default().fg(DIM)),
    ])));
}

/// Push a bare, dimmed header for an empty named group (a labeled shelf to
/// fill). The group under the cursor at group altitude is the exception: DIM
/// is `DarkGray`, exactly `SEL_BG`, so dimming it under the selection bar
/// would render its name invisible (issue #14). It keeps its real color.
fn push_empty_group_header(items: &mut Vec<ListItem<'static>>, state: &PickerState, gi: usize, width: u16) {
    let color = if state.mode == Mode::Groups && gi == state.group_cursor() {
        group_color(&state.groups[gi], gi, &state.active_palette)
    } else {
        DIM
    };
    push_group_header(items, state, gi, width, color);
}

/// Same as `push_empty_group_header`, but skipped entirely in focus mode;
/// returns whether a header was actually pushed. Every call site reaches this
/// only for a group already known to have zero visible sessions (a group with
/// any visible session always gets its real header via `push_group_header`
/// instead), so gating on `focus_mode` alone is correct -- except while
/// `raised` (group altitude): there the user is managing group structure
/// itself, so focus mode's hiding is suspended and every group, including
/// empty ones, must stay reachable to the group cursor.
fn push_empty_group_header_unless_focused(
    items: &mut Vec<ListItem<'static>>,
    state: &PickerState,
    gi: usize,
    width: u16,
    raised: bool,
) -> bool {
    if state.focus_mode() && !raised {
        return false;
    }
    push_empty_group_header(items, state, gi, width);
    true
}

fn group_name(g: &Group, upper: bool) -> String {
    if upper { g.name.to_uppercase() } else { g.name.clone() }
}

/// Prefixes a group's display label with the inbox icon when it's the
/// inbox group. Used anywhere plain text needs to match the rendered label.
fn group_label(g: &Group, upper: bool, icon: &str) -> String {
    let name = group_name(g, upper);
    if g.inbox { format!("{icon} {name}") } else { name }
}

fn group_label_width(g: &Group, upper: bool, icon: &str) -> usize {
    group_label(g, upper, icon).chars().count()
}

fn group_label_spans(g: &Group, upper: bool, color: Color, icon: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if g.inbox {
        spans.push(Span::styled(format!("{icon} "), Style::default().fg(color)));
    }
    spans.push(Span::styled(
        group_name(g, upper),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ));
    spans
}

/// An in-flight group name: the typed buffer uppercased and bold, trailed by
/// a caret in the border color. Shared by an inline rename or create on a
/// group header and by the `⇧N` quick-create phantom row. While `buf` is
/// empty, shows a slow-blinking `placeholder` (`placeholder_style`; see call
/// sites for why the style differs: one is always the selected row, one
/// never is) instead of a caret -- the caret only appears once real typed
/// text exists. `blink_visible` (from `PickerState::blink_visible`) is
/// whether this frame shows it.
fn group_edit_line(
    buf: &str,
    icon: Option<&str>,
    caret_color: Color,
    placeholder: &str,
    placeholder_style: Style,
    blink_visible: bool,
    caret: Option<&'static str>,
) -> Line<'static> {
    let mut spans = Vec::new();
    if let Some(icon) = icon {
        spans.push(Span::raw(format!("{icon} ")));
    }
    if buf.is_empty() {
        if blink_visible {
            spans.push(Span::styled(placeholder.to_string(), placeholder_style));
        }
    } else {
        spans.push(Span::styled(buf.to_uppercase(), Style::default().add_modifier(Modifier::BOLD)));
        if let Some(c) = caret {
            spans.push(Span::styled(c, Style::default().fg(caret_color)));
        }
    }
    Line::from(spans)
}

fn header_item(state: &PickerState, gi: usize, width: u16, color: Color) -> ListItem<'static> {
    let g = &state.groups[gi];
    let icon = state.inbox_icon.as_str();
    let highlighted = state.mode == Mode::Groups && gi == state.group_cursor();
    if highlighted && state.group_editing() {
        return ListItem::new(group_edit_line(
            state.group_edit_buffer().unwrap_or(""),
            g.inbox.then_some(icon),
            color_from_name(&state.border_color),
            "GROUP NAME",
            Style::default().fg(Color::Gray),
            state.blink_visible(),
            caret_glyph(state),
        ));
    }
    // The post-`⇧J`/`⇧K` flash rides in the rule, replacing two of its cells
    // so the header's width never shifts as the marker comes and goes.
    let marker = swap_marker_glyph(state.group_swap_marker(&g.name), highlighted);
    let marker_width = if marker.is_some() { 2 } else { 0 };
    let rule_len = (width as usize).saturating_sub(group_label_width(g, true, icon) + 2 + marker_width);
    let mut spans = group_label_spans(g, true, color, icon);
    spans.push(Span::raw(" "));
    if let Some((glyph, marker_color)) = marker {
        spans.push(Span::styled(glyph, Style::default().fg(marker_color)));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled("─".repeat(rule_len), Style::default().fg(color)));
    ListItem::new(Line::from(spans))
}

/// The "{n} window(s)" count token for a session, singular for exactly one.
fn window_count(wins: usize) -> String {
    let label = if wins == 1 { "window" } else { "windows" };
    format!("{wins} {label}")
}

fn hidden_status(sessions: usize, windows: usize) -> String {
    let session_label = if sessions == 1 { "session" } else { "sessions" };
    if windows == 0 {
        return format!("{sessions} {session_label} hidden");
    }
    let window_label = if windows == 1 { "window" } else { "windows" };
    format!("{sessions} {session_label}, {windows} {window_label} hidden")
}

fn footer_rule(width: u16, state: &PickerState) -> String {
    let width = width as usize;
    if !state.focus_mode() {
        return "─".repeat(width);
    }
    let label = format!(
        "─ {} ",
        hidden_status(state.hidden_dormant_count(), state.hidden_dormant_window_count())
    );
    let label_width = label.chars().count();
    if label_width >= width {
        return label.chars().take(width).collect();
    }
    format!("{}{}", label, "─".repeat(width - label_width))
}

fn command_footer_hint(state: &PickerState) -> String {
    if state.creating() {
        return match state.create_stage() {
            Some(CreateStage::WindowName) => CREATE_WINDOW_FOOTER_HINT.to_string(),
            _ => CREATE_SESSION_FOOTER_HINT.to_string(),
        };
    }
    if state.mode == Mode::Groups {
        return ALTITUDE_FOOTER_HINT.to_string();
    }
    if let Some(warning) = state.pending_kill_warning() {
        return warning;
    }
    if let Some(warning) = state.pending_window_move_warning() {
        return warning.to_string();
    }
    if state.focus_mode() {
        FOOTER_HINT.replace("f foc", "f show")
    } else {
        FOOTER_HINT.to_string()
    }
}

fn search_footer_hint(state: &PickerState) -> String {
    if state.focus_mode() {
        SEARCH_FOOTER_HINT.replace("⌃f focus", "⌃f show")
    } else {
        SEARCH_FOOTER_HINT.to_string()
    }
}

/// Style a `"key desc · key desc"` hint line so each segment's leading key
/// token renders in `key_color` (the configured shortcut highlight color,
/// issue #106; Gray by default), a step brighter than its DarkGray
/// description, giving shortcut areas contrast against the rest of the dim
/// chrome (issue #86). Without Bold it reads as a gentle nudge rather than a
/// shout: an earlier version used Bold with the plain default fg and was too
/// bright. Shared by all four footer-hint render sites.
fn styled_hint(text: &str, key_color: Color) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, segment) in text.split(" · ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(DIM)));
        }
        match segment.split_once(' ') {
            Some((key, desc)) => {
                spans.push(Span::styled(key.to_string(), Style::default().fg(key_color)));
                spans.push(Span::styled(format!(" {desc}"), Style::default().fg(DIM)));
            }
            None => spans.push(Span::styled(segment.to_string(), Style::default().fg(DIM))),
        }
    }
    Line::from(spans)
}

/// The footer's key-shortcut legend, or -- when `always_show_shortcuts` is
/// false -- a minimal nudge naming the key that opens the full shortcuts
/// overlay (issue #156; previously this nudge was a per-popup reveal
/// toggle, issue #107).
fn shortcut_hint_line(state: &PickerState, text: &str) -> Line<'static> {
    if state.shortcuts_visible() {
        styled_hint(text, color_from_name(&state.shortcut_color))
    } else {
        Line::from(Span::styled("? shortcuts", Style::default().fg(DIM)))
    }
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
    /// capped so metadata never falls off the card. `has_gutter` reserves one
    /// extra leading column when the caller renders a colored gutter bar
    /// before the jump number (draw_command); draw_search renders no gutter
    /// and must not reserve space for one. `wide_numbering` reserves one more
    /// column for every row in this render (not just the affected ones) when
    /// any session reached the two-character `⌥N` jump label (11-20): that
    /// label has no trailing space of its own, unlike single-digit labels, so
    /// the attached-session dot would otherwise land jammed against the
    /// digit on that one row. Kept as a single render-wide flag rather than
    /// per-row width so this stays a uniform, alignment-safe column count,
    /// not per-row conditional math.
    fn compute<'a>(sessions: impl Iterator<Item = &'a Session>, width: u16, has_gutter: bool, wide_numbering: bool) -> Self {
        let gutter_width = if has_gutter { 1 } else { 0 };
        let numbering_pad = if wide_numbering { 1 } else { 0 };
        let mut longest_prefix = 0usize;
        let mut count_width = 0usize;
        for s in sessions {
            longest_prefix = longest_prefix.max(gutter_width + SESSION_PREFIX + numbering_pad + s.name.chars().count());
            count_width = count_width.max(window_count(s.windows.len()).chars().count());
        }
        let target = META_COL.max(longest_prefix + META_GAP);
        let cap = (width as usize).saturating_sub(META_BUDGET).max(META_COL);
        MetaLayout { col: target.min(cap), count_width }
    }
}

/// Two-character jump-number label for a session's gutter, given its
/// 1-based stable position. Slots 1-9 show the digit; slot 10 shows "0"
/// (previously unbound, now the 10th session); slots 11-20 show the macOS
/// Option-key glyph plus the digit that reaches them via Alt+digit
/// (Alt+1 = 11th session ... Alt+0 = 20th) — Alt itself has no printable
/// character to echo back, unlike the plain-digit case. Callers cap
/// `number` at 20 before it reaches here.
fn jump_label(n: usize) -> String {
    match n {
        1..=9 => format!("{n} "),
        10 => "0 ".to_string(),
        11..=19 => format!("⌥{}", n - 10),
        _ => "⌥0".to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn session_item(
    sess: &Session,
    expanded: bool,
    selected: bool,
    number: Option<usize>,
    meta: MetaLayout,
    dormant: bool,
    attached_color: Color,
    group_tag: Option<(String, Color, usize)>,
    gutter: Option<Color>,
    metric: SessionMetric,
    rename_buf: Option<&str>,
    dim_buf: bool,
    blink_visible: bool,
    swap_marker: Option<(SwapDirection, bool)>,
    wide_numbering: bool,
    caret: Option<&'static str>,
) -> ListItem<'static> {
    let glyph = if expanded { "▾" } else { "▸" };
    let num = match number { Some(n) => jump_label(n), None => "  ".to_string() };
    let name_style = if dormant {
        dormant_session(selected)
    } else if sess.attached {
        Style::default().fg(attached_color).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let dot = if sess.attached { "●" } else { " " };
    let dot_style = if dormant { dormant_session(selected) } else { Style::default().fg(attached_color) };
    if let Some(buf) = rename_buf {
        let mut spans = Vec::new();
        if let Some(color) = gutter {
            spans.push(Span::styled("│", Style::default().fg(color)));
        }
        spans.push(Span::styled(num, secondary(selected)));
        if wide_numbering {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(format!("{dot} "), dot_style));
        spans.push(Span::styled(format!("{glyph} "), secondary(selected)));
        if dim_buf {
            // No caret while empty -- the blinking placeholder text itself
            // signals "type here"; the caret only appears once real typed
            // text exists (see the `else` branch below).
            if blink_visible {
                spans.push(Span::styled(buf.to_string(), dormant_session(selected)));
            }
        } else {
            spans.push(Span::styled(buf.to_string(), Style::default().add_modifier(Modifier::BOLD)));
            if let Some(c) = caret {
                spans.push(Span::raw(c));
            }
        }
        return ListItem::new(Line::from(spans));
    }
    let gutter_width = if gutter.is_some() { 1 } else { 0 };
    let numbering_pad = if wide_numbering { 1 } else { 0 };
    let prefix_len = gutter_width + SESSION_PREFIX + numbering_pad + sess.name.chars().count(); // gutter + num + numbering-pad + dot-slot + "glyph " + name
    let pad = meta.col.saturating_sub(prefix_len);
    let count = window_count(sess.windows.len());
    let count_pad = meta.count_width.saturating_sub(count.chars().count());
    let age = session_metric_timestamp(sess, metric).map(activity_age);
    let mut spans = Vec::new();
    if let Some(color) = gutter {
        spans.push(Span::styled("│", Style::default().fg(color)));
    }
    spans.push(Span::styled(num, secondary(selected)));
    if wide_numbering {
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(format!("{dot} "), dot_style));
    spans.push(Span::styled(format!("{glyph} "), secondary(selected)));
    spans.push(Span::styled(sess.name.clone(), name_style));
    // The marker (if present) takes the *last* cell of the padding before
    // the shared metadata column instead of adding any extra width: `pad`
    // is normally >= META_GAP (2) by construction (see `MetaLayout::compute`),
    // so there's room for it without shifting anything else on this row.
    // In the already-degenerate case where a very long name forces the
    // shared column against `META_BUDGET`'s cap, `pad` can shrink toward 0
    // for that one row regardless of a marker (pre-existing squeeze); a
    // marker there just spends what little padding is left first.
    let marker = swap_marker_glyph(swap_marker, selected);
    let pad_before_marker = if marker.is_some() { pad.saturating_sub(1) } else { pad };
    spans.push(Span::styled(" ".repeat(pad_before_marker), secondary(selected)));
    if let Some((glyph, color)) = marker {
        spans.push(Span::styled(glyph, Style::default().fg(color)));
    }
    let meta_text = match &age {
        Some(age) => format!("{count}{} · {age}", " ".repeat(count_pad)),
        None => format!("{count}{}", " ".repeat(count_pad)),
    };
    spans.push(Span::styled(meta_text, secondary(selected)));
    // age_pad brings every tagged row's age up to the widest age string among
    // this render's tagged rows, so tags line up in a column even though ages
    // ("4h" vs "53m") naturally differ in width. Untagged rows (command mode,
    // residual search matches) never receive a pad, so their line is unchanged.
    if let Some((tag, color, age_pad)) = group_tag {
        spans.push(Span::styled(" ".repeat(age_pad), secondary(selected)));
        spans.push(Span::styled(" · ", secondary(selected)));
        spans.push(Span::styled(tag.to_uppercase(), Style::default().fg(color)));
    }
    ListItem::new(Line::from(spans))
}

#[allow(clippy::too_many_arguments)]
fn window_item(
    win: &Window,
    last: bool,
    selected: bool,
    gutter: Option<Color>,
    dormant: bool,
    individually_dormant: bool,
    rename_buf: Option<&str>,
    dot_color: Color,
    swap_marker: Option<(SwapDirection, bool)>,
    wide_numbering: bool,
    caret: Option<&'static str>,
) -> ListItem<'static> {
    // 4 leading spaces (not 2): the connector's tree glyph must land under the
    // parent session's expand glyph (issue #76), which itself shifted right by
    // 2 columns when the attached-session dot slot was added (issue #134). One
    // more (5) when `wide_numbering` is set: the session row itself gained a
    // render-wide extra spacer column there too (see `session_item`), so the
    // glyph shifted one column further right in that render.
    let connector = match (last, wide_numbering) {
        (false, false) => "    ├─",
        (true, false) => "    └─",
        (false, true) => "     ├─",
        (true, true) => "     └─",
    };
    let dot = if win.active { "●" } else { " " };
    let dot_style = if dormant { dormant_session(selected) } else { Style::default().fg(dot_color) };
    let base_name_style = if dormant { dormant_session(selected) } else { Style::default() };
    let name_style = if individually_dormant {
        base_name_style.add_modifier(Modifier::CROSSED_OUT)
    } else {
        base_name_style
    };
    if let Some(buf) = rename_buf {
        let mut spans = Vec::new();
        if let Some(color) = gutter {
            spans.push(Span::styled("│", Style::default().fg(color)));
        }
        spans.push(Span::styled(connector.to_string(), secondary(selected)));
        spans.push(Span::styled(format!("{dot} "), Style::default().fg(dot_color)));
        spans.push(Span::styled(buf.to_string(), Style::default().add_modifier(Modifier::BOLD)));
        if let Some(c) = caret {
            spans.push(Span::raw(c));
        }
        return ListItem::new(Line::from(spans));
    }
    let mut spans = Vec::new();
    if let Some(color) = gutter {
        spans.push(Span::styled("│", Style::default().fg(color)));
    }
    spans.push(Span::styled(connector.to_string(), secondary(selected)));
    spans.push(Span::styled(format!("{dot} "), dot_style));
    spans.push(Span::styled(win.name.clone(), name_style));
    if let Some((glyph, color)) = swap_marker_glyph(swap_marker, selected) {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(glyph, Style::default().fg(color)));
    }
    ListItem::new(Line::from(spans))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::model::{Action, CaretStyle, Group, KillTarget, PickerState, Session, Window, WindowMove};
    use crate::model::test_support::grouped_state;
    use crate::store::Config;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::cell::Cell;

    thread_local! {
        pub(super) static TEST_CLOCK: Cell<Option<i64>> = const { Cell::new(None) };
    }

    /// Freezes `now_unix()` at `now` for the lifetime of the returned guard,
    /// so a test's age-string assertions can't race the real wall clock.
    /// Restores the real clock on drop (including on assertion panic).
    struct ClockGuard;

    impl Drop for ClockGuard {
        fn drop(&mut self) {
            TEST_CLOCK.with(|c| c.set(None));
        }
    }

    fn freeze_clock(now: i64) -> ClockGuard {
        TEST_CLOCK.with(|c| c.set(Some(now)));
        ClockGuard
    }

    fn render_to_string(state: &PickerState) -> String {
        render_to_string_sized(state, 80, 20)
    }

    fn render_to_string_sized(state: &PickerState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
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

    fn find_text_x(buf: &ratatui::buffer::Buffer, y: u16, needle: &str) -> Option<u16> {
        let needle: Vec<String> = needle.chars().map(|c| c.to_string()).collect();
        let width = buf.area.width as usize;
        if needle.is_empty() || needle.len() > width {
            return None;
        }
        (0..=width - needle.len())
            .find(|x| needle.iter().enumerate().all(|(i, c)| buf[((*x + i) as u16, y)].symbol() == c))
            .map(|x| x as u16)
    }

    #[test]
    fn app_version_is_a_v_prefixed_string() {
        assert!(
            app_version().starts_with('v'),
            "expected a v-prefixed version string, got {:?}",
            app_version()
        );
    }

    #[test]
    fn draw_shows_headers_and_session_names() {
        let sessions = vec![
            Session { id: String::new(), name: "pr-review".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "scratch".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec![], groups: vec![Group { name: "PINNED".into(), members: vec!["pr-review".into()], color: String::new(), ..Default::default() }],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("rolomux"), "title present");
        assert!(text.contains("PINNED"), "pinned header present");
        assert!(text.contains("⊛ INBOX"), "inbox header present");
        assert!(text.contains("pr-review"), "pinned session present");
        assert!(text.contains("scratch"), "unpinned session present");
    }

    #[test]
    fn main_list_header_shows_glyph_for_the_inbox_group_only() {
        let sessions = vec![
            Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] },
            Session { id: String::new(), name: "b".into(), activity: 1, created: 2, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![
                Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() },
                Group { name: "INBOX".into(), members: vec!["b".into()], inbox: true, ..Default::default() },
            ],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("WORK"), "named group header present");
        assert!(!text.contains("⊛ WORK"), "no glyph on a named group");
        assert!(text.contains("⊛ INBOX"), "glyph precedes the inbox header");
    }

    #[test]
    fn group_mode_shows_glyph_on_the_inbox_row_and_no_separate_anchor_line() {
        let sessions = vec![
            Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_groups();
        let text = render_to_string(&state);
        assert!(text.contains("⊛ INBOX"), "synthesized inbox row carries the glyph");
        assert!(!text.contains("SESSIONS"), "no leftover hardcoded residual label");
    }

    #[test]
    fn inbox_glyph_is_not_bold_but_name_is() {
        let sessions = vec![
            Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] },
        ];
        let cfg = Config::default();
        let state = PickerState::build(sessions, &cfg);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut checked = false;
        for y in 0..buf.area.height {
            let mut glyph_x = None;
            let mut inbox_i_x = None;
            for x in 0..buf.area.width {
                let symbol = buf[(x, y)].symbol();
                if symbol == "⊛" {
                    glyph_x = Some(x);
                } else if glyph_x.is_some() && symbol == "I" {
                    inbox_i_x = Some(x);
                    break;
                }
            }
            if let (Some(glyph_x), Some(inbox_i_x)) = (glyph_x, inbox_i_x) {
                let glyph_style = buf[(glyph_x, y)].style();
                let name_style = buf[(inbox_i_x, y)].style();
                assert!(
                    !glyph_style.add_modifier.contains(Modifier::BOLD),
                    "inbox glyph must not be bold"
                );
                assert!(
                    name_style.add_modifier.contains(Modifier::BOLD),
                    "inbox name remains bold"
                );
                checked = true;
                break;
            }
        }
        assert!(checked, "inbox row was rendered");
    }

    #[test]
    fn inbox_header_renders_the_configured_icon_not_the_old_hardcoded_default() {
        let sessions = vec![
            Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] },
        ];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.inbox_icon = "♧".to_string();

        let text = render_to_string(&state);

        assert!(text.contains("♧ INBOX"), "configured icon should prefix the inbox header");
        assert!(!text.contains("⊛ INBOX"), "old hardcoded default should no longer appear");
    }

    #[test]
    fn draw_shows_create_group_hint_when_only_inbox_exists() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![] },
        ];
        let state = PickerState::build(sessions, &Config::default());
        let text = render_to_string(&state);
        assert!(text.contains("No groups yet"), "first-run group hint is visible");
        assert!(text.contains("g then n"), "hint tells the user how to create a group");
    }

    #[test]
    fn draw_shows_pending_window_move_warning_in_footer() {
        let sessions = vec![Session { id: String::new(),
            name: "work".into(), activity: 1, created: 1, attached: false,
            windows: vec![Window { id: String::new(), index: 0, name: "only".into(), active: true }],
        }];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.arm_window_move(
            WindowMove::SwapWithin { session: "work".into(), a_index: 0, b_index: 1 },
            -1,
        );

        let text = render_to_string(&st);
        assert!(text.contains("closes session"), "armed warning should replace the normal footer hint");
    }

    #[test]
    fn draw_shows_pending_window_move_warning_in_red_not_dim() {
        let sessions = vec![Session { id: String::new(),
            name: "work".into(), activity: 1, created: 1, attached: false,
            windows: vec![Window { id: String::new(), index: 0, name: "only".into(), active: true }],
        }];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.arm_window_move(
            WindowMove::SwapWithin { session: "work".into(), a_index: 0, b_index: 1 },
            -1,
        );

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut warning_red = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(x) = line.find("closes session") {
                warning_red = buf[(x as u16, y)].style().fg == Some(WARNING);
            }
        }
        assert!(warning_red, "armed warning should render in WARNING (red), not the dim footer color");
    }

    #[test]
    fn draw_shows_pending_kill_warning_in_footer() {
        let sessions = vec![Session { id: String::new(),
            name: "work".into(), activity: 1, created: 1, attached: false,
            windows: vec![Window { id: String::new(), index: 0, name: "only".into(), active: true }],
        }];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.arm_kill(KillTarget::Session("work".into()), false);

        let text = render_to_string(&st);
        assert!(text.contains("kill session 'work'"), "armed kill warning should replace the normal footer hint");
    }

    #[test]
    fn draw_shows_pending_kill_warning_in_red_not_dim() {
        let sessions = vec![Session { id: String::new(),
            name: "work".into(), activity: 1, created: 1, attached: false,
            windows: vec![Window { id: String::new(), index: 0, name: "only".into(), active: true }],
        }];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.arm_kill(KillTarget::Session("work".into()), false);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();

        // Mirrors draw_shows_pending_window_move_warning_in_red_not_dim exactly:
        // the whole warning line is one uniformly-styled Span, so a raw
        // byte-offset `.find()` is fine here (unlike the multi-style footer
        // hint line, which needs the column-safe `find_text_x` helper).
        let mut warning_red = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(x) = line.find("kill session") {
                warning_red = buf[(x as u16, y)].style().fg == Some(WARNING);
            }
        }
        assert!(warning_red, "kill warning should render in WARNING (red), not the dim footer color");
    }

    #[test]
    fn footer_hint_gains_x_kill_and_still_fits_84_columns() {
        assert!(FOOTER_HINT.contains("x kill"));
        assert!(FOOTER_HINT.chars().count() <= 78, "FOOTER_HINT must fit the 78-col footer at real 84-col popup width");
    }

    #[test]
    fn command_footer_swaps_foc_for_show_when_focus_mode_is_on() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] }];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        assert!(command_footer_hint(&state).contains("f foc"));
        state.toggle_focus_mode();
        assert!(command_footer_hint(&state).contains("f show"));
    }

    #[test]
    fn draw_hides_create_group_hint_once_a_user_group_exists() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["alpha".into()], ..Default::default() }],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(!text.contains("No groups yet"), "hint disappears after a user group exists");
    }

    #[test]
    fn border_and_title_use_the_configured_border_color() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false,
                                       windows: vec![] }];
        let cfg = Config { border_color: "magenta".to_string(), ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        // Top-left border corner, inset by POPUP_MARGIN.
        let corner = buf[(POPUP_MARGIN, POPUP_MARGIN)].style().fg;
        assert_eq!(corner, Some(Color::Magenta), "border picks up the configured color");
        let mut title_magenta = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(x) = line.find("rolomux") {
                title_magenta = buf[(x as u16, y)].style().fg == Some(Color::Magenta);
            }
        }
        assert!(title_magenta, "title text picks up the configured border color");
    }

    #[test]
    fn no_dim_rule_renders_between_title_and_first_group_header() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false,
                                       windows: vec![] }];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(84, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        // The row directly under the top border (which itself carries the
        // title) used to hold the DarkGray divider rule; it must now be blank.
        let chrome_row = POPUP_MARGIN + 1;
        let sample_x = POPUP_MARGIN + 3;
        assert_ne!(
            buf[(sample_x, chrome_row)].symbol(),
            "─",
            "the row under the title should be blank, not a divider rule"
        );
    }

    #[test]
    fn attached_session_name_uses_the_configured_attached_color() {
        let sessions = vec![Session { id: String::new(), name: "current".into(), activity: 1, created: 1, attached: true,
                                       windows: vec![] }];
        let cfg = Config { attached_color: "lightgreen".to_string(), ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let y = (0..buf.area.height).find(|&y| find_text_x(&buf, y, "current").is_some()).unwrap();
        let x = find_text_x(&buf, y, "current").unwrap();
        assert_eq!(buf[(x, y)].style().fg, Some(Color::LightGreen), "attached session name picks up the configured color");
    }

    #[test]
    fn attached_session_shows_a_dot_before_the_expand_arrow() {
        let sessions = vec![
            Session { id: String::new(), name: "current".into(), activity: 1, created: 1, attached: true,
                      windows: vec![] },
            Session { id: String::new(), name: "other".into(), activity: 1, created: 2, attached: false,
                      windows: vec![] },
        ];
        let cfg = Config { attached_color: "lightgreen".to_string(), ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let current_y = (0..buf.area.height).find(|&y| find_text_x(&buf, y, "current").is_some()).unwrap();
        let current_x = find_text_x(&buf, current_y, "current").unwrap();
        assert!(current_x >= 4, "expected room for gutter + number + dot slot + glyph before the name");
        let dot_cell = &buf[(current_x - 4, current_y)];
        assert_eq!(dot_cell.symbol(), "●", "attached session's row shows the dot 4 cols before its name");
        assert_eq!(dot_cell.style().fg, Some(Color::LightGreen), "dot uses the configured attached_color");

        let other_y = (0..buf.area.height).find(|&y| find_text_x(&buf, y, "other").is_some()).unwrap();
        let other_x = find_text_x(&buf, other_y, "other").unwrap();
        assert_eq!(buf[(other_x - 4, other_y)].symbol(), " ", "non-attached session's dot slot stays blank");
    }

    #[test]
    fn draw_marks_cursor_row_with_background() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
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
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
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
    fn command_footer_keeps_f_hint_in_place_when_focus_mode_is_on() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        let shown_hint = command_footer_hint(&state);

        state.toggle_focus_mode();
        let hidden_hint = command_footer_hint(&state);

        assert_eq!(shown_hint.find("f "), hidden_hint.find("f "));
        assert!(shown_hint.contains("f foc"));
        assert!(hidden_hint.contains("f show"));
    }

    #[test]
    fn search_footer_keeps_ctrl_f_hint_in_place_when_focus_mode_is_on() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        let shown_hint = search_footer_hint(&state);

        state.toggle_focus_mode();
        let hidden_hint = search_footer_hint(&state);

        assert_eq!(shown_hint.find("⌃f"), hidden_hint.find("⌃f"));
        assert!(shown_hint.contains("⌃f focus"));
        assert!(hidden_hint.contains("⌃f show"));
        // Neither ⌃w nor ⌃u should read as if Shift were required.
        assert!(shown_hint.contains("⌃w word"));
        assert!(shown_hint.contains("⌃u clear"));
    }

    #[test]
    fn draw_hides_dormant_sessions_and_shows_command_reminder() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_focus_mode();

        let text = render_to_string(&state);
        assert!(text.contains("alpha"), "active session remains visible");
        assert!(!text.contains("beta"), "dormant session is hidden");
        assert!(text.contains("1 session hidden"), "hidden count reminder is visible");
        assert!(text.contains("f show"), "footer shows how to restore dormant sessions");
    }

    #[test]
    fn draw_search_hides_dormant_sessions_and_shows_reminder() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "bravo".into(), activity: 10, created: 3, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into(), "bravo".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_focus_mode();
        state.enter_search();
        state.search_push('b');

        let text = render_to_string(&state);
        assert!(!text.contains("beta"), "matching dormant session is hidden from search");
        assert!(!text.contains("bravo"), "matching dormant session is hidden from search");
        assert!(text.contains("no matches"), "search reports no visible matches");
        assert!(text.contains("2 sessions hidden"), "search mode shows hidden count reminder");
    }

    #[test]
    fn hidden_status_omits_windows_clause_when_zero() {
        assert_eq!(hidden_status(3, 0), "3 sessions hidden");
        assert_eq!(hidden_status(1, 0), "1 session hidden");
    }

    #[test]
    fn hidden_status_includes_windows_clause_when_nonzero() {
        assert_eq!(hidden_status(3, 2), "3 sessions, 2 windows hidden");
        assert_eq!(hidden_status(1, 1), "1 session, 1 window hidden");
    }

    #[test]
    fn attached_dormant_session_stays_visible_and_excluded_from_hidden_count() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: true,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], dormant: vec!["alpha".into(), "beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_focus_mode();

        let text = render_to_string(&state);
        assert!(text.contains("alpha"), "attached session stays visible even though dormant");
        assert!(!text.contains("beta"), "non-attached dormant session is still hidden");
        assert!(text.contains("1 session hidden"), "hidden count excludes the visible attached session");
    }

    #[test]
    fn focus_mode_keeps_group_header_when_only_member_is_attached_dormant() {
        let sessions = vec![
            Session { id: String::new(), name: "work".into(), activity: 30, created: 1, attached: true,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![Group { name: "PROJECT".into(), members: vec!["work".into()], color: String::new(), ..Default::default() }],
            dormant: vec!["work".into()],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_focus_mode();

        let text = render_to_string(&state);
        assert!(text.contains("PROJECT"), "group header stays visible: its only member is the attached dormant session");
        assert!(text.contains("work"), "attached dormant session itself stays visible");
    }

    #[test]
    fn draw_dims_dormant_session_when_unselected_and_grays_it_when_selected() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg); // cursor starts on "alpha"

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut beta_dim = false;
        for y in 0..buf.area.height {
            if let Some(x) = find_text_x(&buf, y, "beta") {
                beta_dim = buf[(x, y)].style().fg == Some(Color::DarkGray);
            }
        }
        assert!(beta_dim, "unselected dormant session renders dim");

        state.focus_session("beta");
        let backend2 = TestBackend::new(80, 20);
        let mut terminal2 = Terminal::new(backend2).unwrap();
        terminal2.draw(|f| draw(f, &state)).unwrap();
        let buf2 = terminal2.backend().buffer().clone();
        let mut beta_selected_gray = false;
        for y in 0..buf2.area.height {
            if let Some(x) = find_text_x(&buf2, y, "beta") {
                let st = buf2[(x, y)].style();
                beta_selected_gray = st.bg == Some(Color::DarkGray) && st.fg == Some(Color::Gray);
            }
            for x in 0..buf2.area.width {
                let st = buf2[(x, y)].style();
                let invisible = st.bg == Some(Color::DarkGray) && st.fg == Some(Color::DarkGray);
                assert!(!invisible, "selected dormant row has DarkGray-on-DarkGray cell at x={x}, y={y}");
            }
        }
        assert!(beta_selected_gray, "selected dormant session renders gray on the highlight bar");
    }

    #[test]
    fn draw_dims_dormant_sessions_windows() {
        let sessions = vec![
            Session { id: String::new(), name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![
                          Window { id: String::new(), index: 0, name: "editor".into(), active: true },
                          Window { id: String::new(), index: 1, name: "shell".into(), active: false },
                      ] },
        ];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg); // cursor starts on "beta"
        state.expand();

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut name_dim = false;
        let mut dot_dim = false;
        for y in 0..buf.area.height {
            if let Some(x) = find_text_x(&buf, y, "editor") {
                name_dim = buf[(x, y)].style().fg == Some(Color::DarkGray);
                if x >= 2 {
                    dot_dim = buf[(x - 2, y)].style().fg == Some(Color::DarkGray);
                }
            }
        }
        assert!(name_dim, "dormant session's window name renders dim");
        assert!(dot_dim, "dormant session's active-window dot renders dim, not green");
    }

    #[test]
    fn window_row_shows_strikethrough_only_when_individually_dormant() {
        let sessions = vec![Session {
            id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false,
            windows: vec![
                Window { id: "@1".into(), index: 0, name: "keep".into(), active: true },
                Window { id: "@2".into(), index: 1, name: "hide".into(), active: false },
            ],
        }];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.expand();
        state.move_cursor(2); // land on window index 1
        state.toggle_dormant(); // individually dormant, session not dormant

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        // Locate each row's y by scanning rather than hardcoding a guessed
        // offset -- the exact row this lands on depends on how many
        // header/hint lines precede the session row (e.g. the "create
        // group" hint shown when there's no named group yet), which isn't
        // worth re-deriving by hand here.
        let hide_y = (0u16..20).find(|&y| find_text_x(&buf, y, "hide").is_some()).expect("hide row rendered");
        let hide_x = find_text_x(&buf, hide_y, "hide").unwrap();
        assert!(
            buf[(hide_x, hide_y)].modifier.contains(Modifier::CROSSED_OUT),
            "an individually-dormant window's name is struck through"
        );

        let keep_y = (0u16..20).find(|&y| find_text_x(&buf, y, "keep").is_some()).expect("keep row rendered");
        let keep_x = find_text_x(&buf, keep_y, "keep").unwrap();
        assert!(
            !buf[(keep_x, keep_y)].modifier.contains(Modifier::CROSSED_OUT),
            "a window that isn't individually dormant is never struck through"
        );
    }

    #[test]
    fn window_row_shows_dim_and_strikethrough_when_session_and_window_both_dormant() {
        let sessions = vec![Session {
            id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false,
            windows: vec![
                Window { id: "@1".into(), index: 0, name: "keep".into(), active: true },
                Window { id: "@2".into(), index: 1, name: "hide".into(), active: false },
            ],
        }];
        let cfg = Config { groups: vec![], dormant: vec!["a".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.expand();
        state.move_cursor(2); // land on window index 1
        state.toggle_dormant(); // individually dormant, session also dormant
        state.move_cursor(-1); // move off the row so selection highlight doesn't mask the dim color

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let hide_y = (0u16..20).find(|&y| find_text_x(&buf, y, "hide").is_some()).expect("hide row rendered");
        let hide_x = find_text_x(&buf, hide_y, "hide").unwrap();
        let cell = &buf[(hide_x, hide_y)];
        assert_eq!(
            cell.style().fg,
            Some(Color::DarkGray),
            "a window that's dormant via its dormant parent session still renders dim"
        );
        assert!(
            cell.modifier.contains(Modifier::CROSSED_OUT),
            "a window that's both session-dormant and individually dormant is still struck through"
        );
    }

    #[test]
    fn attached_dormant_session_dot_renders_dim_not_attached_color() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: true,
                      windows: vec![] },
        ];
        let cfg = Config { groups: vec![], dormant: vec!["alpha".into()], attached_color: "lightgreen".to_string(), ..Default::default() };
        let state = PickerState::build(sessions, &cfg); // cursor starts on "alpha"
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let y = (0..buf.area.height).find(|&y| find_text_x(&buf, y, "alpha").is_some()).unwrap();
        let x = find_text_x(&buf, y, "alpha").unwrap();
        let dot_cell = &buf[(x - 4, y)];
        assert_eq!(dot_cell.symbol(), "●", "attached session's dot still renders even though dormant");
        assert_ne!(dot_cell.style().fg, Some(Color::LightGreen), "dormant beats attached_color for the dot, same priority order the name style already uses");
    }

    #[test]
    fn attached_session_past_jump_number_ten_shows_a_gap_before_the_dot() {
        // issue #134: rows numbered 11+ use the two-character "⌥N" jump label,
        // which (unlike single-digit labels) has no trailing space of its
        // own; without a render-wide extra spacer column, the
        // attached-session dot would land jammed directly against the digit.
        let mut sessions: Vec<Session> = (1..=11)
            .map(|i| Session {
                id: String::new(),
                name: format!("sess{i:02}"),
                activity: i as i64,
                created: i as i64,
                attached: false,
                windows: vec![],
            })
            .collect();
        let last = sessions.len() - 1;
        sessions[last].attached = true;
        let cfg = Config::default();
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let y = (0..buf.area.height).find(|&y| find_text_x(&buf, y, "sess11").is_some()).unwrap();
        let x = find_text_x(&buf, y, "sess11").unwrap();
        assert_eq!(buf[(x - 4, y)].symbol(), "●", "dot still lands 4 cols before the name");
        assert_eq!(buf[(x - 5, y)].symbol(), " ", "a blank spacer separates the ⌥N jump label from the dot");
    }

    #[test]
    fn draw_expanded_window_name_indents_one_step_past_session_name_with_wide_numbering() {
        // issue #134: past 10 numbered sessions, the session row's jump-number
        // gutter grows one extra column (see `wide_numbering`), so the tree
        // connector must grow to match or the "one step past" invariant from
        // issue #76 breaks.
        let mut sessions: Vec<Session> = (1..=11)
            .map(|i| Session {
                id: String::new(),
                name: format!("sess{i:02}"),
                activity: i as i64,
                created: i as i64,
                attached: false,
                windows: vec![],
            })
            .collect();
        sessions[0].windows = vec![Window { id: String::new(), index: 0, name: "editor".into(), active: true }];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg); // cursor starts on "sess01"
        state.expand();

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let session_x = (0..buf.area.height).find_map(|y| find_text_x(&buf, y, "sess01")).unwrap();
        let window_x = (0..buf.area.height).find_map(|y| find_text_x(&buf, y, "editor")).unwrap();
        assert_eq!(window_x, session_x + 2, "window name should indent one step past its parent session's name even with wide numbering active");
    }

    #[test]
    fn selected_attached_dormant_session_uses_dormant_gray() {
        let sessions = vec![
            Session { id: String::new(), name: "current".into(), activity: 30, created: 1, attached: true,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![],
            dormant: vec!["current".into()],
            attached_color: "lightgreen".to_string(),
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut checked = false;
        for y in 0..buf.area.height {
            if let Some(x) = find_text_x(&buf, y, "current") {
                let fg = buf[(x, y)].style().fg;
                assert_eq!(fg, Some(Color::Gray), "dormant status overrides attached color");
                assert_ne!(fg, Some(Color::LightGreen), "attached color would hide the dormant cue");
                checked = true;
            }
        }
        assert!(checked, "current session row was rendered");
    }

    #[test]
    fn draw_shows_dormant_footer_hint() {
        let sessions = vec![
            Session { id: String::new(), name: "main".into(), activity: 100, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("dim"), "footer hint: dim present");
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
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![Group { name: "G".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() }], ..Default::default() };
        let text = render_to_string(&PickerState::build(sessions, &cfg));
        assert!(!text.contains('★'), "pin star retired");
    }

    #[test]
    fn draw_shows_multiple_group_headers_in_order() {
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "tent".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "ticket".into(), activity: 10, created: 3, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "config".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() },
                Group { name: "tools".into(), members: vec!["tent".into()], color: String::new(), ..Default::default() },
            ],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("CONFIG"), "group name uppercased");
        assert!(text.contains("TOOLS"));
        assert!(text.contains("⊛ INBOX"));
        let (c, t, s) = (text.find("CONFIG"), text.find("TOOLS"), text.find("⊛ INBOX"));
        assert!(c < t && t < s, "sections render top-to-bottom");
    }

    #[test]
    fn draw_shows_empty_group_header_dimmed_in_session_mode() {
        // A named group with no members must still render its header (a shelf to
        // fill), grayed out, and it sits in config order between the group above
        // and the inbox below.
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "loose".into(), activity: 10, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "config".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() },
                Group { name: "tools".into(), members: vec![], color: String::new(), ..Default::default() }, // empty shelf
            ],
            ..Default::default()
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
        // Order: CONFIG (populated) then TOOLS (empty) then the inbox.
        let (c, t, s) = (text.find("CONFIG"), text.find("TOOLS"), text.find("⊛ INBOX"));
        assert!(c < t && t < s, "empty group header sits in order: got {c:?} {t:?} {s:?}");
        assert!(tools_dim, "empty group header renders dimmed (gray)");
        assert!(config_accent, "populated group header keeps the accent color");
    }

    #[test]
    fn focus_mode_hides_group_header_when_all_members_are_dormant() {
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "old-deploy".into(), activity: 10, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec!["old-deploy".into()],
            groups: vec![
                Group { name: "config".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() },
                Group { name: "deploys".into(), members: vec!["old-deploy".into()], color: String::new(), ..Default::default() },
            ],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_focus_mode();

        let text = render_to_string(&state);
        assert!(text.contains("CONFIG"), "group with a visible session still shows");
        assert!(!text.contains("DEPLOYS"), "group whose only member is now dormant is hidden in focus mode");
    }

    #[test]
    fn focus_mode_hides_truly_empty_group_header() {
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec![],
            groups: vec![
                Group { name: "config".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() },
                Group { name: "tools".into(), members: vec![], color: String::new(), ..Default::default() },
            ],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);

        let off_text = render_to_string(&state);
        assert!(off_text.contains("TOOLS"), "focus mode off: empty group still shows dimmed, as before");

        state.toggle_focus_mode();
        let on_text = render_to_string(&state);
        assert!(!on_text.contains("TOOLS"), "focus mode on: genuinely empty group is hidden too");

        state.enter_groups();
        let raised_text = render_to_string(&state);
        assert!(raised_text.contains("TOOLS"), "raised: focus mode suspends hiding so empty groups are reachable");

        state.exit_groups();
        let descended_text = render_to_string(&state);
        assert!(!descended_text.contains("TOOLS"), "descended: focus mode hides the empty group again");
    }

    #[test]
    fn focus_mode_raised_highlight_lands_on_a_focus_hidden_empty_group() {
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec![],
            groups: vec![
                Group { name: "config".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() },
                Group { name: "tools".into(), members: vec![], color: String::new(), ..Default::default() },
            ],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_focus_mode();
        state.enter_groups();
        assert_eq!(state.group_cursor(), 0, "enter_groups snaps to claude's group, config");
        state.group_move_cursor(1); // onto "tools", empty and focus-hidden at session altitude

        let buf = render_buf(&state);
        let tools_y = row_of(&buf, "TOOLS");
        for x in card_interior(&buf) {
            assert_eq!(
                buf[(x, tools_y)].style().bg,
                Some(SEL_BG),
                "the focus-hidden empty group still gets a real selection bar while raised (x={x})"
            );
        }
    }

    #[test]
    fn focus_mode_hides_inbox_header_when_it_has_no_visible_sessions() {
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "loose".into(), activity: 10, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec!["loose".into()],
            groups: vec![
                Group { name: "config".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() },
                Group { name: "INBOX".into(), members: vec![], inbox: true, ..Default::default() },
            ],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_focus_mode();

        let text = render_to_string(&state);
        assert!(text.contains("CONFIG"));
        assert!(!text.contains("INBOX"), "inbox with zero visible sessions is hidden in focus mode too");
    }

    #[test]
    fn draw_command_shows_inline_rename_field() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut st = PickerState::build(sessions, &cfg);
        st.start_rename();
        st.rename_edit_clear();
        for c in "renamed".chars() { st.rename_edit_push(c); }
        let text = render_to_string(&st);
        assert!(text.contains("renamed"), "inline rename buffer visible");
        assert!(!text.contains("alpha"), "old name no longer shown while editing");
    }

    #[test]
    fn caret_glyph_returns_the_configured_style_when_blink_is_off() {
        let mut st = grouped_state();
        st.caret_style = CaretStyle::Underline;
        st.caret_blink = false;
        assert_eq!(caret_glyph(&st), Some("_"));
    }

    #[test]
    fn caret_glyph_blinks_with_the_shared_clock_when_blink_is_on() {
        let mut st = grouped_state();
        st.caret_style = CaretStyle::Block;
        st.caret_blink = true;
        assert_eq!(caret_glyph(&st), Some("█"), "visible immediately after construction");
        st.backdate_blink(std::time::Duration::from_millis(600));
        assert_eq!(caret_glyph(&st), None, "hidden during the blink clock's off half");
    }

    #[test]
    fn draw_command_renders_underline_caret_for_an_in_flight_session_rename() {
        let mut st = grouped_state();
        st.caret_style = CaretStyle::Underline;
        st.start_rename();
        for c in "renamed".chars() {
            st.rename_edit_push(c);
        }
        let text = render_to_string(&st);
        assert!(text.contains("renamed_"), "renamed session shows the underline caret directly after the typed name");
    }

    #[test]
    fn draw_command_hides_caret_mid_blink_when_caret_blink_is_on() {
        let mut st = grouped_state();
        st.caret_style = CaretStyle::Block;
        st.caret_blink = true;
        st.start_rename();
        for c in "renamed".chars() {
            st.rename_edit_push(c);
        }
        st.backdate_blink(std::time::Duration::from_millis(600));
        let text = render_to_string(&st);
        assert!(!text.contains('█'), "caret hidden during the blink clock's off half");
        assert!(text.contains("renamed"), "the typed name itself is unaffected by the caret blink");
    }

    #[test]
    fn draw_command_shows_phantom_group_row_above_target_group_header() {
        let mut st = grouped_state(); // G1=[a], G2=[b], INBOX=[c]
        st.focus_session("b");
        st.start_quick_create();
        for c in "new".chars() { st.quick_create_push(c); }

        let text = render_to_string(&st);
        let lines: Vec<&str> = text.lines().collect();
        let g1_pos = lines.iter().position(|l| l.contains("G1")).expect("G1 header present");
        let phantom_pos = lines.iter().position(|l| l.contains("NEW") && l.contains('▏')).expect("phantom row visible");
        let g2_pos = lines.iter().position(|l| l.contains("G2")).expect("G2 header present");
        assert!(g1_pos < phantom_pos, "phantom row sits below G1's block");
        assert!(phantom_pos < g2_pos, "phantom row sits directly above G2's header");

        st.cancel_quick_create();
        let text_after_cancel = render_to_string(&st);
        assert!(!text_after_cancel.contains('▏'), "phantom row gone after cancel");
        assert!(text_after_cancel.contains("G2"), "G2 header still renders normally after cancel");
    }

    #[test]
    fn draw_command_quick_create_moves_the_selection_bar_off_the_anchor_session() {
        let mut st = grouped_state(); // G1=[a], G2=[b], INBOX=[c]
        st.focus_session("b");
        st.start_quick_create(); // buffer starts empty; selection bar moves onto the phantom row

        let buf = render_buf(&st);
        let placeholder_y = row_of(&buf, "NAME");
        let x = find_text_x(&buf, placeholder_y, "NAME").unwrap();
        assert_eq!(buf[(x, placeholder_y)].style().bg, Some(SEL_BG), "quick-create phantom row now carries the selection bar");
        assert_eq!(buf[(x, placeholder_y)].style().fg, Some(Color::Gray), "placeholder must be Gray, not DarkGray-on-DarkGray");

        let anchor_y = row_of(&buf, "▸ b");
        let anchor_x = find_text_x(&buf, anchor_y, "▸ b").unwrap();
        assert_ne!(
            buf[(anchor_x, anchor_y)].style().bg,
            Some(SEL_BG),
            "anchor session no longer carries the selection bar while its new group is being named"
        );
    }

    #[test]
    fn draw_command_shows_create_phantom_row_above_selected_session() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![] },
            Session { id: String::new(), name: "bravo".into(), activity: 1, created: 2, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![
                Group { name: "G1".into(), members: vec!["alpha".into()], ..Default::default() },
                Group { name: "G2".into(), members: vec!["bravo".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("bravo");
        st.start_create_here();
        for c in "charlie".chars() { st.create_push(c); }

        let text = render_to_string(&st);
        let lines: Vec<&str> = text.lines().collect();
        let g2_pos = lines.iter().position(|l| l.contains("G2")).expect("G2 header present");
        let phantom_pos = lines.iter().position(|l| l.contains("charlie") && l.contains('▏')).expect("phantom row visible");
        let bravo_pos = lines.iter().position(|l| l.contains("bravo")).expect("bravo row present");
        assert!(g2_pos < phantom_pos, "phantom sits within G2's block, below its header");
        assert_eq!(bravo_pos, phantom_pos + 1, "phantom sits directly above the selected session, no gap");

        st.create_cancel();
        let text_after_cancel = render_to_string(&st);
        assert!(!text_after_cancel.contains('▏'), "phantom row gone after cancel");
        assert!(text_after_cancel.contains("bravo"), "bravo row still renders normally after cancel");
    }

    #[test]
    fn draw_command_create_phantom_row_placeholder_is_visible_against_the_selection_bar() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![] },
        ];
        let cfg = Config { groups: vec![Group { name: "G1".into(), members: vec!["alpha".into()], ..Default::default() }], ..Default::default() };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("alpha");
        st.start_create_here(); // buf empty: placeholder row inherits alpha's old selection bar

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let phantom_y = (0..buf.area.height)
            .find(|&y| find_text_x(&buf, y, "name").is_some())
            .expect("placeholder row visible");
        let x = find_text_x(&buf, phantom_y, "name").unwrap();
        assert_eq!(buf[(x, phantom_y)].style().bg, Some(SEL_BG), "precondition: placeholder sits on the selection bar");
        assert_eq!(buf[(x, phantom_y)].style().fg, Some(Color::Gray), "placeholder must be Gray, not DarkGray-on-DarkGray");
    }

    #[test]
    fn draw_command_create_phantom_row_above_parent_session_when_window_row_focused() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![Group { name: "G1".into(), members: vec!["alpha".into()], ..Default::default() }], ..Default::default() };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("alpha");
        st.expand();
        st.move_cursor(1); // onto alpha's window row
        assert!(matches!(st.visible_rows()[st.cursor], Row::Window(_, _)), "precondition: cursor on a window row");

        st.start_create_here();
        for c in "charlie".chars() { st.create_push(c); }

        let text = render_to_string(&st);
        let lines: Vec<&str> = text.lines().collect();
        let phantom_pos = lines.iter().position(|l| l.contains("charlie") && l.contains('▏')).expect("phantom row visible");
        let alpha_pos = lines.iter().position(|l| l.contains("alpha")).expect("alpha row present");
        assert!(phantom_pos < alpha_pos, "phantom sits above the parent session row, not the window row");
    }

    #[test]
    fn draw_command_shows_create_phantom_row_at_end_of_target_group_in_group_mode() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![] },
            Session { id: String::new(), name: "bravo".into(), activity: 1, created: 2, attached: false, windows: vec![] },
            Session { id: String::new(), name: "charlie".into(), activity: 1, created: 3, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![
                Group { name: "G1".into(), members: vec!["alpha".into()], ..Default::default() },
                Group { name: "G2".into(), members: vec!["bravo".into(), "charlie".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.enter_groups();
        st.group_move_cursor(1); // highlight G2
        st.start_create_in_group();
        for c in "delta".chars() { st.create_push(c); }

        let text = render_to_string(&st);
        let lines: Vec<&str> = text.lines().collect();
        let charlie_pos = lines.iter().position(|l| l.contains("charlie")).expect("charlie row present");
        let phantom_pos = lines.iter().position(|l| l.contains("delta") && l.contains('▏')).expect("phantom row visible");
        assert_eq!(phantom_pos, charlie_pos + 1, "phantom sits directly after G2's last member");
    }

    #[test]
    fn draw_command_shows_create_phantom_row_in_empty_target_group() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![
                Group { name: "G1".into(), members: vec!["alpha".into()], ..Default::default() },
                Group { name: "EMPTY".into(), members: vec![], ..Default::default() },
            ],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.enter_groups();
        st.group_move_cursor(1); // highlight EMPTY
        assert_eq!(st.groups[st.group_cursor()].name, "EMPTY");
        st.start_create_in_group();
        for c in "seed".chars() { st.create_push(c); }

        let text = render_to_string(&st);
        let lines: Vec<&str> = text.lines().collect();
        let empty_pos = lines.iter().position(|l| l.contains("EMPTY")).expect("EMPTY header present");
        let phantom_pos = lines.iter().position(|l| l.contains("seed") && l.contains('▏')).expect("phantom row visible");
        assert!(empty_pos < phantom_pos, "phantom sits after the empty group's own header, its only slot");
    }

    #[test]
    fn draw_command_shows_create_phantom_row_at_inbox_end_on_empty_picker() {
        let cfg = Config::default();
        let mut st = PickerState::build(Vec::new(), &cfg);
        st.start_create_here();
        for c in "solo".chars() { st.create_push(c); }

        let text = render_to_string(&st);
        let lines: Vec<&str> = text.lines().collect();
        let inbox_pos = lines.iter().position(|l| l.contains("INBOX")).expect("inbox header present");
        let phantom_pos = lines.iter().position(|l| l.contains("solo") && l.contains('▏')).expect("phantom row visible");
        assert!(inbox_pos < phantom_pos, "phantom sits after the inbox's own header, its only slot on an empty picker");

        st.create_cancel();
        let text_after_cancel = render_to_string(&st);
        assert!(!text_after_cancel.contains('▏'), "phantom row gone after cancel");
        assert!(text_after_cancel.contains("INBOX"), "inbox header still renders normally after cancel");
    }

    #[test]
    fn draw_command_footer_shows_create_hint_over_altitude_hint_in_group_mode() {
        let mut st = grouped_state();
        st.enter_groups();
        st.start_create_in_group();

        let text = render_to_string(&st);
        assert!(text.contains("Enter next"), "create-stage hint wins over the altitude hint while creating in group mode");
        assert!(!text.contains("Enter open"), "altitude hint suppressed while creating");
        assert!(text.contains("session name"), "empty session-name buffer shows a 'session name' placeholder");
    }

    #[test]
    fn draw_command_footer_shows_create_stage_hints_and_clears_after_cancel() {
        let mut st = grouped_state();
        st.focus_session("a");
        st.start_create_here();
        let text = render_to_string(&st);
        assert!(text.contains("Enter next"), "SessionName stage hint shown");
        assert!(text.contains("session name"), "empty session-name buffer shows a 'session name' placeholder");

        for c in "new".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None, "advances to WindowName stage");
        let text2 = render_to_string(&st);
        assert!(text2.contains("Enter skip/create"), "WindowName stage hint shown after advancing");
        assert!(text2.contains("window name"), "empty window-name buffer shows a 'window name' placeholder");

        st.create_cancel();
        let text3 = render_to_string(&st);
        assert!(!text3.contains("Enter skip/create"), "stage hint gone after cancel");
        assert!(!text3.contains('▏'), "phantom row gone after cancel");
    }

    #[test]
    fn draw_shows_footer_hints() {
        let sessions = vec![
            Session { id: String::new(), name: "main".into(), activity: 100, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        // tmux popup is 84 columns wide; test needs 84 to avoid footer truncation
        let text = render_to_string_sized(&state, 84, 20);
        assert!(text.contains("search"), "footer hint: search present");
        assert!(text.contains("grp"), "footer hint: grp present");
        assert!(text.contains("cfg"), "footer hint: cfg present");
        assert!(text.contains("kill"), "footer hint: kill present");
        assert!(text.contains("quit"), "footer hint: quit present");
        assert!(text.contains("rename"), "footer hint: rename present");
        assert!(!text.contains("1-9"), "footer no longer spends space on number shortcuts");
    }

    #[test]
    fn draw_footer_hint_fits_within_real_popup_width_untruncated() {
        let sessions = vec![
            Session { id: String::new(), name: "main".into(), activity: 100, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        // 84 columns matches the real tmux popup width this app is bound at (AGENTS.md).
        let text = render_to_string_sized(&state, 84, 20);
        assert!(
            text.contains(FOOTER_HINT),
            "footer hint must render untruncated at the real 84-column popup width"
        );
    }

    #[test]
    fn styled_hint_brightens_key_and_dims_description() {
        let line = styled_hint("r rename · Esc back", Color::Gray);
        let spans: Vec<(String, Style)> =
            line.spans.iter().map(|s| (s.content.to_string(), s.style)).collect();
        assert_eq!(
            spans,
            vec![
                ("r".to_string(), Style::default().fg(Color::Gray)),
                (" rename".to_string(), Style::default().fg(DIM)),
                (" · ".to_string(), Style::default().fg(DIM)),
                ("Esc".to_string(), Style::default().fg(Color::Gray)),
                (" back".to_string(), Style::default().fg(DIM)),
            ]
        );
    }

    #[test]
    fn command_footer_hint_brightens_key_tokens_in_the_real_render() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false,
                                       windows: vec![] }];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(84, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut checked = false;
        // find_text_x (not a raw `String::find` over concatenated symbols)
        // because the footer line contains multi-byte glyphs ("│", "·")
        // before "rename"; a byte offset would overshoot the real column.
        for y in 0..buf.area.height {
            if let Some(x) = find_text_x(&buf, y, "rename") {
                let key_style = buf[(x - 2, y)].style(); // the "r" in "r rename"
                assert_eq!(key_style.fg, Some(Color::Gray), "key token renders in Gray, not dim");
                assert!(!key_style.add_modifier.contains(Modifier::BOLD), "key token is not bold");
                let desc_style = buf[(x, y)].style();
                assert_eq!(desc_style.fg, Some(Color::DarkGray), "description stays dim");
                checked = true;
            }
        }
        assert!(checked, "expected to find the rename hint in the command-mode footer");
    }

    // Guards against a future edit reverting one footer call site back to a
    // raw `Style::default().fg(DIM)` span, bypassing `styled_hint` -- while
    // command mode already gets a full draw()-level assertion above, the
    // other three modes all end their hint in "Esc back", so one shared
    // helper is enough to catch that regression without three near-duplicate
    // copies of the same render-and-inspect test.
    fn assert_footer_key_is_styled(buf: &ratatui::buffer::Buffer) {
        let mut checked = false;
        for y in 0..buf.area.height {
            if let Some(x) = find_text_x(buf, y, "back") {
                let key_style = buf[(x - 4, y)].style(); // the "E" in "Esc back"
                assert_eq!(key_style.fg, Some(Color::Gray), "key token renders in Gray, not dim");
                assert!(!key_style.add_modifier.contains(Modifier::BOLD), "key token is not bold");
                checked = true;
            }
        }
        assert!(checked, "expected to find the 'Esc back' hint in the footer");
    }

    #[test]
    fn search_footer_hint_uses_styled_hint() {
        let state = searching_state("pr");
        let backend = TestBackend::new(84, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        assert_footer_key_is_styled(terminal.backend().buffer());
    }

    #[test]
    fn settings_footer_hint_uses_styled_hint() {
        let state = settings_view();
        let backend = TestBackend::new(84, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        assert_footer_key_is_styled(terminal.backend().buffer());
    }

    #[test]
    fn draw_numbers_sessions_in_left_gutter() {
        let sessions = vec![
            Session { id: String::new(), name: "main".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "other".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
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
                assert!(line.starts_with("│1 "), "main row gutter: got {line:?}");
            }
            if line.contains("other") {
                assert!(line.starts_with("│2 "), "other row gutter: got {line:?}");
            }
        }
    }

    #[test]
    fn draw_numbers_extend_past_nine_with_alt_glyph() {
        let sessions: Vec<Session> = (1..=12)
            .map(|i| Session { id: String::new(),
                name: format!("s{i}"),
                activity: 0,
                created: i as i64,
                attached: false,
                windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }],
            })
            .collect();
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg); // created-ascending: s1 = #1 ... s12 = #12

        let backend = TestBackend::new(60, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let inner_line = |y: u16| -> String {
            ((POPUP_MARGIN + 1)..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
        };

        let mut saw_s10 = false;
        let mut saw_s11 = false;
        for y in 0..buf.area.height {
            let line = inner_line(y);
            if line.contains("s10") {
                assert!(line.starts_with("│0 "), "10th session shows '0': got {line:?}");
                saw_s10 = true;
            }
            if line.contains("s11") {
                assert!(line.starts_with("│⌥1"), "11th session shows the Alt-glyph label: got {line:?}");
                saw_s11 = true;
            }
        }
        assert!(saw_s10 && saw_s11, "both boundary rows must be visible in the test viewport");
    }

    #[test]
    fn jump_label_covers_both_decades() {
        assert_eq!(jump_label(1), "1 ");
        assert_eq!(jump_label(9), "9 ");
        assert_eq!(jump_label(10), "0 ");
        assert_eq!(jump_label(11), "⌥1");
        assert_eq!(jump_label(19), "⌥9");
        assert_eq!(jump_label(20), "⌥0");
    }

    #[test]
    fn draw_skips_dormant_jump_numbers_when_configured() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "gamma".into(), activity: 10, created: 3, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![],
            dormant: vec!["beta".into()],
            number_dormant_sessions: false,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let inner_line = |y: u16| -> String {
            ((POPUP_MARGIN + 1)..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
        };

        for y in 0..buf.area.height {
            let line = inner_line(y);
            if line.contains("alpha") {
                assert!(line.starts_with("│1 "), "alpha row gutter: got {line:?}");
            }
            if line.contains("beta") {
                assert!(line.starts_with("│    ▸ beta"), "dormant beta has no jump number: got {line:?}");
            }
            if line.contains("gamma") {
                assert!(line.starts_with("│2 "), "gamma closes the numbering gap: got {line:?}");
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
            Session { id: String::new(), name: "a-very-long-session-name-here".into(), activity: 30, created: 1,
                      attached: false, windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "short".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
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
            .map(|i| Window { id: String::new(), index: i, name: "w".into(), active: i == 0 })
            .collect();
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: many },
            Session { id: String::new(), name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
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
    fn session_swap_marker_renders_only_the_moved_rows_arrow() {
        let sessions = vec![
            Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] },
            Session { id: String::new(), name: "b".into(), activity: 1, created: 2, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![
                Group { name: "OTHER".into(), members: vec![], ..Default::default() },
                Group { name: "ONLY".into(), members: vec!["a".into(), "b".into()], inbox: true, ..Default::default() },
            ],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("b");
        state.move_row(-1); // b moves up past a

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let row_of = |name: &str| -> u16 {
            (0..buf.area.height)
                .find(|&y| find_text_x(&buf, y, name).is_some())
                .unwrap_or_else(|| panic!("no row contains {name:?}"))
        };
        let b_y = row_of("b");
        let a_y = row_of("a");
        let up_x = find_text_x(&buf, b_y, "▲").expect("up arrow on b's (moved) row");
        assert!(
            find_text_x(&buf, a_y, "▲").is_none() && find_text_x(&buf, a_y, "▼").is_none(),
            "a's (bumped) row gets no marker"
        );
        assert_eq!(buf[(up_x, b_y)].style().fg, Some(Color::Yellow));
    }

    #[test]
    fn session_swap_marker_dim_stage_uses_gray_on_the_selected_row_not_darkgray() {
        let sessions = vec![
            Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] },
            Session { id: String::new(), name: "b".into(), activity: 1, created: 2, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![
                Group { name: "OTHER".into(), members: vec![], ..Default::default() },
                Group { name: "ONLY".into(), members: vec!["a".into(), "b".into()], inbox: true, ..Default::default() },
            ],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("b");
        state.move_row(-1); // b (selected) moves up past a
        state.backdate_swap_indicator(std::time::Duration::from_millis(300)); // past the bright stage

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let row_of = |name: &str| -> u16 {
            (0..buf.area.height)
                .find(|&y| find_text_x(&buf, y, name).is_some())
                .unwrap_or_else(|| panic!("no row contains {name:?}"))
        };
        let b_y = row_of("b"); // selected: cursor followed the moved session
        let a_y = row_of("a"); // unselected neighbor
        let up_x = find_text_x(&buf, b_y, "▲").expect("up arrow on b's row");
        assert!(
            find_text_x(&buf, a_y, "▲").is_none() && find_text_x(&buf, a_y, "▼").is_none(),
            "a's row gets no marker"
        );
        assert_eq!(buf[(up_x, b_y)].style().fg, Some(Color::Gray), "selected row: Gray, not DarkGray-on-DarkGray");
    }

    #[test]
    fn window_swap_marker_renders_only_the_moved_windows_arrow() {
        let sessions = vec![Session {
            id: String::new(), name: "work".into(), activity: 1, created: 1, attached: false,
            windows: vec![
                Window { id: String::new(), index: 0, name: "alpha".into(), active: true },
                Window { id: String::new(), index: 1, name: "beta".into(), active: false },
            ],
        }];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.expand();
        state.set_window_swap("work", 0, -1); // simulate: "alpha" (now at 0) moved up

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let row_of = |name: &str| -> u16 {
            (0..buf.area.height)
                .find(|&y| find_text_x(&buf, y, name).is_some())
                .unwrap_or_else(|| panic!("no row contains {name:?}"))
        };
        assert!(find_text_x(&buf, row_of("alpha"), "▲").is_some(), "window alpha flashes up");
        let beta_y = row_of("beta");
        assert!(
            find_text_x(&buf, beta_y, "▲").is_none() && find_text_x(&buf, beta_y, "▼").is_none(),
            "window beta (bumped) gets no marker"
        );
    }

    #[test]
    fn session_swap_marker_lands_on_the_stable_metadata_column_not_the_far_right_edge() {
        // Task 8: the marker splices into the padding just before the shared
        // metadata column (`MetaLayout::col`) instead of right-aligning to the
        // card width, so its x-coordinate must be identical whether the
        // swapped session's own name is very long or very short -- it must
        // land wherever the metadata column already sits for every row.
        let sessions = vec![
            Session {
                id: String::new(),
                name: "a-very-long-session-name-for-marker-alignment".into(),
                activity: 1,
                created: 1,
                attached: false,
                windows: vec![],
            },
            Session { id: String::new(), name: "b".into(), activity: 1, created: 2, attached: false, windows: vec![] },
            Session { id: String::new(), name: "control".into(), activity: 1, created: 3, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![Group {
                name: "ONLY".into(),
                members: vec![
                    "a-very-long-session-name-for-marker-alignment".into(),
                    "b".into(),
                    "control".into(),
                ],
                inbox: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("b");
        state.move_row(-1); // b moves up past the long-named session; control is untouched

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let row_of = |name: &str| -> u16 {
            (0..buf.area.height)
                .find(|&y| find_text_x(&buf, y, name).is_some())
                .unwrap_or_else(|| panic!("no row contains {name:?}"))
        };
        let long_y = row_of("a-very-long-session-name-for-marker-alignment");
        let short_y = row_of("b");
        let control_y = row_of("control");

        let up_x = find_text_x(&buf, short_y, "▲").expect("up arrow on b's (moved) row");
        assert!(
            find_text_x(&buf, long_y, "▲").is_none() && find_text_x(&buf, long_y, "▼").is_none(),
            "the long-named (bumped) row gets no marker"
        );

        // `control` never moved, so its row carries no marker and its
        // window-count digit marks exactly where the shared metadata column
        // begins on every row.
        let control_name_end = find_text_x(&buf, control_y, "control").unwrap() + "control".chars().count() as u16;
        let meta_col_x = (control_name_end..buf.area.width)
            .find(|&x| buf[(x, control_y)].symbol().chars().next().is_some_and(|c| c.is_ascii_digit()))
            .expect("control row shows the window-count digit");

        assert_eq!(up_x, meta_col_x - 1, "short name's marker must sit one cell before the shared metadata column");
    }

    #[test]
    fn metadata_stays_at_default_column_for_short_names() {
        // With only short names, the shared column collapses back to META_COL,
        // preserving the original compact layout.
        let sessions = vec![
            Session { id: String::new(), name: "main".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "other".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
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
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
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

    fn searching_state(query: &str) -> PickerState {
        let sessions = vec![
            Session { id: String::new(), name: "pr-review".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "scratch".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec![], groups: vec![Group { name: "PINNED".into(), members: vec!["pr-review".into()], color: String::new(), ..Default::default() }],
            ..Default::default()
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
        assert!(!text.contains("PINNED ─"), "no PINNED section header (rule) in search");
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
    fn draw_search_shows_grouped_sessions_tag_in_group_color() {
        // Lowercase group name so the assertion actually exercises
        // session_item's `.to_uppercase()` call (a fixture already in caps
        // would pass even if the uppercasing were silently dropped).
        let sessions = vec![Session { id: String::new(),
            name: "pr-review".into(), activity: 30, created: 1, attached: false,
            windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }],
        }];
        let cfg = Config {
            dormant: vec![], groups: vec![Group { name: "work".into(), members: vec!["pr-review".into()], color: String::new(), ..Default::default() }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_push('p');
        state.search_push('r');

        let text = render_to_string(&state);
        let mut found = false;
        for line in text.lines() {
            if line.contains("pr-review") {
                assert!(line.contains("WORK"), "grouped match carries its uppercased group tag: {line:?}");
                found = true;
            }
        }
        assert!(found, "pr-review row must be present");

        // Expected color comes from group_color() itself, mirroring the
        // pattern in raised_selected_empty_group_name_is_visible, so
        // this stays correct even if the default palette order changes.
        let expected_color = group_color(&state.groups[0], 0, &state.active_palette);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut tag_fg: Option<Color> = None;
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                if buf[(x, y)].symbol() != "W" || x + 4 >= buf.area.width {
                    continue;
                }
                let is_work = "WORK".chars().enumerate().all(|(i, c)| {
                    buf[((x + i as u16), y)].symbol() == c.to_string().as_str()
                });
                if is_work {
                    tag_fg = buf[(x, y)].style().fg;
                    break;
                }
            }
            if tag_fg.is_some() {
                break;
            }
        }
        assert_eq!(tag_fg, Some(expected_color), "WORK group's tag color matches group_color()");
    }

    #[test]
    fn draw_search_ungrouped_session_has_no_tag() {
        let text = render_to_string(&searching_state("scratch"));
        for line in text.lines() {
            if line.contains("scratch") {
                assert!(!line.contains("PINNED"), "ungrouped match carries no group tag: {line:?}");
            }
        }
    }

    #[test]
    fn draw_search_aligns_tags_across_differing_age_widths() {
        // Two sessions in the same group with deliberately different age
        // string widths ("40m" vs "5h") must still show their DEV tag at the
        // same column; before the age_pad fix, the 3-char age pushed its
        // tag one column right of the 2-char age's tag.
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let sessions = vec![
            Session { id: String::new(), name: "short-age".into(), activity: now - 5 * 3600, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "long-age".into(), activity: now - 40 * 60, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec![], groups: vec![Group {
                name: "dev".into(),
                members: vec!["short-age".into(), "long-age".into()],
                color: String::new(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_push('a');
        state.search_push('g');
        state.search_push('e');

        let text = render_to_string(&state);
        let mut columns: Vec<usize> = Vec::new();
        for line in text.lines() {
            if line.contains("short-age") || line.contains("long-age") {
                let col = line.find("DEV").expect("DEV tag present on both rows");
                columns.push(col);
            }
        }
        assert_eq!(columns.len(), 2, "both rows must be present");
        assert_eq!(columns[0], columns[1], "tags must align despite differing age widths: {columns:?}");
    }

    #[test]
    fn session_row_shows_recency_by_default_and_age_when_switched() {
        let now = 1_700_000_000i64;
        let _clock = freeze_clock(now);
        let sessions = vec![Session { id: String::new(),
            name: "alpha".into(),
            activity: now - 30, // "30s"
            created: now - 7200, // "2h"
            attached: false,
            windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }],
        }];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);

        let text = render_to_string(&state);
        assert!(text.contains("· 30s"), "Recency (default) shows time since activity: {text:?}");
        assert!(!text.contains("· 2h"));

        state.session_metric = SessionMetric::Age;
        let text = render_to_string(&state);
        assert!(text.contains("· 2h"), "Age shows time since created: {text:?}");
        assert!(!text.contains("· 30s"));
    }

    #[test]
    fn session_row_hides_age_and_separator_when_metric_is_hidden() {
        let sessions = vec![Session { id: String::new(),
            name: "alpha".into(),
            activity: 1,
            created: 1,
            attached: false,
            windows: vec![
                Window { id: String::new(), index: 0, name: "w".into(), active: true },
                Window { id: String::new(), index: 1, name: "w2".into(), active: false },
            ],
        }];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.session_metric = SessionMetric::Hidden;
        let text = render_to_string(&state);
        let row = text.lines().find(|l| l.contains("alpha")).expect("session row rendered");
        assert!(row.contains("2 windows"), "window count still shows: {row:?}");
        assert!(!row.contains(" · "), "no middot separator when the age is hidden: {row:?}");
    }

    #[test]
    fn draw_search_still_aligns_tags_when_metric_is_hidden() {
        // Regression guard: age_width degenerates to 0 when Hidden, so the
        // age_pad computation must not panic or misalign group tags.
        let sessions = vec![
            Session { id: String::new(), name: "short-age".into(), activity: 1, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "long-age".into(), activity: 2, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![Group {
                name: "dev".into(),
                members: vec!["short-age".into(), "long-age".into()],
                color: String::new(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.session_metric = SessionMetric::Hidden;
        state.enter_search();
        state.search_push('a');
        state.search_push('g');
        state.search_push('e');

        let text = render_to_string(&state);
        assert!(text.contains("DEV"), "group tag still renders when metric is Hidden: {text:?}");
    }

    #[test]
    fn draw_colors_group_header_by_its_color_in_session_mode() {
        // An explicit group color paints its header; a color-less group falls
        // back to the positional default (HEADER_COLORS[0] == cyan == ACCENT).
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
            Session { id: String::new(), name: "tent".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "config".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() },
                Group { name: "tools".into(), members: vec!["tent".into()], color: "magenta".into(), ..Default::default() },
            ],
            ..Default::default()
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

    /// A picker raised to group altitude over a realistic list: two named
    /// groups plus the inbox, one attached session, one dormant session, and
    /// one expanded session so window rows are on screen too. The cursor
    /// starts on `work`, so ALPHA is the highlighted group.
    fn raised_view() -> PickerState {
        let sessions = vec![
            Session { id: String::new(), name: "work".into(), activity: 30, created: 1, attached: true,
                      windows: vec![Window { id: String::new(), index: 0, name: "editor".into(), active: true }] },
            Session { id: String::new(), name: "stale".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "shell".into(), active: true }] },
            Session { id: String::new(), name: "notes".into(), activity: 10, created: 3, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "draft".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec!["stale".into()],
            groups: vec![
                Group { name: "alpha".into(), members: vec!["work".into()], ..Default::default() },
                Group { name: "beta".into(), members: vec!["stale".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.expand_session("work");
        st.enter_groups();
        st
    }

    fn render_buf(state: &PickerState) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, state)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn row_of(buf: &ratatui::buffer::Buffer, needle: &str) -> u16 {
        (0..buf.area.height)
            .find(|&y| find_text_x(buf, y, needle).is_some())
            .unwrap_or_else(|| panic!("no row contains {needle:?}"))
    }

    /// The x range of the card's interior on any row: everything inside the
    /// popup margin and the border, i.e. exactly the cells a list row owns.
    fn card_interior(buf: &ratatui::buffer::Buffer) -> std::ops::Range<u16> {
        (POPUP_MARGIN + 1)..(buf.area.width - POPUP_MARGIN - 1)
    }

    #[test]
    fn raised_sessions_recede_with_dim_but_keep_identity() {
        let st = raised_view();
        let buf = render_buf(&st);

        // Recede is a whole-plane transform: every cell a session or window
        // row owns is dimmed, not just its name.
        for needle in ["work", "editor", "stale", "notes"] {
            let y = row_of(&buf, needle);
            for x in card_interior(&buf) {
                assert!(
                    buf[(x, y)].style().add_modifier.contains(Modifier::DIM),
                    "row {needle:?} cell at x={x} should be dimmed"
                );
            }
        }

        // ...while each row keeps the identity it has at session altitude.
        let attached_fg = match st.attached_color_mode {
            AttachedColorMode::Match => group_color(&st.groups[0], 0, &st.active_palette),
            AttachedColorMode::Static => color_from_name(&st.attached_color),
        };
        let work_y = row_of(&buf, "work");
        let work_x = find_text_x(&buf, work_y, "work").unwrap();
        assert_eq!(buf[(work_x, work_y)].style().fg, Some(attached_fg), "attached session keeps its color");
        assert!(buf[(work_x, work_y)].style().add_modifier.contains(Modifier::BOLD), "attached session stays bold");
        assert!(find_text_x(&buf, work_y, "●").is_some(), "attached dot survives");

        let stale_y = row_of(&buf, "stale");
        let stale_x = find_text_x(&buf, stale_y, "stale").unwrap();
        assert_eq!(buf[(stale_x, stale_y)].style().fg, Some(Color::DarkGray), "dormant session stays DarkGray");

        let notes_y = row_of(&buf, "notes");
        let notes_x = find_text_x(&buf, notes_y, "notes").unwrap();
        assert_ne!(buf[(notes_x, notes_y)].style().fg, Some(Color::DarkGray), "a normal row is not recolored as dormant");

        // Numbers mean jumpable at every altitude, so they stay on screen.
        let gutter_x = card_interior(&buf).start;
        assert_eq!(buf[(gutter_x, work_y)].symbol(), "│", "group gutter bar leads the row");
        assert_eq!(buf[(gutter_x + 1, work_y)].symbol(), "1", "jump number still rendered while raised");
    }

    #[test]
    fn raised_highlighted_header_carries_selection_bar() {
        let st = raised_view();
        let buf = render_buf(&st);
        let alpha_y = row_of(&buf, "ALPHA");
        let beta_y = row_of(&buf, "BETA");

        for x in card_interior(&buf) {
            assert_eq!(
                buf[(x, alpha_y)].style().bg,
                Some(SEL_BG),
                "the highlighted group's header wears the selection bar across the full row (x={x})"
            );
            assert_ne!(buf[(x, beta_y)].style().bg, Some(SEL_BG), "unhighlighted headers get no bar (x={x})");
        }
        let alpha_x = find_text_x(&buf, alpha_y, "ALPHA").unwrap();
        assert!(buf[(alpha_x, alpha_y)].style().add_modifier.contains(Modifier::BOLD), "selection bar is bold");
    }

    #[test]
    fn raised_headers_are_not_dimmed() {
        let st = raised_view();
        let buf = render_buf(&st);
        for label in ["ALPHA", "BETA", "INBOX"] {
            let y = row_of(&buf, label);
            for x in card_interior(&buf) {
                assert!(
                    !buf[(x, y)].style().add_modifier.contains(Modifier::DIM),
                    "header {label:?} keeps its full color (x={x})"
                );
            }
        }
    }

    #[test]
    fn raised_footer_shows_altitude_hints() {
        let text = render_to_string(&raised_view());
        assert!(text.contains("x delete"), "altitude hint present");
        assert!(text.contains("Enter open"), "altitude hint present");
        assert!(!text.contains("x kill"), "session-altitude hints step aside while raised");
    }

    #[test]
    fn altitude_footer_hint_fits_the_card() {
        assert!(
            ALTITUDE_FOOTER_HINT.chars().count() <= 78,
            "ALTITUDE_FOOTER_HINT must fit the 78-col footer at real 84-col popup width"
        );
    }

    #[test]
    fn altitude_footer_hint_uses_styled_hint() {
        let state = raised_view();
        let backend = TestBackend::new(84, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        assert_footer_key_is_styled(terminal.backend().buffer());
    }

    #[test]
    fn raised_footer_shows_warning_after_a_blocked_inbox_reorder() {
        let mut st = raised_view(); // ALPHA + BETA + INBOX
        st.group_move_cursor(2); // land on the inbox
        st.group_reorder(-1); // blocked
        let text = render_to_string(&st);
        assert!(text.contains("Inbox can't be reordered"));
        assert!(!text.contains("Enter open"), "warning replaces the hint line, not alongside it");
    }

    #[test]
    fn raised_footer_shows_warning_when_delete_is_armed() {
        let mut st = raised_view();
        st.arm_group_delete();
        let text = render_to_string(&st);
        assert!(text.contains("x again to delete group 'alpha'"));
        assert!(!text.contains("Enter open"), "warning replaces the hint line, not alongside it");
    }

    #[test]
    fn raised_footer_warning_is_red_not_dim() {
        let mut st = raised_view();
        st.group_move_cursor(2);
        st.group_reorder(-1);
        let buf = render_buf(&st);
        let y = row_of(&buf, "Inbox can't be reordered");
        let x = find_text_x(&buf, y, "Inbox can't be reordered").unwrap();
        assert_eq!(
            buf[(x, y)].style().fg,
            Some(WARNING),
            "blocked-reorder warning should render in WARNING red, not the dim hint color"
        );
    }

    #[test]
    fn raised_selected_empty_group_name_is_visible() {
        // Issue #14: a newly created (empty) group sits highlighted against
        // the DarkGray selection bar. Dimming its name to DarkGray for being
        // empty renders DarkGray-on-DarkGray: an invisible name. The
        // highlighted group's header always shows its real color.
        let mut st = raised_view();
        st.group_new();
        for c in "shelf".chars() { st.group_edit_push(c); }
        st.group_commit_rename();
        let gi = st.group_cursor();
        let buf = render_buf(&st);

        let expected = group_color(&st.groups[gi], gi, &st.active_palette);
        let mut found_visible_name_cell = false;
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                if cell.style().bg == Some(SEL_BG) && cell.style().fg == Some(expected) {
                    found_visible_name_cell = true;
                }
                let invisible = cell.style().bg == Some(SEL_BG) && cell.style().fg == Some(Color::DarkGray);
                assert!(!invisible, "highlighted empty group row has DarkGray-on-DarkGray cell at x={x}, y={y}");
            }
        }
        assert!(found_visible_name_cell, "highlighted empty group name renders in its real header color");
    }

    #[test]
    fn raised_rename_edits_inline_on_the_header_row() {
        let mut st = raised_view();
        st.group_start_rename();
        st.group_edit_clear();
        for c in "renamed".chars() { st.group_edit_push(c); }
        let buf = render_buf(&st);

        let y = row_of(&buf, "RENAMED");
        let caret_x = find_text_x(&buf, y, "▏").expect("caret trails the buffer on the header row");
        assert_eq!(
            buf[(caret_x, y)].style().fg,
            Some(color_from_name(&st.border_color)),
            "caret takes the border color"
        );
        let text = render_to_string_sized(&st, 80, 24);
        assert!(!text.contains("ALPHA"), "the edit buffer replaces the group's committed name");
    }

    #[test]
    fn raised_new_group_names_inline_on_its_own_header_row() {
        let mut st = raised_view();
        st.group_new();
        for c in "fresh".chars() { st.group_edit_push(c); }
        let text = render_to_string_sized(&st, 80, 24);
        assert!(text.contains("FRESH▏"), "a just-created group is named in place, uppercased, with a caret");
    }

    #[test]
    fn raised_new_group_placeholder_is_visible_against_the_selection_bar() {
        let mut st = raised_view();
        st.group_new(); // buffer starts empty: this header row carries the selection bar
        let buf = render_buf(&st);
        let placeholder_y = row_of(&buf, "NAME");
        let x = find_text_x(&buf, placeholder_y, "NAME").unwrap();
        assert_eq!(buf[(x, placeholder_y)].style().bg, Some(SEL_BG), "precondition: placeholder sits on the selection bar");
        assert_eq!(buf[(x, placeholder_y)].style().fg, Some(Color::Gray), "placeholder must be Gray, not DarkGray-on-DarkGray");
    }

    #[test]
    fn raised_group_swap_marker_flashes_on_the_moved_header() {
        let mut st = raised_view();
        st.group_reorder(1); // ALPHA moves down past BETA
        let buf = render_buf(&st);
        assert!(find_text_x(&buf, row_of(&buf, "ALPHA"), "▼").is_some(), "ALPHA moved down");
        let beta_y = row_of(&buf, "BETA");
        assert!(
            find_text_x(&buf, beta_y, "▲").is_none() && find_text_x(&buf, beta_y, "▼").is_none(),
            "BETA (bumped) gets no marker"
        );
    }

    #[test]
    fn raised_still_shows_the_create_group_hint() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![] },
        ];
        let mut st = PickerState::build(sessions, &Config::default());
        st.enter_groups();
        let text = render_to_string(&st);
        assert!(text.contains("No groups yet"), "first-run group hint survives the raise");
        assert!(text.contains("g then n"), "hint tells the user how to create a group");
    }

    #[test]
    fn create_group_hint_still_names_live_keys() {
        // The hint promises `g` then `n`; both must still be what they say.
        assert!(CREATE_GROUP_HINT.contains("g then n"));
        assert!(FOOTER_HINT.contains("g grp"), "`g` still raises to group altitude");
        assert!(ALTITUDE_FOOTER_HINT.contains("n new"), "`n` still creates a group once raised");
    }

    #[test]
    fn draw_is_graceful_on_tiny_popup() {
        let sessions = vec![
            Session { id: String::new(), name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);

        // Smaller than 2*margin+1: must not panic and must keep its size.
        let backend = TestBackend::new(3, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        assert_eq!(buf.area.width, 3);
        assert_eq!(buf.area.height, 3);
    }

    #[test]
    fn color_from_name_maps_all_sixteen_named_colors() {
        assert_eq!(color_from_name("black"), Color::Black);
        assert_eq!(color_from_name("red"), Color::Red);
        assert_eq!(color_from_name("green"), Color::Green);
        assert_eq!(color_from_name("yellow"), Color::Yellow);
        assert_eq!(color_from_name("blue"), Color::Blue);
        assert_eq!(color_from_name("magenta"), Color::Magenta);
        assert_eq!(color_from_name("cyan"), Color::Cyan);
        assert_eq!(color_from_name("gray"), Color::Gray);
        assert_eq!(color_from_name("darkgray"), Color::DarkGray);
        assert_eq!(color_from_name("lightred"), Color::LightRed);
        assert_eq!(color_from_name("lightgreen"), Color::LightGreen);
        assert_eq!(color_from_name("lightyellow"), Color::LightYellow);
        assert_eq!(color_from_name("lightblue"), Color::LightBlue);
        assert_eq!(color_from_name("lightmagenta"), Color::LightMagenta);
        assert_eq!(color_from_name("lightcyan"), Color::LightCyan);
        assert_eq!(color_from_name("white"), Color::White);
        assert_eq!(color_from_name("bogus"), Color::Cyan, "unknown name falls back to the accent");
    }

    #[test]
    fn draw_colors_group_header_from_active_palette_not_a_fixed_const() {
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![Group { name: "config".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() }],
            active_palette: vec!["white".to_string()],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut config_white = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(i) = line.find("CONFIG") {
                config_white = buf[(i as u16, y)].style().fg == Some(Color::White);
            }
        }
        assert!(config_white, "positional default reads the configured active_palette");
    }

    #[test]
    fn draw_shows_settings_footer_hint() {
        let sessions = vec![
            Session { id: String::new(), name: "main".into(), activity: 100, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config::default();
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("cfg"), "footer hint: cfg present");
    }

    fn settings_view() -> PickerState {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false,
                                       windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] }];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.enter_settings();
        st
    }

    #[test]
    fn draw_settings_shows_description_of_selected_row() {
        let text = render_to_string(&settings_view());
        // Cursor starts on the first row, DefaultMode.
        assert!(text.contains("On launch, rolomux opens in Command mode."));
    }

    #[test]
    fn draw_settings_description_updates_as_cursor_moves() {
        let mut st = settings_view();
        st.settings_move_cursor(1); // DormantNumbering
        let text = render_to_string(&st);
        assert!(text.contains("Visible dormant sessions receive jump numbers (1-20)."));
    }

    #[test]
    fn draw_settings_description_line_sits_above_the_key_hint_line() {
        let text = render_to_string(&settings_view());
        let lines: Vec<&str> = text.lines().collect();
        let description_idx = lines
            .iter()
            .position(|l| l.contains("On launch, rolomux opens in Command mode."))
            .expect("description line rendered");
        let hint_idx = lines
            .iter()
            .position(|l| l.contains(settings::SETTINGS_FOOTER_HINT))
            .expect("key-hint line rendered");
        assert!(description_idx < hint_idx, "description should render above the key-hint line");
    }

    #[test]
    fn draw_settings_description_renders_at_full_contrast_not_dim() {
        let state = settings_view();
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut found_description = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(i) = line.find("On launch, rolomux opens") {
                found_description = true;
                assert_ne!(
                    buf[(i as u16, y)].style().fg,
                    Some(Color::DarkGray),
                    "description line should render at full contrast, not dimmed"
                );
            }
        }
        assert!(found_description, "description line rendered");
    }

    #[test]
    fn draw_settings_shows_rows_and_footer() {
        // Taller than the usual 80x20 for several stacking reasons: the added
        // Session metadata row pushes later rows down (mirrors the palette
        // tests), so do the added Show shortcuts/Shortcut color/Active window
        // color/Border color policy rows, and the footer grew from 2 to
        // 3 rows (rule, key-hint, description), consuming one more row of
        // the list area.
        let text = render_to_string_sized(&settings_view(), 80, 29);
        assert!(text.contains("Default mode"));
        assert!(text.contains("Command"));
        assert!(text.contains("Number dormant sessions"));
        assert!(text.contains("On"));
        assert!(text.contains("New group color"));
        assert!(text.contains("Rotate"));
        assert!(text.contains("Group palette"));
        assert!(text.contains("active"));
        assert!(text.contains("Esc"));
    }

    #[test]
    fn draw_settings_shows_remember_expanded_row() {
        let text = render_to_string(&settings_view());
        assert!(text.contains("Remember expanded sessions"));
        assert!(text.contains("Off"), "defaults to Off");
    }

    #[test]
    fn draw_settings_remember_expanded_shows_on_when_toggled_on() {
        let mut st = settings_view();
        st.settings_move_cursor(2); // RememberExpanded
        st.settings_step_right();
        let text = render_to_string(&st);
        let row = text
            .lines()
            .find(|line| line.contains("Remember expanded sessions"))
            .expect("Remember expanded sessions row is rendered");
        // "Number dormant sessions" also renders "On" by default, so the
        // assertion must target this row specifically rather than the
        // whole screen's text.
        assert!(row.contains("On"), "row should show On once toggled on: {row:?}");
    }

    #[test]
    fn draw_settings_shows_clear_dormant_on_attach_row() {
        let text = render_to_string(&settings_view());
        assert!(text.contains("Clear dormant on attach"));
        assert!(text.contains("Off"), "defaults to Off");
    }

    #[test]
    fn draw_settings_clear_dormant_on_attach_shows_on_when_toggled_on() {
        let mut st = settings_view();
        st.settings_move_cursor(4); // ClearDormantOnAttach
        st.settings_step_right();
        let text = render_to_string(&st);
        let row = text
            .lines()
            .find(|line| line.contains("Clear dormant on attach"))
            .expect("Clear dormant on attach row is rendered");
        assert!(row.contains("On"), "row should show On once toggled on: {row:?}");
    }

    #[test]
    fn draw_settings_shows_session_metric_row() {
        let text = render_to_string(&settings_view());
        assert!(text.contains("Session metadata"));
        assert!(text.contains("Recency"), "defaults to Recency");
    }

    #[test]
    fn draw_settings_session_metric_cycles_through_labels() {
        let mut st = settings_view();
        st.settings_move_cursor(3); // SessionMetric
        st.settings_step_right();
        let text = render_to_string(&st);
        let row = text
            .lines()
            .find(|line| line.contains("Session metadata"))
            .expect("Session metadata row is rendered");
        assert!(row.contains("Age"), "row should show Age after one step: {row:?}");
        st.settings_step_right();
        let text = render_to_string(&st);
        let row = text
            .lines()
            .find(|line| line.contains("Session metadata"))
            .expect("Session metadata row is rendered");
        assert!(row.contains("Hidden"), "row should show Hidden after two steps: {row:?}");
    }

    #[test]
    fn draw_settings_shows_attached_and_border_color_rows() {
        let text = render_to_string_sized(&settings_view(), 80, 24);
        assert!(text.contains("Attached session color"));
        assert!(text.contains("Border color"));
        // Attached session color, Active window color, and Border color are
        // now contiguous rows in the COLORS section, all defaulting to
        // Static green, so all three swatches render together here.
        assert_eq!(text.matches("green").count(), 3, "one swatch label per collapsed color row");
    }

    #[test]
    fn draw_settings_shows_border_color_policy_row() {
        let text = render_to_string_sized(&settings_view(), 80, 24);
        assert!(text.contains("Border color"));
        assert!(text.contains("Static"), "default policy label shown");
    }

    #[test]
    fn draw_settings_shows_version_right_aligned_in_border_color_in_the_bottom_border() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false,
                                       windows: vec![] }];
        let cfg = Config { border_color: "magenta".to_string(), ..Default::default() };
        let mut st = PickerState::build(sessions, &cfg);
        st.enter_settings();
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let bottom_border_row = buf.area.height - 1 - POPUP_MARGIN;
        // Cell-indexed, not byte-indexed: the row is mostly multi-byte box-
        // drawing glyphs, so a plain `String::find` offset wouldn't line up
        // with the buffer's `x` coordinates.
        let cells: Vec<&str> = (0..buf.area.width).map(|x| buf[(x, bottom_border_row)].symbol()).collect();
        let version = app_version();
        let version_len = version.chars().count();
        let x = (0..=cells.len().saturating_sub(version_len))
            .find(|&i| cells[i..i + version_len].concat() == version)
            .unwrap_or_else(|| panic!("version should render in the bottom border row: {cells:?}"));
        assert!(
            x as u16 > buf.area.width / 2,
            "version should sit in the right half of the bottom border, found at x={x}: {cells:?}"
        );
        assert_eq!(
            buf[(x as u16, bottom_border_row)].style().fg,
            Some(Color::Magenta),
            "version text should mirror the configured border color, same as the top title"
        );
        // Flush against the corner: the very last content cell before the
        // right border corner is the connecting "─", same shape as the top
        // title's leading dash.
        let corner_x = buf.area.width - 1 - POPUP_MARGIN;
        assert_eq!(
            buf[(corner_x - 1, bottom_border_row)].symbol(),
            "─",
            "a connecting dash should sit flush between the version title and the corner"
        );
    }

    #[test]
    fn draw_command_mode_does_not_show_the_version() {
        // The version footer is a Settings-only affordance; Command mode's
        // border stays clean.
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false,
                                       windows: vec![] }];
        let state = PickerState::build(sessions, &Config::default());
        let text = render_to_string(&state);
        assert!(
            !text.contains(app_version()),
            "version should not render outside Settings mode: {text:?}"
        );
    }

    #[test]
    fn draw_settings_attached_color_row_shows_mode_label_and_hides_swatch_in_match_mode() {
        let mut st = settings_view();
        st.settings_move_cursor(7); // AttachedColor
        let text = render_to_string(&st);
        let row = text
            .lines()
            .find(|l| l.contains("Attached session color"))
            .expect("row rendered");
        assert!(row.contains("Static"), "default mode shown: {row:?}");
        assert!(row.contains("green"), "swatch + color name shown while Static: {row:?}");

        st.settings_step_right(); // Static -> Match
        let text = render_to_string(&st);
        let row = text
            .lines()
            .find(|l| l.contains("Attached session color"))
            .expect("row rendered");
        assert!(row.contains("Group"), "mode label matches DotColorMode's own Group label: {row:?}");
        assert!(!row.contains("green"), "no swatch/color name while Match: {row:?}");
    }

    #[test]
    fn draw_settings_expanded_shortcut_color_shows_radio_glyphs() {
        let mut st = settings_view();
        st.settings_move_cursor(13); // ShortcutColor
        st.settings_step_right();
        let text = render_to_string(&st);
        assert!(text.contains("●"));
        assert!(text.contains("○"));
    }

    #[test]
    fn draw_settings_expanded_palette_shows_swatches_and_checkboxes() {
        let mut st = settings_view();
        st.settings_move_cursor(12); // Palette
        st.settings_step_right(); // expand
        // Taller than the usual 80x20: section headers and the added Show
        // shortcuts/Shortcut color/Active window color/Border color
        // policy/Inbox icon rows now push the palette rows further down
        // than the default viewport reveals.
        let text = render_to_string_sized(&st, 80, 31);
        assert!(text.contains("[x]"), "active color checked");
        assert!(text.contains("[ ]"), "inactive color unchecked");
        assert!(text.contains("green"));
        assert!(text.contains("black"));
    }

    #[test]
    fn draw_settings_shows_static_color_value_when_policy_is_static() {
        let mut st = settings_view();
        st.settings_move_cursor(11); // ColorPolicy row
        st.settings_step_right(); // Rotate -> Random
        st.settings_step_right(); // Random -> Static
        st.static_color = "magenta".to_string();
        let text = render_to_string(&st);
        assert!(text.contains("Static"));
        assert!(text.contains("magenta"), "the selected static color is visible on the row");
    }

    #[test]
    fn draw_settings_does_not_show_a_color_value_for_rotate_or_random() {
        // Taller than the default 80x20: the COLORS section's own header
        // plus its rows push "New group color" further down the list.
        let text = render_to_string_sized(&settings_view(), 80, 28); // default policy is Rotate
        // "Rotate" itself is on screen, but no color name should follow it
        // since Rotate has no single fixed color to show. The four swatches
        // on screen are Attached session color, Active window color, and
        // Border color policy (all Static by default), plus the
        // always-present Shortcut color row; Rotate/Random must not add a
        // fifth for the New group color policy row itself.
        assert!(text.contains("Rotate"));
        assert_eq!(
            text.matches("██").count(),
            4,
            "no extra swatch for Rotate/Random policies beyond the other four color rows"
        );
    }

    #[test]
    fn draw_settings_rows_start_with_a_dim_gutter_bar() {
        let text = render_to_string(&settings_view());
        let row = text
            .lines()
            .find(|line| line.contains("Default mode"))
            .expect("Default mode row is rendered");
        // Strip margin and frame border to check the actual content.
        let content = row.chars().skip(3).collect::<String>();
        assert!(content.starts_with("│"), "settings row should start with a gutter bar: {row:?}");
    }

    #[test]
    fn draw_settings_gutter_bar_is_dim_colored() {
        let state = settings_view();
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut found_dim_bar = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(label_x) = line.find("Default mode") {
                for x in (POPUP_MARGIN + 1)..(label_x as u16) {
                    let cell = &buf[(x, y)];
                    if cell.symbol() == "│" && cell.style().fg == Some(Color::DarkGray) {
                        found_dim_bar = true;
                    }
                }
            }
        }
        assert!(found_dim_bar, "Default mode row shows a dim gutter bar");
    }

    #[test]
    fn draw_settings_row_one_shows_its_jump_number() {
        let text = render_to_string(&settings_view());
        let row = text
            .lines()
            .find(|line| line.contains("Default mode"))
            .expect("Default mode row is rendered");
        // Strip margin and frame border to check the actual content.
        let content = row.chars().skip(3).collect::<String>();
        assert!(content.starts_with("│1"), "row 1 (Default mode) shows jump number 1: {row:?}");
    }

    #[test]
    fn draw_settings_row_eleven_shows_the_alt_glyph_jump_label() {
        // Border palette (row 11) sits below the fold at the default 80x20
        // size; needs height >= 24 to render, same as the other pre-existing
        // tests that check this row.
        let text = render_to_string_sized(&settings_view(), 80, 24);
        let row = text
            .lines()
            .find(|line| line.contains("Border palette"))
            .expect("Border palette row (jump number 11) is rendered");
        // Strip margin and frame border to check the actual content.
        let content = row.chars().skip(3).collect::<String>();
        assert!(content.starts_with("│⌥1"), "row 11 (Border palette) shows the Alt+1 jump label: {row:?}");
    }

    #[test]
    fn draw_settings_child_color_option_rows_are_unchanged_by_jump_numbering() {
        let mut st = settings_view();
        st.settings_move_cursor(13); // ShortcutColor
        st.settings_step_right(); // expand
        let text = render_to_string(&st);
        let row = text
            .lines()
            .find(|line| line.contains("○"))
            .expect("an unselected color radio-option row is rendered");
        // Strip margin and frame border to check the actual content.
        let content = row.chars().skip(3).collect::<String>();
        assert!(
            content.starts_with("│     ○"),
            "child rows keep their original 5-space indent, no jump number inserted: {row:?}"
        );
    }

    #[test]
    fn draw_settings_gutter_bar_continues_through_expanded_color_options() {
        let mut st = settings_view();
        st.settings_move_cursor(13); // ShortcutColor
        st.settings_step_right(); // expand
        let text = render_to_string(&st);
        let row = text
            .lines()
            .find(|line| line.contains("○"))
            .expect("an unselected color radio-option row is rendered");
        // Strip margin and frame border to check the actual content.
        let content = row.chars().skip(3).collect::<String>();
        assert!(content.starts_with("│"), "expanded color option row should continue the gutter bar: {row:?}");
    }

    #[test]
    fn draw_settings_gutter_bar_continues_through_expanded_palette_rows() {
        let mut st = settings_view();
        st.settings_move_cursor(12); // Palette
        st.settings_step_right(); // expand
        // Taller than the usual 80x20: section headers and the added Show
        // shortcuts/Shortcut color/Active window color/Border color
        // policy/Inbox icon rows now push the palette rows further down
        // than the default viewport reveals.
        let text = render_to_string_sized(&st, 80, 30);
        let row = text
            .lines()
            .find(|line| line.contains("[ ]"))
            .expect("an inactive palette checkbox row is rendered");
        // Strip margin and frame border to check the actual content.
        let content = row.chars().skip(3).collect::<String>();
        assert!(content.starts_with("│"), "expanded palette row should continue the gutter bar: {row:?}");
    }

    #[test]
    fn draw_settings_color_policy_row_continues_the_gutter_bar() {
        // Taller than the default 80x20: the COLORS section's own header
        // plus its rows push "New group color" further down the list.
        let text = render_to_string_sized(&settings_view(), 80, 28);
        let row = text
            .lines()
            .find(|line| line.contains("New group color"))
            .expect("New group color row is rendered");
        // Strip margin and frame border to check the actual content.
        let content = row.chars().skip(3).collect::<String>();
        assert!(content.starts_with("│"), "ColorPolicy row should continue the gutter bar: {row:?}");
    }

    #[test]
    fn draw_settings_border_palette_row_is_dimmed_when_border_color_is_static() {
        let st = settings_view(); // border_color_policy defaults to Static
        let backend = TestBackend::new(80, 31);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let y = row_of(&buf, "Border palette");
        for x in card_interior(&buf) {
            assert!(
                buf[(x, y)].style().add_modifier.contains(Modifier::DIM),
                "Border palette row should dim while Border color is Static"
            );
        }
    }

    #[test]
    fn draw_settings_border_palette_row_is_not_dimmed_once_border_color_leaves_static() {
        let mut st = settings_view();
        st.settings_move_cursor(9); // BorderColorPolicy, defaults to Static
        st.settings_step_right(); // Static -> Rotate
        let backend = TestBackend::new(80, 31);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let y = row_of(&buf, "Border palette");
        for x in card_interior(&buf) {
            assert!(
                !buf[(x, y)].style().add_modifier.contains(Modifier::DIM),
                "Border palette row should not dim once Border color leaves Static"
            );
        }
    }

    #[test]
    fn draw_settings_expanded_border_palette_child_rows_dim_too_when_border_color_is_static() {
        let mut st = settings_view(); // border_color_policy defaults to Static
        st.settings_jump(11); // BorderPalette, expands to first active swatch
        let backend = TestBackend::new(80, 31);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let y = row_of(&buf, "[x]");
        for x in card_interior(&buf) {
            assert!(
                buf[(x, y)].style().add_modifier.contains(Modifier::DIM),
                "expanded Border palette child row should dim too"
            );
        }
    }

    #[test]
    fn draw_settings_group_palette_row_is_not_dimmed_when_new_group_color_is_rotate() {
        let st = settings_view(); // new_group_color_policy defaults to Rotate
        let backend = TestBackend::new(80, 31);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let y = row_of(&buf, "Group palette");
        for x in card_interior(&buf) {
            assert!(
                !buf[(x, y)].style().add_modifier.contains(Modifier::DIM),
                "Group palette row should not dim while New group color is Rotate"
            );
        }
    }

    #[test]
    fn draw_settings_group_palette_row_dims_once_new_group_color_becomes_static() {
        let mut st = settings_view();
        st.settings_move_cursor(11); // ColorPolicy
        st.settings_step_right();
        st.settings_step_right(); // Rotate -> Random -> Static
        let backend = TestBackend::new(80, 31);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let y = row_of(&buf, "Group palette");
        for x in card_interior(&buf) {
            assert!(
                buf[(x, y)].style().add_modifier.contains(Modifier::DIM),
                "Group palette row should dim once New group color is Static"
            );
        }
    }

    #[test]
    fn draw_settings_shows_all_three_section_headers() {
        // Taller than the default 80x20: APPEARANCE now renders at content
        // line 18 (after both BEHAVIOR's 7 rows and the whole COLORS
        // section), well past the default viewport.
        let text = render_to_string_sized(&settings_view(), 80, 31);
        assert!(text.contains("BEHAVIOR"), "Behavior section header is rendered");
        assert!(text.contains("COLORS"), "Colors section header is rendered");
        assert!(text.contains("APPEARANCE"), "Appearance section header is rendered");
    }

    #[test]
    fn draw_settings_behavior_header_precedes_default_mode_row() {
        let text = render_to_string(&settings_view());
        let lines: Vec<&str> = text.lines().collect();
        let header_idx = lines.iter().position(|l| l.contains("BEHAVIOR")).expect("BEHAVIOR header rendered");
        let row_idx = lines.iter().position(|l| l.contains("Default mode")).expect("Default mode row rendered");
        assert!(header_idx < row_idx, "BEHAVIOR header should render above the Default mode row");
    }

    #[test]
    fn draw_settings_colors_header_precedes_attached_color_row() {
        let text = render_to_string_sized(&settings_view(), 80, 22);
        let lines: Vec<&str> = text.lines().collect();
        let header_idx = lines.iter().position(|l| l.contains("COLORS")).expect("COLORS header rendered");
        let row_idx = lines.iter().position(|l| l.contains("Attached session color")).expect("Attached session color row rendered");
        assert!(header_idx < row_idx, "COLORS header should render above the Attached session color row");
    }

    #[test]
    fn draw_settings_appearance_header_precedes_inbox_icon_row() {
        let text = render_to_string_sized(&settings_view(), 80, 31);
        let lines: Vec<&str> = text.lines().collect();
        let header_idx = lines.iter().position(|l| l.contains("APPEARANCE")).expect("APPEARANCE header rendered");
        let row_idx = lines.iter().position(|l| l.contains("Inbox icon")).expect("Inbox icon row rendered");
        assert!(header_idx < row_idx, "APPEARANCE header should render above the Inbox icon row");
    }

    #[test]
    fn draw_settings_section_headers_are_colored_by_section() {
        let state = settings_view();
        let backend = TestBackend::new(80, 31);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut behavior_color = None;
        let mut colors_color = None;
        let mut appearance_color = None;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(i) = line.find("BEHAVIOR") {
                behavior_color = Some(buf[(i as u16, y)].style().fg);
            }
            if let Some(i) = line.find("COLORS") {
                colors_color = Some(buf[(i as u16, y)].style().fg);
            }
            if let Some(i) = line.find("APPEARANCE") {
                appearance_color = Some(buf[(i as u16, y)].style().fg);
            }
        }
        assert_eq!(behavior_color, Some(Some(Color::Blue)), "BEHAVIOR header should render in Blue");
        assert_eq!(colors_color, Some(Some(Color::Magenta)), "COLORS header should render in Magenta");
        assert_eq!(appearance_color, Some(Some(Color::Green)), "APPEARANCE header should render in Green");
    }

    #[test]
    fn draw_settings_blank_line_separates_behavior_and_colors_sections() {
        let text = render_to_string(&settings_view());
        let lines: Vec<&str> = text.lines().collect();
        let colors_idx = lines.iter().position(|l| l.contains("COLORS")).expect("COLORS header rendered");
        let prev_line = lines[colors_idx - 1];
        // Strip the popup's outer margin and left/right border chars, which are
        // present on every line, before checking that the list content itself is blank.
        let content_start = (POPUP_MARGIN + 1) as usize;
        let content_end = prev_line.chars().count() - content_start;
        let content: String = prev_line.chars().skip(content_start).take(content_end - content_start).collect();
        assert!(
            content.trim().is_empty(),
            "a blank line should separate BEHAVIOR's rows from the COLORS header, got: {:?}",
            prev_line
        );
    }

    #[test]
    fn draw_settings_blank_line_separates_colors_and_appearance_sections() {
        // Taller than the default 80x20: APPEARANCE now renders well past
        // the default viewport (see draw_settings_shows_all_three_section_headers).
        let text = render_to_string_sized(&settings_view(), 80, 31);
        let lines: Vec<&str> = text.lines().collect();
        let appearance_idx = lines.iter().position(|l| l.contains("APPEARANCE")).expect("APPEARANCE header rendered");
        let prev_line = lines[appearance_idx - 1];
        let content_start = (POPUP_MARGIN + 1) as usize;
        let content_end = prev_line.chars().count() - content_start;
        let content: String = prev_line.chars().skip(content_start).take(content_end - content_start).collect();
        assert!(
            content.trim().is_empty(),
            "a blank line should separate COLORS' rows from the APPEARANCE header, got: {:?}",
            prev_line
        );
    }

    #[test]
    fn draw_settings_selection_stays_aligned_with_cursor_after_headers_are_spliced_in() {
        let mut st = settings_view();
        st.settings_move_cursor(2); // RememberExpanded: 3rd row in the model's flat, header-free list
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut selected_row_highlighted = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if line.contains("Remember expanded sessions") {
                selected_row_highlighted = buf[(POPUP_MARGIN + 1, y)].style().bg == Some(Color::DarkGray);
            }
        }
        assert!(
            selected_row_highlighted,
            "Remember expanded sessions should still render highlighted as the cursor row, even though the BEHAVIOR header now precedes it in the rendered list"
        );
    }

    #[test]
    fn draw_shows_group_color_gutter_next_to_its_sessions() {
        // A named group's session rows get a leading '│' in the group's color.
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "tools".into(), members: vec!["claude".into()], color: "magenta".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut found_magenta_bar = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(name_x) = line.find("claude") {
                // The gutter is the new leading column, immediately before the
                // jump-number/glyph prefix that precedes the name.
                for x in (POPUP_MARGIN + 1)..(name_x as u16) {
                    let cell = &buf[(x, y)];
                    if cell.symbol() == "│" && cell.style().fg == Some(Color::Magenta) {
                        found_magenta_bar = true;
                    }
                }
            }
        }
        assert!(found_magenta_bar, "claude's row shows a magenta gutter bar matching its group's color");
    }

    #[test]
    fn draw_shows_accent_gutter_for_residual_sessions_section() {
        // Sessions in no named group (the inbox group) get the same
        // treatment, in ACCENT (cyan).
        let sessions = vec![
            Session { id: String::new(), name: "scratch".into(), activity: 20, created: 2, attached: false,
                      windows: vec![] },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut found_cyan_bar = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(name_x) = line.find("scratch") {
                for x in (POPUP_MARGIN + 1)..(name_x as u16) {
                    let cell = &buf[(x, y)];
                    if cell.symbol() == "│" && cell.style().fg == Some(Color::Cyan) {
                        found_cyan_bar = true;
                    }
                }
            }
        }
        assert!(found_cyan_bar, "inbox row shows a cyan (ACCENT) gutter bar");
    }

    #[test]
    fn draw_gutter_continues_through_expanded_window_rows() {
        // A window row under an expanded session inherits the parent
        // session's (i.e. its group's) gutter color.
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "editor".into(), active: true }] },
        ];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "tools".into(), members: vec!["claude".into()], color: "magenta".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.expand(); // cursor starts on "claude" (the only session)

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut found_magenta_bar_on_window_row = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(name_x) = line.find("editor") {
                for x in (POPUP_MARGIN + 1)..(name_x as u16) {
                    let cell = &buf[(x, y)];
                    if cell.symbol() == "│" && cell.style().fg == Some(Color::Magenta) {
                        found_magenta_bar_on_window_row = true;
                    }
                }
            }
        }
        assert!(found_magenta_bar_on_window_row, "editor window row shows the parent session's magenta gutter bar");
    }

    #[test]
    fn draw_expanded_window_name_indents_one_step_past_session_name() {
        // issue #76: expanded window rows were indented too far right,
        // because the tree connector reserved more cells than the session's
        // number gutter it is meant to stand in for. The connector should
        // land under the parent session's expand glyph, one indent step (the
        // width of the number gutter windows don't have) to the right of
        // where the session's own name starts.
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "editor".into(), active: true }] },
        ];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.expand();

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let session_x = (0..buf.area.height).find_map(|y| find_text_x(&buf, y, "claude")).unwrap();
        let window_x = (0..buf.area.height).find_map(|y| find_text_x(&buf, y, "editor")).unwrap();
        assert_eq!(window_x, session_x + 2, "window name should indent one step past its parent session's name");
    }

    #[test]
    fn draw_search_results_have_no_gutter_bar() {
        // Out of scope per spec: search's flat results list keeps its
        // existing inline group tag and gets no leading gutter column.
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "tools".into(), members: vec!["claude".into()], color: "magenta".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        // Search results should not have a leading gutter bar before the session name.
        // Check that where a session name appears, it is not preceded by a gutter bar.
        let mut found_gutter_before_session = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(name_x) = line.find("claude") {
                // Check the area just before the session name
                if name_x > 0 {
                    // Look for a pipe character in the few chars before the name
                    let pre_name_start = (POPUP_MARGIN + 1) as usize;
                    if name_x > pre_name_start {
                        for x in pre_name_start..name_x {
                            if buf[(x as u16, y)].symbol() == "│" {
                                // Found a pipe in the content area before the name
                                found_gutter_before_session = true;
                            }
                        }
                    }
                }
            }
        }
        assert!(!found_gutter_before_session, "search mode should not render a gutter bar before session names");
    }

    #[test]
    fn draw_search_shows_window_rows_when_session_is_expanded() {
        let mut state = searching_state("pr");
        state.search_expand();
        let text = render_to_string(&state);
        assert!(text.contains("└─"), "expanded session's single window row renders its tree connector");
    }

    #[test]
    fn draw_search_expanded_window_name_indents_one_step_past_session_name() {
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "editor".into(), active: true }] },
        ];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_expand();

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let session_x = (0..buf.area.height).find_map(|y| find_text_x(&buf, y, "claude")).unwrap();
        let window_x = (0..buf.area.height).find_map(|y| find_text_x(&buf, y, "editor")).unwrap();
        assert_eq!(window_x, session_x + 2, "search's expanded window name indents one step, same as command mode");
    }

    #[test]
    fn draw_search_expanded_window_rows_have_no_gutter_bar() {
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "editor".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![Group { name: "tools".into(), members: vec!["claude".into()], color: "magenta".into(), ..Default::default() }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_expand();

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut found_gutter_before_window = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(name_x) = line.find("editor") {
                for x in (POPUP_MARGIN + 1)..(name_x as u16) {
                    if buf[(x, y)].symbol() == "│" {
                        found_gutter_before_window = true;
                    }
                }
            }
        }
        assert!(!found_gutter_before_window, "search's expanded window rows must not render command mode's colored gutter bar");
    }

    #[test]
    fn search_enter_on_a_window_row_switches_to_that_window() {
        let mut state = searching_state("pr");
        state.search_expand();
        state.search_move(1); // onto pr-review's single window row
        assert_eq!(
            state.search_selected_action(),
            Some(Action::SwitchWindow("pr-review".into(), 0))
        );
    }

    #[test]
    fn gutter_colors_inbox_sessions_with_the_inbox_groups_own_color() {
        let sessions = vec![
            Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![Group { name: "INBOX".into(), color: "magenta".into(), inbox: true, ..Default::default() }],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let magenta = color_from_name("magenta");
        let mut found_magenta_bar = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(name_x) = line.find("a") {
                // The gutter is the new leading column, immediately before the
                // jump-number/glyph prefix that precedes the name.
                for x in (POPUP_MARGIN + 1)..(name_x as u16) {
                    let cell = &buf[(x, y)];
                    if cell.symbol() == "│" && cell.style().fg == Some(magenta) {
                        found_magenta_bar = true;
                    }
                }
            }
        }
        assert!(found_magenta_bar, "inbox session's gutter uses the inbox's configured color");
    }

    #[test]
    fn search_results_tag_inbox_members_same_as_named_group_members() {
        let sessions = vec![
            Session { id: String::new(), name: "solo".into(), activity: 1, created: 1, attached: false, windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![Group { name: "TRIAGE".into(), inbox: true, ..Default::default() }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_push('s');
        let text = render_to_string(&state);
        assert!(text.contains("TRIAGE"), "search result shows the inbox group's tag, no carve-out");
    }

    /// Shared setup for the active-window-dot-color tests (issue #53): one
    /// expanded session with an active window, filed in a "magenta" group so
    /// Group mode's inherited color is unambiguous against the Static default.
    fn dot_color_test_state(cfg_overrides: Config) -> PickerState {
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config {
            groups: vec![Group { name: "tools".into(), members: vec!["claude".into()], color: "magenta".into(), ..Default::default() }],
            ..cfg_overrides
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.expand_session("claude");
        state
    }

    fn find_dot_color(buf: &ratatui::buffer::Buffer) -> Option<Color> {
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                if buf[(x, y)].symbol() == "●" {
                    return buf[(x, y)].style().fg;
                }
            }
        }
        None
    }

    #[test]
    fn dot_color_static_mode_uses_the_configured_color_not_the_group_color() {
        let state = dot_color_test_state(Config { dot_color: "lightblue".to_string(), ..Default::default() });
        assert_eq!(state.dot_color_mode, DotColorMode::Static, "default mode is Static");
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        assert_eq!(find_dot_color(&buf), Some(Color::LightBlue), "Static mode ignores the group color");
    }

    #[test]
    fn dot_color_group_mode_inherits_the_sessions_group_color() {
        let state = dot_color_test_state(Config { dot_color_mode: DotColorMode::Group, ..Default::default() });
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        assert_eq!(find_dot_color(&buf), Some(Color::Magenta), "Group mode picks up the tools group's color");
    }

    /// Shared setup for the attached-session-color tests: one attached
    /// session filed in a "magenta" group so Match mode's inherited color is
    /// unambiguous against the Static default ("green").
    fn attached_color_test_state(cfg_overrides: Config) -> PickerState {
        let sessions = vec![
            Session { id: String::new(), name: "claude".into(), activity: 30, created: 1, attached: true,
                      windows: vec![] },
        ];
        let cfg = Config {
            groups: vec![Group { name: "tools".into(), members: vec!["claude".into()], color: "magenta".into(), ..Default::default() }],
            ..cfg_overrides
        };
        PickerState::build(sessions, &cfg)
    }

    #[test]
    fn attached_color_static_mode_uses_the_configured_color_not_the_group_color() {
        let state = attached_color_test_state(Config { attached_color: "lightblue".to_string(), ..Default::default() });
        assert_eq!(state.attached_color_mode, AttachedColorMode::Static, "default mode is Static");
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let y = (0..buf.area.height).find(|&y| find_text_x(&buf, y, "claude").is_some()).unwrap();
        let x = find_text_x(&buf, y, "claude").unwrap();
        assert_eq!(buf[(x, y)].style().fg, Some(Color::LightBlue), "Static mode ignores the group color");
    }

    #[test]
    fn attached_color_match_mode_inherits_the_sessions_group_color() {
        let state = attached_color_test_state(Config { attached_color_mode: AttachedColorMode::Match, ..Default::default() });
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let y = (0..buf.area.height).find(|&y| find_text_x(&buf, y, "claude").is_some()).unwrap();
        let x = find_text_x(&buf, y, "claude").unwrap();
        assert_eq!(buf[(x, y)].style().fg, Some(Color::Magenta), "Match mode picks up the tools group's color");
    }

    #[test]
    fn attached_color_match_mode_also_applies_in_search_view() {
        let mut state = attached_color_test_state(Config { attached_color_mode: AttachedColorMode::Match, ..Default::default() });
        state.mode = Mode::Search;
        state.search_push('c');
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let y = (0..buf.area.height).find(|&y| find_text_x(&buf, y, "claude").is_some()).unwrap();
        let x = find_text_x(&buf, y, "claude").unwrap();
        assert_eq!(buf[(x, y)].style().fg, Some(Color::Magenta), "Match mode picks up the group color in search too");
    }

    #[test]
    fn shortcut_color_setting_changes_the_footer_key_token_color() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] }];
        let cfg = Config { shortcut_color: "magenta".to_string(), ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(84, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut checked = false;
        for y in 0..buf.area.height {
            if let Some(x) = find_text_x(&buf, y, "rename") {
                let key_style = buf[(x - 2, y)].style(); // the "r" in "r rename"
                assert_eq!(key_style.fg, Some(Color::Magenta), "key token follows the configured shortcut_color");
                checked = true;
            }
        }
        assert!(checked, "expected to find the rename hint in the command-mode footer");
    }

    #[test]
    fn always_show_shortcuts_false_keeps_the_nudge_and_never_reveals_the_legend() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] }];
        let cfg = Config { always_show_shortcuts: false, ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string_sized(&state, 84, 20);
        assert!(!text.contains("rename"), "legend stays hidden when always_show_shortcuts is false");
        assert!(text.contains("? shortcuts"), "a minimal nudge names the key that opens the full shortcuts overlay");
    }

    #[test]
    fn question_mark_overlay_shows_full_command_shortcuts_in_a_tall_terminal() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] }];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.open_help();
        let text = render_to_string_sized(&state, 84, 40);
        assert!(text.contains("Command Shortcuts"));
        assert!(text.contains("kill session/window"));
        assert!(text.contains("Esc / q / ? close"));
        assert!(!text.contains("more"), "everything fits, so no truncation marker");
    }

    #[test]
    fn question_mark_overlay_truncates_gracefully_in_a_minimal_terminal() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] }];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.open_help();
        let text = render_to_string_sized(&state, 84, 20);
        assert!(text.contains("Command Shortcuts"));
        assert!(text.contains("more"), "the full command list doesn't fit at the CI-minimum terminal size");
    }

    #[test]
    fn question_mark_overlay_shows_groups_shortcuts_in_group_mode() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] }];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_groups();
        state.open_help();
        let text = render_to_string_sized(&state, 84, 40);
        assert!(text.contains("Group Altitude Shortcuts"));
        assert!(text.contains("reorder group"));
    }

    #[test]
    fn question_mark_overlay_shows_settings_shortcuts_in_settings_mode() {
        let sessions = vec![Session { id: String::new(), name: "a".into(), activity: 1, created: 1, attached: false, windows: vec![] }];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_settings();
        state.open_help();
        let text = render_to_string_sized(&state, 84, 40);
        assert!(text.contains("Settings Shortcuts"));
        assert!(text.contains("activate row"));
    }
}

