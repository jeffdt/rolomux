//! Group mode: the full-screen overlay for creating, renaming, recoloring,
//! reordering, and deleting groups. Operates purely on `self.groups` and the
//! group-mode cursor/edit state; never touches session rows.

use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

impl PickerState {
    /// Enter the full-screen group-management mode with the cursor on the first
    /// group (clamped when there are none).
    pub fn enter_groups(&mut self) {
        self.mode = Mode::Groups;
        self.group_edit = None;
        self.group_cursor = self.group_cursor.min(self.groups.len().saturating_sub(1));
    }

    /// Leave group mode back to session command mode, dropping any in-flight edit.
    pub fn exit_groups(&mut self) {
        if self.group_editing() {
            self.group_cancel_rename();
        }
        self.mode = Mode::Command;
    }

    /// The current cursor position within the group list.
    pub fn group_cursor(&self) -> usize { self.group_cursor }

    /// Whether a rename is currently in progress.
    pub fn group_editing(&self) -> bool { self.group_edit.is_some() }

    /// The in-flight rename buffer, if a rename is in progress.
    pub fn group_edit_buffer(&self) -> Option<&str> { self.group_edit.as_deref() }

    /// Move the group cursor by `delta`, wrapping between the first and last group.
    pub fn group_move_cursor(&mut self, delta: i32) {
        self.group_cursor = move_index_with_edge_wrap(self.group_cursor, delta, self.groups.len());
    }

    /// Reorder the selected group among the named groups (clamped at the ends).
    /// A no-op if either side of the swap is the inbox: the inbox always
    /// sits in the trailing slot (see `ensure_inbox_last`) and can't be
    /// reordered, only renamed/recolored -- issue #23 originally let it move
    /// freely, but that just created problems (see #111) rather than solving
    /// any, so it's pinned now.
    /// A group with an unset (positional-default) color resolves its display
    /// color from its index (see `ui::group_color`), so before swapping we
    /// pin each of the two groups' *currently visible* color explicitly --
    /// same "resolve, then store explicit" move `group_cycle_color` already
    /// makes -- so the swap itself never changes what's on screen (issue
    /// #118: without this, repeated ⇧J/⇧K on a fresh, never-flipped group
    /// cycled its color every press).
    pub fn group_reorder(&mut self, delta: i32) {
        let gc = self.group_cursor;
        let target = gc as i32 + delta;
        if target < 0 || target >= self.groups.len() as i32 {
            self.group_reorder_blocked = false; // plain edge clamp, not an inbox block
            return;
        }
        let target = target as usize;
        if self.groups[gc].inbox || self.groups[target].inbox {
            self.group_reorder_blocked = true;
            return;
        }
        self.group_reorder_blocked = false;
        if !self.active_palette.is_empty() {
            let palette = &self.active_palette;
            let resolve = |g: &Group, idx: usize| {
                if g.color.is_empty() { palette[idx % palette.len()].clone() } else { g.color.clone() }
            };
            let gc_color = resolve(&self.groups[gc], gc);
            let target_color = resolve(&self.groups[target], target);
            self.groups[gc].color = gc_color;
            self.groups[target].color = target_color;
        }
        // Captured before the swap: `moved_from_gc` is the group under the
        // cursor, the one the user actually moved.
        let moved_from_gc = self.groups[gc].name.clone();
        self.groups.swap(gc, target);
        self.group_cursor = target;
        self.dirty = true;
        let direction = if delta < 0 { SwapDirection::Up } else { SwapDirection::Down };
        self.set_group_swap(&moved_from_gc, direction);
    }

    /// The message to show in place of the group-mode footer hint after a
    /// blocked inbox-reorder attempt, or `None` the rest of the time.
    pub fn group_reorder_blocked_warning(&self) -> Option<&'static str> {
        self.group_reorder_blocked.then_some("Inbox can't be reordered")
    }

    /// Drop the one-shot blocked-reorder warning. Called for any group-mode
    /// input other than `⇧J`/`⇧K`, mirroring `clear_pending_window_move`'s
    /// clear-on-any-other-key lifecycle.
    pub fn clear_group_reorder_warning(&mut self) {
        self.group_reorder_blocked = false;
    }

    /// Insert a new empty group and begin naming it. The header color is
    /// resolved from the current new-group-color policy: Rotate leaves it
    /// unset (positional default, resolved at render/cycle time from the
    /// live active palette); Random picks once now from the active palette;
    /// Static uses the configured static color. Neither Random nor Static
    /// retroactively touch any other group. The insertion point is governed
    /// by `new_group_position`: `Top` is the absolute top of `groups`;
    /// `Bottom` lands immediately above the inbox, which always occupies the
    /// trailing slot (see `ensure_inbox_last`) -- so "Bottom" reads as "the
    /// bottom of the named groups," not literally the end of the vector.
    pub fn group_new(&mut self) {
        let color = match self.new_group_color_policy {
            ColorPolicy::Rotate => String::new(),
            ColorPolicy::Random => pick_random_color(&self.active_palette, random_seed()),
            ColorPolicy::Static => self.static_color.clone(),
        };
        let group = Group { name: String::new(), members: Vec::new(), color, inbox: false };
        let index = match self.new_group_position {
            NewGroupPosition::Top => 0,
            NewGroupPosition::Bottom => self.inbox_index().unwrap_or(self.groups.len()),
        };
        self.groups.insert(index, group);
        self.group_cursor = index;
        self.group_edit = Some(String::new());
    }

    /// Advance the selected group's header color to the next in the live
    /// `active_palette`, wrapping around. Starts from the group's effective
    /// color (its explicit name, or the positional default) and stores the
    /// result explicitly so it no longer shifts when groups are reordered.
    /// Guarded against an empty palette (should not happen -- Settings mode's
    /// min-1 toggle guard and the config loader's empty-palette fallback both
    /// prevent it -- but never panics if it somehow does).
    pub fn group_cycle_color(&mut self) {
        let gi = self.group_cursor;
        if gi >= self.groups.len() {
            return;
        }
        if self.active_palette.is_empty() {
            return;
        }
        let palette = self.active_palette.clone();
        let current = if self.groups[gi].color.is_empty() {
            palette[gi % palette.len()].clone()
        } else {
            self.groups[gi].color.clone()
        };
        let idx = palette.iter().position(|c| c == &current).unwrap_or(0);
        self.groups[gi].color = palette[(idx + 1) % palette.len()].clone();
        self.dirty = true;
    }

    /// Begin editing the selected group's name (seeded with its current name).
    pub fn group_start_rename(&mut self) {
        if let Some(g) = self.groups.get(self.group_cursor) {
            self.group_edit = Some(g.name.clone());
        }
    }

    /// Push a character onto the in-flight rename buffer.
    pub fn group_edit_push(&mut self, c: char) {
        if let Some(buf) = self.group_edit.as_mut() { buf.push(c); }
    }

    /// Remove the last character from the in-flight rename buffer.
    pub fn group_edit_backspace(&mut self) {
        if let Some(buf) = self.group_edit.as_mut() { buf.pop(); }
    }

    /// Delete the trailing word from the in-flight rename buffer (Ctrl-W convention).
    pub fn group_edit_delete_word(&mut self) {
        if let Some(buf) = self.group_edit.as_mut() {
            let trimmed = buf.trim_end_matches(char::is_whitespace);
            let cut = trimmed.trim_end_matches(|c: char| !c.is_whitespace());
            buf.truncate(cut.len());
        }
    }

    /// Clear the entire in-flight rename buffer (Ctrl-U convention).
    pub fn group_edit_clear(&mut self) {
        if let Some(buf) = self.group_edit.as_mut() { buf.clear(); }
    }

    /// Commit the in-flight name. An empty result discards a still-unnamed new group
    /// and is a no-op for an already-named group.
    pub fn group_commit_rename(&mut self) {
        let buf = match self.group_edit.take() { Some(b) => b, None => return };
        let name = buf.trim().to_string();
        let gc = self.group_cursor;
        if name.is_empty() {
            if self.groups.get(gc).map(|g| g.name.is_empty()).unwrap_or(false) {
                self.groups.remove(gc);
                self.group_cursor = self.group_cursor.min(self.groups.len().saturating_sub(1));
            }
            return;
        }
        if let Some(g) = self.groups.get_mut(gc) {
            g.name = name;
            self.dirty = true;
        }
    }

    /// Cancel the in-flight edit, discarding a never-named new group.
    pub fn group_cancel_rename(&mut self) {
        self.group_edit = None;
        let gc = self.group_cursor;
        if self.groups.get(gc).map(|g| g.name.is_empty()).unwrap_or(false) {
            self.groups.remove(gc);
            self.group_cursor = self.group_cursor.min(self.groups.len().saturating_sub(1));
        }
    }

    /// Delete the selected group; its members fall back into the inbox group.
    pub fn group_delete(&mut self) {
        if self.group_cursor >= self.groups.len() { return; }
        if self.groups[self.group_cursor].inbox { return; } // undeletable
        self.groups.remove(self.group_cursor);
        self.group_cursor = self.group_cursor.min(self.groups.len().saturating_sub(1));
        self.dirty = true;
    }
}

/// Deterministic pick for the Random new-group-color policy: `seed modulo
/// palette.len()`. Empty palette yields an empty string (the caller treats
/// that the same as an unset/positional color). Pure and directly testable
/// with fixed seed literals; the one production call site (`group_new`)
/// sources `seed` from `random_seed` below.
pub fn pick_random_color(palette: &[String], seed: u64) -> String {
    if palette.is_empty() {
        return String::new();
    }
    palette[(seed as usize) % palette.len()].clone()
}

fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::pick_random_color;
    use crate::model::*;
    use crate::model::test_support::*;
    use crate::store::Config;

    #[test]
    fn clear_group_reorder_warning_drops_the_flag() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_move_cursor(2); // INBOX
        st.group_reorder(-1);
        assert!(st.group_reorder_blocked_warning().is_some());
        st.clear_group_reorder_warning();
        assert!(st.group_reorder_blocked_warning().is_none());
    }

    #[test]
    fn enter_and_exit_groups_toggles_mode() {
        let mut st = grouped_state();
        assert_eq!(st.mode, Mode::Command);
        st.enter_groups();
        assert_eq!(st.mode, Mode::Groups);
        st.exit_groups();
        assert_eq!(st.mode, Mode::Command);
    }

    #[test]
    fn group_cycle_color_is_a_guarded_noop_on_an_empty_active_palette() {
        let mut st = grouped_state();
        st.active_palette = vec![];
        st.enter_groups();
        st.group_cycle_color();
        assert!(st.groups[0].color.is_empty(), "never panics or divides by zero");
        assert!(!st.dirty);
    }

    #[test]
    fn group_cycle_color_uses_the_customized_active_palette() {
        let mut st = grouped_state();
        st.active_palette = vec!["white".to_string(), "black".to_string()];
        st.enter_groups(); // cursor on group 0
        st.group_cycle_color();
        // group 0's positional default is active_palette[0 % 2] = "white"; flip advances to "black".
        assert_eq!(st.groups[0].color, "black");
    }

    #[test]
    fn group_delete_is_a_noop_on_the_inbox_row() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config {
            groups: vec![
                Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() },
                Group { name: "INBOX".into(), inbox: true, ..Default::default() },
            ],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.enter_groups();
        st.group_move_cursor(1); // land on INBOX
        assert!(st.groups[st.group_cursor()].inbox);
        st.group_delete();
        assert_eq!(st.groups.len(), 2, "inbox group must survive delete");
        assert!(!st.dirty);
    }

    #[test]
    fn group_delete_spills_members_to_inbox() {
        let mut st = grouped_state();
        st.enter_groups(); // cursor on G1 (member a)
        st.group_delete();
        assert_eq!(st.groups.len(), 2); // G2 + the synthesized inbox
        assert_eq!(st.groups[0].name, "G2");
        assert_eq!(st.group_index_of("a"), st.inbox_index()); // a fell into the inbox
        assert!(st.dirty);
    }

    #[test]
    fn group_edit_buffer_backspace_and_delete_word() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_start_rename();
        // seed with the group's current name so there is content to edit
        assert!(st.group_edit_buffer().is_some());
        for c in " extra word".chars() { st.group_edit_push(c); }
        // buffer is "G1 extra word"
        st.group_edit_delete_word(); // drops "word"
        assert_eq!(st.group_edit_buffer(), Some("G1 extra "));
        st.group_edit_backspace(); // drops trailing space
        assert_eq!(st.group_edit_buffer(), Some("G1 extra"));
    }

    #[test]
    fn group_move_cursor_wraps_between_first_and_last_group() {
        let mut st = grouped_state();
        st.enter_groups();
        assert_eq!(st.group_cursor(), 0);
        st.group_move_cursor(-1);
        assert_eq!(st.group_cursor(), st.groups.len() - 1);
        st.group_move_cursor(1);
        assert_eq!(st.group_cursor(), 0);
    }

    #[test]
    fn group_new_defaults_to_bottom() {
        let mut st = grouped_state();
        assert_eq!(st.new_group_position, NewGroupPosition::Bottom);
        st.enter_groups();
        st.group_new();
        assert_eq!(st.groups[2].name, "", "default position lands above the inbox");
        assert!(st.groups[3].inbox);
    }

    #[test]
    fn group_new_leaves_color_positional_and_cycle_pins_explicit() {
        let mut st = grouped_state();
        st.new_group_position = NewGroupPosition::Bottom; // lands at index 2, just above the inbox
        st.enter_groups();
        st.group_new(); // empty color -> positional default (HEADER_COLORS[index])
        assert!(st.groups[2].color.is_empty(), "new group defaults to positional color");

        st.dirty = false;
        // Cursor is on the new group (index 2); its positional color is
        // HEADER_COLORS[2] ("yellow"), so a flip advances to "magenta".
        st.group_cycle_color();
        assert_eq!(st.groups[2].color, "magenta");
        assert!(st.dirty, "flipping a color dirties state");

        // Cycling wraps around the palette back to the start.
        st.groups[2].color = HEADER_COLORS[HEADER_COLORS.len() - 1].to_string();
        st.group_cycle_color();
        assert_eq!(st.groups[2].color, HEADER_COLORS[0]);
    }

    #[test]
    fn group_new_position_bottom_lands_immediately_above_the_inbox() {
        let mut st = grouped_state();
        st.new_group_position = NewGroupPosition::Bottom;
        st.enter_groups();
        st.group_new();
        assert_eq!(st.groups.len(), 4);
        assert_eq!(st.groups[2].name, "", "new group lands just above the inbox, not at the absolute end");
        assert_eq!(st.group_cursor(), 2);
        assert!(st.groups[3].inbox, "inbox stays in the trailing slot");
    }

    #[test]
    fn group_new_position_top_inserts_at_index_zero_and_starts_rename() {
        let mut st = grouped_state();
        st.new_group_position = NewGroupPosition::Top;
        st.enter_groups();
        st.group_new();
        assert_eq!(st.groups.len(), 4);
        assert_eq!(st.groups[0].name, "");
        assert!(st.groups[0].members.is_empty());
        assert_eq!(st.group_cursor(), 0);
        assert!(st.group_editing());
        for c in "TOOLS".chars() { st.group_edit_push(c); }
        st.group_commit_rename();
        assert_eq!(st.groups[0].name, "TOOLS");
        assert!(!st.group_editing());
        assert!(st.dirty);
        assert_eq!(st.groups[1].name, "G1", "existing groups keep their relative order");
        assert_eq!(st.groups[2].name, "G2");
    }

    #[test]
    fn group_new_then_cancel_discards() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_new();
        st.group_cancel_rename();
        assert_eq!(st.groups.len(), 3);
        assert!(!st.group_editing());
    }

    #[test]
    fn group_new_under_random_policy_picks_from_the_active_palette() {
        let mut st = grouped_state();
        st.new_group_color_policy = ColorPolicy::Random;
        st.new_group_position = NewGroupPosition::Top;
        st.enter_groups();
        st.group_new();
        let picked = st.groups.first().unwrap().color.clone();
        assert!(
            st.active_palette.contains(&picked),
            "random pick must come from the active palette"
        );
    }

    #[test]
    fn group_new_under_rotate_policy_leaves_color_empty() {
        let mut st = grouped_state(); // default policy is Rotate
        st.new_group_position = NewGroupPosition::Top;
        st.enter_groups();
        st.group_new();
        assert!(st.groups.first().unwrap().color.is_empty(), "unchanged Rotate behavior");
    }

    #[test]
    fn group_new_under_static_policy_uses_the_configured_static_color() {
        let mut st = grouped_state();
        st.new_group_color_policy = ColorPolicy::Static;
        st.static_color = "magenta".to_string();
        st.new_group_position = NewGroupPosition::Top;
        st.enter_groups();
        st.group_new();
        assert_eq!(st.groups.first().unwrap().color, "magenta");
    }

    #[test]
    fn group_rename_existing_commits_and_cancel_reverts() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_move_cursor(1); // cursor on G2
        st.group_start_rename();
        st.group_edit_clear();
        for c in "MISC".chars() { st.group_edit_push(c); }
        st.group_commit_rename();
        assert_eq!(st.groups[1].name, "MISC");

        st.group_start_rename();
        st.group_edit_clear();
        st.group_cancel_rename();
        assert_eq!(st.groups[1].name, "MISC"); // unchanged on cancel
    }

    #[test]
    fn group_reorder_blocked_warning_clears_on_a_plain_edge_clamp() {
        let mut st = grouped_state();
        st.enter_groups(); // cursor on G1, index 0
        st.group_move_cursor(1); // G2
        st.group_reorder(1); // blocked: inbox
        assert!(st.group_reorder_blocked_warning().is_some());
        st.group_move_cursor(-1); // back to G1
        st.group_reorder(-1); // plain edge clamp (already first), unrelated to the inbox
        assert!(
            st.group_reorder_blocked_warning().is_none(),
            "an ordinary out-of-bounds clamp is not an inbox block"
        );
    }

    #[test]
    fn group_reorder_blocked_warning_clears_on_a_successful_reorder() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_move_cursor(1); // G2
        st.group_reorder(1); // blocked: would swap into the inbox's slot
        assert!(st.group_reorder_blocked_warning().is_some());
        st.group_reorder(-1); // a real swap this time (G2 <-> G1)
        assert!(st.group_reorder_blocked_warning().is_none(), "a successful reorder clears the warning");
    }

    #[test]
    fn group_reorder_is_a_noop_when_the_inbox_is_the_selected_group() {
        let mut st = grouped_state(); // groups: G1, G2, INBOX (synthesized last)
        st.enter_groups();
        st.group_move_cursor(2); // land on INBOX
        assert!(st.groups[st.group_cursor()].inbox);
        assert!(st.group_reorder_blocked_warning().is_none());
        st.group_reorder(-1); // try to move it up past G2
        assert_eq!(
            st.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
            vec!["G1", "G2", "INBOX"],
            "inbox never moves, even when explicitly targeted"
        );
        assert!(!st.dirty);
        assert_eq!(st.group_reorder_blocked_warning(), Some("Inbox can't be reordered"));
    }

    #[test]
    fn group_reorder_is_a_noop_when_the_target_slot_is_the_inbox() {
        let mut st = grouped_state(); // groups: G1, G2, INBOX (synthesized last)
        st.enter_groups();
        st.group_move_cursor(1); // land on G2
        st.group_reorder(1); // try to swap down into the inbox's slot
        assert_eq!(
            st.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
            vec!["G1", "G2", "INBOX"],
            "a named group can never swap into the inbox's trailing slot"
        );
        assert!(!st.dirty);
        assert_eq!(st.group_reorder_blocked_warning(), Some("Inbox can't be reordered"));
    }

    #[test]
    fn group_reorder_pins_positional_colors_so_a_moved_group_keeps_its_look() {
        // Issue #118: a freshly created group under the default Rotate policy
        // has an unset color, resolved positionally from its index at render
        // time. Without pinning, every ⇧J/⇧K swap recomputes a new color
        // purely from the new index -- the group's visible color cycles on
        // every press even though the user never touched color settings.
        let mut st = grouped_state();
        st.enter_groups();
        assert!(st.groups[0].color.is_empty(), "G1 starts on the positional default");
        assert!(st.groups[1].color.is_empty(), "G2 starts on the positional default");

        st.group_reorder(1); // move G1 (idx 0, "cyan") down past G2 (idx 1, "green")
        assert_eq!(st.groups[0].name, "G2");
        assert_eq!(st.groups[0].color, HEADER_COLORS[1], "G2 keeps the color it had at index 1");
        assert_eq!(st.groups[1].name, "G1");
        assert_eq!(st.groups[1].color, HEADER_COLORS[0], "G1 keeps the color it had at index 0");

        // Moving back and forth repeatedly must not keep cycling the color.
        st.group_reorder(-1);
        assert_eq!(st.groups[0].name, "G1");
        assert_eq!(st.groups[0].color, HEADER_COLORS[0]);
        assert_eq!(st.groups[1].name, "G2");
        assert_eq!(st.groups[1].color, HEADER_COLORS[1]);
    }

    #[test]
    fn group_reorder_swaps_named_groups() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_reorder(1); // move G1 down
        assert_eq!(st.groups[0].name, "G2");
        assert_eq!(st.groups[1].name, "G1");
        assert!(st.dirty);
    }

    #[test]
    fn group_reorder_swap_marks_only_the_moved_group() {
        let mut st = grouped_state(); // G1, G2, INBOX (synthesized last)
        st.enter_groups();
        st.group_reorder(1); // G1 (cursor) moves down past G2
        assert_eq!(st.group_swap_marker("G1"), Some((SwapDirection::Down, true)));
        assert_eq!(st.group_swap_marker("G2"), None, "neighbor gets no marker");
    }

    #[test]
    fn group_reorder_blocked_by_inbox_sets_no_swap_marker() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_move_cursor(1); // G2
        st.group_reorder(1); // blocked: target is the inbox
        assert_eq!(st.group_swap_marker("G2"), None);
    }

    #[test]
    fn pick_random_color_empty_palette_returns_empty_string() {
        assert_eq!(pick_random_color(&[], 42), "");
    }

    #[test]
    fn pick_random_color_selects_by_seed_modulo_palette_len() {
        let palette = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(pick_random_color(&palette, 0), "a");
        assert_eq!(pick_random_color(&palette, 1), "b");
        assert_eq!(pick_random_color(&palette, 2), "c");
        assert_eq!(pick_random_color(&palette, 3), "a", "wraps");
    }
}
