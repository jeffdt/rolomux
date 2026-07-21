//! Type-to-filter search: query editing, best-match ranking (via the
//! `crate::search` matcher), and the search-mode cursor. Read-only over the
//! session model -- a query never mutates persisted state.

use super::*;

impl PickerState {
    /// The text a session is matched against in search. Today just its name; the
    /// seam where window names can later be folded in (a session matches if its
    /// name OR any window name matches) without touching the interaction layer.
    fn session_haystack(s: &Session) -> String {
        s.name.clone()
    }

    /// Sessions for the current search query. Empty query returns the normal
    /// command-mode order; a non-empty query returns matches ranked best-first.
    /// Read-only -- never mutates state.
    pub fn search_results(&self) -> Vec<&Session> {
        let base = self.ordered();
        if self.query.is_empty() {
            return base;
        }
        let haystacks: Vec<String> = base.iter().map(|s| Self::session_haystack(s)).collect();
        crate::search::rank(&self.query, &haystacks)
            .into_iter()
            .map(|i| base[i])
            .collect()
    }

    /// Flatten the current search results into session/window rows, the
    /// search-mode counterpart to `visible_rows()`. A session's own row
    /// always precedes its window rows, so index 0 is always a
    /// `Row::Session` -- this is what lets `search_push`'s unconditional
    /// `search_cursor = 0` reset keep landing on a session even with window
    /// rows in the mix.
    pub fn search_rows(&self) -> Vec<Row> {
        let results = self.search_results();
        let mut rows = Vec::new();
        for (si, sess) in results.iter().enumerate() {
            rows.push(Row::Session(si));
            if self.expanded.contains(&sess.name) {
                for wi in 0..sess.windows.len() {
                    rows.push(Row::Window(si, wi));
                }
            }
        }
        rows
    }

    pub fn enter_search(&mut self) {
        self.mode = Mode::Search;
        self.query.clear();
        self.search_cursor = 0;
    }

    /// Leave search for command mode, parking the command cursor on whatever match
    /// was highlighted so command verbs (sort, reorder) act on it.
    pub fn exit_search(&mut self) {
        let landing = self.search_cursor_name();
        self.mode = Mode::Command;
        self.query.clear();
        self.search_cursor = 0;
        if let Some(name) = landing {
            self.focus_session(&name);
        }
    }

    pub fn search_push(&mut self, c: char) {
        self.query.push(c);
        self.search_cursor = 0; // every query change re-selects the top match
    }

    pub fn search_backspace(&mut self) {
        self.query.pop();
        self.search_cursor = 0;
    }

    /// Delete the trailing word: strip trailing whitespace, then the run of
    /// non-whitespace before it (the Ctrl-W / Alt-Backspace convention).
    pub fn search_delete_word(&mut self) {
        let trimmed = self.query.trim_end_matches(char::is_whitespace);
        let cut = trimmed.trim_end_matches(|c: char| !c.is_whitespace());
        self.query.truncate(cut.len());
        self.search_cursor = 0;
    }

    /// Clear the entire query (the Ctrl-U convention).
    pub fn search_clear(&mut self) {
        self.query.clear();
        self.search_cursor = 0;
    }

    pub fn search_move(&mut self, delta: i32) {
        self.search_cursor = move_index_with_edge_wrap(
            self.search_cursor,
            delta,
            self.search_results().len(),
        );
    }

    /// Accessor for rendering (the field is private). Wired in Task 6.
    pub fn search_cursor(&self) -> usize {
        self.search_cursor
    }

    pub fn search_cursor_name(&self) -> Option<String> {
        self.search_results()
            .get(self.search_cursor)
            .map(|s| s.name.clone())
    }

    pub fn search_selected_action(&self) -> Option<Action> {
        self.search_results()
            .get(self.search_cursor)
            .map(|s| Action::SwitchSession(s.name.clone()))
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
        assert_eq!(state.mode, Mode::Command);
        assert!(state.query.is_empty());
        // Command cursor now sits on the match we had highlighted.
        assert_eq!(state.cursor_session_name().as_deref(), Some("pr-review"));
        assert!(!state.dirty, "search is read-only");
    }

    #[test]
    fn live_mode_changes_never_rewrite_the_persisted_default() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config { default_mode: DefaultMode::Search, ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        assert_eq!(state.mode, Mode::Search);
        state.exit_search(); // navigates back to Command at runtime
        assert_eq!(state.mode, Mode::Command);
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
}
