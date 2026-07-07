use crate::model::{
    ColorPolicy, DefaultMode, Group, Mode, PickerState, Row, Session, SettingsRow, Window,
    ALL_NAMED_COLORS,
};
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
/// only separation between rolomux's frame and the surrounding tmux panes; it
/// keeps the picker from sitting flush against busy content behind the popup.
const POPUP_MARGIN: u16 = 2;
/// Rows reserved right under the top border for the letterhead divider and a
/// blank breathing-room row, separating the title's chrome from the first
/// group header so content doesn't start flush against it.
const TITLE_CHROME_ROWS: u16 = 2;

const FOOTER_HINT: &str =
    "/ search · 1-9 · ⇧JK mv · g groups · , settings · d dim · q quit";

const SEARCH_FOOTER_HINT: &str = "↑↓ move · ⌃W word · ⌃U clear · Esc back";

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
    let border_color = color_from_name(&state.border_color);
    let border_style = Style::default().fg(border_color);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Line::from(vec![
            Span::styled("─", border_style),
            Span::styled("‹ rolomux ›", border_style.add_modifier(Modifier::BOLD | Modifier::ITALIC)),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(TITLE_CHROME_ROWS), Constraint::Min(0)])
        .split(inner);
    let chrome_area = chunks[0];
    let content = chunks[1];

    let rule = "─".repeat(chrome_area.width as usize);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(rule, Style::default().fg(DIM)))),
        chrome_area,
    );

    match state.mode {
        Mode::Command => draw_command(frame, state, content),
        Mode::Search => draw_search(frame, state, content),
        Mode::Groups => draw_groups(frame, state, content),
        Mode::Settings => draw_settings(frame, state, content),
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
    let meta = MetaLayout::compute(session_refs, list_area.width, true);
    let attached_color = color_from_name(&state.attached_color);

    let group_ids = state.ordered_group_ids();
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_line: Option<usize> = None;
    let mut last_section: Option<Option<usize>> = None;
    let mut current_gutter_color: Color = ACCENT;
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
                            let color = group_color(&state.groups[gi], gi, &state.active_palette);
                            push_section_header(&mut items, &state.groups[gi].name.to_uppercase(), list_area.width, color);
                            current_gutter_color = color;
                            next_group = gi + 1;
                        }
                        None => {
                            push_section_header(&mut items, "SESSIONS", list_area.width, ACCENT);
                            current_gutter_color = ACCENT;
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
                    state.is_dormant(&sess.name),
                    attached_color,
                    None,
                    Some(current_gutter_color),
                ));
            }
            Row::Window(si, wi) => {
                let sess = ordered[*si];
                let selected = Some(*row) == cursor_row;
                if selected {
                    selected_line = Some(items.len());
                }
                let last = *wi + 1 == sess.windows.len();
                items.push(window_item(&sess.windows[*wi], last, selected, current_gutter_color));
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
        Span::styled("▏", Style::default().fg(color_from_name(&state.border_color))),
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
        let meta = MetaLayout::compute(results.iter().copied(), chunks[1].width, false);
        let attached_color = color_from_name(&state.attached_color);
        // Widest age string among tagged rows, so every tag in this render
        // starts at the same column regardless of its own row's age width.
        let age_width = results
            .iter()
            .filter(|s| state.group_index_of(&s.name).is_some())
            .map(|s| activity_age(s.activity).chars().count())
            .max()
            .unwrap_or(0);
        for (i, sess) in results.iter().enumerate() {
            let selected = i == state.search_cursor();
            if selected {
                selected_line = Some(items.len());
            }
            let group_tag = state.group_index_of(&sess.name).map(|gi| {
                let age_pad = age_width.saturating_sub(activity_age(sess.activity).chars().count());
                (state.groups[gi].name.clone(), group_color(&state.groups[gi], gi, &state.active_palette), age_pad)
            });
            // Flat, collapsed, no jump number (None), normal metadata.
            items.push(session_item(sess, false, selected, None, meta, state.is_dormant(&sess.name), attached_color, group_tag, None));
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
                Span::styled("▏", Style::default().fg(color_from_name(&state.border_color))),
            ])
        } else {
            // Group mode always shows a group's real (flippable) header color,
            // regardless of membership: dimming empty groups here can collide
            // with the DarkGray selection bar and render the name invisible
            // (issue #14).
            let name_color = group_color(g, gi, &state.active_palette);
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
        format!("SESSIONS  · {}", state.group_session_count(state.inbox_index().unwrap_or(0))),
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

const SETTINGS_FOOTER_HINT: &str =
    "j/k move · h/l cycle · Space toggle · c color · Esc back";

fn draw_settings(frame: &mut Frame, state: &PickerState, inner: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(inner);
    let list_area = chunks[0];
    let footer_area = chunks[1];

    let rows = state.settings_visible_rows();
    // Computed once: PaletteColor rows below index into this instead of
    // rebuilding the 16-entry palette Vec on every iteration.
    let palette_entries = state.settings_palette_rows();
    let mut items: Vec<ListItem> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let selected = i == state.settings_cursor();
        let line = match row {
            SettingsRow::DefaultMode => {
                settings_value_line("Default mode", default_mode_label(state.default_mode), selected)
            }
            SettingsRow::AttachedColor => {
                settings_color_line("Attached session color", &state.attached_color, state.attached_color_expanded(), selected)
            }
            SettingsRow::AttachedColorOption(idx) => {
                settings_color_option_line(ALL_NAMED_COLORS[*idx], &state.attached_color, selected)
            }
            SettingsRow::BorderColor => {
                settings_color_line("Border color", &state.border_color, state.border_color_expanded(), selected)
            }
            SettingsRow::BorderColorOption(idx) => {
                settings_color_option_line(ALL_NAMED_COLORS[*idx], &state.border_color, selected)
            }
            SettingsRow::ColorPolicy => {
                let mut spans = vec![
                    Span::styled("New group color", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!("  {}", color_policy_label(state.new_group_color_policy)),
                        secondary(selected),
                    ),
                ];
                if state.new_group_color_policy == ColorPolicy::Static {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        "██",
                        Style::default().fg(color_from_name(&state.static_color)),
                    ));
                    spans.push(Span::styled(format!(" {}", state.static_color), secondary(selected)));
                }
                Line::from(spans)
            }
            SettingsRow::Palette => {
                let glyph = if state.palette_expanded() { "▾" } else { "▸" };
                Line::from(vec![
                    Span::styled(format!("{glyph} "), secondary(selected)),
                    Span::styled("Color palette", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!("  {} active", state.active_palette.len()),
                        secondary(selected),
                    ),
                ])
            }
            SettingsRow::PaletteColor(idx) => {
                let (name, active) = &palette_entries[*idx];
                let checkbox = if *active { "[x]" } else { "[ ]" };
                Line::from(vec![
                    Span::raw("     "),
                    Span::styled(checkbox.to_string(), secondary(selected)),
                    Span::raw(" "),
                    Span::styled("██", Style::default().fg(color_from_name(name))),
                    Span::raw(" "),
                    Span::raw(name.clone()),
                ])
            }
        };
        items.push(ListItem::new(line));
    }

    let list = List::new(items)
        .highlight_style(Style::default().bg(SEL_BG).add_modifier(Modifier::BOLD));
    let mut list_state = ListState::default();
    list_state.select(Some(state.settings_cursor()));
    frame.render_stateful_widget(list, list_area, &mut list_state);

    let rule = "─".repeat(footer_area.width as usize);
    let footer = Paragraph::new(vec![
        Line::from(Span::styled(rule, Style::default().fg(DIM))),
        Line::from(Span::styled(SETTINGS_FOOTER_HINT, Style::default().fg(DIM))),
    ]);
    frame.render_widget(footer, footer_area);
}

fn settings_value_line(label: &str, value: &str, selected: bool) -> Line<'static> {
    Line::from(vec![
        Span::styled(label.to_string(), Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {value}"), secondary(selected)),
    ])
}

/// Render a collapsed single-color settings row: an expand glyph, the bold
/// label, a swatch, and the color's name. Shared by Attached session color
/// and Border color.
fn settings_color_line(label: &str, color_name: &str, expanded: bool, selected: bool) -> Line<'static> {
    let glyph = if expanded { "▾" } else { "▸" };
    Line::from(vec![
        Span::styled(format!("{glyph} "), secondary(selected)),
        Span::styled(label.to_string(), Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("██", Style::default().fg(color_from_name(color_name))),
        Span::styled(format!(" {color_name}"), secondary(selected)),
    ])
}

/// Render one child row of an expanded single-color picker: a radio glyph
/// (`●` if `name` is the currently selected color, `○` otherwise), a swatch,
/// and the name. Distinct from `PaletteColor`'s `[x]`/`[ ]` checkbox glyph,
/// which communicates "pick many" instead of "pick one."
fn settings_color_option_line(name: &str, current: &str, selected: bool) -> Line<'static> {
    let radio = if name == current { "●" } else { "○" };
    Line::from(vec![
        Span::raw("     "),
        Span::styled(radio.to_string(), secondary(selected)),
        Span::raw(" "),
        Span::styled("██", Style::default().fg(color_from_name(name))),
        Span::raw(" "),
        Span::raw(name.to_string()),
    ])
}

fn default_mode_label(m: DefaultMode) -> &'static str {
    match m {
        DefaultMode::Command => "Command",
        DefaultMode::Search => "Search",
    }
}

fn color_policy_label(p: ColorPolicy) -> &'static str {
    match p {
        ColorPolicy::Rotate => "Rotate",
        ColorPolicy::Random => "Random",
        ColorPolicy::Static => "Static",
    }
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
    /// capped so metadata never falls off the card. `has_gutter` reserves one
    /// extra leading column when the caller renders a colored gutter bar
    /// before the jump number (draw_command); draw_search renders no gutter
    /// and must not reserve space for one.
    fn compute<'a>(sessions: impl Iterator<Item = &'a Session>, width: u16, has_gutter: bool) -> Self {
        let gutter_width = if has_gutter { 1 } else { 0 };
        let mut longest_prefix = 0usize;
        let mut count_width = 0usize;
        for s in sessions {
            longest_prefix = longest_prefix.max(gutter_width + SESSION_PREFIX + s.name.chars().count());
            count_width = count_width.max(window_count(s.windows.len()).chars().count());
        }
        let target = META_COL.max(longest_prefix + META_GAP);
        let cap = (width as usize).saturating_sub(META_BUDGET).max(META_COL);
        MetaLayout { col: target.min(cap), count_width }
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
) -> ListItem<'static> {
    let glyph = if expanded { "▾" } else { "▸" };
    let num = match number { Some(n) => format!("{n} "), None => "  ".to_string() };
    let name_style = if sess.attached {
        Style::default().fg(attached_color).add_modifier(Modifier::BOLD)
    } else if dormant {
        secondary(selected)
    } else {
        Style::default()
    };
    let gutter_width = if gutter.is_some() { 1 } else { 0 };
    let prefix_len = gutter_width + SESSION_PREFIX + sess.name.chars().count(); // gutter + num + "glyph " + name
    let pad = meta.col.saturating_sub(prefix_len);
    let count = window_count(sess.windows.len());
    let count_pad = meta.count_width.saturating_sub(count.chars().count());
    let age = activity_age(sess.activity);
    let mut spans = Vec::new();
    if let Some(color) = gutter {
        spans.push(Span::styled("│", Style::default().fg(color)));
    }
    spans.push(Span::styled(num, secondary(selected)));
    spans.push(Span::styled(format!("{glyph} "), secondary(selected)));
    spans.push(Span::styled(sess.name.clone(), name_style));
    spans.push(Span::styled(
        format!("{}{count}{} · {age}", " ".repeat(pad), " ".repeat(count_pad)),
        secondary(selected),
    ));
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

fn window_item(win: &Window, last: bool, selected: bool, gutter_color: Color) -> ListItem<'static> {
    // Three leading spaces align under the session's number gutter. No window
    // number is shown: numbers are reserved for things you can jump to, and
    // windows aren't jumpable yet.
    let connector = if last { "   └─ " } else { "   ├─ " };
    let dot = if win.active { "●" } else { " " };
    ListItem::new(Line::from(vec![
        Span::styled("│", Style::default().fg(gutter_color)),
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
    EnterSettings,
    MoveUp,
    MoveDown,
    EnterSearch,
    ToggleDormant,
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
        KeyCode::Char('J') | KeyCode::Down if shift => GroupInput::MoveDown,
        KeyCode::Char('K') | KeyCode::Up if shift => GroupInput::MoveUp,
        KeyCode::Char('j') | KeyCode::Down => GroupInput::Down,
        KeyCode::Char('k') | KeyCode::Up => GroupInput::Up,
        KeyCode::Char('n') => GroupInput::New,
        KeyCode::Enter | KeyCode::Char('r') => GroupInput::Rename,
        KeyCode::Char('c') => GroupInput::CycleColor,
        KeyCode::Char('d') => GroupInput::Delete,
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('g') => GroupInput::Exit,
        _ => GroupInput::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsInput {
    Up,
    Down,
    Left,
    Right,
    Activate,
    CycleColor,
    Exit,
    None,
}

/// Key mapping for settings mode. `,` exits (mirroring how it also enters,
/// same as `g` for Groups mode), alongside the usual `q`/`Esc`. The palette
/// checklist has a fixed display order (`ALL_NAMED_COLORS` canonical order),
/// so there is no reorder key here (unlike Groups mode's `⇧JK`).
pub fn map_settings_key(key: KeyEvent) -> SettingsInput {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => SettingsInput::Down,
        KeyCode::Char('k') | KeyCode::Up => SettingsInput::Up,
        KeyCode::Char('l') | KeyCode::Right => SettingsInput::Right,
        KeyCode::Char('h') | KeyCode::Left => SettingsInput::Left,
        KeyCode::Enter | KeyCode::Char(' ') => SettingsInput::Activate,
        KeyCode::Char('c') => SettingsInput::CycleColor,
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(',') => SettingsInput::Exit,
        _ => SettingsInput::None,
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
        KeyCode::Char('K') | KeyCode::Up if shift => Input::MoveUp,
        KeyCode::Char('J') | KeyCode::Down if shift => Input::MoveDown,
        KeyCode::Char('j') | KeyCode::Down => Input::Down,
        KeyCode::Char('k') | KeyCode::Up => Input::Up,
        KeyCode::Char('l') | KeyCode::Right => Input::Expand,
        KeyCode::Char('h') | KeyCode::Left => Input::Collapse,
        KeyCode::Char('z') => Input::ToggleAll,
        KeyCode::Enter => Input::Select,
        KeyCode::Char('g') => Input::EnterGroups,
        KeyCode::Char(',') => Input::EnterSettings,
        KeyCode::Char('/') => Input::EnterSearch,
        KeyCode::Char('d') => Input::ToggleDormant,
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

    use crate::model::{Group, PickerState, Session, Window};
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
            dormant: vec![], groups: vec![Group { name: "PINNED".into(), members: vec!["pr-review".into()], color: String::new(), ..Default::default() }],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("rolomux"), "title present");
        assert!(text.contains("PINNED"), "pinned header present");
        assert!(text.contains("SESSIONS"), "sessions header present");
        assert!(text.contains("pr-review"), "pinned session present");
        assert!(text.contains("scratch"), "unpinned session present");
    }

    #[test]
    fn border_and_title_use_the_configured_border_color() {
        let sessions = vec![Session { name: "a".into(), activity: 1, created: 1, attached: false,
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
    fn attached_session_name_uses_the_configured_attached_color() {
        let sessions = vec![Session { name: "current".into(), activity: 1, created: 1, attached: true,
                                       windows: vec![] }];
        let cfg = Config { attached_color: "lightgreen".to_string(), ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut found_lightgreen = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(x) = line.find("current") {
                found_lightgreen = buf[(x as u16, y)].style().fg == Some(Color::LightGreen);
            }
        }
        assert!(found_lightgreen, "attached session name picks up the configured color");
    }

    #[test]
    fn draw_marks_cursor_row_with_background() {
        let sessions = vec![
            Session { name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
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
            Session { name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
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
        assert_eq!(map_key(shift(KeyCode::Up)), Input::MoveUp);
        assert_eq!(map_key(shift(KeyCode::Down)), Input::MoveDown);
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
    fn maps_toggle_dormant_key() {
        assert_eq!(map_key(key(KeyCode::Char('d'))), Input::ToggleDormant);
    }

    #[test]
    fn draw_dims_dormant_session_when_unselected_but_not_when_selected() {
        let sessions = vec![
            Session { name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg); // cursor starts on "alpha"

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let mut beta_dim = false;
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(i) = line.find("beta") {
                beta_dim = buf[(i as u16, y)].style().fg == Some(Color::DarkGray);
            }
        }
        assert!(beta_dim, "unselected dormant session renders dim");

        // Move the cursor onto "beta": dormant + selected must not go invisible.
        state.focus_session("beta");
        let backend2 = TestBackend::new(80, 20);
        let mut terminal2 = Terminal::new(backend2).unwrap();
        terminal2.draw(|f| draw(f, &state)).unwrap();
        let buf2 = terminal2.backend().buffer().clone();
        for y in 0..buf2.area.height {
            for x in 0..buf2.area.width {
                let st = buf2[(x, y)].style();
                let invisible = st.bg == Some(Color::DarkGray) && st.fg == Some(Color::DarkGray);
                assert!(!invisible, "selected dormant row has DarkGray-on-DarkGray cell at x={x}, y={y}");
            }
        }
    }

    #[test]
    fn draw_shows_dormant_footer_hint() {
        let sessions = vec![
            Session { name: "main".into(), activity: 100, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
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
            Session { name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![Group { name: "G".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() }], ..Default::default() };
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
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("search"), "footer hint: search present");
        assert!(text.contains("groups"), "footer hint: groups present");
        assert!(text.contains("settings"), "footer hint: settings present");
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
            .map(|i| Window { index: i, name: "w".into(), active: i == 0 })
            .collect();
        let sessions = vec![
            Session { name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: many },
            Session { name: "beta".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
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
    fn metadata_stays_at_default_column_for_short_names() {
        // With only short names, the shared column collapses back to META_COL,
        // preserving the original compact layout.
        let sessions = vec![
            Session { name: "main".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "other".into(), activity: 20, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
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
            Session { name: "alpha".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
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
        let sessions = vec![Session {
            name: "pr-review".into(), activity: 30, created: 1, attached: false,
            windows: vec![Window { index: 0, name: "w".into(), active: true }],
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
        // pattern in draw_groups_selected_empty_group_name_is_visible, so
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
            Session { name: "short-age".into(), activity: now - 5 * 3600, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "long-age".into(), activity: now - 40 * 60, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
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
    fn group_keys_map_to_ops() {
        assert_eq!(map_group_key(key(KeyCode::Char('j'))), GroupInput::Down);
        assert_eq!(map_group_key(key(KeyCode::Char('k'))), GroupInput::Up);
        assert_eq!(map_group_key(shift(KeyCode::Char('J'))), GroupInput::MoveDown);
        assert_eq!(map_group_key(shift(KeyCode::Char('K'))), GroupInput::MoveUp);
        assert_eq!(map_group_key(shift(KeyCode::Down)), GroupInput::MoveDown);
        assert_eq!(map_group_key(shift(KeyCode::Up)), GroupInput::MoveUp);
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
    fn settings_keys_map_to_ops() {
        assert_eq!(map_settings_key(key(KeyCode::Char('j'))), SettingsInput::Down);
        assert_eq!(map_settings_key(key(KeyCode::Down)), SettingsInput::Down);
        assert_eq!(map_settings_key(key(KeyCode::Char('k'))), SettingsInput::Up);
        assert_eq!(map_settings_key(key(KeyCode::Up)), SettingsInput::Up);
        assert_eq!(map_settings_key(key(KeyCode::Char('l'))), SettingsInput::Right);
        assert_eq!(map_settings_key(key(KeyCode::Right)), SettingsInput::Right);
        assert_eq!(map_settings_key(key(KeyCode::Char('h'))), SettingsInput::Left);
        assert_eq!(map_settings_key(key(KeyCode::Left)), SettingsInput::Left);
        assert_eq!(map_settings_key(key(KeyCode::Enter)), SettingsInput::Activate);
        assert_eq!(map_settings_key(key(KeyCode::Char(' '))), SettingsInput::Activate);
        assert_eq!(map_settings_key(key(KeyCode::Char('c'))), SettingsInput::CycleColor);
        assert_eq!(map_settings_key(key(KeyCode::Esc)), SettingsInput::Exit);
        assert_eq!(map_settings_key(key(KeyCode::Char('q'))), SettingsInput::Exit);
        assert_eq!(map_settings_key(key(KeyCode::Char(','))), SettingsInput::Exit);
        assert_eq!(map_settings_key(key(KeyCode::Char('x'))), SettingsInput::None);
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

    fn groups_view(edit: bool) -> PickerState {
        let sessions = vec![
            Session { name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
            Session { name: "ticket".into(), activity: 10, created: 2, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config { groups: vec![Group { name: "config".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() }], ..Default::default() };
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
    fn draw_groups_selected_empty_group_name_is_visible() {
        // Issue #14: a newly created (empty) group sits selected against the
        // DarkGray highlight bar. Dimming its name to DarkGray for being empty
        // renders DarkGray-on-DarkGray: an invisible name. Group-mode names
        // must always show the group's real color, regardless of membership.
        let mut st = groups_view(false);
        st.group_new();
        for c in "test".chars() { st.group_edit_push(c); }
        st.group_commit_rename();
        let gi = st.group_cursor();

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &st)).unwrap();
        let buf = terminal.backend().buffer().clone();

        let expected = group_color(&st.groups[gi], gi, &st.active_palette);
        let mut found_visible_name_cell = false;
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                if cell.style().bg == Some(Color::DarkGray) && cell.style().fg == Some(expected) {
                    found_visible_name_cell = true;
                }
                let invisible = cell.style().bg == Some(Color::DarkGray)
                    && cell.style().fg == Some(Color::DarkGray);
                assert!(!invisible, "selected empty group row has DarkGray-on-DarkGray cell at x={x}, y={y}");
            }
        }
        assert!(found_visible_name_cell, "selected empty group name renders in its real header color");
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
            Session { name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
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
    fn comma_enters_settings_from_command_mode() {
        assert_eq!(map_key(key(KeyCode::Char(','))), Input::EnterSettings);
    }

    #[test]
    fn draw_shows_settings_footer_hint() {
        let sessions = vec![
            Session { name: "main".into(), activity: 100, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
        ];
        let cfg = Config::default();
        let state = PickerState::build(sessions, &cfg);
        let text = render_to_string(&state);
        assert!(text.contains("settings"), "footer hint: settings present");
    }

    fn settings_view() -> PickerState {
        let sessions = vec![Session { name: "a".into(), activity: 1, created: 1, attached: false,
                                       windows: vec![Window { index: 0, name: "w".into(), active: true }] }];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.enter_settings();
        st
    }

    #[test]
    fn draw_settings_shows_three_rows_and_footer() {
        let text = render_to_string(&settings_view());
        assert!(text.contains("Default mode"));
        assert!(text.contains("Command"));
        assert!(text.contains("New group color"));
        assert!(text.contains("Rotate"));
        assert!(text.contains("Color palette"));
        assert!(text.contains("active"));
        assert!(text.contains("Esc"));
    }

    #[test]
    fn draw_settings_shows_attached_and_border_color_rows() {
        let text = render_to_string(&settings_view());
        assert!(text.contains("Attached session color"));
        assert!(text.contains("Border color"));
        // Both default to cyan and render collapsed with a swatch + name.
        assert_eq!(text.matches("cyan").count(), 2, "one swatch label per collapsed color row");
    }

    #[test]
    fn draw_settings_expanded_attached_color_shows_radio_glyphs() {
        let mut st = settings_view();
        st.settings_move_cursor(1); // AttachedColor
        st.settings_step_right(); // expand
        let text = render_to_string(&st);
        assert!(text.contains("●"), "the currently selected color is marked filled");
        assert!(text.contains("○"), "unselected colors are marked hollow");
        // "white" (the last option) sits below the 12-row test viewport once
        // the list scrolls to keep the selected "cyan" row in view; assert on
        // colors that are actually on screen instead (mirrors the sibling
        // draw_settings_expanded_palette_shows_swatches_and_checkboxes test).
        assert!(text.contains("black"));
        assert!(text.contains("red"));
    }

    #[test]
    fn draw_settings_expanded_border_color_shows_radio_glyphs() {
        let mut st = settings_view();
        st.settings_move_cursor(2); // BorderColor
        st.settings_step_right();
        let text = render_to_string(&st);
        assert!(text.contains("●"));
        assert!(text.contains("○"));
    }

    #[test]
    fn draw_settings_expanded_palette_shows_swatches_and_checkboxes() {
        let mut st = settings_view();
        st.settings_move_cursor(4); // Palette
        st.settings_step_right(); // expand
        let text = render_to_string(&st);
        assert!(text.contains("[x]"), "active color checked");
        assert!(text.contains("[ ]"), "inactive color unchecked");
        assert!(text.contains("cyan"));
        assert!(text.contains("black"));
    }

    #[test]
    fn draw_settings_shows_static_color_value_when_policy_is_static() {
        let mut st = settings_view();
        st.settings_move_cursor(3); // ColorPolicy row
        st.settings_step_right(); // Rotate -> Random
        st.settings_step_right(); // Random -> Static
        st.static_color = "magenta".to_string();
        let text = render_to_string(&st);
        assert!(text.contains("Static"));
        assert!(text.contains("magenta"), "the selected static color is visible on the row");
    }

    #[test]
    fn draw_settings_does_not_show_a_color_value_for_rotate_or_random() {
        let text = render_to_string(&settings_view()); // default policy is Rotate
        // "Rotate" itself is on screen, but no color name should follow it
        // since Rotate has no single fixed color to show. The only two
        // swatches on screen are the always-present Attached/Border color
        // rows; Rotate/Random must not add a third for the policy row.
        assert!(text.contains("Rotate"));
        assert_eq!(
            text.matches("██").count(),
            2,
            "no extra swatch for Rotate/Random policies beyond the Attached/Border color rows"
        );
    }

    #[test]
    fn draw_shows_group_color_gutter_next_to_its_sessions() {
        // A named group's session rows get a leading '│' in the group's color.
        let sessions = vec![
            Session { name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "w".into(), active: true }] },
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
        // Sessions in no named group (the SESSIONS bucket) get the same
        // treatment, in ACCENT (cyan).
        let sessions = vec![
            Session { name: "scratch".into(), activity: 20, created: 2, attached: false,
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
        assert!(found_cyan_bar, "residual SESSIONS row shows a cyan (ACCENT) gutter bar");
    }

    #[test]
    fn draw_gutter_continues_through_expanded_window_rows() {
        // A window row under an expanded session inherits the parent
        // session's (i.e. its group's) gutter color.
        let sessions = vec![
            Session { name: "claude".into(), activity: 30, created: 1, attached: false,
                      windows: vec![Window { index: 0, name: "editor".into(), active: true }] },
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
    fn draw_search_results_have_no_gutter_bar() {
        // Out of scope per spec: search's flat results list keeps its
        // existing inline group tag and gets no leading gutter column.
        let sessions = vec![
            Session { name: "claude".into(), activity: 30, created: 1, attached: false, windows: vec![] },
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
}
