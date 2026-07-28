//! Settings-overlay rendering: `draw_settings` and its exclusive row, label,
//! and color-line builders. Shared style helpers (`styled_hint`,
//! `color_from_name`, `secondary`) and the palette constants stay in the parent
//! `ui` module and are reached through `use super::*`.

use super::*;

pub(super) const SETTINGS_FOOTER_HINT: &str =
    "j/k move · h/l cycle · Space toggle · 1-9 jump · c color · Esc back";

pub(super) fn draw_settings(frame: &mut Frame, state: &PickerState, inner: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(inner);
    let list_area = chunks[0];
    let footer_area = chunks[1];

    let rows = state.settings_visible_rows();
    // Computed once: PaletteColor rows below index into this instead of
    // rebuilding the 16-entry palette Vec on every iteration.
    let palette_entries = state.settings_palette_rows();
    let border_palette_entries = state.settings_border_palette_rows();
    let mut items: Vec<ListItem> = Vec::new();
    let mut selected_line: Option<usize> = None;
    for (i, row) in rows.iter().enumerate() {
        match row {
            SettingsRow::DefaultMode => push_settings_section_header(&mut items, "BEHAVIOR", list_area.width),
            SettingsRow::InboxIcon => push_settings_section_header(&mut items, "APPEARANCE", list_area.width),
            _ => {}
        }
        let selected = i == state.settings_cursor();
        if selected {
            selected_line = Some(items.len());
        }
        let line = match row {
            SettingsRow::DefaultMode => {
                settings_value_line(*row, "Default mode", default_mode_label(state.default_mode), selected)
            }
            SettingsRow::DormantNumbering => {
                settings_value_line(
                    *row,
                    "Number dormant sessions",
                    dormant_numbering_label(state.number_dormant_sessions),
                    selected,
                )
            }
            SettingsRow::RememberExpanded => {
                settings_value_line(
                    *row,
                    "Remember expanded sessions",
                    remember_expanded_label(state.remember_expanded_sessions),
                    selected,
                )
            }
            SettingsRow::SessionMetric => {
                settings_value_line(*row, "Session metadata", session_metric_label(state.session_metric), selected)
            }
            SettingsRow::ClearDormantOnAttach => {
                settings_value_line(
                    *row,
                    "Clear dormant on attach",
                    clear_dormant_on_attach_label(state.clear_dormant_on_attach),
                    selected,
                )
            }
            SettingsRow::StartFocusMode => {
                settings_value_line(
                    *row,
                    "Start in focus mode",
                    start_focus_mode_label(state.start_focus_mode),
                    selected,
                )
            }
            SettingsRow::ShortcutVisibility => {
                settings_value_line(
                    *row,
                    "Show shortcuts",
                    always_show_shortcuts_label(state.always_show_shortcuts),
                    selected,
                )
            }
            SettingsRow::InboxIcon => {
                settings_value_line(*row, "Inbox icon", &state.inbox_icon, selected)
            }
            SettingsRow::AttachedColor => {
                let mut spans = vec![gutter_span()];
                spans.extend(settings_number_span(*row, selected));
                spans.push(Span::styled("Attached session color", Style::default().add_modifier(Modifier::BOLD)));
                spans.push(Span::styled(
                    format!("  {}", attached_color_mode_label(state.attached_color_mode)),
                    secondary(selected),
                ));
                if state.attached_color_mode == AttachedColorMode::Static {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        "██",
                        Style::default().fg(color_from_name(&state.attached_color)),
                    ));
                    spans.push(Span::styled(format!(" {}", state.attached_color), secondary(selected)));
                }
                Line::from(spans)
            }
            SettingsRow::BorderColorPolicy => {
                let mut spans = vec![gutter_span()];
                spans.extend(settings_number_span(*row, selected));
                spans.push(Span::styled("Border color", Style::default().add_modifier(Modifier::BOLD)));
                spans.push(Span::styled(
                    format!("  {}", color_policy_label(state.border_color_policy)),
                    secondary(selected),
                ));
                if state.border_color_policy == ColorPolicy::Static {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        "██",
                        Style::default().fg(color_from_name(&state.border_color)),
                    ));
                    spans.push(Span::styled(format!(" {}", state.border_color), secondary(selected)));
                }
                Line::from(spans)
            }
            SettingsRow::BorderPalette => {
                border_palette_row_line(
                    SettingsRow::BorderPalette,
                    state.border_active_palette.len(),
                    state.border_palette_expanded(),
                    selected,
                )
            }
            SettingsRow::BorderPaletteColor(idx) => {
                let (name, active) = &border_palette_entries[*idx];
                let checkbox = if *active { "[x]" } else { "[ ]" };
                Line::from(vec![
                    gutter_span(),
                    Span::raw("     "),
                    Span::styled(checkbox.to_string(), secondary(selected)),
                    Span::raw(" "),
                    Span::styled("██", Style::default().fg(color_from_name(name))),
                    Span::raw(" "),
                    Span::raw(name.clone()),
                ])
            }
            SettingsRow::ShortcutColor => {
                settings_color_line(*row, "Shortcut highlight color", &state.shortcut_color, state.shortcut_color_expanded(), selected)
            }
            SettingsRow::ShortcutColorOption(idx) => {
                settings_color_option_line(ALL_NAMED_COLORS[*idx], &state.shortcut_color, selected)
            }
            SettingsRow::DotColorMode => {
                let mut spans = vec![gutter_span()];
                spans.extend(settings_number_span(*row, selected));
                spans.push(Span::styled("Active window dot color", Style::default().add_modifier(Modifier::BOLD)));
                spans.push(Span::styled(
                    format!("  {}", dot_color_mode_label(state.dot_color_mode)),
                    secondary(selected),
                ));
                if state.dot_color_mode == DotColorMode::Static {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        "██",
                        Style::default().fg(color_from_name(&state.dot_color)),
                    ));
                    spans.push(Span::styled(format!(" {}", state.dot_color), secondary(selected)));
                }
                Line::from(spans)
            }
            SettingsRow::ColorPolicy => {
                let mut spans = vec![gutter_span()];
                spans.extend(settings_number_span(*row, selected));
                spans.push(Span::styled("New group color", Style::default().add_modifier(Modifier::BOLD)));
                spans.push(Span::styled(
                    format!("  {}", color_policy_label(state.new_group_color_policy)),
                    secondary(selected),
                ));
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
                let mut spans = vec![gutter_span()];
                spans.extend(settings_number_span(SettingsRow::Palette, selected));
                spans.push(Span::styled(format!("{glyph} "), secondary(selected)));
                spans.push(Span::styled("Color palette", Style::default().add_modifier(Modifier::BOLD)));
                spans.push(Span::styled(format!("  {} active", state.active_palette.len()), secondary(selected)));
                Line::from(spans)
            }
            SettingsRow::PaletteColor(idx) => {
                let (name, active) = &palette_entries[*idx];
                let checkbox = if *active { "[x]" } else { "[ ]" };
                Line::from(vec![
                    gutter_span(),
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
    list_state.select(selected_line);
    frame.render_stateful_widget(list, list_area, &mut list_state);

    let rule = "─".repeat(footer_area.width as usize);
    let current_description = rows[state.settings_cursor().min(rows.len().saturating_sub(1))].description(state);
    let footer = Paragraph::new(vec![
        Line::from(Span::styled(rule, Style::default().fg(DIM))),
        description_line(&current_description, color_from_name(&state.shortcut_color)),
        shortcut_hint_line(state, SETTINGS_FOOTER_HINT),
    ]);
    frame.render_widget(footer, footer_area);
}

/// Renders a settings row's description, highlighting any literal `?` (a
/// reference to the shortcuts-overlay key) in the shortcut accent color so
/// it reads consistently with how `?` is highlighted in the overlay itself.
fn description_line(text: &str, key_color: Color) -> Line<'static> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(idx) = rest.find('?') {
        if idx > 0 {
            spans.push(Span::styled(rest[..idx].to_string(), Style::default()));
        }
        spans.push(Span::styled("?", Style::default().fg(key_color)));
        rest = &rest[idx + 1..];
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_string(), Style::default()));
    }
    Line::from(spans)
}

/// The dim leading `│` every Settings row renders in its first column,
/// tying rows visually to their section header. Unlike the main session
/// list's per-group gutter color, every Settings row uses the same dim
/// color — there is no per-section color coding.
fn gutter_span() -> Span<'static> {
    Span::styled("│", Style::default().fg(DIM))
}

/// The 2-character jump-number label for a top-level row (reusing the
/// session list's `⌥N` glyph via `jump_label`), or a blank placeholder for
/// a child row that has none. Settings always has rows 11-15 on screen, so
/// unlike the session list's `wide_numbering` flag this padding is
/// unconditional here -- there's no narrower case to optimize for.
fn settings_number_span(row: SettingsRow, selected: bool) -> Vec<Span<'static>> {
    match row.jump_number() {
        Some(n) => vec![Span::styled(jump_label(n), secondary(selected)), Span::raw(" ")],
        None => vec![],
    }
}

fn settings_section_header_item(label: &str, width: u16) -> ListItem<'static> {
    let rule_len = (width as usize).saturating_sub(label.chars().count() + 2);
    ListItem::new(Line::from(vec![
        Span::styled(label.to_string(), Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled("─".repeat(rule_len), Style::default().fg(DIM)),
    ]))
}

fn push_settings_section_header(items: &mut Vec<ListItem<'static>>, label: &str, width: u16) {
    if !items.is_empty() {
        items.push(ListItem::new(Line::from("")));
    }
    items.push(settings_section_header_item(label, width));
}

fn settings_value_line(row: SettingsRow, label: &str, value: &str, selected: bool) -> Line<'static> {
    let mut spans = vec![gutter_span()];
    spans.extend(settings_number_span(row, selected));
    spans.push(Span::styled(label.to_string(), Style::default().add_modifier(Modifier::BOLD)));
    spans.push(Span::styled(format!("  {value}"), secondary(selected)));
    Line::from(spans)
}

/// Render the collapsed `BorderPalette` row: expand glyph, bold label, and
/// an "N active" count. Same shape as the inline `Palette` row in
/// `draw_settings`, extracted so its text is independently testable.
fn border_palette_row_line(row: SettingsRow, active_count: usize, expanded: bool, selected: bool) -> Line<'static> {
    let glyph = if expanded { "▾" } else { "▸" };
    let mut spans = vec![gutter_span()];
    spans.extend(settings_number_span(row, selected));
    spans.push(Span::styled(format!("{glyph} "), secondary(selected)));
    spans.push(Span::styled("Border palette", Style::default().add_modifier(Modifier::BOLD)));
    spans.push(Span::styled(format!("  {active_count} active"), secondary(selected)));
    Line::from(spans)
}

/// Render a collapsed single-color settings row: a gutter bar, an expand
/// glyph, the bold label, a swatch, and the color's name. Used by Shortcut
/// highlight color, the one remaining row with an expandable direct-pick list.
fn settings_color_line(row: SettingsRow, label: &str, color_name: &str, expanded: bool, selected: bool) -> Line<'static> {
    let glyph = if expanded { "▾" } else { "▸" };
    let mut spans = vec![gutter_span()];
    spans.extend(settings_number_span(row, selected));
    spans.push(Span::styled(format!("{glyph} "), secondary(selected)));
    spans.push(Span::styled(label.to_string(), Style::default().add_modifier(Modifier::BOLD)));
    spans.push(Span::raw("  "));
    spans.push(Span::styled("██", Style::default().fg(color_from_name(color_name))));
    spans.push(Span::styled(format!(" {color_name}"), secondary(selected)));
    Line::from(spans)
}

/// Render one child row of an expanded single-color picker: a gutter bar, a
/// radio glyph (`●` if `name` is the currently selected color, `○`
/// otherwise), a swatch, and the name. Distinct from `PaletteColor`'s
/// `[x]`/`[ ]` checkbox glyph, which communicates "pick many" instead of
/// "pick one."
fn settings_color_option_line(name: &str, current: &str, selected: bool) -> Line<'static> {
    let radio = if name == current { "●" } else { "○" };
    Line::from(vec![
        gutter_span(),
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

fn dormant_numbering_label(number_dormant_sessions: bool) -> &'static str {
    if number_dormant_sessions { "Yes" } else { "No" }
}

fn session_metric_label(m: SessionMetric) -> &'static str {
    match m {
        SessionMetric::Recency => "Recency",
        SessionMetric::Age => "Age",
        SessionMetric::Hidden => "Hidden",
    }
}

fn remember_expanded_label(remember_expanded_sessions: bool) -> &'static str {
    if remember_expanded_sessions { "Yes" } else { "No" }
}

fn clear_dormant_on_attach_label(clear_dormant_on_attach: bool) -> &'static str {
    if clear_dormant_on_attach { "Yes" } else { "No" }
}

fn start_focus_mode_label(m: StartFocusMode) -> &'static str {
    match m {
        StartFocusMode::Remember => "Remember",
        StartFocusMode::Always => "Always",
        StartFocusMode::Never => "Never",
    }
}

fn color_policy_label(p: ColorPolicy) -> &'static str {
    match p {
        ColorPolicy::Rotate => "Rotate",
        ColorPolicy::Random => "Random",
        ColorPolicy::Static => "Static",
    }
}

fn always_show_shortcuts_label(always_show_shortcuts: bool) -> &'static str {
    if always_show_shortcuts { "Yes" } else { "No" }
}

fn dot_color_mode_label(m: DotColorMode) -> &'static str {
    match m {
        DotColorMode::Static => "Static",
        DotColorMode::Group => "Group",
    }
}

fn attached_color_mode_label(m: AttachedColorMode) -> &'static str {
    match m {
        AttachedColorMode::Static => "Static",
        AttachedColorMode::Match => "Group",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_settings_section_header_adds_blank_line_before_subsequent_headers() {
        let mut items: Vec<ListItem> = vec![ListItem::new(Line::from("existing row"))];
        push_settings_section_header(&mut items, "APPEARANCE", 40);
        assert_eq!(items.len(), 3, "a blank spacer plus the header should be appended after existing rows");
    }

    #[test]
    fn push_settings_section_header_skips_blank_line_when_list_is_empty() {
        let mut items: Vec<ListItem> = Vec::new();
        push_settings_section_header(&mut items, "BEHAVIOR", 40);
        assert_eq!(items.len(), 1, "no blank spacer should precede the very first header");
    }

    #[test]
    fn start_focus_mode_label_covers_all_three_states() {
        assert_eq!(start_focus_mode_label(StartFocusMode::Remember), "Remember");
        assert_eq!(start_focus_mode_label(StartFocusMode::Always), "Always");
        assert_eq!(start_focus_mode_label(StartFocusMode::Never), "Never");
    }

    #[test]
    fn description_line_highlights_question_mark_in_key_color() {
        let line = description_line("Core shortcuts are hidden. Press ? for the full shortcut list.", Color::Cyan);
        let spans = line.spans;
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "Core shortcuts are hidden. Press ");
        assert_eq!(spans[0].style.fg, None);
        assert_eq!(spans[1].content, "?");
        assert_eq!(spans[1].style.fg, Some(Color::Cyan));
        assert_eq!(spans[2].content, " for the full shortcut list.");
        assert_eq!(spans[2].style.fg, None);
    }

    #[test]
    fn description_line_with_no_question_mark_is_a_single_plain_span() {
        let line = description_line("On launch, rolomux opens in Command mode.", Color::Cyan);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content, "On launch, rolomux opens in Command mode.");
        assert_eq!(line.spans[0].style.fg, None);
    }

    #[test]
    fn draw_settings_shows_border_palette_row_with_active_count() {
        let line = border_palette_row_line(SettingsRow::BorderPalette, 6, false, false);
        let rendered = line.spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(rendered.contains("Border palette"));
        assert!(rendered.contains("6 active"));
    }
}
