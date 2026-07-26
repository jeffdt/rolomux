//! `⇧N` quick-create: naming a brand-new group around the currently-selected
//! session, at session altitude. Holds the in-flight buffer edit
//! (`self.quick_create_edit`) and, on commit, creates the group and moves
//! the session into it as sole member. Mirrors `rename.rs`'s buffer-editing
//! shape; unlike a rename, there is no existing name to seed the buffer
//! with, so `start_quick_create` always begins it empty.

use super::PickerState;

impl PickerState {
    /// Whether a `⇧N` quick-create is currently in progress.
    pub fn quick_creating(&self) -> bool {
        self.quick_create_edit.is_some()
    }

    /// The in-flight quick-create buffer, if one is in progress.
    pub fn quick_create_buffer(&self) -> Option<&str> {
        self.quick_create_edit.as_deref()
    }

    /// Begin naming a new group around whatever row the cursor is on
    /// (a window row acts on its parent session). A no-op if the cursor
    /// addresses no row.
    pub fn start_quick_create(&mut self) {
        if self.cursor_session_name().is_some() {
            self.quick_create_edit = Some(String::new());
        }
    }

    /// Push a character onto the in-flight quick-create buffer.
    pub fn quick_create_push(&mut self, c: char) {
        if let Some(buf) = self.quick_create_edit.as_mut() { buf.push(c); }
    }

    /// Remove the last character from the in-flight quick-create buffer.
    pub fn quick_create_backspace(&mut self) {
        if let Some(buf) = self.quick_create_edit.as_mut() { buf.pop(); }
    }

    /// Delete the trailing word from the in-flight quick-create buffer (Ctrl-W convention).
    pub fn quick_create_delete_word(&mut self) {
        if let Some(buf) = self.quick_create_edit.as_mut() {
            let trimmed = buf.trim_end_matches(char::is_whitespace);
            let cut = trimmed.trim_end_matches(|c: char| !c.is_whitespace());
            buf.truncate(cut.len());
        }
    }

    /// Clear the entire in-flight quick-create buffer (Ctrl-U convention).
    pub fn quick_create_clear(&mut self) {
        if let Some(buf) = self.quick_create_edit.as_mut() { buf.clear(); }
    }

    /// Cancel the in-flight quick-create, discarding the buffer.
    pub fn cancel_quick_create(&mut self) {
        self.quick_create_edit = None;
    }

    /// Consume the in-flight quick-create buffer and, if it names a real
    /// group (non-empty after trimming), create a group named from it
    /// immediately above the selected session's current group, colored per
    /// the current `new_group_color_policy` (same policy `group_new`
    /// applies, via the shared `new_group_color` helper), move the session
    /// into it as sole member (removing it from its old group's explicit
    /// `members`, if it was listed there -- inbox fallback membership
    /// requires no explicit removal), and follow the cursor to it. Returns
    /// `true` iff a group was created. A trimmed-empty buffer is a cancel:
    /// returns `false`, nothing created, matching `group_commit_rename`'s
    /// empty-name guard (and, like it, no duplicate-name guard either --
    /// duplicates are allowed).
    pub fn commit_quick_create(&mut self) -> bool {
        let buf = match self.quick_create_edit.take() { Some(b) => b, None => return false };
        let name = buf.trim().to_string();
        if name.is_empty() {
            return false;
        }
        let Some(session_name) = self.cursor_session_name() else { return false };
        let Some(old_gi) = self.group_index_of(&session_name) else { return false };
        let color = self.new_group_color();
        let index = self.insert_group_above(old_gi);
        let new_old_gi = if index <= old_gi { old_gi + 1 } else { old_gi };
        self.groups[new_old_gi].members.retain(|m| m != &session_name);
        self.groups[index].name = name;
        self.groups[index].members = vec![session_name.clone()];
        self.groups[index].color = color;
        super::ensure_inbox_last(&mut self.groups);
        self.focus_session(&session_name);
        self.dirty = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::model::*;
    use crate::model::test_support::*;
    use crate::store::Config;

    #[test]
    fn quick_create_push_backspace_and_clear() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.start_quick_create();
        assert!(st.quick_creating());
        assert_eq!(st.quick_create_buffer(), Some(""));
        for c in "beta".chars() { st.quick_create_push(c); }
        assert_eq!(st.quick_create_buffer(), Some("beta"));
        st.quick_create_backspace();
        assert_eq!(st.quick_create_buffer(), Some("bet"));
        st.quick_create_clear();
        assert_eq!(st.quick_create_buffer(), Some(""));
    }

    #[test]
    fn quick_create_delete_word_removes_trailing_word() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.start_quick_create();
        for c in "foo bar".chars() { st.quick_create_push(c); }
        st.quick_create_delete_word();
        assert_eq!(st.quick_create_buffer(), Some("foo "));
    }

    #[test]
    fn cancel_quick_create_discards_buffer_without_changing_anything() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.start_quick_create();
        st.quick_create_push('x');
        st.cancel_quick_create();
        assert!(!st.quick_creating());
        assert_eq!(st.quick_create_buffer(), None);
    }

    #[test]
    fn start_quick_create_is_a_noop_with_no_row_under_the_cursor() {
        let cfg = Config::default();
        let mut st = PickerState::build(Vec::new(), &cfg);
        st.start_quick_create();
        assert!(!st.quick_creating(), "no row under the cursor, nothing to name a group around");
    }

    #[test]
    fn commit_moves_session_into_new_group_above_its_old_group() {
        // grouped_state: G1=[a], G2=[b], INBOX=[c] (synthesized last)
        let mut st = grouped_state();
        st.focus_session("b"); // explicit member of G2
        st.start_quick_create();
        for c in "NEW".chars() { st.quick_create_push(c); }
        assert!(st.commit_quick_create());

        assert_eq!(
            st.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
            vec!["G1", "NEW", "G2", "INBOX"],
            "NEW lands directly above G2, b's old group"
        );
        assert_eq!(st.groups[1].members, vec!["b".to_string()]);
        assert_eq!(st.groups[2].members, Vec::<String>::new(), "b removed from its old group G2");
        assert!(st.dirty);
        assert_eq!(st.cursor_session_name().as_deref(), Some("b"), "cursor follows the session");
    }

    #[test]
    fn commit_leaves_sibling_members_of_the_old_group_untouched() {
        // state_with_two_groups: G1=[a, b], G2=[c]; d, e fall back to the inbox.
        let mut st = state_with_two_groups();
        st.focus_session("a"); // explicit member of G1, alongside sibling "b"
        st.start_quick_create();
        for c in "NEW".chars() { st.quick_create_push(c); }
        assert!(st.commit_quick_create());

        assert_eq!(
            st.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
            vec!["NEW", "G1", "G2", "INBOX"],
            "NEW lands directly above G1, a's old group"
        );
        assert_eq!(st.groups[0].members, vec!["a".to_string()], "a moved into the new group alone");
        assert_eq!(
            st.groups[1].members,
            vec!["b".to_string()],
            "sibling member b stays behind in G1, neither dropped nor duplicated"
        );
    }

    #[test]
    fn commit_from_window_row_acts_on_parent_session() {
        let mut st = grouped_state(); // G1=[a], G2=[b], INBOX=[c]
        st.focus_session("b");
        st.expand();
        st.move_cursor(1); // onto b's window row
        assert!(matches!(st.visible_rows()[st.cursor], Row::Window(_, _)), "precondition: cursor is on a window row");

        st.start_quick_create();
        for c in "NEW".chars() { st.quick_create_push(c); }
        assert!(st.commit_quick_create());

        assert_eq!(st.groups[1].name, "NEW", "acted on b, the window's parent session");
        assert_eq!(st.groups[1].members, vec!["b".to_string()]);
        assert_eq!(st.groups[2].members, Vec::<String>::new(), "b removed from its old group G2");
    }

    #[test]
    fn empty_commit_is_a_cancel() {
        let mut st = grouped_state();
        st.focus_session("a");
        let groups_before = st.groups.clone();

        st.start_quick_create();
        assert!(!st.commit_quick_create(), "empty buffer cancels");
        assert!(!st.quick_creating());
        assert_eq!(st.groups, groups_before, "nothing created");
        assert!(!st.dirty);

        st.start_quick_create();
        st.quick_create_push(' '); // whitespace-only also trims to empty
        assert!(!st.commit_quick_create(), "whitespace-only buffer also cancels");
        assert_eq!(st.groups, groups_before);
        assert!(!st.dirty);
    }

    #[test]
    fn commit_applies_static_color_policy_like_group_new() {
        use crate::model::ColorPolicy;
        let mut st = grouped_state(); // G1=[a], G2=[b], INBOX=[c]
        st.new_group_color_policy = ColorPolicy::Static;
        st.static_color = "Magenta".to_string();
        st.focus_session("b");
        st.start_quick_create();
        for c in "NEW".chars() { st.quick_create_push(c); }
        assert!(st.commit_quick_create());

        let new_group = st.groups.iter().find(|g| g.name == "NEW").unwrap();
        assert_eq!(
            new_group.color, "Magenta",
            "quick-create should honor the Static new-group-color policy, same as group_new"
        );
    }

    #[test]
    fn commit_from_inbox_session_lands_group_just_above_inbox() {
        let mut st = grouped_state(); // G1=[a], G2=[b], INBOX=[c] (c is fallback, not explicit)
        st.focus_session("c");
        let inbox_idx_before = st.inbox_index().unwrap();
        assert_eq!(st.group_index_of("c"), Some(inbox_idx_before), "precondition: c falls back to the inbox");

        st.start_quick_create();
        for c in "NEW".chars() { st.quick_create_push(c); }
        assert!(st.commit_quick_create());

        assert_eq!(st.groups[inbox_idx_before].name, "NEW", "new group sits where the inbox used to be");
        assert!(st.groups[inbox_idx_before + 1].inbox, "inbox shifted one slot down, still last");
        assert_eq!(st.groups[inbox_idx_before].members, vec!["c".to_string()]);
    }
}
