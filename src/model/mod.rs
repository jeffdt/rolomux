mod types;
pub use types::*;

mod reorder;
pub use reorder::WindowMove;
use reorder::PendingWindowMove;

mod kill;
pub use kill::KillTarget;
use kill::PendingKill;

mod rename;
pub use rename::{PendingRename, RenameTarget};

mod settings;
pub use settings::SettingsRow;
use settings::SettingsUiState;

mod groups;

mod search;

mod dormant;

mod swap_indicator;
pub use swap_indicator::SwapDirection;
use swap_indicator::SwapIndicator;

use crate::store::Config;
use std::collections::HashSet;

pub struct PickerState {
    all: Vec<Session>,
    pub groups: Vec<Group>,
    expanded: HashSet<String>,
    dormant: HashSet<String>,
    focus_mode: bool,
    pub start_focus_mode: StartFocusMode,
    pub cursor: usize,
    pub dirty: bool,
    pub mode: Mode,
    pub query: String,
    search_cursor: usize,
    /// Cursor position within the group list in `Mode::Groups`.
    pub group_cursor: usize,
    /// In-flight rename buffer; `Some` while a rename is in progress.
    pub group_edit: Option<String>,
    /// In-flight session/window rename buffer; `Some` while a rename is in progress.
    rename_edit: Option<String>,
    /// In-flight window-move confirmation, armed when a press would destroy
    /// a session; `Some` until the same-direction key repeats it or any
    /// other key clears it.
    pending_window_move: Option<PendingWindowMove>,
    /// In-flight kill confirmation, armed when `x` targets a session or
    /// window; `Some` until a second `x` confirms it or any other key
    /// clears it. See `src/model/kill.rs`.
    pending_kill: Option<PendingKill>,
    /// The brief post-⇧J/⇧K directional flash (issue #130): `Some` for about
    /// a second after a session/window/group reorder, then cleared by
    /// `tick_swap_indicator`.
    swap_indicator: Option<SwapIndicator>,
    /// One-shot flag set when `group_reorder` refuses to move the inbox;
    /// cleared by any other group-mode input, mirroring
    /// `pending_window_move`'s clear-on-any-other-key lifecycle.
    group_reorder_blocked: bool,
    pub default_mode: DefaultMode,
    pub number_dormant_sessions: bool,
    pub remember_expanded_sessions: bool,
    pub clear_dormant_on_attach: bool,
    pub session_metric: SessionMetric,
    pub new_group_position: NewGroupPosition,
    pub new_group_color_policy: ColorPolicy,
    pub static_color: String,
    pub active_palette: Vec<String>,
    pub attached_color: String,
    pub attached_color_mode: AttachedColorMode,
    pub border_color: String,
    pub border_color_policy: ColorPolicy,
    pub dot_color_mode: DotColorMode,
    pub dot_color: String,
    pub shortcut_color: String,
    pub shortcut_visibility: ShortcutVisibility,
    /// One-shot `?` toggle: reveals the footer's shortcut legend for the rest
    /// of this popup's lifetime when `shortcut_visibility` is `OnDemand`.
    /// Never persisted -- each fresh popup starts collapsed again, same as
    /// `remember_expanded_sessions` off. See `shortcuts_visible`.
    show_shortcuts_now: bool,
    pub inbox_icon: String,
    /// Transient per-open state for the settings overlay (see `SettingsUiState`).
    settings_ui: SettingsUiState,
}

impl PickerState {
    pub fn build(sessions: Vec<Session>, config: &Config) -> PickerState {
        let mut state = Self::build_with_focus(sessions, config, INITIAL_FOCUS, None);
        state.apply_border_color_policy();
        state
    }

    /// Like `build`, but with an explicit initial-focus policy and current
    /// session. `build` calls this with `INITIAL_FOCUS` and no precise current
    /// (the `attached` flag is the fallback); tests use it to exercise each
    /// policy and the precise-current path directly.
    fn build_with_focus(
        sessions: Vec<Session>,
        config: &Config,
        focus: InitialFocus,
        current: Option<&str>,
    ) -> PickerState {
        let mut groups = config.groups.clone();
        ensure_single_inbox(&mut groups);
        ensure_inbox_last(&mut groups);
        let mut state = PickerState {
            all: sessions,
            groups,
            expanded: if config.remember_expanded_sessions {
                config.expanded.iter().cloned().collect()
            } else {
                HashSet::new()
            },
            dormant: config.dormant.iter().cloned().collect(),
            focus_mode: match config.start_focus_mode {
                StartFocusMode::Remember => config.focus_mode,
                StartFocusMode::Always => true,
                StartFocusMode::Never => false,
            },
            cursor: 0,
            dirty: false,
            mode: config.default_mode.as_mode(),
            query: String::new(),
            search_cursor: 0,
            group_cursor: 0,
            group_edit: None,
            rename_edit: None,
            pending_window_move: None,
            pending_kill: None,
            swap_indicator: None,
            group_reorder_blocked: false,
            default_mode: config.default_mode,
            start_focus_mode: config.start_focus_mode,
            number_dormant_sessions: config.number_dormant_sessions,
            remember_expanded_sessions: config.remember_expanded_sessions,
            clear_dormant_on_attach: config.clear_dormant_on_attach,
            session_metric: config.session_metric,
            new_group_position: config.new_group_position,
            new_group_color_policy: config.new_group_color_policy,
            static_color: config.static_color.clone(),
            active_palette: config.active_palette.clone(),
            attached_color: config.attached_color.clone(),
            attached_color_mode: config.attached_color_mode,
            border_color: config.border_color.clone(),
            border_color_policy: config.border_color_policy,
            dot_color_mode: config.dot_color_mode,
            dot_color: config.dot_color.clone(),
            shortcut_color: config.shortcut_color.clone(),
            shortcut_visibility: config.shortcut_visibility,
            show_shortcuts_now: false,
            inbox_icon: config.inbox_icon.clone(),
            settings_ui: SettingsUiState::default(),
        };
        state.apply_clear_dormant_on_attach();
        state.apply_initial_focus(focus, current);
        state
    }

    /// Like `build`, but seeds the transient expand set from `expanded`
    /// instead of from `config.expanded`/`remember_expanded_sessions`. Used
    /// to rebuild the picker after a mid-run session/window rename, so a
    /// session expanded this run (even with "remember expanded" off)
    /// survives the rebuild under its possibly-new name.
    pub fn build_with_expanded(sessions: Vec<Session>, config: &Config, expanded: Vec<String>) -> PickerState {
        let mut state = Self::build_with_focus(sessions, config, INITIAL_FOCUS, None);
        state.expanded = expanded.into_iter().collect();
        state
    }

    /// Refine the initial cursor with the precise current-session name resolved
    /// from `$TMUX` (which `build` can't see). Only applies under the
    /// `CurrentSession` policy, so swapping `INITIAL_FOCUS` to `FirstRow` is
    /// still honored. Called by `main` right after `build`.
    pub fn refocus_current(&mut self, current: Option<&str>) {
        if let (InitialFocus::CurrentSession, Some(name)) = (INITIAL_FOCUS, current) {
            self.focus_session(name);
        }
    }

    /// Place the cursor according to `focus`. For `CurrentSession`, prefer the
    /// precise `current` name (resolved from `$TMUX`), then the `attached`
    /// flag, then leave it on the first row (the `cursor: 0` default).
    fn apply_initial_focus(&mut self, focus: InitialFocus, current: Option<&str>) {
        if let InitialFocus::CurrentSession = focus {
            let target = current
                .map(str::to_string)
                .or_else(|| self.all.iter().find(|s| s.attached).map(|s| s.name.clone()));
            if let Some(name) = target {
                self.focus_session(&name);
            }
        }
    }

    /// The index of the group that owns `name`: either the group whose
    /// `members` literally lists it, or -- if no group does -- the inbox
    /// group, which absorbs anything not explicitly filed elsewhere. A
    /// session belongs to at most one *explicit* group (first match wins if
    /// config lists it twice); this only returns `None` if the inbox
    /// invariant somehow doesn't hold, which `ensure_single_inbox` prevents.
    pub fn group_index_of(&self, name: &str) -> Option<usize> {
        self.groups
            .iter()
            .position(|g| g.members.iter().any(|m| m == name))
            .or_else(|| self.inbox_index())
    }

    /// Live sessions currently attributed to group `gi` (via `group_index_of`,
    /// so an inbox group's count includes fallback members it hasn't
    /// persisted into `members` yet, not just its explicit list).
    pub fn group_session_count(&self, gi: usize) -> usize {
        self.all.iter().filter(|s| self.group_index_of(&s.name) == Some(gi)).count()
    }

    /// Sessions that fall back to inbox group `gi` (via `group_index_of`)
    /// and aren't excluded by `is_excluded`, sorted oldest-created-first.
    /// Shared by the move logic below (`effective_order`, which excludes
    /// this group's own already-persisted members) and by `ordered()`'s
    /// restructuring in Task 4 (which excludes whatever's already been
    /// placed in the output so far) -- both need "the inbox's virtual,
    /// never-persisted tail," and should not duplicate this filter/sort.
    fn inbox_overflow(&self, gi: usize, mut is_excluded: impl FnMut(&str) -> bool) -> Vec<&Session> {
        let mut rest: Vec<&Session> = self
            .all
            .iter()
            .filter(|s| !is_excluded(&s.name) && self.group_index_of(&s.name) == Some(gi))
            .collect();
        rest.sort_by(|a, b| a.created.cmp(&b.created).then(a.name.cmp(&b.name)));
        rest
    }

    /// The index of the one group flagged `inbox: true`. `PickerState`
    /// always has exactly one after construction (see `build_with_focus`),
    /// so this is only `None` before that invariant is established.
    pub fn inbox_index(&self) -> Option<usize> {
        self.groups.iter().position(|g| g.inbox)
    }

    /// Group id for each entry of `ordered()`. Parallel to `ordered()` so the
    /// UI can emit a section header wherever this value changes. Always
    /// resolvable now -- every session belongs to some group, the inbox at
    /// worst -- so unlike the pre-issue-#23 residual bucket there is no
    /// `None` case left to represent.
    pub fn ordered_group_ids(&self) -> Vec<usize> {
        self.ordered()
            .iter()
            .map(|s| self.group_index_of(&s.name).unwrap_or(0))
            .collect()
    }

    pub fn ordered(&self) -> Vec<&Session> {
        let mut out: Vec<&Session> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for (gi, g) in self.groups.iter().enumerate() {
            for name in &g.members {
                if seen.contains(name.as_str()) {
                    continue; // guard against a session listed in two groups
                }
                if let Some(sess) = self.all.iter().find(|s| &s.name == name) {
                    out.push(sess);
                    seen.insert(name.as_str());
                }
            }
            if g.inbox {
                // Sessions nobody has explicitly filed anywhere: they belong
                // to this block too (via group_index_of's fallback), but
                // aren't in `members` yet. Render them right after this
                // group's real members, oldest-created first, wherever this
                // group currently sits -- not appended at the very end of
                // the whole list.
                for sess in self.inbox_overflow(gi, |name| seen.contains(name)) {
                    out.push(sess);
                    seen.insert(sess.name.as_str());
                }
            }
        }
        if self.focus_mode {
            out.retain(|s| !self.dormant.contains(&s.name) || s.attached);
        }
        out
    }

    pub fn visible_rows(&self) -> Vec<Row> {
        let ordered = self.ordered();
        let mut rows = Vec::new();
        for (si, sess) in ordered.iter().enumerate() {
            rows.push(Row::Session(si));
            if self.expanded.contains(&sess.name) {
                for wi in 0..sess.windows.len() {
                    rows.push(Row::Window(si, wi));
                }
            }
        }
        rows
    }

    pub fn move_cursor(&mut self, delta: i32) {
        self.cursor = move_index_with_edge_wrap(self.cursor, delta, self.visible_rows().len());
    }

    fn cursor_ordered_index(&self) -> Option<usize> {
        let rows = self.visible_rows();
        rows.get(self.cursor).map(|r| match r {
            Row::Session(si) => *si,
            Row::Window(si, _) => *si,
        })
    }

    pub fn cursor_session_name(&self) -> Option<String> {
        let si = self.cursor_ordered_index()?;
        self.ordered().get(si).map(|s| s.name.clone())
    }

    pub fn is_expanded(&self, name: &str) -> bool {
        self.expanded.contains(name)
    }

    pub fn expand(&mut self) {
        if let Some(name) = self.cursor_session_name() {
            self.expanded.insert(name);
            if self.remember_expanded_sessions {
                self.dirty = true;
            }
        }
    }

    pub fn collapse(&mut self) {
        if let Some(name) = self.cursor_session_name() {
            self.expanded.remove(&name);
            if self.remember_expanded_sessions {
                self.dirty = true;
            }
            self.focus_session(&name);
        }
    }

    pub fn expanded_list(&self) -> Vec<String> {
        let mut v: Vec<String> = self.expanded.iter().cloned().collect();
        v.sort();
        v
    }

    /// Copy the current mutable picker state into the persisted config model.
    pub fn apply_to_config(&self, config: &mut Config) {
        config.groups = self.groups.clone();
        config.dormant = self.dormant_list();
        config.focus_mode = self.focus_mode();
        config.start_focus_mode = self.start_focus_mode;
        config.default_mode = self.default_mode;
        config.number_dormant_sessions = self.number_dormant_sessions;
        config.new_group_position = self.new_group_position;
        config.new_group_color_policy = self.new_group_color_policy;
        config.static_color = self.static_color.clone();
        config.active_palette = self.active_palette.clone();
        config.attached_color = self.attached_color.clone();
        config.attached_color_mode = self.attached_color_mode;
        config.border_color = self.border_color.clone();
        config.border_color_policy = self.border_color_policy;
        config.inbox_icon = self.inbox_icon.clone();
        config.remember_expanded_sessions = self.remember_expanded_sessions;
        config.clear_dormant_on_attach = self.clear_dormant_on_attach;
        config.session_metric = self.session_metric;
        config.dot_color_mode = self.dot_color_mode;
        config.dot_color = self.dot_color.clone();
        config.shortcut_color = self.shortcut_color.clone();
        config.shortcut_visibility = self.shortcut_visibility;
        config.expanded = self.expanded_list();
    }

    /// Whether the footer's key-shortcut legend should render this frame:
    /// always when the persisted preference is `Always`, otherwise only
    /// after `toggle_shortcuts` has revealed it for this popup (issue #107).
    pub fn shortcuts_visible(&self) -> bool {
        self.shortcut_visibility == ShortcutVisibility::Always || self.show_shortcuts_now
    }

    /// `?`: flip the transient reveal. Not persisted and not `dirty` -- like
    /// the search query or an in-flight rename buffer, this is per-popup UI
    /// state, not a saved preference.
    pub fn toggle_shortcuts(&mut self) {
        self.show_shortcuts_now = !self.show_shortcuts_now;
    }

    pub fn focus_session(&mut self, name: &str) {
        let rows = self.visible_rows();
        let ordered = self.ordered();
        for (i, r) in rows.iter().enumerate() {
            if let Row::Session(si) = r {
                if ordered[*si].name == name {
                    self.cursor = i;
                    return;
                }
            }
        }
    }

    /// Focus a specific window row by session name and stable tmux window
    /// index (unlike a window's name, its index survives a rename). Requires
    /// the session to already be expanded, which holds after a window rename
    /// since only a visible window row can be renamed in the first place.
    pub fn focus_window(&mut self, session: &str, index: u32) {
        let rows = self.visible_rows();
        let ordered = self.ordered();
        for (i, r) in rows.iter().enumerate() {
            if let Row::Window(si, wi) = r {
                if ordered[*si].name == session && ordered[*si].windows[*wi].index == index {
                    self.cursor = i;
                    return;
                }
            }
        }
    }

    /// Force `name` into the expanded set regardless of its prior state --
    /// used after a cross-session window move lands in a session that may
    /// not already be expanded.
    pub fn expand_session(&mut self, name: &str) {
        self.expanded.insert(name.to_string());
        if self.remember_expanded_sessions {
            self.dirty = true;
        }
    }

    /// Remove `name` from the expanded set regardless of prior state -- the
    /// name-based counterpart to `expand_session`, used by search mode's
    /// collapse (`search_collapse`), which re-focuses within its own row
    /// list rather than command mode's `visible_rows`.
    pub fn collapse_session(&mut self, name: &str) {
        self.expanded.remove(name);
        if self.remember_expanded_sessions {
            self.dirty = true;
        }
    }

    pub fn selected_action(&self) -> Option<Action> {
        let rows = self.visible_rows();
        let ordered = self.ordered();
        match rows.get(self.cursor)? {
            Row::Session(si) => {
                Some(Action::SwitchSession(ordered[*si].name.clone()))
            }
            Row::Window(si, wi) => {
                let sess = ordered[*si];
                Some(Action::SwitchWindow(sess.name.clone(), sess.windows[*wi].index))
            }
        }
    }

    fn numbered_order(&self) -> Vec<&Session> {
        self.ordered()
            .into_iter()
            .filter(|s| self.number_dormant_sessions || !self.is_dormant(&s.name))
            .collect()
    }

    /// Switch action for the session at 1-based display number `n` (grouped #1
    /// down, stable regardless of what is expanded). Visible dormant sessions
    /// participate only when `number_dormant_sessions` is enabled. `None` if
    /// out of range.
    pub fn action_for_session_number(&self, n: usize) -> Option<Action> {
        if n == 0 {
            return None;
        }
        self.numbered_order()
            .get(n - 1)
            .map(|s| Action::SwitchSession(s.name.clone()))
    }

    /// Expand every session, or collapse all if everything is already expanded.
    /// Keeps the cursor on the same session.
    pub fn toggle_all(&mut self) {
        let focus = self.cursor_session_name();
        if self.expanded.len() >= self.all.len() {
            self.expanded.clear();
        } else {
            self.expanded = self.all.iter().map(|s| s.name.clone()).collect();
        }
        if self.remember_expanded_sessions {
            self.dirty = true;
        }
        if let Some(name) = focus {
            self.focus_session(&name);
        }
    }
}

/// Move within a bounded list, wrapping one-row cursor steps at the edges.
fn move_index_with_edge_wrap(index: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let max = len - 1;
    let index = index.min(max);
    if delta == -1 && index == 0 {
        max
    } else if delta == 1 && index == max {
        0
    } else {
        (index as i32 + delta).clamp(0, max as i32) as usize
    }
}

/// Shared constructors for the `model` submodule test suites. `pub(crate)` and
/// `#[cfg(test)]` so every module's own `mod tests` can build fixtures the same
/// way without redefining them.
#[cfg(test)]
pub(crate) mod test_support {
    use super::{Group, PickerState, Session, Window};
    use crate::store::Config;

    pub fn s(name: &str, activity: i64, created: i64) -> Session {
        Session { id: String::new(),
            name: name.into(),
            activity,
            created,
            attached: false,
            windows: vec![Window { id: String::new(), index: 0, name: "w".into(), active: true }],
        }
    }

    pub fn win(index: u32, name: &str) -> Window {
        Window { id: String::new(), index, name: name.into(), active: false }
    }

    pub fn session_with_windows(name: &str, created: i64, windows: Vec<Window>) -> Session {
        Session { id: String::new(), name: name.into(), activity: created, created, attached: false, windows }
    }

    pub fn state_with_two_groups() -> PickerState {
        // groups: G1=[a,b], G2=[c]; residual d,e by activity (d 40 > e 30)
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("c", 1, 3), s("d", 40, 4), s("e", 30, 5)];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "G1".into(), members: vec!["a".into(), "b".into()], color: String::new(), ..Default::default() },
                Group { name: "G2".into(), members: vec!["c".into()], color: String::new(), ..Default::default() },
            ],
            ..Default::default()
        };
        PickerState::build(sessions, &cfg)
    }

    pub fn grouped_state() -> PickerState {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("c", 1, 3)];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "G1".into(), members: vec!["a".into()], color: String::new(), ..Default::default() },
                Group { name: "G2".into(), members: vec!["b".into()], color: String::new(), ..Default::default() },
            ],
            ..Default::default()
        };
        PickerState::build(sessions, &cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::store::Config;

    #[test]
    fn expand_session_marks_dirty_only_when_remembering() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.expand_session("a");
        assert!(st.is_expanded("a"));
        assert!(!st.dirty); // remember_expanded_sessions defaults to false
    }

    #[test]
    fn collapse_session_removes_regardless_of_cursor_and_marks_dirty_only_when_remembering() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.expand_session("a");
        st.collapse_session("a");
        assert!(!st.is_expanded("a"));
        assert!(!st.dirty, "off: collapse_session does not persist");

        st.remember_expanded_sessions = true;
        st.expand_session("a");
        st.dirty = false;
        st.collapse_session("a");
        assert!(st.dirty, "on: collapse_session persists");
    }

    #[test]
    fn group_index_of_falls_back_to_inbox_for_unlisted_sessions() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2)];
        let cfg = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() }],
            ..Default::default()
        };
        let st = PickerState::build(sessions, &cfg);
        assert_eq!(st.group_index_of("a"), Some(0));
        assert_eq!(st.group_index_of("b"), st.inbox_index());
        assert_ne!(st.inbox_index(), Some(0));
    }

    #[test]
    fn group_session_count_includes_inbox_fallback_members() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("c", 1, 3)];
        let cfg = Config {
            groups: vec![
                Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() },
                Group { name: "INBOX".into(), members: vec!["b".into()], inbox: true, ..Default::default() },
            ],
            ..Default::default()
        };
        let st = PickerState::build(sessions, &cfg);
        assert_eq!(st.group_session_count(0), 1); // WORK: just "a"
        assert_eq!(st.group_session_count(1), 2); // INBOX: persisted "b" + fallback "c"
    }

    #[test]
    fn all_named_colors_has_sixteen_unique_entries() {
        assert_eq!(ALL_NAMED_COLORS.len(), 16);
        let mut sorted = ALL_NAMED_COLORS.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 16, "no duplicate color names");
    }

    #[test]
    fn initial_focus_prefers_precise_current_over_attached() {
        // Ordered top is "a"; "b" carries the attached flag, but the precise
        // current (from $TMUX) is "c" -- the precise signal must win.
        let mut sessions = vec![s("a", 30, 1), s("b", 20, 2), s("c", 10, 3)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], ..Default::default() };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::CurrentSession, Some("c"));
        assert_eq!(state.cursor_session_name().as_deref(), Some("c"));
    }

    #[test]
    fn initial_focus_current_falls_back_to_attached_flag() {
        // No precise current; the attached flag ("b") is the fallback.
        let mut sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], ..Default::default() };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::CurrentSession, None);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn initial_focus_first_row_ignores_current_and_attached() {
        let mut sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], ..Default::default() };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::FirstRow, Some("b"));
        assert_eq!(state.cursor, 0);
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
    }

    #[test]
    fn initial_focus_current_falls_back_to_first_row_when_nothing_matches() {
        let sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::CurrentSession, None);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
    }

    #[test]
    fn build_defaults_to_current_focus_via_attached_fallback() {
        // Canary for the shipped INITIAL_FOCUS default; update if it is swapped.
        let mut sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn refocus_current_moves_to_named_session_and_no_ops_on_none() {
        let sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg); // no attached -> row 0 ("a")
        state.refocus_current(Some("b"));
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
        state.refocus_current(None); // no-op
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn ordered_lists_groups_in_order_then_residual_by_created() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2), s("c", 20, 3), s("d", 40, 4)];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "CONFIG".into(), members: vec!["c".into()], color: String::new(), ..Default::default() },
                Group { name: "TOOLS".into(), members: vec!["a".into()], color: String::new(), ..Default::default() },
            ],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        // groups first in config order (c, then a), residual unlisted by created asc (b 2, d 4)
        assert_eq!(names, vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn ordered_breaks_ties_by_name_ascending() {
        let sessions = vec![s("zebra", 50, 5), s("apple", 50, 5)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        // both unranked with the same created time, so sort by name ascending: apple before zebra
        assert_eq!(names, vec!["apple", "zebra"]);
    }

    #[test]
    fn expand_reveals_windows_and_cursor_moves_over_them() {
        let mut sessions = vec![s("a", 10, 1), s("b", 5, 2)];
        sessions[0].windows = vec![
            Window { id: String::new(), index: 0, name: "e".into(), active: true },
            Window { id: String::new(), index: 1, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        // Collapsed: two session rows only.
        assert_eq!(state.visible_rows().len(), 2);

        // Cursor on "a" (first), expand it.
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
        state.expand();
        assert!(state.is_expanded("a"));
        assert!(!state.is_expanded("b"));
        assert_eq!(state.visible_rows().len(), 4); // a, a:0, a:1, b

        // Move down twice -> still within a's windows / onto b.
        state.move_cursor(1);
        state.move_cursor(1);
        assert!(matches!(state.visible_rows()[state.cursor], Row::Window(0, 1)));

        // Large jumps still land on the edge.
        state.move_cursor(5);
        assert_eq!(state.cursor, 3);
        // Moving past either edge wraps.
        state.move_cursor(1);
        assert_eq!(state.cursor, 0);
        state.move_cursor(-1);
        assert_eq!(state.cursor, 3);
    }

    #[test]
    fn selected_action_session_vs_window() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].windows = vec![
            Window { id: String::new(), index: 0, name: "e".into(), active: true },
            Window { id: String::new(), index: 3, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        // On the session row.
        assert_eq!(state.selected_action(), Some(Action::SwitchSession("a".into())));

        // Expand and move onto the second window (tmux index 3).
        state.expand();
        state.move_cursor(2);
        assert_eq!(state.selected_action(), Some(Action::SwitchWindow("a".into(), 3)));
    }

    #[test]
    fn action_for_session_number_uses_stable_pinned_first_order() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2), s("c", 20, 3)];
        let cfg = Config {
            dormant: vec![], groups: vec![Group { name: "PINNED".into(), members: vec!["c".into()], color: String::new(), ..Default::default() }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg); // order: c, a, b (a/b unranked by created asc)

        assert_eq!(state.action_for_session_number(1), Some(Action::SwitchSession("c".into())));
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("a".into())));
        assert_eq!(state.action_for_session_number(3), Some(Action::SwitchSession("b".into())));
        assert_eq!(state.action_for_session_number(0), None);
        assert_eq!(state.action_for_session_number(4), None);

        // Numbers are stable even when a session is expanded (no renumbering).
        state.expand(); // expands "c" (cursor at top)
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("a".into())));
        assert_eq!(state.action_for_session_number(3), Some(Action::SwitchSession("b".into())));
    }

    #[test]
    fn action_for_session_number_extends_past_nine() {
        let sessions: Vec<Session> = (1..=12).map(|i| s(&format!("s{i}"), 0, i as i64)).collect();
        let cfg = Config::default();
        let state = PickerState::build(sessions, &cfg);

        assert_eq!(state.action_for_session_number(10), Some(Action::SwitchSession("s10".into())));
        assert_eq!(state.action_for_session_number(11), Some(Action::SwitchSession("s11".into())));
        assert_eq!(state.action_for_session_number(12), Some(Action::SwitchSession("s12".into())));
        assert_eq!(state.action_for_session_number(13), None);
    }

    #[test]
    fn dormant_sessions_can_be_skipped_by_jump_numbering() {
        let sessions = vec![s("alpha", 10, 1), s("beta", 20, 2), s("gamma", 30, 3)];
        let cfg = Config {
            dormant: vec!["beta".into()],
            number_dormant_sessions: false,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);

        assert_eq!(state.action_for_session_number(1), Some(Action::SwitchSession("alpha".into())));
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("gamma".into())));
        assert_eq!(state.action_for_session_number(3), None);
    }

    #[test]
    fn hidden_dormant_sessions_are_never_jump_numbered() {
        let sessions = vec![s("alpha", 10, 1), s("beta", 20, 2), s("gamma", 30, 3)];
        let cfg = Config {
            dormant: vec!["beta".into()],
            focus_mode: true,
            number_dormant_sessions: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);

        assert_eq!(state.action_for_session_number(1), Some(Action::SwitchSession("alpha".into())));
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("gamma".into())));
        assert_eq!(state.action_for_session_number(3), None);
    }

    #[test]
    fn ordered_manual_empty_list_is_created_ascending() {
        // No manual placements yet: ungrouped read oldest -> newest (created asc),
        // so a freshly created session naturally lands at the bottom.
        let sessions = vec![s("a", 99, 3), s("b", 99, 1), s("c", 99, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["b", "c", "a"]); // created 1, 2, 3
    }

    #[test]
    fn ordered_manual_lists_then_remaining_excluding_pinned() {
        let sessions = vec![s("a", 1, 10), s("b", 1, 20), s("c", 1, 30), s("d", 1, 40)];
        // d is in a PINNED group (and also wrongly listed in inbox to prove it is
        // filtered out of the inbox tail); c then a are the inbox placements;
        // b is unlisted and falls in after, by created asc.
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "PINNED".into(), members: vec!["d".into()], ..Default::default() },
                Group { name: "INBOX".into(), members: vec!["d".into(), "c".into(), "a".into()], inbox: true, ..Default::default() }
            ],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["d", "c", "a", "b"]);
    }

    #[test]
    fn ordered_manual_new_session_sinks_to_bottom() {
        // "x" is the newest (highest created) and unlisted -> appears last.
        let sessions = vec![s("old", 1, 1), s("mid", 1, 2), s("x", 1, 99)];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "INBOX".into(), members: vec!["mid".into(), "old".into()], inbox: true, ..Default::default() }
            ],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["mid", "old", "x"]);
    }

    #[test]
    fn default_mode_is_command() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        assert_eq!(state.mode, Mode::Command);
        assert!(state.query.is_empty());
    }

    #[test]
    fn toggle_all_expands_then_collapses_keeping_focus() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        assert_eq!(state.visible_rows().len(), 2); // both collapsed

        state.toggle_all(); // expand all -> 2 sessions + 2 windows
        assert!(state.is_expanded("a"));
        assert!(state.is_expanded("b"));
        assert_eq!(state.visible_rows().len(), 4);
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));

        state.toggle_all(); // collapse all
        assert!(!state.is_expanded("a"));
        assert!(!state.is_expanded("b"));
        assert_eq!(state.visible_rows().len(), 2);
    }

    #[test]
    fn build_seeds_expanded_from_config_only_when_remembering() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2)];

        let cfg_off = Config {
            remember_expanded_sessions: false,
            expanded: vec!["a".to_string()],
            ..Default::default()
        };
        let state_off = PickerState::build(sessions.clone(), &cfg_off);
        assert!(!state_off.is_expanded("a"), "off: config's expanded list is ignored");

        let cfg_on = Config {
            remember_expanded_sessions: true,
            expanded: vec!["a".to_string()],
            ..Default::default()
        };
        let state_on = PickerState::build(sessions, &cfg_on);
        assert!(state_on.is_expanded("a"), "on: config's expanded list is restored");
        assert!(!state_on.is_expanded("b"));
    }

    #[test]
    fn expand_and_collapse_only_mark_dirty_when_remembering() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default(); // remember_expanded_sessions: false
        let mut state = PickerState::build(sessions, &cfg);
        state.expand();
        assert!(state.is_expanded("a"));
        assert!(!state.dirty, "off: expand does not persist");
        state.collapse();
        assert!(!state.dirty, "off: collapse does not persist");

        state.remember_expanded_sessions = true;
        state.expand();
        assert!(state.dirty, "on: expand persists");
        state.dirty = false;
        state.collapse();
        assert!(state.dirty, "on: collapse persists");
    }

    #[test]
    fn toggle_all_only_marks_dirty_when_remembering() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2)];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_all();
        assert!(!state.dirty, "off: toggle_all does not persist");

        state.dirty = false;
        state.remember_expanded_sessions = true;
        state.toggle_all();
        assert!(state.dirty, "on: toggle_all persists");
    }

    #[test]
    fn expanded_list_is_sorted_snapshot() {
        let sessions = vec![s("charlie", 1, 1), s("alpha", 1, 2), s("bravo", 1, 3)];
        let cfg = Config { groups: vec![], remember_expanded_sessions: true, ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("charlie");
        state.expand();
        state.focus_session("alpha");
        state.expand();
        assert_eq!(state.expanded_list(), vec!["alpha".to_string(), "charlie".to_string()]);
    }

    #[test]
    fn residual_count_excludes_grouped() {
        let st = grouped_state(); // a,b grouped; c residual
        assert_eq!(st.group_session_count(st.inbox_index().unwrap()), 1);
    }

    #[test]
    fn build_seeds_mode_and_fields_from_config_default_mode() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config {
            default_mode: DefaultMode::Search,
            number_dormant_sessions: false,
            new_group_color_policy: ColorPolicy::Static,
            static_color: "red".to_string(),
            active_palette: vec!["red".to_string(), "white".to_string()],
            dot_color_mode: DotColorMode::Group,
            dot_color: "lightred".to_string(),
            shortcut_color: "lightyellow".to_string(),
            shortcut_visibility: ShortcutVisibility::OnDemand,
            attached_color_mode: AttachedColorMode::Match,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert_eq!(state.mode, Mode::Search, "startup mode follows config.default_mode");
        assert_eq!(state.default_mode, DefaultMode::Search);
        assert!(!state.number_dormant_sessions);
        assert_eq!(state.new_group_color_policy, ColorPolicy::Static);
        assert_eq!(state.static_color, "red");
        assert_eq!(state.active_palette, vec!["red".to_string(), "white".to_string()]);
        assert_eq!(state.dot_color_mode, DotColorMode::Group);
        assert_eq!(state.dot_color, "lightred");
        assert_eq!(state.shortcut_color, "lightyellow");
        assert_eq!(state.shortcut_visibility, ShortcutVisibility::OnDemand);
        assert_eq!(state.attached_color_mode, AttachedColorMode::Match);
    }

    #[test]
    fn shortcuts_visible_is_always_true_by_default() {
        let st = PickerState::build(vec![s("a", 1, 1)], &Config::default());
        assert!(st.shortcuts_visible(), "default shortcut_visibility is Always");
    }

    #[test]
    fn toggle_shortcuts_reveals_the_legend_when_on_demand_and_never_dirties() {
        let cfg = Config { shortcut_visibility: ShortcutVisibility::OnDemand, ..Default::default() };
        let mut st = PickerState::build(vec![s("a", 1, 1)], &cfg);
        assert!(!st.shortcuts_visible(), "OnDemand starts collapsed");
        st.toggle_shortcuts();
        assert!(st.shortcuts_visible());
        assert!(!st.dirty, "the transient reveal is not a persisted preference");
        st.toggle_shortcuts();
        assert!(!st.shortcuts_visible(), "toggles back off");
    }

    #[test]
    fn build_normalizes_a_stored_inbox_position_back_to_the_trailing_slot() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("new", 1, 3)];
        // A config saved before issue #111 pinned the inbox down (or a
        // hand-edited TOML) might store the inbox anywhere; `build` must
        // normalize it back to the trailing slot via `ensure_inbox_last`.
        let cfg = Config {
            groups: vec![
                Group { name: "INBOX".into(), members: vec!["b".into()], inbox: true, ..Default::default() },
                Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        let st = PickerState::build(sessions, &cfg);
        assert_eq!(st.groups.last().map(|g| g.name.as_str()), Some("INBOX"));
        assert_eq!(st.groups.first().map(|g| g.name.as_str()), Some("WORK"));
        let names: Vec<&str> = st.ordered().iter().map(|s| s.name.as_str()).collect();
        // "new" is never explicitly listed anywhere, so it renders inside the
        // inbox's own (now always-trailing) block, right after "b".
        assert_eq!(names, vec!["a", "b", "new"]);
    }

    #[test]
    fn ordered_group_ids_are_never_none() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2)];
        let cfg = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() }],
            ..Default::default()
        };
        let st = PickerState::build(sessions, &cfg);
        let ids = st.ordered_group_ids();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], 0); // "a" in WORK
        assert_eq!(ids[1], st.inbox_index().unwrap()); // "b" falls back to inbox
    }

    #[test]
    fn ordered_group_ids_track_sections() {
        let st = state_with_two_groups(); // G1=[a,b], G2=[c], residual d,e (falls back to inbox)
        let inbox = st.inbox_index().unwrap();
        assert_eq!(
            st.ordered_group_ids(),
            vec![0, 0, 1, inbox, inbox]
        );
    }

    #[test]
    fn apply_to_config_persists_settings_preferences() {
        let dir = std::env::temp_dir().join(format!("rolomux-state-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let mut cfg = Config::default();
        let mut st = PickerState::build(vec![s("a", 1, 1)], &cfg);
        st.attached_color = "magenta".to_string();
        st.attached_color_mode = AttachedColorMode::Match;
        st.border_color = "yellow".to_string();
        st.default_mode = DefaultMode::Search;
        st.number_dormant_sessions = false;
        st.focus_mode = true;
        st.new_group_position = NewGroupPosition::Bottom;
        st.new_group_color_policy = ColorPolicy::Static;
        st.static_color = "white".to_string();
        st.active_palette = vec!["red".to_string(), "white".to_string()];
        st.remember_expanded_sessions = true;
        st.clear_dormant_on_attach = true;
        st.session_metric = SessionMetric::Age;
        st.dot_color_mode = DotColorMode::Group;
        st.dot_color = "lightblue".to_string();
        st.shortcut_color = "lightcyan".to_string();
        st.shortcut_visibility = ShortcutVisibility::OnDemand;

        st.apply_to_config(&mut cfg);
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);

        assert_eq!(reloaded.attached_color, "magenta");
        assert_eq!(reloaded.attached_color_mode, AttachedColorMode::Match);
        assert_eq!(reloaded.border_color, "yellow");
        assert_eq!(reloaded.default_mode, DefaultMode::Search);
        assert!(!reloaded.number_dormant_sessions);
        assert!(reloaded.focus_mode);
        assert_eq!(reloaded.new_group_position, NewGroupPosition::Bottom);
        assert_eq!(reloaded.new_group_color_policy, ColorPolicy::Static);
        assert_eq!(reloaded.static_color, "white");
        assert_eq!(reloaded.active_palette, vec!["red".to_string(), "white".to_string()]);
        assert!(reloaded.remember_expanded_sessions);
        assert!(reloaded.clear_dormant_on_attach);
        assert_eq!(reloaded.session_metric, SessionMetric::Age);
        assert_eq!(reloaded.dot_color_mode, DotColorMode::Group);
        assert_eq!(reloaded.dot_color, "lightblue");
        assert_eq!(reloaded.shortcut_color, "lightcyan");
        assert_eq!(reloaded.shortcut_visibility, ShortcutVisibility::OnDemand);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn build_with_expanded_seeds_expand_set_regardless_of_remember_setting() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        let cfg = Config { remember_expanded_sessions: false, ..Default::default() };
        let st = PickerState::build_with_expanded(sessions, &cfg, vec!["alpha".to_string()]);
        assert!(st.is_expanded("alpha"));
        assert!(!st.is_expanded("beta"));
    }

    #[test]
    fn build_with_expanded_ignores_config_expanded_field() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config {
            remember_expanded_sessions: true,
            expanded: vec!["alpha".to_string()],
            ..Default::default()
        };
        // Even though config.expanded lists "alpha", the explicit (empty) override wins.
        let st = PickerState::build_with_expanded(sessions, &cfg, vec![]);
        assert!(!st.is_expanded("alpha"));
    }

    #[test]
    fn build_with_expanded_does_not_reapply_border_color_policy() {
        let mut config = Config { border_color_policy: ColorPolicy::Rotate, ..Default::default() };
        config.border_color = "green".to_string();
        let sessions = vec![s("a", 1, 1)];
        let state = PickerState::build(sessions.clone(), &config);
        assert_ne!(state.border_color, "green", "build() should have applied Rotate once");

        let rebuilt = PickerState::build_with_expanded(sessions, &config, vec![]);
        assert_eq!(
            rebuilt.border_color, config.border_color,
            "build_with_expanded must not re-apply the policy on top of the original config value"
        );
    }
}
