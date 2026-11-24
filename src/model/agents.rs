//! Agent view: every agent pane across all sessions, grouped by state
//! (Waiting, Done, Working) rather than by session. Reads the same
//! `window_agents` data the tree badges use; the cursor is independent of
//! the tree cursor so leaving the view returns to the exact tree row.

use super::{move_index_with_edge_wrap, Action, AgentRow, Mode, PickerState};

impl PickerState {
    /// Agent panes the tree would show (focus mode hides dormant sessions and
    /// windows here too), sorted by urgency, then longest in that state first;
    /// panes with no known start time keep tree order after those that have one.
    pub fn agent_rows(&self) -> Vec<AgentRow> {
        let mut rows: Vec<AgentRow> = self
            .ordered()
            .into_iter()
            .flat_map(|sess| {
                sess.windows.iter().filter(|win| self.window_visible(&sess.name, win.index, win.active)).flat_map(move |win| {
                    self.window_agents(win).iter().map(move |p| AgentRow {
                        session: sess.name.clone(),
                        window_index: win.index,
                        window_name: win.name.clone(),
                        pane_id: p.pane_id.clone(),
                        path: p.path.clone(),
                        state: p.state,
                        background: p.background,
                        since: p.since,
                        note: p.note.clone(),
                        dormant: self.is_dormant(&sess.name) || self.is_window_dormant(&sess.name, win.index),
                    })
                })
            })
            .collect();
        rows.sort_by_key(|r| (r.state, r.since == 0, r.since));
        rows
    }

    /// Enter the agent view with the cursor on the first row.
    pub fn enter_agents(&mut self) {
        self.mode = Mode::Agents;
        self.agent_cursor = 0;
    }

    /// Leave the agent view; the tree cursor is untouched.
    pub fn exit_agents(&mut self) {
        self.mode = Mode::Picker;
    }

    /// Index of the selected row in `agent_rows`.
    pub fn agent_cursor(&self) -> usize {
        self.agent_cursor
    }

    /// Moves the agent cursor by `delta` rows, wrapping at the ends.
    pub fn agent_move(&mut self, delta: i32) {
        self.agent_cursor = move_index_with_edge_wrap(self.agent_cursor, delta, self.agent_rows().len());
    }

    /// Action that switches to the highlighted agent pane, if any.
    pub fn agent_selected_action(&self) -> Option<Action> {
        self.agent_action_at(self.agent_cursor)
    }

    /// Action for the 1-based jump number `n` (only 1..=9 are valid).
    pub fn action_for_agent_number(&self, n: usize) -> Option<Action> {
        if !(1..=9).contains(&n) {
            return None;
        }
        self.agent_action_at(n - 1)
    }

    fn agent_action_at(&self, i: usize) -> Option<Action> {
        let r = self.agent_rows().into_iter().nth(i)?;
        Some(Action::SwitchPane(r.session, r.window_index, r.pane_id))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    use crate::model::{AgentPane, AgentState, Group};
    use crate::store::Config;

    fn pane(id: &str, state: AgentState) -> AgentPane {
        AgentPane { pane_id: id.into(), state, reported_state: state, path: format!("/p/{id}"), background: 0, since: 0, note: String::new() }
    }

    /// Sessions `a` (window @1) and `b` (window @2), in that order.
    fn two_sessions(a: Vec<AgentPane>, b: Vec<AgentPane>) -> PickerState {
        let mut sa = s("a", 1, 1);
        sa.windows = vec![crate::model::Window { id: "@1".into(), index: 3, name: "wa".into(), active: false }];
        let mut sb = s("b", 1, 2);
        sb.windows = vec![crate::model::Window { id: "@2".into(), index: 0, name: "wb".into(), active: false }];
        let cfg = Config {
            groups: vec![Group { name: "G".into(), members: vec!["a".into(), "b".into()], inbox: true, ..Default::default() }],
            ..Default::default()
        };
        let mut st = PickerState::build(vec![sa, sb], &cfg);
        st.set_agents([("@1".to_string(), a), ("@2".to_string(), b)].into_iter().collect());
        st
    }

    fn mixed() -> PickerState {
        two_sessions(
            vec![pane("%1", AgentState::Working), pane("%2", AgentState::Waiting)],
            vec![pane("%3", AgentState::Waiting), pane("%4", AgentState::Done)],
        )
    }

    #[test]
    fn rows_sort_by_urgency_with_visual_order_ties() {
        let ids: Vec<String> = mixed().agent_rows().into_iter().map(|r| r.pane_id).collect();
        assert_eq!(ids, ["%2", "%3", "%4", "%1"]);
    }

    #[test]
    fn row_carries_window_and_path() {
        let r = &mixed().agent_rows()[0];
        assert_eq!((r.session.as_str(), r.window_index, r.window_name.as_str(), r.path.as_str()), ("a", 3, "wa", "/p/%2"));
    }

    #[test]
    fn focus_mode_hides_dormant_sessions_agents() {
        let mut st = mixed();
        st.dormant.insert("b".into());
        assert_eq!(st.agent_rows().len(), 4, "dormant alone only dims");
        st.focus_mode = true;
        let ids: Vec<String> = st.agent_rows().into_iter().map(|r| r.pane_id).collect();
        assert_eq!(ids, ["%2", "%1"]);
    }

    #[test]
    fn rows_flag_dormant_sessions_and_windows() {
        let mut st = mixed();
        st.dormant_windows.insert(("a".into(), 3));
        st.dormant.insert("b".into());
        assert!(st.agent_rows().iter().all(|r| r.dormant));
    }

    #[test]
    fn focus_toggle_clamps_the_agent_cursor() {
        let mut st = mixed();
        st.enter_agents();
        st.agent_move(3);
        st.dormant.insert("b".into());
        st.toggle_focus_mode();
        assert_eq!(st.agent_cursor(), 1);
    }

    #[test]
    fn focus_mode_hides_dormant_windows_agents() {
        let mut st = mixed();
        st.dormant_windows.insert(("b".into(), 0));
        st.focus_mode = true;
        assert_eq!(st.agent_rows().len(), 2);
    }

    #[test]
    fn longest_in_state_sorts_first_and_unknown_start_goes_last() {
        let at = |id, since| AgentPane { since, ..pane(id, AgentState::Waiting) };
        let st = two_sessions(vec![at("%1", 0), at("%2", 200)], vec![at("%3", 100)]);
        let ids: Vec<String> = st.agent_rows().into_iter().map(|r| r.pane_id).collect();
        assert_eq!(ids, ["%3", "%2", "%1"]);
    }

    #[test]
    fn move_wraps_at_edges() {
        let mut st = mixed();
        st.enter_agents();
        st.agent_move(-1);
        assert_eq!(st.agent_cursor(), 3);
        st.agent_move(1);
        assert_eq!(st.agent_cursor(), 0);
    }

    #[test]
    fn enter_and_exit_switch_mode() {
        let mut st = mixed();
        st.enter_agents();
        assert_eq!(st.mode, Mode::Agents);
        st.exit_agents();
        assert_eq!(st.mode, Mode::Picker);
    }

    #[test]
    fn selected_and_numbered_actions_switch_pane() {
        let mut st = mixed();
        st.enter_agents();
        st.agent_move(1);
        assert_eq!(st.agent_selected_action(), Some(Action::SwitchPane("b".into(), 0, "%3".into())));
        assert_eq!(st.action_for_agent_number(1), Some(Action::SwitchPane("a".into(), 3, "%2".into())));
        assert_eq!(st.action_for_agent_number(4), Some(Action::SwitchPane("a".into(), 3, "%1".into())));
        assert_eq!(st.action_for_agent_number(5), None);
    }

    #[test]
    fn number_zero_and_ten_are_none() {
        let st = mixed();
        assert_eq!(st.action_for_agent_number(0), None);
        assert_eq!(st.action_for_agent_number(10), None);
    }

    #[test]
    fn empty_state_has_no_rows_or_actions() {
        let mut st = two_sessions(vec![], vec![]);
        assert!(st.agent_rows().is_empty());
        st.agent_move(1);
        assert_eq!(st.agent_selected_action(), None);
        assert_eq!(st.action_for_agent_number(1), None);
    }
}
