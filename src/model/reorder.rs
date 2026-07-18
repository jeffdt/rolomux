//! `⇧J`/`⇧K` movement machinery at both altitudes: session-level reordering
//! within and across groups (`move_row` and friends), and window-level moves
//! within and across sessions (`plan_window_move` plus its arm/confirm cycle).
//! All of it is pure planning or `members`-list bookkeeping; the actual tmux
//! `swap-window`/`move-window` calls happen in `main.rs`.

use super::{PickerState, Row};

/// A planned window-level `⇧J`/`⇧K` action, computed by `plan_window_move`.
/// Pure data -- no tmux call happens until `main.rs` commits it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowMove {
    /// Swap two windows within the same session (`tmux swap-window`).
    SwapWithin { session: String, a_index: u32, b_index: u32 },
    /// Move a window into an adjacent session (`tmux move-window`).
    /// `before: true` inserts before `dst_anchor_index` (landing first),
    /// `false` inserts after it (landing last). `kills_source` is true iff
    /// `window_index` is the only window left in `src_session` -- moving it
    /// away would destroy that session. `src_attached` is `src_session`'s
    /// `Session::attached` flag, carried through so callers don't need to
    /// re-look it up.
    CrossSession {
        src_session: String,
        window_index: u32,
        dst_session: String,
        dst_anchor_index: u32,
        before: bool,
        kills_source: bool,
        src_attached: bool,
    },
}

/// What's armed by `arm_window_move`, and which direction confirms it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingWindowMove {
    mv: WindowMove,
    delta: i32,
}

impl PickerState {
    /// Arm a confirm: the next matching-direction `MoveUp`/`MoveDown` will
    /// commit `mv` instead of re-planning; anything else clears it.
    pub fn arm_window_move(&mut self, mv: WindowMove, delta: i32) {
        self.pending_window_move = Some(PendingWindowMove { mv, delta });
    }

    /// Consume and return the armed move if `delta` matches what was armed;
    /// otherwise leaves the arm untouched (a different direction doesn't
    /// confirm it) and returns `None`.
    pub fn take_confirmed_window_move(&mut self, delta: i32) -> Option<WindowMove> {
        match &self.pending_window_move {
            Some(p) if p.delta == delta => self.pending_window_move.take().map(|p| p.mv),
            _ => None,
        }
    }

    /// Drop any armed confirm with no side effect. Called for every input
    /// other than `MoveUp`/`MoveDown`.
    pub fn clear_pending_window_move(&mut self) {
        self.pending_window_move = None;
    }

    /// The footer warning to show while a confirm is armed, or `None`.
    pub fn pending_window_move_warning(&self) -> Option<&'static str> {
        self.pending_window_move.as_ref().map(|p| {
            if p.delta < 0 {
                "⇧K again to move last window — closes session · Esc cancels"
            } else {
                "⇧J again to move last window — closes session · Esc cancels"
            }
        })
    }

    /// Compute what a window-row `⇧J`/`⇧K` press should do, without doing
    /// it. Returns `None` when the cursor isn't on a window row, or when
    /// there is truly nowhere for the window to go (a single session with a
    /// single window). Never mutates `self` -- the actual tmux call and
    /// state rebuild happen in `main.rs`, mirroring how rename works.
    pub fn plan_window_move(&self, delta: i32) -> Option<WindowMove> {
        let rows = self.visible_rows();
        let (si, wi) = match rows.get(self.cursor) {
            Some(Row::Window(si, wi)) => (*si, *wi),
            _ => return None,
        };
        let ordered = self.ordered();
        let sess = ordered[si];

        if delta < 0 && wi > 0 {
            let neighbor = &sess.windows[wi - 1];
            return Some(WindowMove::SwapWithin {
                session: sess.name.clone(),
                a_index: sess.windows[wi].index,
                b_index: neighbor.index,
            });
        }
        if delta > 0 && wi + 1 < sess.windows.len() {
            let neighbor = &sess.windows[wi + 1];
            return Some(WindowMove::SwapWithin {
                session: sess.name.clone(),
                a_index: sess.windows[wi].index,
                b_index: neighbor.index,
            });
        }

        // At the edge of this session's own window list: cross into the
        // adjacent session in the flat visible order, wrapping around the
        // whole list -- unless this is the only session on screen, in
        // which case there is nowhere else to go.
        if ordered.len() <= 1 {
            return None;
        }
        let dst_si = if delta < 0 {
            (si + ordered.len() - 1) % ordered.len()
        } else {
            (si + 1) % ordered.len()
        };
        let dst = ordered[dst_si];
        let (dst_anchor_index, before) = if delta < 0 {
            // Moving up and out: land at the *last* slot of the session above.
            (dst.windows.last().expect("a session always has >= 1 window").index, false)
        } else {
            // Moving down and out: land at the *first* slot of the session below.
            (dst.windows[0].index, true)
        };
        Some(WindowMove::CrossSession {
            src_session: sess.name.clone(),
            window_index: sess.windows[wi].index,
            dst_session: dst.name.clone(),
            dst_anchor_index,
            before,
            kills_source: sess.windows.len() == 1,
            src_attached: sess.attached,
        })
    }

    /// Move the session under the cursor by `delta` rows, crossing group
    /// boundaries into the group above/below when at an edge -- the inbox
    /// group included, since it's just another entry in `self.groups` now.
    /// Wraps around at the very top and bottom of the whole list (top wraps
    /// to the end of the last group, bottom wraps to the front of the first
    /// group) rather than clamping.
    pub fn move_row(&mut self, delta: i32) {
        let name = match self.cursor_session_name() { Some(n) => n, None => return };
        let gi = match self.group_index_of(&name) { Some(g) => g, None => return };
        if delta < 0 {
            self.move_up(gi, &name);
        } else {
            self.move_down(gi, &name);
        }
    }

    /// The effective visual order of group `gi`: its persisted `members`,
    /// then -- only possible when `gi` is the inbox -- every other session
    /// that falls back to it but isn't persisted yet, oldest-created first.
    /// For any non-inbox group this is just `members.clone()`, since a
    /// named group never has fallback content.
    fn effective_order(&self, gi: usize) -> Vec<String> {
        let mut order = self.groups[gi].members.clone();
        if self.groups[gi].inbox {
            let overflow: Vec<String> = self
                .inbox_overflow(gi, |name| order.iter().any(|m| m == name))
                .into_iter()
                .map(|s| s.name.clone())
                .collect();
            order.extend(overflow);
        }
        order
    }

    /// Move the session at the top of its group up into the group above,
    /// wrapping around to the *last* group when this is already the first
    /// group -- there is no longer a clamp at the very top of the list.
    fn move_up(&mut self, gi: usize, name: &str) {
        let order = self.effective_order(gi);
        let pos = match order.iter().position(|n| n == name) { Some(p) => p, None => return };
        if let Some(prev) = self.previous_visible_position(&order, pos) {
            self.commit_swap(gi, order, pos, prev, name);
            return;
        }
        let dest_gi = (gi + self.groups.len() - 1) % self.groups.len();
        self.cross_into_group(gi, dest_gi, name, true);
    }

    /// Move the session at the bottom of its group down into the group
    /// below, wrapping around to the *first* group when this is already the
    /// last group -- there is no longer a clamp at the very bottom of the list.
    fn move_down(&mut self, gi: usize, name: &str) {
        let order = self.effective_order(gi);
        let pos = match order.iter().position(|n| n == name) { Some(p) => p, None => return };
        if let Some(next) = self.next_visible_position(&order, pos) {
            self.commit_swap(gi, order, pos, next, name);
            return;
        }
        let dest_gi = (gi + 1) % self.groups.len();
        self.cross_into_group(gi, dest_gi, name, false);
    }

    /// Move `name` out of `src_gi` and into `dest_gi`, landing at the end
    /// (`append: true`) or the front (`append: false`) of `dest_gi`'s full
    /// rendered content -- not just its already-persisted `members` prefix.
    ///
    /// This must freeze `dest_gi`'s *effective* order (persisted members
    /// plus any never-touched inbox fallback overflow) before inserting,
    /// not just push/insert onto the raw `members` list: `members` always
    /// renders before fallback overflow in `ordered()`, so a naive
    /// `members.push` would land `name` ahead of any untouched fallback
    /// sessions instead of after them, when `dest_gi` is an inbox with
    /// fallback content still unfrozen. The defensive `retain` after
    /// computing `dest_order` is required, not optional: once `name` is
    /// removed from `src_gi.members`, `group_index_of(name)` falls back to
    /// the inbox, so if `dest_gi` is itself the inbox, `effective_order`
    /// may already include `name` via that same fallback -- without the
    /// retain, `name` would be double-inserted.
    fn cross_into_group(&mut self, src_gi: usize, dest_gi: usize, name: &str, append: bool) {
        self.groups[src_gi].members.retain(|m| m != name);
        let mut dest_order = self.effective_order(dest_gi);
        dest_order.retain(|n| n != name);
        if append {
            dest_order.push(name.to_string());
        } else {
            dest_order.insert(0, name.to_string());
        }
        self.groups[dest_gi].members = dest_order;
        self.dirty = true;
        self.focus_session(name);
    }

    fn previous_visible_position(&self, order: &[String], pos: usize) -> Option<usize> {
        order[..pos]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, name)| self.session_visible(name))
            .map(|(i, _)| i)
    }

    fn next_visible_position(&self, order: &[String], pos: usize) -> Option<usize> {
        order
            .iter()
            .enumerate()
            .skip(pos + 1)
            .find(|(_, name)| self.session_visible(name))
            .map(|(i, _)| i)
    }

    /// Commit an in-group swap: freezes `order` (with positions `a` and `b`
    /// already swapped) as the group's persisted `members`. For a named
    /// group this is behaviorally identical to swapping in place. For the
    /// inbox, this is also what "freezes" any never-touched fallback
    /// members into a concrete, persisted order on first touch.
    fn commit_swap(&mut self, gi: usize, mut order: Vec<String>, a: usize, b: usize, name: &str) {
        order.swap(a, b);
        self.groups[gi].members = order;
        self.dirty = true;
        self.focus_session(name);
    }
}

#[cfg(test)]
mod tests {
    use crate::model::*;
    use crate::model::test_support::*;
    use crate::store::Config;

    #[test]
    fn plan_window_move_swaps_with_previous_window_in_same_session() {
        let sessions = vec![session_with_windows("work", 1, vec![win(0, "a"), win(1, "b"), win(2, "c")])];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("work");
        st.expand();
        st.move_cursor(3); // rows: [work, a, b, c]; lands on "c" (wi = 2)
        assert_eq!(
            st.plan_window_move(-1),
            Some(WindowMove::SwapWithin { session: "work".into(), a_index: 2, b_index: 1 })
        );
    }

    #[test]
    fn plan_window_move_swaps_with_next_window_in_same_session() {
        let sessions = vec![session_with_windows("work", 1, vec![win(0, "a"), win(1, "b"), win(2, "c")])];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("work");
        st.expand();
        st.move_cursor(1); // rows: [work, a, b, c]; lands on "a" (wi = 0)
        assert_eq!(
            st.plan_window_move(1),
            Some(WindowMove::SwapWithin { session: "work".into(), a_index: 0, b_index: 1 })
        );
    }

    #[test]
    fn plan_window_move_up_from_first_window_crosses_into_session_above_last_slot() {
        let sessions = vec![
            session_with_windows("alpha", 1, vec![win(0, "a1"), win(3, "a2")]),
            session_with_windows("beta", 2, vec![win(0, "b1")]),
        ];
        let cfg = Config {
            groups: vec![Group {
                name: "ONLY".into(),
                members: vec!["alpha".into(), "beta".into()],
                inbox: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("beta");
        st.expand();
        st.move_cursor(1); // rows: [alpha, a1, a2, beta, b1]; lands on "b1" (wi = 0, only window)
        assert_eq!(
            st.plan_window_move(-1),
            Some(WindowMove::CrossSession {
                src_session: "beta".into(),
                window_index: 0,
                dst_session: "alpha".into(),
                dst_anchor_index: 3,
                before: false,
                kills_source: true,
                src_attached: false,
            })
        );
    }

    #[test]
    fn plan_window_move_down_from_last_window_crosses_into_session_below_first_slot() {
        let sessions = vec![
            session_with_windows("alpha", 1, vec![win(0, "a1"), win(1, "a2")]),
            session_with_windows("beta", 2, vec![win(0, "b1"), win(2, "b2")]),
        ];
        let cfg = Config {
            groups: vec![Group {
                name: "ONLY".into(),
                members: vec!["alpha".into(), "beta".into()],
                inbox: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("alpha");
        st.expand();
        st.move_cursor(2); // rows: [alpha, a1, a2, beta]; lands on "a2" (wi = 1, last window)
        assert_eq!(
            st.plan_window_move(1),
            Some(WindowMove::CrossSession {
                src_session: "alpha".into(),
                window_index: 1,
                dst_session: "beta".into(),
                dst_anchor_index: 0,
                before: true,
                kills_source: false,
                src_attached: false,
            })
        );
    }

    #[test]
    fn plan_window_move_wraps_from_first_session_up_to_last_session_last_slot() {
        let sessions = vec![
            session_with_windows("alpha", 1, vec![win(0, "a1")]),
            session_with_windows("beta", 2, vec![win(0, "b1"), win(1, "b2")]),
        ];
        let cfg = Config {
            groups: vec![Group {
                name: "ONLY".into(),
                members: vec!["alpha".into(), "beta".into()],
                inbox: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("alpha");
        st.expand();
        st.move_cursor(1); // rows: [alpha, a1, beta, b1, b2]; lands on "a1" (wi = 0, only window)
        assert_eq!(
            st.plan_window_move(-1),
            Some(WindowMove::CrossSession {
                src_session: "alpha".into(),
                window_index: 0,
                dst_session: "beta".into(), // wraps around to the last session on screen
                dst_anchor_index: 1,
                before: false,
                kills_source: true,
                src_attached: false,
            })
        );
    }

    #[test]
    fn plan_window_move_is_none_when_only_one_session_with_one_window() {
        let sessions = vec![session_with_windows("solo", 1, vec![win(0, "only")])];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("solo");
        st.expand();
        st.move_cursor(1); // the only window
        assert_eq!(st.plan_window_move(-1), None);
        assert_eq!(st.plan_window_move(1), None);
    }

    #[test]
    fn plan_window_move_is_none_when_cursor_is_on_a_session_row() {
        let sessions = vec![session_with_windows("solo", 1, vec![win(0, "only"), win(1, "other")])];
        let cfg = Config::default();
        let st = PickerState::build(sessions, &cfg); // cursor defaults onto the session row
        assert_eq!(st.plan_window_move(-1), None);
    }

    #[test]
    fn arm_and_take_confirmed_window_move_round_trips_on_matching_direction() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        let mv = WindowMove::SwapWithin { session: "a".into(), a_index: 0, b_index: 1 };
        st.arm_window_move(mv.clone(), -1);
        assert!(st.pending_window_move_warning().is_some());
        assert_eq!(st.take_confirmed_window_move(1), None, "wrong direction doesn't confirm");
        assert!(st.pending_window_move_warning().is_some(), "still armed after a non-matching direction");
        assert_eq!(st.take_confirmed_window_move(-1), Some(mv), "matching direction confirms and consumes it");
        assert!(st.pending_window_move_warning().is_none());
    }

    #[test]
    fn clear_pending_window_move_drops_the_arm() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        let mv = WindowMove::SwapWithin { session: "a".into(), a_index: 0, b_index: 1 };
        st.arm_window_move(mv, -1);
        st.clear_pending_window_move();
        assert!(st.pending_window_move_warning().is_none());
        assert_eq!(st.take_confirmed_window_move(-1), None);
    }

    #[test]
    fn move_row_unpinned_in_manual_freezes_then_swaps_and_dirties() {
        // Manual + empty list => base order is created asc: a(1), b(2), c(3).
        let sessions = vec![s("a", 9, 1), s("b", 9, 2), s("c", 9, 3)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        state.focus_session("b");
        state.move_row(-1); // move b up past a
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["b", "a", "c"]);
        // The full ungrouped order is frozen into the inbox group's members on the first move.
        assert_eq!(
            state.groups[state.inbox_index().unwrap()].members,
            vec!["b".to_string(), "a".to_string(), "c".to_string()]
        );
        assert!(state.dirty);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));

        // Moving up at the top now wraps to the bottom of the (single) group.
        state.dirty = false;
        state.move_row(-1);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["a", "c", "b"]);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
        assert!(state.dirty);
    }

    #[test]
    fn move_row_unpinned_at_residual_bottom_wraps_to_front() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("b");
        state.move_row(1);
        assert!(state.dirty);
        assert_eq!(
            state.groups[state.inbox_index().unwrap()].members,
            vec!["b".to_string(), "a".to_string()]
        );
    }

    #[test]
    fn move_row_on_pinned_reorders_pins_in_any_mode() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config {
            dormant: vec![], groups: vec![Group { name: "PINNED".into(), members: vec!["a".into(), "b".into()], color: String::new(), ..Default::default() }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("a");
        state.move_row(1);
        assert_eq!(state.groups[0].members, vec!["b".to_string(), "a".to_string()]);
        assert!(state.dirty);
    }

    #[test]
    fn move_row_skips_hidden_dormant_sessions() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2), s("gamma", 1, 3)];
        let cfg = Config {
            groups: vec![Group {
                name: "INBOX".into(),
                members: vec!["alpha".into(), "beta".into(), "gamma".into()],
                inbox: true,
                ..Default::default()
            }],
            dormant: vec!["beta".into()],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_focus_mode();
        state.focus_session("alpha");

        state.move_row(1);

        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["gamma", "alpha"]);
        let all_members = &state.groups[state.inbox_index().unwrap()].members;
        assert_eq!(all_members, &vec!["gamma".to_string(), "beta".to_string(), "alpha".to_string()]);
    }

    #[test]
    fn move_up_from_group_top_joins_end_of_group_above() {
        let mut st = state_with_two_groups();
        st.focus_session("c"); // top (only) of G2
        st.move_row(-1);
        assert_eq!(st.groups[0].members, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!(st.groups[1].members, Vec::<String>::new());
        assert_eq!(st.cursor_session_name().as_deref(), Some("c"));
        assert!(st.dirty);
    }

    #[test]
    fn move_up_within_group_swaps() {
        let mut st = state_with_two_groups();
        st.focus_session("b");
        st.move_row(-1);
        assert_eq!(st.groups[0].members, vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn move_up_at_very_top_wraps_to_last_group_end() {
        let mut st = state_with_two_groups();
        st.focus_session("a"); // top of first group == top of the whole visible list
        st.move_row(-1);
        assert_eq!(st.groups[0].members, vec!["b".to_string()]);
        let inbox = st.inbox_index().unwrap();
        // inbox (the last group here) has untouched fallback members d, e;
        // "a" must land after them, not before -- this is the case that
        // needs the effective-order-aware cross_into_group helper below,
        // not a naive `.members.push`.
        assert_eq!(
            st.groups[inbox].members,
            vec!["d".to_string(), "e".to_string(), "a".to_string()]
        );
        assert_eq!(st.cursor_session_name().as_deref(), Some("a"));
        assert!(st.dirty);
    }

    #[test]
    fn move_down_from_group_bottom_joins_front_of_group_below() {
        let mut st = state_with_two_groups();
        st.focus_session("b"); // bottom of G1
        st.move_row(1);
        assert_eq!(st.groups[0].members, vec!["a".to_string()]);
        assert_eq!(st.groups[1].members, vec!["b".to_string(), "c".to_string()]);
    }

    #[test]
    fn move_down_from_last_group_bottom_drops_into_residual() {
        let mut st = state_with_two_groups();
        st.focus_session("c"); // bottom of last group G2
        st.move_row(1);
        assert_eq!(st.groups[1].members, Vec::<String>::new());
        assert_eq!(st.group_index_of("c"), st.inbox_index());
    }

    #[test]
    fn move_up_from_residual_top_joins_last_group() {
        let mut st = state_with_two_groups();
        st.focus_session("d"); // residual top (activity 40)
        st.move_row(-1);
        assert_eq!(st.groups[1].members, vec!["c".to_string(), "d".to_string()]);
        assert_ne!(st.group_index_of("d"), st.inbox_index());
    }

    #[test]
    fn move_down_at_residual_bottom_wraps_to_first_group_front() {
        let mut st = state_with_two_groups();
        st.focus_session("e"); // bottom of inbox == bottom of the whole visible list
        st.move_row(1);
        assert_eq!(
            st.groups[0].members,
            vec!["e".to_string(), "a".to_string(), "b".to_string()]
        );
        assert_eq!(st.cursor_session_name().as_deref(), Some("e"));
        assert!(st.dirty);
    }

    #[test]
    fn move_wraps_within_a_single_group_when_only_one_group_exists() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("c", 1, 3)];
        let cfg = Config {
            groups: vec![Group {
                name: "ONLY".into(),
                members: vec!["a".into(), "b".into(), "c".into()],
                inbox: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("a"); // top of the only group
        st.move_row(-1);
        assert_eq!(
            st.groups[0].members,
            vec!["b".to_string(), "c".to_string(), "a".to_string()]
        );
        assert_eq!(st.cursor_session_name().as_deref(), Some("a"));
    }
}
