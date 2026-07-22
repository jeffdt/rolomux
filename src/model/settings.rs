//! The full-screen settings overlay: the `SettingsRow` row model plus the
//! `PickerState` state machine that drives cursor movement, expand/collapse
//! of the color sub-lists, and every toggle/cycle/palette edit.

use super::*;

/// Transient per-open UI state for the settings overlay: the cursor row and
/// which of the three color sub-lists are currently expanded. Rebuilt on every
/// open (never persisted), so it starts at its `Default`.
#[derive(Default)]
pub(super) struct SettingsUiState {
    cursor: usize,
    palette_expanded: bool,
    border_color_expanded: bool,
    shortcut_color_expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRow {
    DefaultMode,
    DormantNumbering,
    RememberExpanded,
    SessionMetric,
    ClearDormantOnAttach,
    StartFocusMode,
    NewGroupPosition,
    ShortcutVisibility,
    InboxIcon,
    AttachedColor,
    BorderColor,
    /// Index into `ALL_NAMED_COLORS`.
    BorderColorOption(usize),
    ShortcutColor,
    /// Index into `ALL_NAMED_COLORS`.
    ShortcutColorOption(usize),
    DotColorMode,
    ColorPolicy,
    Palette,
    /// Index into `PickerState::settings_palette_rows()`'s display order.
    PaletteColor(usize),
}

impl SettingsRow {
    /// A short, single-line explanation of what this setting does, shown
    /// on the Settings footer's description line. Child/option rows
    /// (individual color choices) reuse their parent setting's text since
    /// the option itself (a named color, a checkbox) is self-explanatory.
    pub fn description(&self) -> &'static str {
        match self {
            SettingsRow::DefaultMode => {
                "Whether the picker opens in Command mode or straight into Search."
            }
            SettingsRow::DormantNumbering => {
                "Whether visible dormant sessions get jump numbers (1-20)."
            }
            SettingsRow::RememberExpanded => {
                "When on, expand/collapse state persists across popups."
            }
            SettingsRow::SessionMetric => {
                "Whether the row's trailing timestamp shows Recency, Age, or is Hidden."
            }
            SettingsRow::ClearDormantOnAttach => {
                "When on, attaching to a dormant session automatically clears its dormant flag."
            }
            SettingsRow::StartFocusMode => {
                "Whether the picker starts in focus mode: Remember the last state, Always start in it, or Never start in it."
            }
            SettingsRow::NewGroupPosition => {
                "Where a newly created group is inserted: Top of the list, or Bottom (just above the inbox)."
            }
            SettingsRow::ShortcutVisibility => {
                "Whether the footer's key-shortcut legend is always visible, or hidden until you press ?."
            }
            SettingsRow::InboxIcon => "Which glyph prefixes the inbox group's header.",
            SettingsRow::AttachedColor => {
                "Highlight color for the session your tmux client is attached to."
            }
            SettingsRow::BorderColor | SettingsRow::BorderColorOption(_) => {
                "rolomux's own border frame color."
            }
            SettingsRow::ShortcutColor | SettingsRow::ShortcutColorOption(_) => {
                "Highlight color for key tokens in the footer's shortcut hints."
            }
            SettingsRow::DotColorMode => {
                "Color of the \u{25cf} marking a session's active window: a fixed color, or the session's own group color."
            }
            SettingsRow::ColorPolicy => {
                "How a new group picks its header color: Rotate, Random, or Static."
            }
            SettingsRow::Palette | SettingsRow::PaletteColor(_) => {
                "Which of the 16 terminal colors are in rotation for new group headers."
            }
        }
    }
}

impl PickerState {
    /// Enter the full-screen settings overlay.
    pub fn enter_settings(&mut self) {
        self.mode = Mode::Settings;
    }

    /// Leave settings mode back to session command mode.
    pub fn exit_settings(&mut self) {
        self.mode = Mode::Command;
    }

    /// The current cursor position within the settings rows (Task 4).
    pub fn settings_cursor(&self) -> usize {
        self.settings_ui.cursor
    }

    /// Whether the color-palette checklist is currently expanded (Task 4).
    pub fn palette_expanded(&self) -> bool {
        self.settings_ui.palette_expanded
    }

    /// Whether the Border color picker is currently expanded.
    pub fn border_color_expanded(&self) -> bool {
        self.settings_ui.border_color_expanded
    }

    /// Whether the Shortcut highlight color picker is currently expanded.
    pub fn shortcut_color_expanded(&self) -> bool {
        self.settings_ui.shortcut_color_expanded
    }

    /// The flat, ordered list of settings rows currently on screen. Three
    /// expandable sections (Border color, Shortcut color, Color palette) each
    /// splice their child rows in directly below themselves while expanded,
    /// same shape as the original Palette/PaletteColor pattern.
    pub fn settings_visible_rows(&self) -> Vec<SettingsRow> {
        let mut rows = vec![
            SettingsRow::DefaultMode,
            SettingsRow::DormantNumbering,
            SettingsRow::RememberExpanded,
            SettingsRow::SessionMetric,
            SettingsRow::ClearDormantOnAttach,
            SettingsRow::StartFocusMode,
            SettingsRow::NewGroupPosition,
            SettingsRow::ShortcutVisibility,
            SettingsRow::InboxIcon,
            SettingsRow::AttachedColor,
        ];
        rows.push(SettingsRow::BorderColor);
        if self.settings_ui.border_color_expanded {
            for i in 0..ALL_NAMED_COLORS.len() {
                rows.push(SettingsRow::BorderColorOption(i));
            }
        }
        rows.push(SettingsRow::ShortcutColor);
        if self.settings_ui.shortcut_color_expanded {
            for i in 0..ALL_NAMED_COLORS.len() {
                rows.push(SettingsRow::ShortcutColorOption(i));
            }
        }
        rows.push(SettingsRow::DotColorMode);
        rows.push(SettingsRow::ColorPolicy);
        rows.push(SettingsRow::Palette);
        if self.settings_ui.palette_expanded {
            for i in 0..ALL_NAMED_COLORS.len() {
                rows.push(SettingsRow::PaletteColor(i));
            }
        }
        rows
    }

    /// All 16 named colors in fixed `ALL_NAMED_COLORS` canonical order, each
    /// paired with whether it's currently active. The order never changes as
    /// colors are toggled, so checking/unchecking a color never reshuffles
    /// the list.
    pub fn settings_palette_rows(&self) -> Vec<(String, bool)> {
        ALL_NAMED_COLORS
            .iter()
            .map(|name| (name.to_string(), self.active_palette.iter().any(|c| c == name)))
            .collect()
    }

    /// Move the settings cursor by `delta`, wrapping between the first and last row.
    pub fn settings_move_cursor(&mut self, delta: i32) {
        self.settings_ui.cursor = move_index_with_edge_wrap(
            self.settings_ui.cursor,
            delta,
            self.settings_visible_rows().len(),
        );
    }

    /// The settings row the cursor currently sits on.
    fn current_settings_row(&self) -> SettingsRow {
        let rows = self.settings_visible_rows();
        rows[self.settings_ui.cursor.min(rows.len().saturating_sub(1))]
    }

    /// Place the cursor on `target`, found by scanning a freshly rebuilt
    /// `settings_visible_rows()`. Needed because expanding one section can
    /// shift every row below it by up to 16 positions, so a fixed index (as
    /// the codebase used before this section existed) is no longer safe.
    /// Falls back to row 0 if `target` isn't present (should not happen for
    /// any caller here, but never panics).
    fn focus_settings_row(&mut self, target: SettingsRow) {
        let rows = self.settings_visible_rows();
        self.settings_ui.cursor = rows.iter().position(|r| *r == target).unwrap_or(0);
    }

    /// Expand the Border color picker with the cursor starting on the
    /// currently selected color, not row 0 -- opening the picker always
    /// lands on the current value, like a standard radio picker.
    fn expand_border_color(&mut self) {
        self.settings_ui.border_color_expanded = true;
        let idx = ALL_NAMED_COLORS.iter().position(|c| *c == self.border_color).unwrap_or(0);
        self.focus_settings_row(SettingsRow::BorderColorOption(idx));
    }

    /// Commit `idx` as the new border color, collapse, and return the cursor
    /// to the parent row.
    fn select_border_color(&mut self, idx: usize) {
        self.border_color = ALL_NAMED_COLORS[idx].to_string();
        self.settings_ui.border_color_expanded = false;
        self.dirty = true;
        self.focus_settings_row(SettingsRow::BorderColor);
    }

    /// Same as `expand_border_color`, for Shortcut highlight color.
    fn expand_shortcut_color(&mut self) {
        self.settings_ui.shortcut_color_expanded = true;
        let idx = ALL_NAMED_COLORS.iter().position(|c| *c == self.shortcut_color).unwrap_or(0);
        self.focus_settings_row(SettingsRow::ShortcutColorOption(idx));
    }

    /// Same as `select_border_color`, for Shortcut highlight color.
    fn select_shortcut_color(&mut self, idx: usize) {
        self.shortcut_color = ALL_NAMED_COLORS[idx].to_string();
        self.settings_ui.shortcut_color_expanded = false;
        self.dirty = true;
        self.focus_settings_row(SettingsRow::ShortcutColor);
    }

    fn toggle_dormant_numbering(&mut self) {
        self.number_dormant_sessions = !self.number_dormant_sessions;
        self.dirty = true;
    }

    fn toggle_remember_expanded_sessions(&mut self) {
        self.remember_expanded_sessions = !self.remember_expanded_sessions;
        self.dirty = true;
    }

    fn toggle_clear_dormant_on_attach(&mut self) {
        self.clear_dormant_on_attach = !self.clear_dormant_on_attach;
        self.dirty = true;
    }

    fn cycle_start_focus_mode(&mut self) {
        self.start_focus_mode = self.start_focus_mode.next();
        self.dirty = true;
    }

    fn toggle_new_group_position(&mut self) {
        self.new_group_position = self.new_group_position.next();
        self.dirty = true;
    }

    fn cycle_session_metric(&mut self) {
        self.session_metric = self.session_metric.next();
        self.dirty = true;
    }

    fn toggle_shortcut_visibility(&mut self) {
        self.shortcut_visibility = self.shortcut_visibility.next();
        self.dirty = true;
    }

    fn toggle_dot_color_mode(&mut self) {
        self.dot_color_mode = self.dot_color_mode.next();
        self.dirty = true;
    }

    fn cycle_attached_color_mode(&mut self) {
        self.attached_color_mode = self.attached_color_mode.next();
        self.dirty = true;
    }

    /// `h` on the current settings row: step Default Mode / Color Policy
    /// backward, collapse an expanded section, or (from inside an expanded
    /// section's child row) cancel -- collapse without changing the value --
    /// and jump back to that section's parent row.
    pub fn settings_step_left(&mut self) {
        match self.current_settings_row() {
            SettingsRow::DefaultMode => {
                self.default_mode = self.default_mode.next();
                self.dirty = true;
            }
            SettingsRow::DormantNumbering => self.toggle_dormant_numbering(),
            SettingsRow::RememberExpanded => self.toggle_remember_expanded_sessions(),
            SettingsRow::SessionMetric => self.cycle_session_metric(),
            SettingsRow::ClearDormantOnAttach => self.toggle_clear_dormant_on_attach(),
            SettingsRow::StartFocusMode => self.cycle_start_focus_mode(),
            SettingsRow::NewGroupPosition => self.toggle_new_group_position(),
            SettingsRow::ShortcutVisibility => self.toggle_shortcut_visibility(),
            SettingsRow::InboxIcon => {
                self.inbox_icon = Self::cycle_inbox_icon(&self.inbox_icon, -1);
                self.dirty = true;
            }
            SettingsRow::AttachedColor => self.cycle_attached_color_mode(),
            SettingsRow::BorderColor => self.settings_ui.border_color_expanded = false,
            SettingsRow::BorderColorOption(_) => {
                self.settings_ui.border_color_expanded = false;
                self.focus_settings_row(SettingsRow::BorderColor);
            }
            SettingsRow::ShortcutColor => self.settings_ui.shortcut_color_expanded = false,
            SettingsRow::ShortcutColorOption(_) => {
                self.settings_ui.shortcut_color_expanded = false;
                self.focus_settings_row(SettingsRow::ShortcutColor);
            }
            SettingsRow::DotColorMode => self.toggle_dot_color_mode(),
            SettingsRow::ColorPolicy => {
                self.new_group_color_policy = self.new_group_color_policy.prev();
                self.dirty = true;
            }
            SettingsRow::Palette => self.settings_ui.palette_expanded = false,
            SettingsRow::PaletteColor(_) => {
                self.settings_ui.palette_expanded = false;
                self.focus_settings_row(SettingsRow::Palette);
            }
        }
    }

    /// `l` on the current settings row: step Default Mode / Color Policy
    /// forward, or expand a section (Border color, Color palette). A no-op
    /// on an already-expanded section's child row -- there is nothing
    /// further to expand, and selection there happens via `Enter`/`Space`
    /// (`settings_activate`), not `l`.
    pub fn settings_step_right(&mut self) {
        match self.current_settings_row() {
            SettingsRow::DefaultMode => {
                self.default_mode = self.default_mode.next();
                self.dirty = true;
            }
            SettingsRow::DormantNumbering => self.toggle_dormant_numbering(),
            SettingsRow::RememberExpanded => self.toggle_remember_expanded_sessions(),
            SettingsRow::SessionMetric => self.cycle_session_metric(),
            SettingsRow::ClearDormantOnAttach => self.toggle_clear_dormant_on_attach(),
            SettingsRow::StartFocusMode => self.cycle_start_focus_mode(),
            SettingsRow::NewGroupPosition => self.toggle_new_group_position(),
            SettingsRow::ShortcutVisibility => self.toggle_shortcut_visibility(),
            SettingsRow::InboxIcon => {
                self.inbox_icon = Self::cycle_inbox_icon(&self.inbox_icon, 1);
                self.dirty = true;
            }
            SettingsRow::AttachedColor => self.cycle_attached_color_mode(),
            SettingsRow::BorderColor => self.expand_border_color(),
            SettingsRow::BorderColorOption(_) => {}
            SettingsRow::ShortcutColor => self.expand_shortcut_color(),
            SettingsRow::ShortcutColorOption(_) => {}
            SettingsRow::DotColorMode => self.toggle_dot_color_mode(),
            SettingsRow::ColorPolicy => {
                self.new_group_color_policy = self.new_group_color_policy.next();
                self.dirty = true;
            }
            SettingsRow::Palette => self.settings_ui.palette_expanded = true,
            SettingsRow::PaletteColor(_) => {}
        }
    }

    /// `Enter`/`Space` on the current settings row: steps Default Mode /
    /// Color Policy forward (same as `l`), expands a collapsed color section
    /// (same as `l`), commits the color under the cursor on an expanded
    /// section's child row (radio-select: pick one, collapse), or toggles a
    /// palette color's active state (checkbox: pick many, stays expanded).
    pub fn settings_activate(&mut self) {
        match self.current_settings_row() {
            SettingsRow::DefaultMode
            | SettingsRow::DormantNumbering
            | SettingsRow::RememberExpanded
            | SettingsRow::SessionMetric
            | SettingsRow::ClearDormantOnAttach
            | SettingsRow::StartFocusMode
            | SettingsRow::NewGroupPosition
            | SettingsRow::ShortcutVisibility
            | SettingsRow::InboxIcon
            | SettingsRow::AttachedColor
            | SettingsRow::DotColorMode
            | SettingsRow::ColorPolicy => self.settings_step_right(),
            SettingsRow::BorderColor => self.expand_border_color(),
            SettingsRow::BorderColorOption(idx) => self.select_border_color(idx),
            SettingsRow::ShortcutColor => self.expand_shortcut_color(),
            SettingsRow::ShortcutColorOption(idx) => self.select_shortcut_color(idx),
            SettingsRow::Palette => {}
            SettingsRow::PaletteColor(_) => self.settings_toggle_palette_color(),
        }
    }

    /// The palette-checklist index under the cursor, if the cursor is
    /// currently on a `PaletteColor` row. Shared by the palette mutation
    /// methods below so they resolve "which color" the same way `h`/`l`/`Enter`
    /// resolve "which row".
    fn current_palette_color_idx(&self) -> Option<usize> {
        match self.current_settings_row() {
            SettingsRow::PaletteColor(i) => Some(i),
            _ => None,
        }
    }

    /// Toggle the palette color under the cursor active/inactive. `active_palette`
    /// is kept in `ALL_NAMED_COLORS` canonical order on every mutation, so
    /// rotation/cycle order always matches the checklist's fixed display order
    /// and toggling a color never reshuffles the list. Guarded: the last
    /// active color can never be deactivated (several resolution paths
    /// divide/index by `active_palette.len()`).
    fn settings_toggle_palette_color(&mut self) {
        let idx = match self.current_palette_color_idx() {
            Some(i) => i,
            None => return,
        };
        let name = ALL_NAMED_COLORS[idx];
        if self.active_palette.iter().any(|c| c == name) {
            if self.active_palette.len() <= 1 {
                return;
            }
            self.active_palette.retain(|c| c != name);
        } else {
            self.active_palette.push(name.to_string());
            self.active_palette
                .sort_by_key(|c| ALL_NAMED_COLORS.iter().position(|n| n == c).unwrap_or(usize::MAX));
        }
        self.dirty = true;
    }

    /// Step `current` forward one position through `ALL_NAMED_COLORS`,
    /// wrapping from white back to black. Shared by every raw single-color
    /// cycle in Settings so the wrap-around index logic lives in exactly one
    /// place.
    fn cycle_named_color(current: &str) -> String {
        let idx = ALL_NAMED_COLORS.iter().position(|c| *c == current).unwrap_or(0);
        ALL_NAMED_COLORS[(idx + 1) % ALL_NAMED_COLORS.len()].to_string()
    }

    /// Step `current`'s position in `INBOX_ICONS` by `delta` (`+1` or `-1`),
    /// wrapping in both directions. Unlike `cycle_named_color` (forward-only,
    /// used by the `c` quick-cycle), this backs `h`/`l` on the `InboxIcon`
    /// row directly, so it needs a real "previous": with 22 entries,
    /// forward-only stepping to reach the previous one would take up to 21
    /// keystrokes.
    fn cycle_inbox_icon(current: &str, delta: i32) -> String {
        let len = INBOX_ICONS.len() as i32;
        let idx = INBOX_ICONS.iter().position(|c| *c == current).unwrap_or(0) as i32;
        let next = (idx + delta).rem_euclid(len);
        INBOX_ICONS[next as usize].to_string()
    }

    /// `c`: cycle the current row's raw color value forward through all 16
    /// named colors. Applies to the Color Policy and Active window dot color
    /// rows only while their mode is Static (the nested `static_color` /
    /// `dot_color`), and to the three standalone color rows (`attached_color`,
    /// `border_color`, `shortcut_color`) whether collapsed or expanded. A
    /// no-op everywhere else, so `c` never surprises a row that isn't a raw
    /// color picker.
    pub fn settings_cycle_color(&mut self) {
        match self.current_settings_row() {
            SettingsRow::ColorPolicy if self.new_group_color_policy == ColorPolicy::Static => {
                self.static_color = Self::cycle_named_color(&self.static_color);
                self.dirty = true;
            }
            SettingsRow::DotColorMode if self.dot_color_mode == DotColorMode::Static => {
                self.dot_color = Self::cycle_named_color(&self.dot_color);
                self.dirty = true;
            }
            SettingsRow::AttachedColor => {
                self.attached_color = Self::cycle_named_color(&self.attached_color);
                self.dirty = true;
            }
            SettingsRow::BorderColor => {
                self.border_color = Self::cycle_named_color(&self.border_color);
                self.dirty = true;
            }
            SettingsRow::ShortcutColor => {
                self.shortcut_color = Self::cycle_named_color(&self.shortcut_color);
                self.dirty = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::*;
    use crate::model::test_support::*;
    use crate::store::Config;

    fn settings_state() -> PickerState {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default();
        PickerState::build(sessions, &cfg)
    }

    #[test]
    fn activate_also_cycles_default_mode_forward() {
        let mut st = settings_state();
        st.settings_activate();
        assert_eq!(st.default_mode, DefaultMode::Search);
    }

    #[test]
    fn activate_cannot_deactivate_the_last_active_color() {
        let mut st = settings_state();
        st.active_palette = vec!["cyan".to_string()];
        st.settings_move_cursor(14); // Palette
        st.settings_step_right();
        let cyan_idx = st.settings_palette_rows().iter().position(|(n, _)| n == "cyan").unwrap();
        st.settings_move_cursor(1 + cyan_idx as i32); // the only active color
        st.settings_activate();
        assert_eq!(st.active_palette, vec!["cyan".to_string()], "guard: last active color stays");
    }

    #[test]
    fn activate_on_a_border_color_option_commits_and_collapses() {
        let mut st = settings_state();
        st.settings_move_cursor(10); // BorderColor
        st.settings_step_right(); // expand, cursor lands on index 2 (green)
        st.settings_move_cursor(1); // step to index 3 ("yellow")
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::BorderColorOption(3));
        st.settings_activate();
        assert_eq!(st.border_color, "yellow");
        assert!(st.dirty);
        assert_eq!(st.settings_cursor(), 10, "cursor returned to the BorderColor row");
    }

    #[test]
    fn activate_on_a_shortcut_color_option_commits_and_collapses() {
        let mut st = settings_state();
        st.settings_move_cursor(11); // ShortcutColor
        st.settings_step_right(); // expand, cursor lands on the current color (gray)
        st.settings_move_cursor(1);
        let SettingsRow::ShortcutColorOption(idx) = st.settings_visible_rows()[st.settings_cursor()] else {
            panic!("expected a ShortcutColorOption row");
        };
        st.settings_activate();
        assert_eq!(st.shortcut_color, ALL_NAMED_COLORS[idx]);
        assert!(st.dirty);
        assert_eq!(st.settings_visible_rows().len(), 15, "collapsed after committing");
        assert_eq!(st.settings_cursor(), 11, "cursor returned to the ShortcutColor row");
    }

    #[test]
    fn activate_reactivates_an_inactive_color_at_its_canonical_position() {
        let mut st = settings_state(); // active: cyan, green, yellow, magenta, blue, red
        st.settings_move_cursor(14); // Palette
        st.settings_step_right();
        let black_idx = st.settings_palette_rows().iter().position(|(n, _)| n == "black").unwrap();
        st.settings_move_cursor(1 + black_idx as i32); // descend onto the "black" child row
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::PaletteColor(black_idx));
        st.settings_activate();
        assert!(st.active_palette.contains(&"black".to_string()));
        // "black" is first in ALL_NAMED_COLORS canonical order, so reactivating
        // it inserts it at the front of active_palette, not the end: rotation
        // order always matches the checklist's fixed display order.
        assert_eq!(
            st.active_palette.first(),
            Some(&"black".to_string()),
            "newly activated color is inserted at its canonical position"
        );
    }

    #[test]
    fn activate_toggles_a_palette_color_off() {
        let mut st = settings_state();
        st.settings_move_cursor(14); // Palette
        st.settings_step_right(); // expand
        let cyan_idx = st.settings_palette_rows().iter().position(|(n, _)| n == "cyan").unwrap();
        st.settings_move_cursor(1 + cyan_idx as i32); // descend onto the "cyan" child row
        assert_eq!(st.settings_palette_rows()[cyan_idx], ("cyan".to_string(), true));
        st.settings_activate();
        assert!(!st.active_palette.contains(&"cyan".to_string()));
        assert!(st.dirty);
    }

    #[test]
    fn border_color_expands_and_collapses_via_step_right_and_left() {
        let mut st = settings_state();
        st.settings_move_cursor(10); // row 10: BorderColor
        st.settings_step_right();
        assert_eq!(st.settings_visible_rows().len(), 15 + 16);
        assert_eq!(
            st.settings_visible_rows()[st.settings_cursor()],
            SettingsRow::BorderColorOption(2),
            "cursor lands on the currently selected color (green, index 2)"
        );
        st.settings_step_left();
        assert_eq!(st.settings_visible_rows().len(), 15);
        assert_eq!(st.settings_cursor(), 10, "cursor returned to the BorderColor row");
    }

    #[test]
    fn shortcut_color_expands_and_collapses_via_step_right_and_left() {
        let mut st = settings_state();
        st.settings_move_cursor(11); // row 11: ShortcutColor
        st.settings_step_right();
        assert_eq!(st.settings_visible_rows().len(), 15 + 16);
        assert_eq!(
            st.settings_visible_rows()[st.settings_cursor()],
            SettingsRow::ShortcutColorOption(7),
            "cursor lands on the currently selected color (gray, index 7)"
        );
        st.settings_step_left();
        assert_eq!(st.settings_visible_rows().len(), 15);
        assert_eq!(st.settings_cursor(), 11, "cursor returned to the ShortcutColor row");
    }

    #[test]
    fn c_key_is_a_noop_off_a_color_row() {
        let mut st = settings_state();
        st.settings_move_cursor(13); // ColorPolicy
        st.settings_step_right(); st.settings_step_right(); // -> Static
        st.settings_move_cursor(-13); // back to DefaultMode row
        st.settings_cycle_color();
        assert_eq!(st.static_color, "cyan", "cursor must be on a color row");
    }

    #[test]
    fn c_key_only_cycles_static_color_when_policy_is_static() {
        let mut st = settings_state();
        st.settings_move_cursor(13); // ColorPolicy row, policy still Rotate
        st.settings_cycle_color();
        assert_eq!(st.static_color, "cyan", "no-op: policy is not Static");

        st.settings_step_right(); // Rotate -> Random
        st.settings_cycle_color();
        assert_eq!(st.static_color, "cyan", "no-op: policy is Random, not Static");

        st.settings_step_right(); // Random -> Static
        assert_eq!(st.new_group_color_policy, ColorPolicy::Static);
        st.settings_cycle_color();
        assert_eq!(st.static_color, "gray", "cycles to the next of all 16 named colors after cyan");
        assert!(st.dirty);
    }

    #[test]
    fn c_key_quick_cycles_attached_color_without_expanding() {
        let mut st = settings_state();
        st.settings_move_cursor(9); // AttachedColor, collapsed
        st.settings_cycle_color();
        assert_eq!(st.attached_color, "yellow", "green -> yellow, next in ALL_NAMED_COLORS");
        assert!(st.dirty);
        assert_eq!(st.settings_visible_rows().len(), 15, "stays collapsed");
    }

    #[test]
    fn c_key_quick_cycles_border_color_without_expanding() {
        let mut st = settings_state();
        st.settings_move_cursor(10); // BorderColor, collapsed
        st.settings_cycle_color();
        assert_eq!(st.border_color, "yellow");
        assert!(st.dirty);
        assert_eq!(st.settings_visible_rows().len(), 15, "stays collapsed");
    }

    #[test]
    fn c_key_quick_cycles_shortcut_color_without_expanding() {
        let mut st = settings_state();
        st.settings_move_cursor(11); // ShortcutColor, collapsed
        st.settings_cycle_color();
        assert_eq!(st.shortcut_color, "darkgray", "gray -> darkgray, next in ALL_NAMED_COLORS");
        assert!(st.dirty);
        assert_eq!(st.settings_visible_rows().len(), 15, "stays collapsed");
    }

    #[test]
    fn c_key_only_cycles_dot_color_when_mode_is_static() {
        let mut st = settings_state();
        st.settings_move_cursor(12); // DotColorMode row, mode still Static (the default)
        st.settings_cycle_color();
        assert_eq!(st.dot_color, "yellow", "green -> yellow, next in ALL_NAMED_COLORS");
        assert!(st.dirty);

        st.settings_step_right(); // Static -> Group
        st.dirty = false;
        st.settings_cycle_color();
        assert_eq!(st.dot_color, "yellow", "no-op: mode is Group, not Static");
        assert!(!st.dirty);
    }

    #[test]
    fn inbox_icon_defaults_to_circled_asterisk() {
        let st = settings_state();
        assert_eq!(st.inbox_icon, "⊛");
    }

    #[test]
    fn h_and_l_cycle_inbox_icon_forward_and_backward_with_wraparound() {
        let mut st = settings_state();
        st.settings_move_cursor(8); // InboxIcon
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::InboxIcon);
        st.settings_step_right();
        assert_eq!(st.inbox_icon, "☆", "steps to the second curated glyph");
        st.settings_step_left();
        assert_eq!(st.inbox_icon, "⊛", "steps back to the default");
        st.settings_step_left();
        assert_eq!(st.inbox_icon, "♧", "wraps backward to the last glyph in the curated list");
        st.settings_step_right();
        assert_eq!(st.inbox_icon, "⊛", "wraps forward from the last glyph back to the default");
        assert!(st.dirty);
    }

    #[test]
    fn enter_on_inbox_icon_row_steps_forward_same_as_l() {
        let mut st = settings_state();
        st.settings_move_cursor(8);
        st.settings_activate();
        assert_eq!(st.inbox_icon, "☆");
    }

    #[test]
    fn enter_and_exit_settings_toggles_mode() {
        let mut st = grouped_state();
        assert_eq!(st.mode, Mode::Command);
        st.enter_settings();
        assert_eq!(st.mode, Mode::Settings);
        st.exit_settings();
        assert_eq!(st.mode, Mode::Command);
    }

    #[test]
    fn expanding_and_collapsing_palette_still_refocuses_correctly_with_other_sections_expanded() {
        // Regression guard for the dynamic collapse-cursor refactor: Palette's
        // own index is no longer fixed once BorderColor/ShortcutColor can also
        // expand above it. (AttachedColor no longer expands since gaining a
        // Static/Match mode -- see settings_step_left_and_right_toggle_attached_color_mode.)
        let mut st = settings_state();
        st.settings_move_cursor(10); // BorderColor
        st.settings_step_right(); // expand BorderColor: 16 rows now sit between it and ShortcutColor/DotColorMode/ColorPolicy/Palette
        st.settings_move_cursor(-1);
        st.settings_step_left(); // collapse BorderColor again, back to the 15-row layout
        assert_eq!(st.settings_visible_rows().len(), 15);
        st.settings_move_cursor(4); // BorderColor(10) -> Palette(14)
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::Palette);
        st.settings_step_right(); // expand Palette
        st.settings_move_cursor(1); // first PaletteColor child
        st.settings_step_left(); // collapse
        assert_eq!(st.settings_cursor(), 14, "Palette collapse still lands on index 14");
    }

    #[test]
    fn palette_expands_and_collapses_via_step_right_and_left() {
        let mut st = settings_state();
        st.settings_move_cursor(14); // row 14: Palette
        assert!(!st.palette_expanded());
        st.settings_step_right();
        assert!(st.palette_expanded());
        assert_eq!(st.settings_visible_rows().len(), 15 + 16);
        st.settings_step_left();
        assert!(!st.palette_expanded());
        assert_eq!(st.settings_visible_rows().len(), 15);
    }

    #[test]
    fn settings_cycles_session_metric_through_all_three_states() {
        let mut st = settings_state();
        st.settings_move_cursor(3); // SessionMetric
        assert_eq!(st.session_metric, SessionMetric::Recency);
        st.settings_step_right();
        assert_eq!(st.session_metric, SessionMetric::Age);
        st.settings_step_right();
        assert_eq!(st.session_metric, SessionMetric::Hidden);
        st.settings_step_left(); // h also advances a 3-state cycle, same as l
        assert_eq!(st.session_metric, SessionMetric::Recency, "wraps back via next()");
        st.settings_activate();
        assert_eq!(st.session_metric, SessionMetric::Age);
        assert!(st.dirty);
    }

    #[test]
    fn settings_cycles_start_focus_mode_through_all_three_states() {
        let mut st = settings_state();
        st.settings_move_cursor(5); // StartFocusMode
        assert_eq!(st.current_settings_row(), SettingsRow::StartFocusMode);
        assert_eq!(st.start_focus_mode, StartFocusMode::Remember);
        st.settings_step_right();
        assert_eq!(st.start_focus_mode, StartFocusMode::Always);
        st.settings_step_right();
        assert_eq!(st.start_focus_mode, StartFocusMode::Never);
        st.settings_step_left(); // h also advances a 3-state cycle, same as l
        assert_eq!(st.start_focus_mode, StartFocusMode::Remember, "wraps back via next()");
        st.settings_activate();
        assert_eq!(st.start_focus_mode, StartFocusMode::Always);
        assert!(st.dirty);
    }

    #[test]
    fn settings_move_cursor_wraps_between_first_and_last_row() {
        let mut st = settings_state();
        assert_eq!(st.settings_cursor(), 0);
        st.settings_move_cursor(-1);
        assert_eq!(st.settings_cursor(), 14, "moving up from the top wraps to bottom");
        st.settings_move_cursor(1);
        assert_eq!(st.settings_cursor(), 0, "moving down from the bottom wraps to top");
        st.settings_move_cursor(1);
        assert_eq!(st.settings_cursor(), 1);
        st.settings_move_cursor(99);
        assert_eq!(st.settings_cursor(), 14, "large jumps still land on the edge");
    }

    #[test]
    fn settings_palette_rows_lists_all_sixteen_in_fixed_canonical_order() {
        let st = settings_state(); // default active_palette = HEADER_COLORS: cyan,green,yellow,magenta,blue,red
        let rows = st.settings_palette_rows();
        assert_eq!(rows.len(), 16);
        // Order never changes with active/inactive status: it's always
        // ALL_NAMED_COLORS canonical order, so a toggle never reshuffles it.
        assert_eq!(&rows[0], &("black".to_string(), false));
        assert_eq!(&rows[1], &("red".to_string(), true));
        assert_eq!(&rows[2], &("green".to_string(), true));
        assert_eq!(&rows[3], &("yellow".to_string(), true));
        assert_eq!(&rows[4], &("blue".to_string(), true));
        assert_eq!(&rows[5], &("magenta".to_string(), true));
        assert_eq!(&rows[6], &("cyan".to_string(), true));
        assert_eq!(&rows[7], &("gray".to_string(), false));
    }

    #[test]
    fn settings_step_left_and_right_toggle_new_group_position() {
        let mut st = grouped_state();
        assert_eq!(st.new_group_position, NewGroupPosition::Bottom);
        st.settings_move_cursor(6); // NewGroupPosition row (after ClearDormantOnAttach, StartFocusMode)
        assert_eq!(st.current_settings_row(), SettingsRow::NewGroupPosition);
        st.settings_step_right();
        assert_eq!(st.new_group_position, NewGroupPosition::Top);
        st.settings_step_right();
        assert_eq!(st.new_group_position, NewGroupPosition::Bottom, "only two values, so right also wraps");
        st.settings_step_left();
        assert_eq!(st.new_group_position, NewGroupPosition::Top);
    }

    #[test]
    fn settings_step_left_and_right_toggle_shortcut_visibility() {
        let mut st = settings_state();
        assert_eq!(st.shortcut_visibility, ShortcutVisibility::Always);
        st.settings_move_cursor(7); // ShortcutVisibility row (after NewGroupPosition)
        assert_eq!(st.current_settings_row(), SettingsRow::ShortcutVisibility);
        st.settings_step_right();
        assert_eq!(st.shortcut_visibility, ShortcutVisibility::OnDemand);
        st.settings_step_right();
        assert_eq!(st.shortcut_visibility, ShortcutVisibility::Always, "only two values, so right also wraps");
        st.settings_step_left();
        assert_eq!(st.shortcut_visibility, ShortcutVisibility::OnDemand);
        assert!(st.dirty);
    }

    #[test]
    fn settings_step_left_and_right_toggle_dot_color_mode() {
        let mut st = settings_state();
        assert_eq!(st.dot_color_mode, DotColorMode::Static);
        st.settings_move_cursor(12); // DotColorMode row
        assert_eq!(st.current_settings_row(), SettingsRow::DotColorMode);
        st.settings_step_right();
        assert_eq!(st.dot_color_mode, DotColorMode::Group);
        st.settings_step_right();
        assert_eq!(st.dot_color_mode, DotColorMode::Static, "only two values, so right also wraps");
        st.settings_step_left();
        assert_eq!(st.dot_color_mode, DotColorMode::Group);
        assert!(st.dirty);
        st.settings_activate();
        assert_eq!(st.dot_color_mode, DotColorMode::Static, "Enter/Space also steps forward");
    }

    #[test]
    fn settings_step_left_and_right_toggle_attached_color_mode() {
        let mut st = settings_state();
        assert_eq!(st.attached_color_mode, AttachedColorMode::Static);
        st.settings_move_cursor(9); // AttachedColor row
        assert_eq!(st.current_settings_row(), SettingsRow::AttachedColor);
        st.settings_step_right();
        assert_eq!(st.attached_color_mode, AttachedColorMode::Match);
        st.settings_step_right();
        assert_eq!(st.attached_color_mode, AttachedColorMode::Static, "only two values, so right also wraps");
        st.settings_step_left();
        assert_eq!(st.attached_color_mode, AttachedColorMode::Match);
        assert!(st.dirty);
        st.settings_activate();
        assert_eq!(st.attached_color_mode, AttachedColorMode::Static, "Enter/Space also steps forward");
    }

    #[test]
    fn settings_toggle_dormant_numbering_in_either_direction() {
        let mut st = settings_state();
        st.settings_move_cursor(1); // DormantNumbering
        assert!(st.number_dormant_sessions);
        st.settings_step_right();
        assert!(!st.number_dormant_sessions);
        st.settings_step_left();
        assert!(st.number_dormant_sessions);
        st.settings_activate();
        assert!(!st.number_dormant_sessions);
        assert!(st.dirty);
    }

    #[test]
    fn settings_toggle_remember_expanded_in_either_direction() {
        let mut st = settings_state();
        st.settings_move_cursor(2); // RememberExpanded
        assert!(!st.remember_expanded_sessions);
        st.settings_step_right();
        assert!(st.remember_expanded_sessions);
        st.settings_step_left();
        assert!(!st.remember_expanded_sessions);
        st.settings_activate();
        assert!(st.remember_expanded_sessions);
        assert!(st.dirty);
    }

    #[test]
    fn settings_visible_rows_collapsed_shows_fifteen_rows_in_order() {
        let st = settings_state();
        assert_eq!(
            st.settings_visible_rows(),
            vec![
                SettingsRow::DefaultMode,
                SettingsRow::DormantNumbering,
                SettingsRow::RememberExpanded,
                SettingsRow::SessionMetric,
                SettingsRow::ClearDormantOnAttach,
                SettingsRow::StartFocusMode,
                SettingsRow::NewGroupPosition,
                SettingsRow::ShortcutVisibility,
                SettingsRow::InboxIcon,
                SettingsRow::AttachedColor,
                SettingsRow::BorderColor,
                SettingsRow::ShortcutColor,
                SettingsRow::DotColorMode,
                SettingsRow::ColorPolicy,
                SettingsRow::Palette,
            ]
        );
    }

    #[test]
    fn static_color_persists_across_policy_switches() {
        let mut st = settings_state();
        st.settings_move_cursor(13); // ColorPolicy
        st.settings_step_right(); st.settings_step_right(); // -> Static
        st.settings_cycle_color(); // cyan -> gray
        assert_eq!(st.static_color, "gray");
        st.settings_step_right(); // Static -> Rotate
        assert_eq!(st.static_color, "gray", "not cleared by switching away from Static");
        st.settings_step_right(); st.settings_step_right(); // Random -> Static
        assert_eq!(st.static_color, "gray", "round-trips back without loss");
    }

    #[test]
    fn step_cycles_color_policy_forward_and_backward() {
        let mut st = settings_state();
        st.settings_move_cursor(13); // row 13: ColorPolicy
        assert_eq!(st.new_group_color_policy, ColorPolicy::Rotate);
        st.settings_step_right();
        assert_eq!(st.new_group_color_policy, ColorPolicy::Random);
        st.settings_step_right();
        assert_eq!(st.new_group_color_policy, ColorPolicy::Static);
        st.settings_step_right();
        assert_eq!(st.new_group_color_policy, ColorPolicy::Rotate, "wraps forward");
        st.settings_step_left();
        assert_eq!(st.new_group_color_policy, ColorPolicy::Static, "wraps backward");
    }

    #[test]
    fn step_cycles_default_mode_in_either_direction() {
        let mut st = settings_state(); // cursor on row 0, DefaultMode
        assert_eq!(st.default_mode, DefaultMode::Command);
        st.settings_step_right();
        assert_eq!(st.default_mode, DefaultMode::Search);
        st.settings_step_right();
        assert_eq!(st.default_mode, DefaultMode::Command, "2-state cycle wraps");
        st.settings_step_left();
        assert_eq!(st.default_mode, DefaultMode::Search, "h also flips a 2-state toggle");
        assert!(st.dirty);
    }

    #[test]
    fn step_left_on_a_palette_color_row_collapses_and_refocuses_the_parent() {
        let mut st = settings_state();
        st.settings_move_cursor(14); // Palette
        st.settings_step_right(); // expand
        st.settings_move_cursor(1); // onto the first PaletteColor child
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::PaletteColor(0));
        st.settings_step_left();
        assert!(!st.palette_expanded());
        assert_eq!(st.settings_cursor(), 14, "cursor returns to the Palette row");
    }

    #[test]
    fn toggling_a_color_never_reorders_the_checklist() {
        let mut st = settings_state();
        st.settings_move_cursor(14); // Palette
        st.settings_step_right(); // expand
        let before: Vec<String> =
            st.settings_palette_rows().into_iter().map(|(n, _)| n).collect();
        let cyan_idx = before.iter().position(|n| n == "cyan").unwrap();
        st.settings_move_cursor(1 + cyan_idx as i32);
        st.settings_activate(); // toggle cyan off
        let after: Vec<String> = st.settings_palette_rows().into_iter().map(|(n, _)| n).collect();
        assert_eq!(before, after, "deactivating a color must not move any row");
    }

    #[test]
    fn settings_row_description_describes_default_mode() {
        assert_eq!(
            SettingsRow::DefaultMode.description(),
            "Whether the picker opens in Command mode or straight into Search."
        );
    }

    #[test]
    fn settings_row_description_describes_start_focus_mode() {
        assert_eq!(
            SettingsRow::StartFocusMode.description(),
            "Whether the picker starts in focus mode: Remember the last state, Always start in it, or Never start in it."
        );
    }

    #[test]
    fn settings_row_description_child_rows_reuse_parent_text() {
        assert_eq!(
            SettingsRow::BorderColorOption(0).description(),
            SettingsRow::BorderColor.description()
        );
        assert_eq!(
            SettingsRow::PaletteColor(0).description(),
            SettingsRow::Palette.description()
        );
        assert_eq!(
            SettingsRow::ShortcutColorOption(0).description(),
            SettingsRow::ShortcutColor.description()
        );
    }

    #[test]
    fn static_color_defaults_to_cyan() {
        let st = settings_state();
        assert_eq!(st.static_color, "cyan");
    }

    #[test]
    fn attached_and_border_color_default_to_green() {
        let st = settings_state();
        assert_eq!(st.attached_color, "green");
        assert_eq!(st.border_color, "green");
    }

    #[test]
    fn dot_color_defaults_to_static_green() {
        let st = settings_state();
        assert_eq!(st.dot_color_mode, DotColorMode::Static);
        assert_eq!(st.dot_color, "green");
    }

    #[test]
    fn shortcut_color_defaults_to_gray() {
        let st = settings_state();
        assert_eq!(st.shortcut_color, "gray");
    }
}
