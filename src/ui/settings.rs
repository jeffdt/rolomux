//! Settings-overlay rendering: `draw_settings` and its exclusive row, label,
//! and color-line builders. Shared style helpers (`styled_hint`,
//! `color_from_name`, `secondary`) and the palette constants stay in the parent
//! `ui` module and are reached through `use super::*`.

use super::*;

pub(super) const SETTINGS_FOOTER_HINT: &str =
    "j/k move · h/l cycle · Space toggle · c color · r/R reset · Esc back";

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
            SettingsRow::DefaultMode => push_settings_section_header(&mut items, "BEHAVIOR", Color::Blue, list_area.width),
            SettingsRow::AttachedColor => push_settings_section_header(&mut items, "COLORS", Color::Magenta, list_area.width),
            SettingsRow::InboxIcon => push_settings_section_header(&mut items, "APPEARANCE", Color::Green, list_area.width),
            _ => {}
        }
        let selected = i == state.settings_cursor();
        if selected {
            selected_line = Some(items.len());
        }
        let width = list_area.width;
        let changed = row.is_changed(state);
        let value = |text: &str| vec![Span::raw(text.to_string())];
        let line = match row {
            SettingsRow::DefaultMode => settings_row_line(
                *row,
                "Default mode",
                value(default_mode_label(state.default_mode)),
                changed,
                selected,
                width,
            ),
            SettingsRow::DormantNumbering => settings_row_line(
                *row,
                "Number dormant sessions",
                value(dormant_numbering_label(state.number_dormant_sessions)),
                changed,
                selected,
                width,
            ),
            SettingsRow::RememberExpanded => settings_row_line(
                *row,
                "Remember expanded sessions",
                value(remember_expanded_label(state.remember_expanded_sessions)),
                changed,
                selected,
                width,
            ),
            SettingsRow::SessionMetric => settings_row_line(
                *row,
                "Session metadata",
                value(session_metric_label(state.session_metric)),
                changed,
                selected,
                width,
            ),
            SettingsRow::ClearDormantOnAttach => settings_row_line(
                *row,
                "Clear dormant on attach",
                value(clear_dormant_on_attach_label(state.clear_dormant_on_attach)),
                changed,
                selected,
                width,
            ),
            SettingsRow::StartFocusMode => settings_row_line(
                *row,
                "Start in focus mode",
                value(start_focus_mode_label(state.start_focus_mode)),
                changed,
                selected,
                width,
            ),
            SettingsRow::ShortcutVisibility => settings_row_line(
                *row,
                "Show shortcuts",
                value(always_show_shortcuts_label(state.always_show_shortcuts)),
                changed,
                selected,
                width,
            ),
            SettingsRow::InboxIcon => {
                settings_row_line(*row, "Inbox icon", value(&state.inbox_icon), changed, selected, width)
            }
            SettingsRow::AttachedColor => settings_row_line(
                *row,
                "Attached session color",
                mode_with_static_swatch(
                    attached_color_mode_label(state.attached_color_mode),
                    (state.attached_color_mode == AttachedColorMode::Static).then_some(state.attached_color.as_str()),
                ),
                changed,
                selected,
                width,
            ),
            SettingsRow::BorderColorPolicy => settings_row_line(
                *row,
                "Border color",
                mode_with_static_swatch(
                    color_policy_label(state.border_color_policy),
                    (state.border_color_policy == ColorPolicy::Static).then_some(state.border_color.as_str()),
                ),
                changed,
                selected,
                width,
            ),
            SettingsRow::BorderPalette => settings_row_line(
                *row,
                &expandable_label("Border palette", state.border_palette_expanded()),
                value(&active_count_label(state.border_active_palette.len())),
                changed,
                selected,
                width,
            ),
            SettingsRow::BorderPaletteColor(idx) => {
                let (name, active) = &border_palette_entries[*idx];
                settings_checkbox_line(name, *active, selected)
            }
            SettingsRow::ShortcutColor => settings_row_line(
                *row,
                &expandable_label("Shortcut highlight color", state.shortcut_color_expanded()),
                swatch_value(&state.shortcut_color),
                changed,
                selected,
                width,
            ),
            SettingsRow::ShortcutColorOption(idx) => {
                settings_color_option_line(ALL_NAMED_COLORS[*idx], &state.shortcut_color, selected)
            }
            SettingsRow::DotColorMode => settings_row_line(
                *row,
                "Active window color",
                mode_with_static_swatch(
                    dot_color_mode_label(state.dot_color_mode),
                    (state.dot_color_mode == DotColorMode::Static).then_some(state.dot_color.as_str()),
                ),
                changed,
                selected,
                width,
            ),
            SettingsRow::ColorPolicy => settings_row_line(
                *row,
                "New group color",
                mode_with_static_swatch(
                    color_policy_label(state.new_group_color_policy),
                    (state.new_group_color_policy == ColorPolicy::Static).then_some(state.static_color.as_str()),
                ),
                changed,
                selected,
                width,
            ),
            SettingsRow::Palette => settings_row_line(
                *row,
                &expandable_label("Group palette", state.palette_expanded()),
                value(&active_count_label(state.active_palette.len())),
                changed,
                selected,
                width,
            ),
            SettingsRow::PaletteColor(idx) => {
                let (name, active) = &palette_entries[*idx];
                settings_checkbox_line(name, *active, selected)
            }
            SettingsRow::CaretStyle => {
                settings_row_line(*row, "Caret style", value(state.caret_style.glyph()), changed, selected, width)
            }
            SettingsRow::CaretBlink => settings_row_line(
                *row,
                "Caret blink",
                value(caret_blink_label(state.caret_blink)),
                changed,
                selected,
                width,
            ),
        };
        let dim_unused_palette = match row {
            SettingsRow::BorderPalette | SettingsRow::BorderPaletteColor(_) => {
                state.border_color_policy == ColorPolicy::Static
            }
            SettingsRow::Palette | SettingsRow::PaletteColor(_) => {
                state.new_group_color_policy == ColorPolicy::Static
            }
            _ => false,
        };
        items.push(dim_when(ListItem::new(line), dim_unused_palette));
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
        match state.settings_reset_all_warning() {
            Some(warning) => {
                Line::from(Span::styled(warning, Style::default().fg(WARNING).add_modifier(Modifier::BOLD)))
            }
            None => shortcut_hint_line(state, SETTINGS_FOOTER_HINT),
        },
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

/// Dim a palette row (and its expanded swatch children) while the palette
/// it belongs to has no effect: `BorderPalette` when Border color is Static,
/// `Palette` when New group color is Static. Same `Modifier::DIM`-under-spans
/// technique as `ui::recede`, applied here instead of reusing that function
/// since its doc is specific to group-altitude fade, a different concept.
fn dim_when(item: ListItem<'static>, dim: bool) -> ListItem<'static> {
    if dim {
        item.style(Style::default().add_modifier(Modifier::DIM))
    } else {
        item
    }
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

fn settings_section_header_item(label: &str, color: Color, width: u16) -> ListItem<'static> {
    let rule_len = (width as usize).saturating_sub(label.chars().count() + 2);
    ListItem::new(Line::from(vec![
        Span::styled(label.to_string(), Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled("─".repeat(rule_len), Style::default().fg(DIM)),
    ]))
}

fn push_settings_section_header(items: &mut Vec<ListItem<'static>>, label: &str, color: Color, width: u16) {
    if !items.is_empty() {
        items.push(ListItem::new(Line::from("")));
    }
    items.push(settings_section_header_item(label, color, width));
}

/// Marks a value that differs from its default, in front of the value and
/// in the same accent color the value itself takes on.
const CHANGED_MARKER: &str = "\u{2022} ";
const CHANGED_COLOR: Color = Color::Yellow;

/// Render a top-level Settings row: gutter, jump number and bold label on
/// the left, the value flush against the right edge (one cell of padding).
/// A value that differs from its default is drawn in `CHANGED_COLOR` behind
/// a `CHANGED_MARKER`; spans that already carry a color (swatches) keep it.
fn settings_row_line(
    row: SettingsRow,
    label: &str,
    value: Vec<Span<'static>>,
    changed: bool,
    selected: bool,
    width: u16,
) -> Line<'static> {
    let mut spans = vec![gutter_span()];
    spans.extend(settings_number_span(row, selected));
    spans.push(Span::styled(label.to_string(), Style::default().add_modifier(Modifier::BOLD)));
    let value_style = if changed { Style::default().fg(CHANGED_COLOR) } else { secondary(selected) };
    let mut right: Vec<Span<'static>> = Vec::new();
    if changed {
        right.push(Span::styled(CHANGED_MARKER, Style::default().fg(CHANGED_COLOR)));
    }
    right.extend(value.into_iter().map(|span| {
        if span.style.fg.is_some() { span } else { span.style(value_style) }
    }));
    let used: usize = spans.iter().chain(right.iter()).map(Span::width).sum();
    let gap = (width as usize).saturating_sub(used + 1).max(2);
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);
    Line::from(spans)
}

/// A label prefixed with its expand/collapse glyph, for a row that owns a
/// child list (the two palettes and Shortcut highlight color).
fn expandable_label(label: &str, expanded: bool) -> String {
    let glyph = if expanded { "\u{25be}" } else { "\u{25b8}" };
    format!("{glyph} {label}")
}

fn active_count_label(count: usize) -> String {
    format!("{count} active")
}

/// A color swatch followed by the color's name.
fn swatch_value(color_name: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled("\u{2588}\u{2588}", Style::default().fg(color_from_name(color_name))),
        Span::raw(format!(" {color_name}")),
    ]
}

/// A mode label, followed by its swatch while the mode is Static.
fn mode_with_static_swatch(mode_label: &str, static_color: Option<&str>) -> Vec<Span<'static>> {
    let mut spans = vec![Span::raw(mode_label.to_string())];
    if let Some(color) = static_color {
        spans.push(Span::raw("  "));
        spans.extend(swatch_value(color));
    }
    spans
}

/// Render one child row of an expanded palette checklist: a `[x]`/`[ ]`
/// checkbox, a swatch, and the name.
fn settings_checkbox_line(name: &str, active: bool, selected: bool) -> Line<'static> {
    let checkbox = if active { "[x]" } else { "[ ]" };
    Line::from(vec![
        gutter_span(),
        Span::raw("     "),
        Span::styled(checkbox.to_string(), secondary(selected)),
        Span::raw(" "),
        Span::styled("\u{2588}\u{2588}", Style::default().fg(color_from_name(name))),
        Span::raw(" "),
        Span::raw(name.to_string()),
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
    if number_dormant_sessions { "On" } else { "Off" }
}

fn caret_blink_label(caret_blink: bool) -> &'static str {
    if caret_blink { "On" } else { "Off" }
}

fn session_metric_label(m: SessionMetric) -> &'static str {
    match m {
        SessionMetric::Recency => "Recency",
        SessionMetric::Age => "Age",
        SessionMetric::Hidden => "Hidden",
    }
}

fn remember_expanded_label(remember_expanded_sessions: bool) -> &'static str {
    if remember_expanded_sessions { "On" } else { "Off" }
}

fn clear_dormant_on_attach_label(clear_dormant_on_attach: bool) -> &'static str {
    if clear_dormant_on_attach { "On" } else { "Off" }
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
    if always_show_shortcuts { "On" } else { "Off" }
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
        push_settings_section_header(&mut items, "APPEARANCE", Color::Green, 40);
        assert_eq!(items.len(), 3, "a blank spacer plus the header should be appended after existing rows");
    }

    #[test]
    fn push_settings_section_header_skips_blank_line_when_list_is_empty() {
        let mut items: Vec<ListItem> = Vec::new();
        push_settings_section_header(&mut items, "BEHAVIOR", Color::Blue, 40);
        assert_eq!(items.len(), 1, "no blank spacer should precede the very first header");
    }

    #[test]
    fn start_focus_mode_label_covers_all_three_states() {
        assert_eq!(start_focus_mode_label(StartFocusMode::Remember), "Remember");
        assert_eq!(start_focus_mode_label(StartFocusMode::Always), "Always");
        assert_eq!(start_focus_mode_label(StartFocusMode::Never), "Never");
    }

    #[test]
    fn caret_blink_label_covers_both_states() {
        assert_eq!(caret_blink_label(true), "On");
        assert_eq!(caret_blink_label(false), "Off");
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

    fn rendered(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn settings_footer_hint_and_reset_warning_fit_the_card() {
        let mut st = crate::model::PickerState::build(vec![], &crate::store::Config::default());
        st.settings_reset_all();
        let warning = st.settings_reset_all_warning().unwrap();
        for text in [SETTINGS_FOOTER_HINT, warning] {
            assert!(text.chars().count() <= 78, "{text:?} must fit the 78-col footer at real 84-col popup width");
        }
    }

    #[test]
    fn settings_row_line_pushes_the_value_flush_right_with_one_cell_of_padding() {
        let line = settings_row_line(SettingsRow::DefaultMode, "Default mode", vec![Span::raw("Command")], false, false, 40);
        let text = rendered(&line);
        assert_eq!(line.width(), 39, "fills the row except the trailing padding cell");
        let left = "\u{2502}1  Default mode";
        let gap = 39 - left.chars().count() - "Command".len();
        assert_eq!(text, format!("{left}{}Command", " ".repeat(gap)));
    }

    #[test]
    fn settings_row_line_marks_a_changed_value_in_the_accent_color() {
        let line = settings_row_line(SettingsRow::CaretBlink, "Caret blink", vec![Span::raw("On")], true, false, 40);
        let marker = line.spans.iter().find(|s| s.content == CHANGED_MARKER).expect("changed marker present");
        assert_eq!(marker.style.fg, Some(CHANGED_COLOR));
        let value = line.spans.last().unwrap();
        assert_eq!(value.content, "On");
        assert_eq!(value.style.fg, Some(CHANGED_COLOR));
        assert_eq!(line.width(), 39, "the marker is absorbed into the gap, not pushed past the edge");
    }

    #[test]
    fn settings_row_line_leaves_an_unchanged_value_unmarked_and_dim() {
        let line = settings_row_line(SettingsRow::CaretBlink, "Caret blink", vec![Span::raw("Off")], false, false, 40);
        assert!(!rendered(&line).contains(CHANGED_MARKER.trim()));
        assert_eq!(line.spans.last().unwrap().style.fg, Some(DIM));
    }

    #[test]
    fn a_changed_swatch_keeps_its_own_color() {
        let line = settings_row_line(
            SettingsRow::BorderColorPolicy,
            "Border color",
            mode_with_static_swatch("Static", Some("red")),
            true,
            false,
            60,
        );
        let swatch = line.spans.iter().find(|s| s.content == "\u{2588}\u{2588}").unwrap();
        assert_eq!(swatch.style.fg, Some(Color::Red));
        assert_eq!(line.spans.last().unwrap().content, " red");
        assert_eq!(line.spans.last().unwrap().style.fg, Some(CHANGED_COLOR));
    }

    #[test]
    fn draw_settings_shows_border_palette_row_with_active_count() {
        let line = settings_row_line(
            SettingsRow::BorderPalette,
            &expandable_label("Border palette", false),
            vec![Span::raw(active_count_label(6))],
            false,
            false,
            60,
        );
        let text = rendered(&line);
        assert!(text.contains("\u{25b8} Border palette"));
        assert!(text.ends_with("6 active"));
    }
}
