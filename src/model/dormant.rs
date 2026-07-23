//! Dormant sessions and focus-mode filtering. A dormant session is dimmed but
//! normal; focus mode hides dormant sessions (except the attached one) and any
//! group left with nothing visible. Also the cursor clamps that keep both the
//! command and search cursors in range after the visible set shrinks.

use super::*;

impl PickerState {
    /// Whether `name` is marked dormant. When dormant sessions are shown, they
    /// are dimmed but otherwise fully normal; `focus_mode` is the only filter
    /// that removes them from the picker, except for the attached session,
    /// which always stays visible even while dormant (see `is_attached`).
    pub fn is_dormant(&self, name: &str) -> bool {
        self.dormant.contains(name)
    }

    /// Whether `name` is the session the current tmux client is attached to.
    fn is_attached(&self, name: &str) -> bool {
        self.all.iter().any(|s| s.name == name && s.attached)
    }

    /// Whether the window at `index` within session `session` is marked
    /// dormant, independent of the session's own dormant flag.
    pub fn is_window_dormant(&self, session: &str, index: u32) -> bool {
        self.dormant_windows.contains(&(session.to_string(), index))
    }

    /// Whether that window should render as a row in an expanded session:
    /// exactly the session-level `session_visible` rule, one altitude
    /// down. `active` is the window's own live `active` flag -- tmux
    /// guarantees exactly one active window per session, so this
    /// exemption can never leave an expanded session showing nothing,
    /// even when every window (including the active one) is individually
    /// dormant, and even when the session's own dormant flag is also set
    /// (an attached-dormant session still applies this filter to its
    /// windows once shown).
    pub fn window_visible(&self, session: &str, index: u32, active: bool) -> bool {
        !self.focus_mode || !self.is_window_dormant(session, index) || active
    }

    pub fn focus_mode(&self) -> bool {
        self.focus_mode
    }

    /// Count of dormant sessions actually hidden by focus mode -- excludes
    /// the attached session, which stays visible even while dormant.
    pub fn hidden_dormant_count(&self) -> usize {
        if !self.focus_mode {
            return 0;
        }
        self.all.iter().filter(|s| self.is_dormant(&s.name) && !s.attached).count()
    }

    /// Count of dormant, non-active windows hidden by focus mode --
    /// scoped to sessions that are currently *expanded*, so this reflects
    /// what's actually elided from the current view rather than a global
    /// total for structure that hasn't been drilled into yet. Grows as
    /// more sessions are expanded; equals the true total once every
    /// session is expanded. Feeds the footer's hidden-count wording.
    pub fn hidden_dormant_window_count(&self) -> usize {
        if !self.focus_mode {
            return 0;
        }
        self.all
            .iter()
            .filter(|s| self.expanded.contains(&s.name) && self.session_visible(&s.name))
            .map(|s| {
                s.windows
                    .iter()
                    .filter(|w| self.is_window_dormant(&s.name, w.index) && !w.active)
                    .count()
            })
            .sum()
    }

    pub(super) fn session_visible(&self, name: &str) -> bool {
        !self.focus_mode || !self.is_dormant(name) || self.is_attached(name)
    }

    /// Toggle whether focus mode (hiding dormant sessions, and any group left
    /// with nothing visible) is on. The filter is persisted as a preference so
    /// it survives closing and reopening the popup, same as the dormant set
    /// itself.
    pub fn toggle_focus_mode(&mut self) {
        let command_focus = self.cursor_session_name();
        let search_focus = self.search_cursor_name();
        self.focus_mode = !self.focus_mode;
        self.dirty = true;
        if let Some(name) = command_focus.as_deref().filter(|name| self.session_visible(name)) {
            self.focus_session(name);
        } else {
            self.clamp_cursor_to_visible_rows();
        }
        if let Some(name) = search_focus.as_deref() {
            if let Some(i) = self.search_results().iter().position(|s| s.name == name) {
                self.search_cursor = i;
            } else {
                self.clamp_search_cursor_to_results();
            }
        } else {
            self.clamp_search_cursor_to_results();
        }
    }

    /// Applied once at construction: if `clear_dormant_on_attach` is on,
    /// drop dormant status for every session that is both attached and
    /// dormant. This is the opt-in cleanup; `is_attached`'s always-visible
    /// exemption in `ordered`/`session_visible` is the always-on safety net
    /// that applies regardless of this setting.
    pub(super) fn apply_clear_dormant_on_attach(&mut self) {
        if !self.clear_dormant_on_attach {
            return;
        }
        let names: Vec<String> = self
            .all
            .iter()
            .filter(|s| s.attached && self.dormant.contains(&s.name))
            .map(|s| s.name.clone())
            .collect();
        if names.is_empty() {
            return;
        }
        for name in names {
            self.dormant.remove(&name);
        }
        self.dirty = true;
    }

    fn clamp_cursor_to_visible_rows(&mut self) {
        let len = self.visible_rows().len();
        if len == 0 {
            self.cursor = 0;
        } else {
            self.cursor = self.cursor.min(len - 1);
        }
    }

    fn clamp_search_cursor_to_results(&mut self) {
        let len = self.search_results().len();
        if len == 0 {
            self.search_cursor = 0;
        } else {
            self.search_cursor = self.search_cursor.min(len - 1);
        }
    }

    /// Toggle dormant status for whatever's under the cursor: a session
    /// row toggles that session's own flag (unchanged); a window row
    /// toggles that specific window's flag instead, fully independent of
    /// its parent session's flag -- no auto-linking in either direction.
    pub fn toggle_dormant(&mut self) {
        let rows = self.visible_rows();
        let Some(row) = rows.get(self.cursor).copied() else { return };
        match row {
            Row::Session(_) => {
                let Some(name) = self.cursor_session_name() else { return };
                if !self.dormant.remove(&name) {
                    self.dormant.insert(name.clone());
                }
                self.dirty = true;
                if self.session_visible(&name) {
                    self.focus_session(&name);
                } else {
                    self.clamp_cursor_to_visible_rows();
                    self.clamp_search_cursor_to_results();
                }
            }
            Row::Window(si, wi) => {
                let ordered = self.ordered();
                let sess = ordered[si];
                let session_name = sess.name.clone();
                let window = &sess.windows[wi];
                let key = (session_name.clone(), window.index);
                if !self.dormant_windows.remove(&key) {
                    self.dormant_windows.insert(key);
                }
                self.dirty = true;
                self.clamp_cursor_to_visible_rows();
                self.clamp_search_cursor_to_results();
            }
        }
    }

    /// Sorted snapshot of every dormant session name.
    pub fn dormant_list(&self) -> Vec<String> {
        let mut v: Vec<String> = self.dormant.iter().cloned().collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use crate::model::*;
    use crate::model::test_support::*;
    use crate::store::Config;

    #[test]
    fn attached_dormant_session_stays_visible_in_focus_mode() {
        let mut sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        sessions[1].attached = true;
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into(), "b".into()],
            focus_mode: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            visible,
            vec!["b"],
            "attached dormant session stays visible; non-attached dormant session stays hidden"
        );
        assert_eq!(
            state.hidden_dormant_count(),
            1,
            "only the non-attached dormant session counts toward the hidden count"
        );
    }

    #[test]
    fn build_clears_dormant_flag_for_attached_session_when_setting_on() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].attached = true;
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            clear_dormant_on_attach: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(!state.is_dormant("a"), "attaching to a dormant session clears its dormant flag when the setting is on");
        assert!(state.dirty, "the cleared flag is marked dirty so it flushes to config");
    }

    #[test]
    fn build_leaves_dormant_flag_for_non_attached_session_even_with_setting_on() {
        let sessions = vec![s("a", 30, 1)]; // not attached
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            clear_dormant_on_attach: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.is_dormant("a"), "only the attached session's dormant flag is cleared");
    }

    #[test]
    fn build_leaves_dormant_flag_when_clear_on_attach_setting_is_off() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].attached = true;
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            clear_dormant_on_attach: false,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.is_dormant("a"), "dormant flag stays untouched when the setting is off");
        assert!(!state.dirty, "nothing changed, so build shouldn't mark state dirty");
    }

    #[test]
    fn dormant_list_is_sorted_snapshot() {
        let sessions = vec![s("charlie", 1, 1), s("alpha", 1, 2), s("bravo", 1, 3)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("charlie");
        state.toggle_dormant();
        state.focus_session("alpha");
        state.toggle_dormant();
        assert_eq!(state.dormant_list(), vec!["alpha".to_string(), "charlie".to_string()]);
    }

    #[test]
    fn dormant_loads_from_config() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config { groups: vec![], dormant: vec!["a".into()], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.is_dormant("a"));
    }

    #[test]
    fn focus_mode_clamps_cursor_when_selected_session_disappears() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("beta");
        assert_eq!(state.cursor_session_name().as_deref(), Some("beta"));

        state.toggle_focus_mode();

        assert_eq!(state.cursor_session_name().as_deref(), Some("alpha"));
        assert_eq!(state.visible_rows().len(), 1);
    }

    #[test]
    fn focus_mode_loads_from_config() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            focus_mode: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.focus_mode());
        assert_eq!(state.hidden_dormant_count(), 1);
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["b"]);
    }

    #[test]
    fn start_focus_mode_always_overrides_a_saved_false_focus_mode() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            focus_mode: false,
            start_focus_mode: StartFocusMode::Always,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.focus_mode(), "Always forces focus mode on regardless of the saved value");
    }

    #[test]
    fn start_focus_mode_never_overrides_a_saved_true_focus_mode() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            focus_mode: true,
            start_focus_mode: StartFocusMode::Never,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(!state.focus_mode(), "Never forces focus mode off regardless of the saved value");
    }

    #[test]
    fn start_focus_mode_remember_uses_the_saved_focus_mode() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config {
            groups: vec![],
            focus_mode: true,
            start_focus_mode: StartFocusMode::Remember,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.focus_mode(), "Remember (default) reproduces today's behavior exactly");
    }

    #[test]
    fn session_visible_true_for_attached_dormant_session_in_focus_mode() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].attached = true;
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            focus_mode: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.session_visible("a"));
    }

    #[test]
    fn toggle_dormant_flips_and_dirties() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg); // cursor starts on "a"

        assert!(!state.is_dormant("a"));
        state.toggle_dormant();
        assert!(state.is_dormant("a"));
        assert!(state.dirty);

        state.dirty = false;
        state.toggle_dormant();
        assert!(!state.is_dormant("a"));
        assert!(state.dirty);
    }

    #[test]
    fn toggle_dormant_on_attached_session_keeps_cursor_focused_in_focus_mode() {
        let mut sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        sessions[0].attached = true;
        let cfg = Config { groups: vec![], focus_mode: true, ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg); // cursor starts on "a" (attached)
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));

        state.toggle_dormant(); // marks "a" dormant while it's the attached session
        assert!(state.is_dormant("a"));
        assert_eq!(
            state.cursor_session_name().as_deref(),
            Some("a"),
            "cursor stays on the attached session even though it's now dormant"
        );
    }

    #[test]
    fn toggle_dormant_on_a_window_row_affects_only_that_window() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].windows = vec![
            Window { id: "@1".into(), index: 0, name: "e".into(), active: true },
            Window { id: "@2".into(), index: 1, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        state.expand();
        state.move_cursor(1); // land on the first window row
        assert!(matches!(state.visible_rows()[state.cursor], Row::Window(0, 0)));

        state.toggle_dormant();
        assert!(state.is_window_dormant("a", 0), "toggling on a window row marks that window");
        assert!(!state.is_dormant("a"), "the parent session's own flag is untouched");
        assert!(state.dirty);
    }

    #[test]
    fn toggle_dormant_on_a_window_row_is_never_blocked_by_a_dormant_session() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].windows = vec![
            Window { id: "@1".into(), index: 0, name: "e".into(), active: true },
            Window { id: "@2".into(), index: 1, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], dormant: vec!["a".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        state.expand();
        state.move_cursor(1);
        state.toggle_dormant();
        assert!(
            state.is_window_dormant("a", 0),
            "the window's own flag toggles regardless of the parent session's dormant state"
        );
    }

    #[test]
    fn window_visible_hides_dormant_non_active_windows_in_focus_mode() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].windows = vec![
            Window { id: "@1".into(), index: 0, name: "e".into(), active: true },
            Window { id: "@2".into(), index: 1, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], focus_mode: true, ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.expand();
        state.move_cursor(2); // land on the second window row (index 1, not active)
        state.toggle_dormant();

        assert!(state.window_visible("a", 0, true), "the active window is always visible");
        assert!(!state.window_visible("a", 1, false), "a dormant, non-active window is hidden in focus mode");
    }

    #[test]
    fn window_visible_active_window_stays_visible_even_when_session_is_also_dormant() {
        let mut sessions = vec![s("a", 30, 1)];
        // Must be attached: a dormant, non-attached session is filtered out
        // of ordered() entirely under focus mode, which would leave nothing
        // for expand()/toggle_dormant() to act on below. The attached
        // exemption is what keeps a dormant session shown at all -- see
        // session_visible/is_attached in dormant.rs.
        sessions[0].attached = true;
        sessions[0].windows = vec![
            Window { id: "@1".into(), index: 0, name: "e".into(), active: true },
        ];
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            dormant_windows: vec![],
            focus_mode: true,
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.expand();
        state.move_cursor(1); // off the session row and onto its one window row
        state.toggle_dormant(); // marks the active window individually dormant too

        assert!(
            state.window_visible("a", 0, true),
            "the active window exemption holds even when the session itself is dormant-but-shown"
        );
    }

    #[test]
    fn window_visible_ignores_dormant_when_focus_mode_is_off() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].windows = vec![
            Window { id: "@1".into(), index: 0, name: "e".into(), active: true },
            Window { id: "@2".into(), index: 1, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], focus_mode: false, ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.expand();
        state.move_cursor(2);
        state.toggle_dormant();

        assert!(state.window_visible("a", 1, false), "focus mode off never hides anything");
    }

    #[test]
    fn hidden_dormant_window_count_only_counts_expanded_sessions() {
        let mut sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        sessions[0].windows = vec![
            Window { id: "@1".into(), index: 0, name: "e".into(), active: true },
            Window { id: "@2".into(), index: 1, name: "l".into(), active: false },
        ];
        sessions[1].windows = vec![
            Window { id: "@3".into(), index: 0, name: "m".into(), active: true },
            Window { id: "@4".into(), index: 1, name: "n".into(), active: false },
        ];
        let cfg = Config { groups: vec![], focus_mode: true, ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        // Mark the non-active window dormant in both sessions, but expand
        // neither yet. Use collapse_session (name-based) rather than the
        // cursor-based collapse(): toggling a window dormant here hides it
        // and clamps the cursor, which can land on the *other* session's
        // row -- collapse() would then collapse the wrong session.
        state.focus_session("a");
        state.expand();
        state.move_cursor(1);
        state.move_cursor(1); // "a"'s window 1 ("l", not active)
        state.toggle_dormant();
        state.collapse_session("a");
        state.focus_session("b");
        state.expand();
        state.move_cursor(1);
        state.move_cursor(1); // "b"'s window 1 ("n", not active)
        state.toggle_dormant();
        state.collapse_session("b");

        assert_eq!(state.hidden_dormant_window_count(), 0, "nothing expanded, so nothing counts as hidden yet");

        state.focus_session("a");
        state.expand();
        assert_eq!(state.hidden_dormant_window_count(), 1, "expanding a reveals its one hidden window");

        state.focus_session("b");
        state.expand();
        assert_eq!(state.hidden_dormant_window_count(), 2, "expanding b too brings the total to both hidden windows");
    }

    #[test]
    fn toggle_focus_mode_filters_command_and_search_and_dirties() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2), s("gamma", 1, 3)];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        assert!(state.is_dormant("beta"));
        let shown: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(shown, vec!["alpha", "beta", "gamma"]);

        state.toggle_focus_mode();
        assert!(state.focus_mode());
        assert_eq!(state.hidden_dormant_count(), 1);
        assert!(state.dirty, "entering focus mode persists the preference");
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["alpha", "gamma"]);

        state.enter_search();
        state.search_push('b');
        assert!(state.search_results().is_empty(), "hidden dormant sessions are absent from search");
        state.search_clear();
        let search_visible: Vec<&str> = state.search_results().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(search_visible, vec!["alpha", "gamma"]);

        state.toggle_focus_mode();
        assert!(!state.focus_mode());
        let restored: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(restored, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn toggling_dormant_while_filter_is_active_hides_the_session() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_focus_mode();
        assert_eq!(state.cursor_session_name().as_deref(), Some("alpha"));

        state.toggle_dormant();

        assert!(state.is_dormant("alpha"));
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["beta"]);
        assert_eq!(state.cursor_session_name().as_deref(), Some("beta"));
    }
}
