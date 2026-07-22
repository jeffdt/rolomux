//! Settings-overlay rendering: `draw_settings` and its exclusive row, label,
//! and color-line builders. Shared style helpers (`styled_hint`,
//! `color_from_name`, `secondary`) and the palette constants stay in the parent
//! `ui` module and are reached through `use super::*`.

use super::*;

pub(super) const SETTINGS_FOOTER_HINT: &str =
    "j/k move · h/l cycle · Space toggle · c color · Esc back";

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
                settings_value_line("Default mode", default_mode_label(state.default_mode), selected)
            }
            SettingsRow::DormantNumbering => {
                settings_value_line(
                    "Number dormant sessions",
                    dormant_numbering_label(state.number_dormant_sessions),
                    selected,
                )
            }
            SettingsRow::RememberExpanded => {
                settings_value_line(
                    "Remember expanded sessions",
                    remember_expanded_label(state.remember_expanded_sessions),
                    selected,
                )
            }
            SettingsRow::SessionMetric => {
                settings_value_line("Session metadata", session_metric_label(state.session_metric), selected)
            }
            SettingsRow::ClearDormantOnAttach => {
                settings_value_line(
                    "Clear dormant on attach",
                    clear_dormant_on_attach_label(state.clear_dormant_on_attach),
                    selected,
                )
            }
            SettingsRow::StartFocusMode => {
                settings_value_line(
                    "Start in focus mode",
                    start_focus_mode_label(state.start_focus_mode),
                    selected,
                )
            }
            SettingsRow::NewGroupPosition => {
                settings_value_line(
                    "New group position",
                    new_group_position_label(state.new_group_position),
                    selected,
                )
            }
            SettingsRow::ShortcutVisibility => {
                settings_value_line(
                    "Show shortcuts",
                    shortcut_visibility_label(state.shortcut_visibility),
                    selected,
                )
            }
            SettingsRow::InboxIcon => {
                settings_value_line("Inbox icon", &state.inbox_icon, selected)
            }
            SettingsRow::AttachedColor => {
                let mut spans = vec![
                    gutter_span(),
                    Span::raw(" "),
                    Span::styled("Attached session color", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!("  {}", attached_color_mode_label(state.attached_color_mode)),
                        secondary(selected),
                    ),
                ];
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
                settings_value_line(
                    "Border color policy",
                    color_policy_label(state.border_color_policy),
                    selected,
                )
            }
            SettingsRow::BorderColor => {
                settings_color_line("Border color", &state.border_color, state.border_color_expanded(), selected)
            }
            SettingsRow::BorderColorOption(idx) => {
                settings_color_option_line(ALL_NAMED_COLORS[*idx], &state.border_color, selected)
            }
            SettingsRow::ShortcutColor => {
                settings_color_line("Shortcut highlight color", &state.shortcut_color, state.shortcut_color_expanded(), selected)
            }
            SettingsRow::ShortcutColorOption(idx) => {
                settings_color_option_line(ALL_NAMED_COLORS[*idx], &state.shortcut_color, selected)
            }
            SettingsRow::DotColorMode => {
                let mut spans = vec![
                    gutter_span(),
                    Span::raw(" "),
                    Span::styled("Active window dot color", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!("  {}", dot_color_mode_label(state.dot_color_mode)),
                        secondary(selected),
                    ),
                ];
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
                let mut spans = vec![
                    gutter_span(),
                    Span::raw(" "),
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
                    gutter_span(),
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
        Line::from(Span::styled(current_description, Style::default())),
        shortcut_hint_line(state, SETTINGS_FOOTER_HINT),
    ]);
    frame.render_widget(footer, footer_area);
}

/// The dim leading `│` every Settings row renders in its first column,
/// tying rows visually to their section header. Unlike the main session
/// list's per-group gutter color, every Settings row uses the same dim
/// color — there is no per-section color coding.
fn gutter_span() -> Span<'static> {
    Span::styled("│", Style::default().fg(DIM))
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

fn settings_value_line(label: &str, value: &str, selected: bool) -> Line<'static> {
    Line::from(vec![
        gutter_span(),
        Span::raw(" "),
        Span::styled(label.to_string(), Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {value}"), secondary(selected)),
    ])
}

/// Render a collapsed single-color settings row: a gutter bar, an expand
/// glyph, the bold label, a swatch, and the color's name. Shared by
/// Attached session color and Border color.
fn settings_color_line(label: &str, color_name: &str, expanded: bool, selected: bool) -> Line<'static> {
    let glyph = if expanded { "▾" } else { "▸" };
    Line::from(vec![
        gutter_span(),
        Span::styled(format!("{glyph} "), secondary(selected)),
        Span::styled(label.to_string(), Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("██", Style::default().fg(color_from_name(color_name))),
        Span::styled(format!(" {color_name}"), secondary(selected)),
    ])
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

fn new_group_position_label(p: NewGroupPosition) -> &'static str {
    match p {
        NewGroupPosition::Top => "Top",
        NewGroupPosition::Bottom => "Bottom",
    }
}

fn color_policy_label(p: ColorPolicy) -> &'static str {
    match p {
        ColorPolicy::Rotate => "Rotate",
        ColorPolicy::Random => "Random",
        ColorPolicy::Static => "Static",
    }
}

fn shortcut_visibility_label(v: ShortcutVisibility) -> &'static str {
    match v {
        ShortcutVisibility::Always => "Always",
        ShortcutVisibility::OnDemand => "On demand (?)",
    }
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
}
