//! Slow blink for the create/rename placeholder text ("session name",
//! "window name", "group name"): hand-rolled by periodically hiding the
//! placeholder for one tick out of every two, rather than relying on the
//! ANSI `SLOW_BLINK` attribute, whose actual on-screen rendering varies by
//! terminal (and can't be detected at runtime) -- see the `main::BLINK_TICK`
//! doc comment for how the event loop wakes up to redraw it. A single
//! free-running clock anchored to when this `PickerState` was constructed,
//! shared by every placeholder rather than a per-prompt timer, so it stays
//! in phase across create/rename/quick-create.

use super::PickerState;
use std::time::Duration;

const BLINK_PERIOD: Duration = Duration::from_millis(500);

impl PickerState {
    /// Whether a blinking placeholder should render its text this frame.
    /// Toggles every `BLINK_PERIOD`.
    pub fn blink_visible(&self) -> bool {
        (self.blink_since.elapsed().as_millis() / BLINK_PERIOD.as_millis()).is_multiple_of(2)
    }

    /// Test-only: back-date the blink clock by `ago`, so tests can exercise
    /// the on/off phases deterministically instead of sleeping in real time.
    #[cfg(test)]
    pub(crate) fn backdate_blink(&mut self, ago: Duration) {
        self.blink_since -= ago;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::test_support::*;

    #[test]
    fn blink_visible_starts_true_and_flips_each_period() {
        let mut st = state_with_two_groups();
        assert!(st.blink_visible(), "visible immediately after construction");

        st.backdate_blink(Duration::from_millis(600));
        assert!(!st.blink_visible(), "hidden during the second half of the cycle");

        st.backdate_blink(Duration::from_millis(500)); // now 1100ms total elapsed
        assert!(st.blink_visible(), "visible again in the next cycle");
    }
}
