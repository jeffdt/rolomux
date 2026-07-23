//! `x` kill machinery: computing what a kill would target (`plan_kill`) and
//! the arm/confirm cycle for the destructive action, mirroring
//! `reorder.rs`'s `pending_window_move`. Simpler than that sibling because
//! there is only one trigger key (`x`, not two directional ones), so
//! confirming doesn't need direction-matching -- any second `x` while armed
//! is unambiguous, since any other keypress (including plain cursor
//! movement) already clears the arm before it could apply to a different
//! row. The actual tmux `kill-session`/`kill-window` call happens in
//! `main.rs`, which is also where the `risky` flag passed to `arm_kill` is
//! computed (it needs an I/O call -- `Tmux::detach_on_destroy_off` -- that
//! has no business in this pure model module).

use super::{PickerState, Row};

/// What a confirmed `x` press should destroy. Carries real tmux identifiers
/// (session name, window index) rather than row/cursor positions, so the
/// confirm survives cursor movement or a rebuild between arm and confirm --
/// though in practice any other keypress already clears the arm first (see
/// `clear_pending_kill`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KillTarget {
    Session(String),
    Window { session: String, index: u32 },
}

/// What's armed by `arm_kill`. `risky` is true when killing `target` would
/// eject an attached tmux client (computed by `main.rs`'s
/// `last_window_risk`-based classification), and selects the scarier
/// footer wording in `pending_kill_warning`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingKill {
    target: KillTarget,
    risky: bool,
}

impl PickerState {
    /// Compute what an `x` press on the current cursor row would target,
    /// without doing anything. `None` only when the cursor isn't on a
    /// session or window row (an empty picker).
    pub fn plan_kill(&self) -> Option<KillTarget> {
        let rows = self.visible_rows();
        let ordered = self.ordered();
        match rows.get(self.cursor)? {
            Row::Session(si) => Some(KillTarget::Session(ordered[*si].name.clone())),
            Row::Window(si, wi) => {
                let sess = ordered[*si];
                Some(KillTarget::Window { session: sess.name.clone(), index: sess.windows[*wi].index })
            }
        }
    }

    /// Arm a confirm: the next `x` press will commit `target` instead of
    /// re-planning; any other key clears it (see `clear_pending_kill`).
    pub fn arm_kill(&mut self, target: KillTarget, risky: bool) {
        self.pending_kill = Some(PendingKill { target, risky });
    }

    /// Consume and return the armed target, if any.
    pub fn take_confirmed_kill(&mut self) -> Option<KillTarget> {
        self.pending_kill.take().map(|p| p.target)
    }

    /// Drop any armed confirm with no side effect. Called for every input
    /// other than `Input::Kill`, mirroring `clear_pending_window_move`.
    pub fn clear_pending_kill(&mut self) {
        self.pending_kill = None;
    }

    /// The footer warning to show while a confirm is armed, or `None`.
    pub fn pending_kill_warning(&self) -> Option<String> {
        self.pending_kill.as_ref().map(|p| {
            let label = match &p.target {
                KillTarget::Session(name) => format!("session '{name}'"),
                KillTarget::Window { session, index } => format!("window {session}:{index}"),
            };
            if p.risky {
                format!("x again to kill {label} \u{2014} will exit tmux \u{b7} Esc cancels")
            } else {
                format!("x again to kill {label} \u{b7} Esc cancels")
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, Window};
    use crate::store::Config;

    fn sess(name: &str, windows: Vec<Window>) -> Session {
        Session { id: String::new(), name: name.into(), activity: 1, created: 1, attached: false, windows }
    }

    fn one_window(name: &str) -> Window {
        Window { id: String::new(), index: 0, name: name.into(), active: true }
    }

    #[test]
    fn plan_kill_on_session_row_targets_the_session() {
        let sessions = vec![sess("alpha", vec![one_window("w")])];
        let cfg = Config { groups: vec![], ..Default::default() };
        let st = PickerState::build(sessions, &cfg);
        assert_eq!(st.plan_kill(), Some(KillTarget::Session("alpha".to_string())));
    }

    #[test]
    fn plan_kill_on_window_row_targets_the_window_by_real_index() {
        let windows = vec![
            Window { id: String::new(), index: 0, name: "editor".into(), active: true },
            Window { id: String::new(), index: 5, name: "logs".into(), active: false },
        ];
        let sessions = vec![sess("alpha", windows)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut st = PickerState::build(sessions, &cfg);
        st.expand();
        st.move_cursor(2); // rows: [session, window 0 "editor", window 5 "logs"]
        assert_eq!(
            st.plan_kill(),
            Some(KillTarget::Window { session: "alpha".to_string(), index: 5 })
        );
    }

    #[test]
    fn arm_and_take_confirmed_kill_round_trips() {
        let sessions = vec![sess("alpha", vec![one_window("w")])];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut st = PickerState::build(sessions, &cfg);
        let target = KillTarget::Session("alpha".to_string());

        st.arm_kill(target.clone(), false);
        assert!(st.pending_kill_warning().is_some());

        assert_eq!(st.take_confirmed_kill(), Some(target));
        assert!(st.pending_kill_warning().is_none(), "confirming consumes the arm");
    }

    #[test]
    fn clear_pending_kill_drops_the_arm_with_no_side_effect() {
        let sessions = vec![sess("alpha", vec![one_window("w")])];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut st = PickerState::build(sessions, &cfg);
        st.arm_kill(KillTarget::Session("alpha".to_string()), false);

        st.clear_pending_kill();

        assert!(st.pending_kill_warning().is_none());
        assert_eq!(st.take_confirmed_kill(), None);
    }

    #[test]
    fn pending_kill_warning_uses_scarier_wording_when_risky() {
        let sessions = vec![sess("alpha", vec![one_window("w")])];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut st = PickerState::build(sessions, &cfg);

        st.arm_kill(KillTarget::Session("alpha".to_string()), true);
        let risky = st.pending_kill_warning().unwrap();
        assert!(risky.contains("exit tmux"), "risky wording warns about ejection: {risky}");

        st.arm_kill(KillTarget::Session("alpha".to_string()), false);
        let safe = st.pending_kill_warning().unwrap();
        assert!(!safe.contains("exit tmux"), "non-risky wording stays plain: {safe}");
    }

    #[test]
    fn pending_kill_warning_names_a_window_target_as_session_colon_index() {
        let sessions = vec![sess("alpha", vec![one_window("w")])];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut st = PickerState::build(sessions, &cfg);
        st.arm_kill(KillTarget::Window { session: "alpha".to_string(), index: 3 }, false);
        assert!(st.pending_kill_warning().unwrap().contains("alpha:3"));
    }
}
