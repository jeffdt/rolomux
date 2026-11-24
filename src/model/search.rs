//! Type-to-filter search: query editing, best-match ranking (via the
//! `crate::search` matcher), and the search-mode cursor. Read-only over the
//! session model -- a query never mutates persisted state.

use super::*;

/// One session in the search results plus the window rows shown under it.
struct SearchEntry<'a> {
    session: &'a Session,
    /// Positions in `session.windows` rendered as rows: every visible window
    /// when the session is expanded, otherwise just the matching ones.
    window_rows: Vec<usize>,
    /// The first matching window, set only when the name itself didn't
    /// match -- that window is the real hit, so the cursor lands on it.
    window_hit: Option<usize>,
}

impl PickerState {
    /// The current search results with their window rows. Empty query keeps
    /// the normal picker-mode order; otherwise matches are ranked best-first.
    /// Window names only match when `search_window_names` is on.
    fn search_entries(&self) -> Vec<SearchEntry<'_>> {
        let base = self.ordered();
        let visible_windows = |sess: &Session| -> Vec<usize> {
            (0..sess.windows.len())
                .filter(|&wi| {
                    let w = &sess.windows[wi];
                    self.window_visible(&sess.name, w.index, w.active)
                })
                .collect()
        };
        let window_rows = |sess: &Session, matched: Vec<usize>| -> Vec<usize> {
            if self.expanded.contains(&sess.name) { visible_windows(sess) } else { matched }
        };
        if self.query.is_empty() {
            return base
                .into_iter()
                .map(|session| SearchEntry { session, window_rows: window_rows(session, Vec::new()), window_hit: None })
                .collect();
        }
        let candidates: Vec<crate::search::Candidate> = base
            .iter()
            .map(|sess| crate::search::Candidate {
                name: &sess.name,
                windows: if self.search_window_names {
                    visible_windows(sess).into_iter().map(|wi| (wi, sess.windows[wi].name.as_str())).collect()
                } else {
                    Vec::new()
                },
            })
            .collect();
        crate::search::rank(&self.query, &candidates)
            .into_iter()
            .map(|hit| {
                let session = base[hit.index];
                let window_hit = if hit.name_matched { None } else { hit.windows.first().copied() };
                SearchEntry { session, window_rows: window_rows(session, hit.windows), window_hit }
            })
            .collect()
    }

    /// Sessions for the current search query, in result order. Read-only --
    /// never mutates state.
    pub fn search_results(&self) -> Vec<&Session> {
        self.search_entries().into_iter().map(|e| e.session).collect()
    }

    /// Flatten the current search results into session/window rows, the
    /// search-mode counterpart to `visible_rows()`. A session's own row
    /// always precedes its window rows.
    pub fn search_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (si, entry) in self.search_entries().iter().enumerate() {
            rows.push(Row::Session(si));
            rows.extend(entry.window_rows.iter().map(|&wi| Row::Window(si, wi)));
        }
        rows
    }

    /// Put the search cursor on the top result after a query change: the
    /// top session's row, or the window that matched when only a window did.
    fn reset_search_cursor(&mut self) {
        self.search_cursor = self
            .search_entries()
            .first()
            .and_then(|top| top.window_hit.and_then(|hit| top.window_rows.iter().position(|&wi| wi == hit)))
            .map_or(0, |pos| pos + 1);
    }

    pub fn enter_search(&mut self) {
        self.mode = Mode::Search;
        self.query.clear();
        self.search_cursor = 0;
    }

    /// Leave search for picker mode. If the search cursor was on a window
    /// row, the picker-mode cursor lands on that exact window (via
    /// `focus_window`) rather than just its parent session -- continuity of
    /// what was highlighted, not just which session it belongs to.
    pub fn exit_search(&mut self) {
        let landing = self.search_cursor_target();
        self.mode = Mode::Picker;
        self.query.clear();
        self.search_cursor = 0;
        if let Some((name, window_index)) = landing {
            match window_index {
                Some(idx) => self.focus_window(&name, idx),
                None => self.focus_session(&name),
            }
        }
    }

    pub fn search_push(&mut self, c: char) {
        self.query.push(c);
        self.reset_search_cursor();
    }

    pub fn search_backspace(&mut self) {
        self.query.pop();
        self.reset_search_cursor();
    }

    /// Delete the trailing word: strip trailing whitespace, then the run of
    /// non-whitespace before it (the Ctrl-W / Alt-Backspace convention).
    pub fn search_delete_word(&mut self) {
        let trimmed = self.query.trim_end_matches(char::is_whitespace);
        let cut = trimmed.trim_end_matches(|c: char| !c.is_whitespace());
        self.query.truncate(cut.len());
        self.reset_search_cursor();
    }

    /// Clear the entire query (the Ctrl-U convention).
    pub fn search_clear(&mut self) {
        self.query.clear();
        self.reset_search_cursor();
    }

    pub fn search_move(&mut self, delta: i32) {
        self.search_cursor = move_index_with_edge_wrap(
            self.search_cursor,
            delta,
            self.search_rows().len(),
        );
    }

    /// Accessor for rendering (the field is private).
    pub fn search_cursor(&self) -> usize {
        self.search_cursor
    }

    /// The session name and, if the cursor is on a window row, that
    /// window's stable tmux index -- the shared resolution logic behind
    /// `search_cursor_name`, `search_selected_action`, and `exit_search`.
    fn search_cursor_target(&self) -> Option<(String, Option<u32>)> {
        let rows = self.search_rows();
        let results = self.search_results();
        match rows.get(self.search_cursor)? {
            Row::Session(si) => Some((results[*si].name.clone(), None)),
            Row::Window(si, wi) => {
                let sess = results[*si];
                Some((sess.name.clone(), Some(sess.windows[*wi].index)))
            }
        }
    }

    pub fn search_cursor_name(&self) -> Option<String> {
        self.search_cursor_target().map(|(name, _)| name)
    }

    pub fn search_selected_action(&self) -> Option<Action> {
        let rows = self.search_rows();
        let results = self.search_results();
        match rows.get(self.search_cursor)? {
            Row::Session(si) => Some(Action::SwitchSession(results[*si].name.clone())),
            Row::Window(si, wi) => {
                let sess = results[*si];
                Some(Action::SwitchWindow(sess.name.clone(), sess.windows[*wi].index))
            }
        }
    }

    /// Expand the session under the cursor, sharing the same `expanded` set
    /// picker mode uses. Re-focuses onto that session's own row afterward
    /// (its row list just grew to include the new window rows).
    pub fn search_expand(&mut self) {
        if let Some(name) = self.search_cursor_name() {
            self.expand_session(&name);
            self.search_focus_session(&name);
        }
    }

    /// Collapse the session under the cursor (which may itself be one of
    /// its window rows) and refocus onto that session's row.
    pub fn search_collapse(&mut self) {
        if let Some(name) = self.search_cursor_name() {
            self.collapse_session(&name);
            self.search_focus_session(&name);
        }
    }

    /// Move the search cursor onto `name`'s own session row within the
    /// current `search_rows()` -- the search-local analog of `focus_session`,
    /// which operates on picker mode's `visible_rows()` instead.
    fn search_focus_session(&mut self, name: &str) {
        let rows = self.search_rows();
        let results = self.search_results();
        for (i, r) in rows.iter().enumerate() {
            if let Row::Session(si) = r {
                if results[*si].name == name {
                    self.search_cursor = i;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::*;
    use crate::model::test_support::*;
    use crate::store::Config;

    #[test]
    fn enter_and_exit_search_preserves_match_under_command_cursor() {
        let sessions = vec![s("provision", 1, 1), s("pr-review", 1, 2), s("scratch", 1, 3)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        state.enter_search();
        assert_eq!(state.mode, Mode::Search);
        state.search_push('p');
        state.search_push('r');
        state.search_push('r'); // "prr" matches pr-review (two r's) but not provision (one r)
        assert_eq!(state.search_cursor_name().as_deref(), Some("pr-review"));

        state.exit_search();
        assert_eq!(state.mode, Mode::Picker);
        assert!(state.query.is_empty());
        // Picker cursor now sits on the match we had highlighted.
        assert_eq!(state.cursor_session_name().as_deref(), Some("pr-review"));
        assert!(!state.dirty, "search is read-only");
    }

    #[test]
    fn live_mode_changes_never_rewrite_the_persisted_default() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config { default_mode: DefaultMode::Search, ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        assert_eq!(state.mode, Mode::Search);
        state.exit_search(); // navigates back to Picker at runtime
        assert_eq!(state.mode, Mode::Picker);
        assert_eq!(
            state.default_mode,
            DefaultMode::Search,
            "startup preference is untouched by runtime navigation"
        );
    }

    #[test]
    fn query_change_resets_to_top_and_move_wraps() {
        let sessions = vec![s("alpha", 1, 1), s("alto", 1, 2), s("alarm", 1, 3)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_push('a');

        state.search_move(1);
        state.search_push('l'); // query changed -> back to top
        assert_eq!(state.search_cursor(), 0, "query change resets to top");

        let n = state.search_results().len();
        assert_eq!(n, 3);
        state.search_move(-1);
        assert_eq!(state.search_cursor(), n - 1, "moving up from the top wraps to bottom");
        state.search_move(1);
        assert_eq!(state.search_cursor(), 0, "moving down from the bottom wraps to top");
    }

    #[test]
    fn search_backspace_shrinks_query_and_clears_to_empty() {
        let sessions = vec![s("api-gateway", 30, 1), s("web", 20, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        state.enter_search();
        state.search_push('a');
        state.search_push('p');
        assert_eq!(state.query, "ap");

        // One backspace shrinks by one character.
        state.search_backspace();
        assert_eq!(state.query, "a", "backspace removes the last char");
        assert_eq!(state.search_cursor(), 0, "cursor resets to top after backspace");

        // Backspace on a single-char query produces an empty string.
        state.search_backspace();
        assert!(state.query.is_empty(), "query is empty after backspace");
        assert_eq!(state.search_cursor(), 0);

        // Backspace on an already-empty query is a no-op (does not panic).
        state.search_backspace();
        assert!(state.query.is_empty(), "extra backspace on empty query is a no-op");

        // Search is read-only: no mutation, no dirty flag.
        assert!(!state.dirty, "search backspace never dirties state");
    }

    #[test]
    fn search_clear_empties_query() {
        let sessions = vec![s("api-gateway", 30, 1)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        state.enter_search();
        for c in "api gate".chars() {
            state.search_push(c);
        }

        state.search_clear();
        assert!(state.query.is_empty(), "clear empties the whole query");
        assert_eq!(state.search_cursor(), 0, "cursor resets to top after clear");

        // Clear on an empty query is a no-op (does not panic).
        state.search_clear();
        assert!(state.query.is_empty());
        assert!(!state.dirty, "search clear never dirties state");
    }

    #[test]
    fn search_delete_word_removes_trailing_word() {
        let sessions = vec![s("api-gateway", 30, 1)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        state.enter_search();
        for c in "api gate".chars() {
            state.search_push(c);
        }
        state.search_cursor = 0;

        state.search_delete_word();
        assert_eq!(state.query, "api ", "deletes the trailing word, keeps the prior space");
        assert_eq!(state.search_cursor(), 0, "cursor resets to top after word delete");

        state.search_delete_word();
        assert_eq!(state.query, "", "deletes through the space and the remaining word");

        // Word delete on an empty query is a no-op (does not panic).
        state.search_delete_word();
        assert!(state.query.is_empty());
        assert!(!state.dirty, "search word delete never dirties state");
    }

    #[test]
    fn search_results_empty_query_is_normal_order() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.search_results().iter().map(|s| s.name.as_str()).collect();
        // Same as ordered(): unranked by created asc -> a, b
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn search_results_filters_and_ranks_by_query() {
        // "prr" matches pr-review (p,r,-,r) tightly and provision not at all
        // (only one 'r'), so pr-review must rank first and scratch is excluded.
        let sessions = vec![s("provision", 1, 1), s("pr-review", 1, 2), s("scratch", 1, 3)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.query = "prr".into();
        let names: Vec<&str> = state.search_results().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names.first().copied(), Some("pr-review"), "strong match first");
        assert!(!names.contains(&"scratch"), "non-match omitted");
        assert!(!names.contains(&"provision"), "non-matching session excluded");
    }

    #[test]
    fn search_rows_are_flat_when_nothing_is_expanded() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        assert_eq!(state.search_rows().len(), 2, "one row per session, nothing expanded");
        assert!(matches!(state.search_rows()[0], Row::Session(0)));
        assert!(matches!(state.search_rows()[1], Row::Session(1)));
    }

    #[test]
    fn search_rows_include_windows_only_for_expanded_sessions() {
        let mut sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        sessions[0].windows = vec![win(0, "e"), win(1, "l")];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.expand_session("alpha");

        let rows = state.search_rows();
        assert_eq!(rows.len(), 4, "alpha (1) + its two windows + beta (1)");
        assert!(matches!(rows[0], Row::Session(_)));
        assert!(matches!(rows[1], Row::Window(0, 0)));
        assert!(matches!(rows[2], Row::Window(0, 1)));
        assert!(matches!(rows[3], Row::Session(_)));
    }

    #[test]
    fn search_selected_action_is_none_with_no_matches() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_push('z');
        state.search_push('z');
        assert_eq!(state.search_selected_action(), None);
    }

    #[test]
    fn search_selected_action_switches_to_highlighted() {
        let sessions = vec![s("provision", 1, 1), s("pr-review", 1, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_push('p');
        state.search_push('r');
        state.search_push('r'); // "prr" matches pr-review (two r's) but not provision (one r)
        assert_eq!(
            state.search_selected_action(),
            Some(Action::SwitchSession("pr-review".into()))
        );
    }

    #[test]
    fn search_move_walks_over_window_rows_and_wraps() {
        let mut sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        sessions[0].windows = vec![win(0, "e"), win(1, "l")];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.expand_session("alpha");

        assert_eq!(state.search_cursor(), 0); // alpha's session row
        state.search_move(1);
        assert_eq!(state.search_cursor(), 1); // alpha:0
        state.search_move(1);
        assert_eq!(state.search_cursor(), 2); // alpha:1
        state.search_move(1);
        assert_eq!(state.search_cursor(), 3); // beta
        state.search_move(1);
        assert_eq!(state.search_cursor(), 0, "wraps back to the top");
    }

    #[test]
    fn search_cursor_name_resolves_the_owning_session_from_a_window_row() {
        let mut sessions = vec![s("alpha", 1, 1)];
        sessions[0].windows = vec![win(0, "e"), win(1, "l")];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.expand_session("alpha");
        state.search_move(1); // onto alpha:0

        assert_eq!(state.search_cursor_name().as_deref(), Some("alpha"));
    }

    #[test]
    fn search_selected_action_switches_to_window_when_row_is_a_window() {
        let mut sessions = vec![s("alpha", 1, 1)];
        sessions[0].windows = vec![win(0, "e"), win(3, "l")];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.expand_session("alpha");
        state.search_move(1); // onto the first window (tmux index 0)
        state.search_move(1); // onto the second window (tmux index 3)

        assert_eq!(
            state.search_selected_action(),
            Some(Action::SwitchWindow("alpha".into(), 3))
        );
    }

    #[test]
    fn search_expand_and_collapse_toggle_shared_state_and_refocus() {
        let mut sessions = vec![s("alpha", 1, 1)];
        sessions[0].windows = vec![win(0, "e")];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();

        state.search_expand();
        assert!(state.is_expanded("alpha"), "search_expand shares picker mode's expanded set");
        assert_eq!(state.search_rows().len(), 2);
        assert_eq!(state.search_cursor(), 0, "cursor stays on alpha's own row after expanding");

        state.search_move(1); // onto the window row
        state.search_collapse();
        assert!(!state.is_expanded("alpha"));
        assert_eq!(state.search_cursor(), 0, "collapsing from a window row refocuses the parent session");
    }

    #[test]
    fn search_rows_hide_dormant_non_active_windows_in_focus_mode() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].windows = vec![
            win_active(0, "e"),
            win_active(1, "l"),
        ];
        let cfg = Config { groups: vec![], focus_mode: true, ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_expand();
        // rows so far: [Session("a"), Window(0,0)="e" (active), Window(0,1)="l" (not active)].
        // Two moves from the session row lands on "l" -- the one we want to
        // mark dormant. One move would land on "e" (the active window),
        // which is the wrong target for this test.
        state.search_move(1);
        state.search_move(1);
        state.exit_search(); // land picker-mode cursor on the window row we want to toggle
        state.toggle_dormant();
        state.enter_search();
        state.search_expand();

        let rows = state.search_rows();
        assert_eq!(rows.len(), 2, "session row plus only the active window row");
    }

    #[test]
    fn exit_search_lands_on_the_selected_window_row() {
        let mut sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        sessions[0].windows = vec![win(0, "e"), win(5, "l")];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_expand();
        state.search_move(1); // onto alpha's first window
        state.search_move(1); // onto alpha's second window (tmux index 5)

        state.exit_search();
        assert_eq!(state.mode, Mode::Picker);
        assert!(matches!(state.visible_rows()[state.cursor], Row::Window(_, _)));
        assert_eq!(
            state.selected_action(),
            Some(Action::SwitchWindow("alpha".into(), 5)),
            "picker-mode cursor lands on the exact window that was selected in search"
        );
    }

    fn searched(sessions: Vec<Session>, cfg: Config, query: &str) -> PickerState {
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        for c in query.chars() {
            state.search_push(c);
        }
        state
    }

    fn api_and_web() -> Vec<Session> {
        vec![s("api", 1, 1), session_with_windows("web", 2, vec![win(0, "zsh"), win(2, "logs"), win(4, "build")])]
    }

    fn windows_on() -> Config {
        Config { search_window_names: true, ..Default::default() }
    }

    #[test]
    fn window_names_are_ignored_when_the_setting_is_off() {
        let state = searched(api_and_web(), Config::default(), "logs");
        assert!(state.search_results().is_empty());
    }

    #[test]
    fn a_window_only_match_shows_just_that_window_and_selects_it() {
        let state = searched(api_and_web(), windows_on(), "logs");
        assert_eq!(state.search_rows(), vec![Row::Session(0), Row::Window(0, 1)]);
        assert_eq!(state.search_cursor(), 1, "cursor lands on the matching window, not its session");
        assert_eq!(state.search_selected_action(), Some(Action::SwitchWindow("web".into(), 2)));
    }

    #[test]
    fn a_window_match_in_an_expanded_session_shows_all_windows_and_selects_the_match() {
        let cfg = Config { expanded: vec!["web".into()], remember_expanded_sessions: true, ..windows_on() };
        let state = searched(api_and_web(), cfg, "build");
        assert_eq!(state.search_rows().len(), 4, "session row plus all three windows");
        assert_eq!(state.search_selected_action(), Some(Action::SwitchWindow("web".into(), 4)));
    }

    #[test]
    fn a_session_name_match_keeps_the_cursor_on_the_session_row() {
        let state = searched(vec![session_with_windows("logs", 1, vec![win(0, "logs")])], windows_on(), "logs");
        assert_eq!(state.search_rows(), vec![Row::Session(0), Row::Window(0, 0)]);
        assert_eq!(state.search_selected_action(), Some(Action::SwitchSession("logs".into())));
    }

    #[test]
    fn hidden_dormant_windows_never_match() {
        let cfg = Config {
            focus_mode: true,
            dormant_windows: vec![crate::store::DormantWindow { session: "web".into(), index: 1, id: String::new() }],
            ..windows_on()
        };
        let state = searched(vec![session_with_windows("web", 1, vec![win_active(0, "zsh"), win(1, "logs")])], cfg, "logs");
        assert!(state.search_results().is_empty());
    }
}
