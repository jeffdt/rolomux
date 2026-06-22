#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Activity,
    Created,
    Manual,
}

impl SortKey {
    pub fn from_config_str(s: &str) -> SortKey {
        match s {
            "created" => SortKey::Created,
            "manual" => SortKey::Manual,
            _ => SortKey::Activity,
        }
    }

    /// The next mode in the in-picker cycle: recency -> age -> manual -> recency.
    pub fn next(self) -> SortKey {
        match self {
            SortKey::Activity => SortKey::Created,
            SortKey::Created => SortKey::Manual,
            SortKey::Manual => SortKey::Activity,
        }
    }
}

/// Where the cursor starts when the picker opens. Like `SortKey`, this is a
/// swappable seam: change the single `INITIAL_FOCUS` constant below to pick a
/// policy without touching `build`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialFocus {
    /// Always start on the first row (top pinned/sorted session). Legacy
    /// behavior. Selected only by swapping `INITIAL_FOCUS`, so it is not
    /// constructed in the shipped binary; the allow keeps that intentional
    /// reserved variant from tripping the dead-code lint.
    #[allow(dead_code)]
    FirstRow,
    /// Start on the session the popup was launched from. Resolved precisely
    /// from `$TMUX` (passed in as `current`), falling back to the `attached`
    /// flag, then the first row.
    CurrentSession,
}

/// The active initial-focus policy. Swap this one constant to change behavior.
pub const INITIAL_FOCUS: InitialFocus = InitialFocus::CurrentSession;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub index: u32,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub activity: i64,
    pub created: i64,
    pub attached: bool,
    pub windows: Vec<Window>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    SwitchSession(String),
    SwitchWindow(String, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Session(usize),
    Window(usize, usize),
}

use crate::store::Config;
use std::collections::HashSet;

pub struct PickerState {
    all: Vec<Session>,
    pub pinned: Vec<String>,
    pub manual_order: Vec<String>,
    pub sort: SortKey,
    expanded: HashSet<String>,
    pub cursor: usize,
    pub dirty: bool,
}

fn sort_value(s: &Session, key: SortKey) -> i64 {
    match key {
        SortKey::Activity => s.activity,
        // Manual never reaches here (`ordered` branches before sorting), but the
        // match must be exhaustive; created is the sensible fallthrough.
        SortKey::Created | SortKey::Manual => s.created,
    }
}

impl PickerState {
    pub fn build(sessions: Vec<Session>, config: &Config) -> PickerState {
        Self::build_with_focus(sessions, config, INITIAL_FOCUS, None)
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
        let mut state = PickerState {
            all: sessions,
            pinned: config.pinned.clone(),
            manual_order: config.manual_order.clone(),
            sort: config.sort,
            expanded: HashSet::new(),
            cursor: 0,
            dirty: false,
        };
        state.apply_initial_focus(focus, current);
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

    pub fn is_pinned(&self, name: &str) -> bool {
        self.pinned.iter().any(|p| p == name)
    }

    pub fn ordered(&self) -> Vec<&Session> {
        let mut out: Vec<&Session> = Vec::new();
        for name in &self.pinned {
            if let Some(s) = self.all.iter().find(|s| &s.name == name) {
                out.push(s);
            }
        }
        let mut rest: Vec<&Session> = self
            .all
            .iter()
            .filter(|s| !self.is_pinned(&s.name))
            .collect();
        if self.sort == SortKey::Manual {
            // Manually placed sessions first (in saved order, skipping any that
            // are now pinned or gone), then everything unlisted by created
            // ascending so the newest session sinks to the bottom.
            rest.sort_by(|a, b| {
                let rank = |s: &Session| {
                    self.manual_order
                        .iter()
                        .position(|n| n == &s.name)
                        .map(|p| (0, p as i64))
                        .unwrap_or((1, s.created))
                };
                rank(a).cmp(&rank(b)).then(a.name.cmp(&b.name))
            });
        } else {
            rest.sort_by(|a, b| {
                sort_value(b, self.sort)
                    .cmp(&sort_value(a, self.sort))
                    .then(a.name.cmp(&b.name))
            });
        }
        out.extend(rest);
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
        let len = self.visible_rows().len() as i32;
        if len == 0 {
            self.cursor = 0;
            return;
        }
        let next = (self.cursor as i32 + delta).clamp(0, len - 1);
        self.cursor = next as usize;
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
        }
    }

    pub fn collapse(&mut self) {
        if let Some(name) = self.cursor_session_name() {
            self.expanded.remove(&name);
            self.focus_session(&name);
        }
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

    pub fn toggle_pin(&mut self) {
        let name = match self.cursor_session_name() {
            Some(n) => n,
            None => return,
        };
        if let Some(pos) = self.pinned.iter().position(|p| p == &name) {
            self.pinned.remove(pos);
        } else {
            self.pinned.push(name.clone());
        }
        self.dirty = true;
        self.focus_session(&name);
    }

    /// Reorder the session under the cursor. Pinned sessions reorder within the
    /// pin list (any sort mode); unpinned sessions reorder only in Manual mode.
    pub fn move_row(&mut self, delta: i32) {
        let name = match self.cursor_session_name() {
            Some(n) => n,
            None => return,
        };
        if self.is_pinned(&name) {
            self.move_pinned(delta);
        } else if self.sort == SortKey::Manual {
            self.move_unpinned(delta);
        }
    }

    /// Reorder an unpinned session within the manual order (Manual mode only).
    /// The first move freezes the current unpinned display order into
    /// `manual_order` so the swap is well-defined even before anything was moved.
    fn move_unpinned(&mut self, delta: i32) {
        let name = match self.cursor_session_name() {
            Some(n) => n,
            None => return,
        };
        if self.is_pinned(&name) {
            return;
        }
        let mut frozen: Vec<String> = self
            .ordered()
            .iter()
            .filter(|s| !self.is_pinned(&s.name))
            .map(|s| s.name.clone())
            .collect();
        let pos = match frozen.iter().position(|p| p == &name) {
            Some(p) => p as i32,
            None => return,
        };
        let target = pos + delta;
        if target < 0 || target >= frozen.len() as i32 {
            return; // clamped at an edge: leave order untouched
        }
        frozen.swap(pos as usize, target as usize);
        self.manual_order = frozen;
        self.dirty = true;
        self.focus_session(&name);
    }

    /// Advance the sort mode to the next in the cycle and mark the state dirty so
    /// the choice persists.
    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.dirty = true;
    }

    pub fn move_pinned(&mut self, delta: i32) {
        let name = match self.cursor_session_name() {
            Some(n) => n,
            None => return,
        };
        let pos = match self.pinned.iter().position(|p| p == &name) {
            Some(p) => p as i32,
            None => return, // unpinned: nothing to reorder
        };
        let target = pos + delta;
        if target < 0 || target >= self.pinned.len() as i32 {
            return;
        }
        self.pinned.swap(pos as usize, target as usize);
        self.dirty = true;
        self.focus_session(&name);
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

    /// Switch action for the session at 1-based display number `n` (pinned #1
    /// down, stable regardless of what is expanded). `None` if out of range.
    pub fn action_for_session_number(&self, n: usize) -> Option<Action> {
        if n == 0 {
            return None;
        }
        self.ordered()
            .get(n - 1)
            .map(|s| Action::SwitchSession(s.name.clone()))
    }

    /// Move the cursor to the session at 1-based display number `n` (pinned #1
    /// down, the same stable order as `action_for_session_number`) and expand it
    /// so its windows show. Unlike a plain-digit switch, this only relocates the
    /// highlight and reveals windows so one can be picked; it does not switch.
    /// No-op if `n` is 0 or out of range.
    pub fn focus_session_number(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        if let Some(name) = self.ordered().get(n - 1).map(|s| s.name.clone()) {
            self.expanded.insert(name.clone());
            self.focus_session(&name);
        }
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
        if let Some(name) = focus {
            self.focus_session(&name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Config;

    fn s(name: &str, activity: i64, created: i64) -> Session {
        Session {
            name: name.into(),
            activity,
            created,
            attached: false,
            windows: vec![Window { index: 0, name: "w".into(), active: true }],
        }
    }

    #[test]
    fn sort_key_parses_with_default_fallback() {
        assert_eq!(SortKey::from_config_str("created"), SortKey::Created);
        assert_eq!(SortKey::from_config_str("activity"), SortKey::Activity);
        assert_eq!(SortKey::from_config_str("garbage"), SortKey::Activity);
        assert_eq!(SortKey::default(), SortKey::Activity);
    }

    #[test]
    fn initial_focus_prefers_precise_current_over_attached() {
        // Ordered top is "a"; "b" carries the attached flag, but the precise
        // current (from $TMUX) is "c" — the precise signal must win.
        let mut sessions = vec![s("a", 30, 1), s("b", 20, 2), s("c", 10, 3)];
        sessions[1].attached = true;
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::CurrentSession, Some("c"));
        assert_eq!(state.cursor_session_name().as_deref(), Some("c"));
    }

    #[test]
    fn initial_focus_current_falls_back_to_attached_flag() {
        // No precise current; the attached flag ("b") is the fallback.
        let mut sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        sessions[1].attached = true;
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::CurrentSession, None);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn initial_focus_first_row_ignores_current_and_attached() {
        let mut sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        sessions[1].attached = true;
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::FirstRow, Some("b"));
        assert_eq!(state.cursor, 0);
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
    }

    #[test]
    fn initial_focus_current_falls_back_to_first_row_when_nothing_matches() {
        let sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
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
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn refocus_current_moves_to_named_session_and_no_ops_on_none() {
        let sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg); // no attached -> row 0 ("a")
        state.refocus_current(Some("b"));
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
        state.refocus_current(None); // no-op
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn ordered_puts_pinned_first_then_unpinned_by_activity() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2), s("c", 20, 3)];
        let cfg = Config { pinned: vec!["c".into()], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        // c is pinned (first); then b (activity 30) before a (activity 10)
        assert_eq!(names, vec!["c", "b", "a"]);
        assert!(state.is_pinned("c"));
        assert!(!state.is_pinned("a"));
    }

    #[test]
    fn ordered_unpinned_by_created_when_configured() {
        let sessions = vec![s("a", 10, 100), s("b", 30, 50)];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Created };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        // created desc: a (100) before b (50)
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn ordered_breaks_ties_by_name_ascending() {
        let sessions = vec![s("zebra", 50, 1), s("apple", 50, 2)];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        // both have activity 50, so sort by name ascending: apple before zebra
        assert_eq!(names, vec!["apple", "zebra"]);
    }

    #[test]
    fn expand_reveals_windows_and_cursor_moves_over_them() {
        let mut sessions = vec![s("a", 10, 1), s("b", 5, 2)];
        sessions[0].windows = vec![
            Window { index: 0, name: "e".into(), active: true },
            Window { index: 1, name: "l".into(), active: false },
        ];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
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

        // Clamp at bottom.
        state.move_cursor(5);
        assert_eq!(state.cursor, 3);
        // Clamp at top.
        state.move_cursor(-99);
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn toggle_pin_adds_then_removes_and_marks_dirty() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg);

        // Cursor on "a"; pin it -> a becomes pinned, still focused.
        state.toggle_pin();
        assert_eq!(state.pinned, vec!["a".to_string()]);
        assert!(state.dirty);
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));

        // Toggle again -> unpinned.
        state.toggle_pin();
        assert!(state.pinned.is_empty());
    }

    #[test]
    fn move_pinned_reorders_within_pins_only() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2), s("c", 10, 3)];
        let cfg = Config { pinned: vec!["a".into(), "b".into()], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg);

        // Cursor starts on "a" (first pinned). Move it down -> [b, a].
        state.move_pinned(1);
        assert_eq!(state.pinned, vec!["b".to_string(), "a".to_string()]);
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
        // Verify dirty flag is set after successful swap.
        assert!(state.dirty);

        // Focus the unpinned "c" and try to move it -> no-op.
        state.focus_session("c");
        state.dirty = false;
        state.move_pinned(-1);
        assert_eq!(state.pinned, vec!["b".to_string(), "a".to_string()]);
        // Unpinned session move must not dirty the state.
        assert!(!state.dirty);

        // Out-of-bounds no-op: focus first pinned "b", try to move up beyond start.
        state.focus_session("b");
        state.dirty = false;
        state.move_pinned(-1);
        assert_eq!(state.pinned, vec!["b".to_string(), "a".to_string()]);
        // Out-of-bounds move must not dirty the state.
        assert!(!state.dirty);
    }

    #[test]
    fn selected_action_session_vs_window() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].windows = vec![
            Window { index: 0, name: "e".into(), active: true },
            Window { index: 3, name: "l".into(), active: false },
        ];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
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
        let cfg = Config { pinned: vec!["c".into()], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg); // order: c, b, a

        assert_eq!(state.action_for_session_number(1), Some(Action::SwitchSession("c".into())));
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("b".into())));
        assert_eq!(state.action_for_session_number(3), Some(Action::SwitchSession("a".into())));
        assert_eq!(state.action_for_session_number(0), None);
        assert_eq!(state.action_for_session_number(4), None);

        // Numbers are stable even when a session is expanded (no renumbering).
        state.expand(); // expands "c" (cursor at top)
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("b".into())));
        assert_eq!(state.action_for_session_number(3), Some(Action::SwitchSession("a".into())));
    }

    #[test]
    fn focus_session_number_moves_cursor_without_switching() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2), s("c", 20, 3)];
        let cfg = Config { pinned: vec!["c".into()], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg); // order: c, b, a

        state.focus_session_number(3); // -> a
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
        assert!(state.is_expanded("a"), "focused session expands");
        state.focus_session_number(1); // -> c
        assert_eq!(state.cursor_session_name().as_deref(), Some("c"));
        assert!(state.is_expanded("c"), "focused session expands");

        // Zero and out-of-range are no-ops (cursor stays put).
        state.focus_session_number(0);
        assert_eq!(state.cursor_session_name().as_deref(), Some("c"));
        state.focus_session_number(9);
        assert_eq!(state.cursor_session_name().as_deref(), Some("c"));

        // Focusing does not switch or dirty state.
        assert!(!state.dirty);
    }

    #[test]
    fn sort_key_parses_manual_and_cycles() {
        assert_eq!(SortKey::from_config_str("manual"), SortKey::Manual);
        assert_eq!(SortKey::Activity.next(), SortKey::Created);
        assert_eq!(SortKey::Created.next(), SortKey::Manual);
        assert_eq!(SortKey::Manual.next(), SortKey::Activity);
    }

    #[test]
    fn ordered_manual_empty_list_is_created_ascending() {
        // No manual placements yet: unpinned read oldest -> newest (created asc),
        // so a freshly created session naturally lands at the bottom.
        let sessions = vec![s("a", 99, 3), s("b", 99, 1), s("c", 99, 2)];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Manual };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["b", "c", "a"]); // created 1, 2, 3
    }

    #[test]
    fn ordered_manual_lists_then_remaining_excluding_pinned() {
        let sessions = vec![s("a", 1, 10), s("b", 1, 20), s("c", 1, 30), s("d", 1, 40)];
        // d is pinned (and also wrongly listed in manual_order to prove it is
        // filtered out of the manual tail); c then a are the manual placements;
        // b is unlisted and falls in after, by created asc.
        let cfg = Config {
            pinned: vec!["d".into()],
            manual_order: vec!["d".into(), "c".into(), "a".into()],
            sort: SortKey::Manual,
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
            pinned: vec![],
            manual_order: vec!["mid".into(), "old".into()],
            sort: SortKey::Manual,
        };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["mid", "old", "x"]);
    }

    #[test]
    fn move_row_unpinned_in_manual_freezes_then_swaps_and_dirties() {
        // Manual + empty list => base order is created asc: a(1), b(2), c(3).
        let sessions = vec![s("a", 9, 1), s("b", 9, 2), s("c", 9, 3)];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Manual };
        let mut state = PickerState::build(sessions, &cfg);

        state.focus_session("b");
        state.move_row(-1); // move b up past a
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["b", "a", "c"]);
        // The full unpinned order is frozen into manual_order on the first move.
        assert_eq!(
            state.manual_order,
            vec!["b".to_string(), "a".to_string(), "c".to_string()]
        );
        assert!(state.dirty);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));

        // Moving up at the top is a clamped no-op.
        state.dirty = false;
        state.move_row(-1);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
        assert!(!state.dirty);
    }

    #[test]
    fn move_row_unpinned_is_noop_outside_manual_mode() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("b");
        state.move_row(1);
        assert!(!state.dirty);
        assert!(state.manual_order.is_empty());
    }

    #[test]
    fn move_row_on_pinned_reorders_pins_in_any_mode() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config {
            pinned: vec!["a".into(), "b".into()],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("a");
        state.move_row(1);
        assert_eq!(state.pinned, vec!["b".to_string(), "a".to_string()]);
        assert!(state.dirty);
    }

    #[test]
    fn cycle_sort_advances_mode_and_dirties() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg);
        state.cycle_sort();
        assert_eq!(state.sort, SortKey::Created);
        assert!(state.dirty);
        state.cycle_sort();
        assert_eq!(state.sort, SortKey::Manual);
        state.cycle_sort();
        assert_eq!(state.sort, SortKey::Activity);
    }

    #[test]
    fn toggle_all_expands_then_collapses_keeping_focus() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { pinned: vec![], manual_order: vec![], sort: SortKey::Activity };
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
}
