//! The brief directional flash shown on the row that just moved via
//! `⇧J`/`⇧K` (issue #130, narrowed to a single arrow per move by #139): a
//! `▲`/`▼` in the row's right margin, bright for `SWAP_INDICATOR_BRIGHT`,
//! dim for the remainder of `SWAP_INDICATOR_TOTAL`, then gone. Session,
//! window, and group reorders (`reorder.rs`, `groups.rs`,
//! `main::commit_window_move`) all funnel into the same
//! `PickerState::swap_indicator` field, so only one flash is ever in flight.

use super::PickerState;
use std::time::{Duration, Instant};

const SWAP_INDICATOR_BRIGHT: Duration = Duration::from_millis(250);
const SWAP_INDICATOR_TOTAL: Duration = Duration::from_millis(1000);

/// Identifies a single row in a way that survives a full `PickerState`
/// rebuild -- window moves commit via real tmux calls and a re-gather
/// (`main::commit_window_move`), so a row index would go stale immediately.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SwapKey {
    Session(String),
    /// Session name, stable tmux window index (survives the rebuild; a
    /// `Vec` position would not).
    Window(String, u32),
    Group(String),
}

/// Which way a marked row moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapDirection {
    Up,
    Down,
}

pub(super) struct SwapIndicator {
    key: SwapKey,
    direction: SwapDirection,
    started: Instant,
}

impl PickerState {
    fn set_swap_indicator(&mut self, key: SwapKey, direction: SwapDirection) {
        self.swap_indicator = Some(SwapIndicator { key, direction, started: Instant::now() });
    }

    /// A session moved within or across a group boundary. `direction` is the
    /// way it travelled; the displaced neighbor (if any) gets no marker.
    pub(super) fn set_session_swap(&mut self, moved_name: &str, direction: SwapDirection) {
        self.set_swap_indicator(SwapKey::Session(moved_name.to_string()), direction);
    }

    /// A session crossed a group boundary with no swap partner (lands at
    /// the front/back of the adjacent group). `moved_up` is `true` when
    /// this came from `move_up`.
    pub(super) fn set_session_cross(&mut self, name: &str, moved_up: bool) {
        let direction = if moved_up { SwapDirection::Up } else { SwapDirection::Down };
        self.set_swap_indicator(SwapKey::Session(name.to_string()), direction);
    }

    /// A group moved. `direction` is the way it travelled; the displaced
    /// neighbor gets no marker.
    pub(super) fn set_group_swap(&mut self, moved_name: &str, direction: SwapDirection) {
        self.set_swap_indicator(SwapKey::Group(moved_name.to_string()), direction);
    }

    /// A window swapped position within `session`. `moved_index` is its
    /// *final* (post-swap) tmux window index -- `main::commit_window_move`
    /// already computes this for `focus_window`. `delta` is the `⇧J`/`⇧K`
    /// press that triggered it: negative means the moved window went up.
    /// The displaced neighbor gets no marker.
    pub fn set_window_swap(&mut self, session: &str, moved_index: u32, delta: i32) {
        let direction = if delta < 0 { SwapDirection::Up } else { SwapDirection::Down };
        self.set_swap_indicator(SwapKey::Window(session.to_string(), moved_index), direction);
    }

    /// A window crossed into an adjacent session with no swap partner,
    /// landing at `(dst_session, dst_index)`. `delta` is the triggering
    /// press: negative means it moved up.
    pub fn set_window_cross(&mut self, dst_session: &str, dst_index: u32, delta: i32) {
        let direction = if delta < 0 { SwapDirection::Up } else { SwapDirection::Down };
        self.set_swap_indicator(SwapKey::Window(dst_session.to_string(), dst_index), direction);
    }

    /// Whether a swap indicator is currently in flight -- `main`'s event
    /// loop uses this to decide whether to poll on a short tick instead of
    /// blocking on the next keypress.
    pub fn swap_indicator_active(&self) -> bool {
        self.swap_indicator.is_some()
    }

    /// Clear the indicator once `SWAP_INDICATOR_TOTAL` has elapsed. A no-op
    /// otherwise, including when there's no active indicator.
    pub fn tick_swap_indicator(&mut self) {
        if matches!(&self.swap_indicator, Some(ind) if ind.started.elapsed() >= SWAP_INDICATOR_TOTAL) {
            self.swap_indicator = None;
        }
    }

    fn swap_stage(&self, key: &SwapKey) -> Option<(SwapDirection, bool)> {
        let ind = self.swap_indicator.as_ref()?;
        let elapsed = ind.started.elapsed();
        if elapsed >= SWAP_INDICATOR_TOTAL || &ind.key != key {
            return None;
        }
        Some((ind.direction, elapsed < SWAP_INDICATOR_BRIGHT))
    }

    /// `(direction, bright)` for a session row, if it's part of the active
    /// swap indicator. `bright` is `true` during the initial flash, `false`
    /// during the dimmer fade that follows.
    pub fn session_swap_marker(&self, name: &str) -> Option<(SwapDirection, bool)> {
        self.swap_stage(&SwapKey::Session(name.to_string()))
    }

    pub fn window_swap_marker(&self, session: &str, index: u32) -> Option<(SwapDirection, bool)> {
        self.swap_stage(&SwapKey::Window(session.to_string(), index))
    }

    pub fn group_swap_marker(&self, name: &str) -> Option<(SwapDirection, bool)> {
        self.swap_stage(&SwapKey::Group(name.to_string()))
    }

    /// Test-only: back-date the active indicator's clock by `ago`, so tests
    /// can exercise the bright/dim/expired stages deterministically instead
    /// of sleeping in real time.
    #[cfg(test)]
    pub(crate) fn backdate_swap_indicator(&mut self, ago: Duration) {
        if let Some(ind) = self.swap_indicator.as_mut() {
            ind.started -= ago;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::test_support::*;

    #[test]
    fn session_swap_marker_is_none_with_no_active_indicator() {
        let st = state_with_two_groups();
        assert_eq!(st.session_swap_marker("a"), None);
    }

    #[test]
    fn set_session_swap_marks_only_the_moved_session() {
        let mut st = state_with_two_groups();
        st.set_session_swap("a", SwapDirection::Up);
        assert_eq!(st.session_swap_marker("a"), Some((SwapDirection::Up, true)));
        assert_eq!(st.session_swap_marker("b"), None, "displaced neighbor gets no marker");
        assert_eq!(st.session_swap_marker("c"), None, "uninvolved session gets no marker");
    }

    #[test]
    fn set_session_cross_marks_only_the_moved_session() {
        let mut st = state_with_two_groups();
        st.set_session_cross("a", true); // moved up, no partner
        assert_eq!(st.session_swap_marker("a"), Some((SwapDirection::Up, true)));
        assert_eq!(st.session_swap_marker("b"), None);

        st.set_session_cross("b", false); // moved down, no partner
        assert_eq!(st.session_swap_marker("b"), Some((SwapDirection::Down, true)));
        assert_eq!(st.session_swap_marker("a"), None, "replaced by the newer indicator");
    }

    #[test]
    fn set_group_swap_marks_only_the_moved_group() {
        let mut st = state_with_two_groups();
        st.set_group_swap("G2", SwapDirection::Up);
        assert_eq!(st.group_swap_marker("G2"), Some((SwapDirection::Up, true)));
        assert_eq!(st.group_swap_marker("G1"), None, "displaced neighbor gets no marker");
    }

    #[test]
    fn set_window_swap_marks_only_the_moved_window() {
        let mut st = state_with_two_groups();
        st.set_window_swap("alpha", 2, -1); // moved up: final index 2 is "up"
        assert_eq!(st.window_swap_marker("alpha", 2), Some((SwapDirection::Up, true)));
        assert_eq!(st.window_swap_marker("alpha", 1), None, "displaced neighbor gets no marker");

        st.set_window_swap("alpha", 1, 1); // moved down: final index 1 is "down"
        assert_eq!(st.window_swap_marker("alpha", 1), Some((SwapDirection::Down, true)));
        assert_eq!(st.window_swap_marker("alpha", 2), None, "displaced neighbor gets no marker");
    }

    #[test]
    fn set_window_cross_marks_only_the_moved_window() {
        let mut st = state_with_two_groups();
        st.set_window_cross("beta", 3, -1); // moved up
        assert_eq!(st.window_swap_marker("beta", 3), Some((SwapDirection::Up, true)));

        st.set_window_cross("beta", 3, 1); // moved down
        assert_eq!(st.window_swap_marker("beta", 3), Some((SwapDirection::Down, true)));
    }

    #[test]
    fn swap_marker_dims_after_the_bright_window_then_expires_after_the_total_window() {
        let mut st = state_with_two_groups();
        st.set_session_swap("a", SwapDirection::Up);
        assert!(st.swap_indicator_active());

        st.backdate_swap_indicator(Duration::from_millis(300)); // past BRIGHT, still within TOTAL
        st.tick_swap_indicator();
        assert_eq!(st.session_swap_marker("a"), Some((SwapDirection::Up, false)), "dim stage");
        assert!(st.swap_indicator_active(), "still active during the dim stage");

        st.backdate_swap_indicator(Duration::from_millis(800)); // now 1100ms total elapsed
        st.tick_swap_indicator();
        assert_eq!(st.session_swap_marker("a"), None, "expired");
        assert!(!st.swap_indicator_active());
    }
}
