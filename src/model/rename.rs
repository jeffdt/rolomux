//! In-place session/window rename editing. Holds the in-flight buffer edits
//! (`self.rename_edit`) and turns a committed buffer into a `PendingRename`
//! describing what `main.rs` should tell tmux to rename.

use super::{PickerState, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameTarget {
    Session(String),
    Window(String, u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRename {
    pub target: RenameTarget,
    pub new_name: String,
}

impl PickerState {
    /// Whether an inline session/window rename is currently in progress.
    pub fn renaming(&self) -> bool {
        self.rename_edit.is_some()
    }

    /// The in-flight rename buffer, if a rename is in progress.
    pub fn rename_edit_buffer(&self) -> Option<&str> {
        self.rename_edit.as_deref()
    }

    /// Begin renaming whatever row the cursor is on, seeding the buffer
    /// with its current name. A no-op if the cursor addresses no row.
    pub fn start_rename(&mut self) {
        let rows = self.visible_rows();
        let ordered = self.ordered();
        if let Some(row) = rows.get(self.cursor) {
            let current_name = match row {
                Row::Session(si) => ordered[*si].name.clone(),
                Row::Window(si, wi) => ordered[*si].windows[*wi].name.clone(),
            };
            self.rename_edit = Some(current_name);
        }
    }

    /// Push a character onto the in-flight rename buffer.
    pub fn rename_edit_push(&mut self, c: char) {
        if let Some(buf) = self.rename_edit.as_mut() { buf.push(c); }
    }

    /// Remove the last character from the in-flight rename buffer.
    pub fn rename_edit_backspace(&mut self) {
        if let Some(buf) = self.rename_edit.as_mut() { buf.pop(); }
    }

    /// Delete the trailing word from the in-flight rename buffer (Ctrl-W convention).
    pub fn rename_edit_delete_word(&mut self) {
        if let Some(buf) = self.rename_edit.as_mut() {
            let trimmed = buf.trim_end_matches(char::is_whitespace);
            let cut = trimmed.trim_end_matches(|c: char| !c.is_whitespace());
            buf.truncate(cut.len());
        }
    }

    /// Clear the entire in-flight rename buffer (Ctrl-U convention).
    pub fn rename_edit_clear(&mut self) {
        if let Some(buf) = self.rename_edit.as_mut() { buf.clear(); }
    }

    /// Cancel the in-flight rename, discarding the buffer.
    pub fn cancel_rename(&mut self) {
        self.rename_edit = None;
    }

    /// Consume the in-flight rename buffer and, if it names a real change
    /// (non-empty after trimming, and different from the current name),
    /// return what to rename and to what. Returns `None` (having still
    /// cleared the buffer) for an empty or unchanged commit -- both are
    /// treated as a no-op, mirroring group rename's empty-name guard.
    pub fn take_rename_commit(&mut self) -> Option<PendingRename> {
        let buf = self.rename_edit.take()?;
        let new_name = buf.trim().to_string();
        if new_name.is_empty() {
            return None;
        }
        let rows = self.visible_rows();
        let ordered = self.ordered();
        let row = *rows.get(self.cursor)?;
        let target = match row {
            Row::Session(si) => {
                let current = &ordered[si].name;
                if *current == new_name {
                    return None;
                }
                RenameTarget::Session(current.clone())
            }
            Row::Window(si, wi) => {
                let win = &ordered[si].windows[wi];
                if win.name == new_name {
                    return None;
                }
                RenameTarget::Window(ordered[si].name.clone(), win.index)
            }
        };
        Some(PendingRename { target, new_name })
    }
}

#[cfg(test)]
mod tests {
    use crate::model::*;
    use crate::model::test_support::*;
    use crate::store::Config;

    #[test]
    fn start_rename_seeds_buffer_with_session_name_on_session_row() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.start_rename();
        assert!(st.renaming());
        assert_eq!(st.rename_edit_buffer(), Some("alpha"));
    }

    #[test]
    fn start_rename_seeds_buffer_with_window_name_on_window_row() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.expand();
        st.move_cursor(1); // onto the window row
        st.start_rename();
        assert_eq!(st.rename_edit_buffer(), Some("w"));
    }

    #[test]
    fn rename_edit_push_backspace_and_clear() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.start_rename();
        st.rename_edit_clear();
        for c in "beta".chars() { st.rename_edit_push(c); }
        assert_eq!(st.rename_edit_buffer(), Some("beta"));
        st.rename_edit_backspace();
        assert_eq!(st.rename_edit_buffer(), Some("bet"));
    }

    #[test]
    fn rename_edit_delete_word_removes_trailing_word() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.start_rename();
        st.rename_edit_clear();
        for c in "foo bar".chars() { st.rename_edit_push(c); }
        st.rename_edit_delete_word();
        assert_eq!(st.rename_edit_buffer(), Some("foo "));
    }

    #[test]
    fn cancel_rename_discards_buffer_without_changing_anything() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.start_rename();
        st.rename_edit_push('x');
        st.cancel_rename();
        assert!(!st.renaming());
        assert_eq!(st.rename_edit_buffer(), None);
    }

    #[test]
    fn take_rename_commit_returns_session_target_on_changed_name() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.start_rename();
        st.rename_edit_clear();
        for c in "beta".chars() { st.rename_edit_push(c); }
        let pending = st.take_rename_commit().expect("changed name commits");
        assert_eq!(pending.target, RenameTarget::Session("alpha".to_string()));
        assert_eq!(pending.new_name, "beta");
        assert!(!st.renaming(), "buffer cleared after commit");
    }

    #[test]
    fn take_rename_commit_returns_window_target_on_changed_name() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.expand();
        st.move_cursor(1);
        st.start_rename();
        st.rename_edit_clear();
        for c in "logs".chars() { st.rename_edit_push(c); }
        let pending = st.take_rename_commit().expect("changed name commits");
        assert_eq!(pending.target, RenameTarget::Window("alpha".to_string(), 0));
        assert_eq!(pending.new_name, "logs");
    }

    #[test]
    fn take_rename_commit_none_on_empty_or_unchanged_name() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config::default();

        let mut st = PickerState::build(sessions.clone(), &cfg);
        st.start_rename();
        st.rename_edit_clear();
        assert!(st.take_rename_commit().is_none(), "empty commit is a no-op");
        assert!(!st.renaming(), "buffer still cleared");

        let mut st = PickerState::build(sessions, &cfg);
        st.start_rename(); // buffer seeded with "alpha", unchanged
        assert!(st.take_rename_commit().is_none(), "unchanged name is a no-op");
    }
}
