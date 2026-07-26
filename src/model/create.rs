//! Two-stage session-create prompt: session name, then optional window
//! name. Holds the in-flight buffers (`self.create_prompt`) and, on final
//! commit, turns them into a `PendingCreate` describing what `main.rs`
//! should ask tmux to create and where the session should land in the
//! group model.

use super::PickerState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateStage {
    SessionName,
    WindowName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatePlacement {
    /// `n` at session altitude: joins `group`, inserted immediately above
    /// the selected session's position in that group's effective order.
    AboveSelected { group: usize, member_index: usize },
    /// `⇧N` at group altitude: appended to the end of `group`'s block.
    EndOfGroup { group: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCreate {
    pub session_name: String,
    pub window_name: Option<String>,
    pub placement: CreatePlacement,
}

/// In-flight two-stage create buffer. `session_buffer` survives stepping
/// back out of the window-name stage (`create_back`), matching the spec's
/// "Esc steps back... buffer preserved".
pub(super) struct CreatePrompt {
    stage: CreateStage,
    session_buffer: String,
    window_buffer: String,
    placement: CreatePlacement,
}

impl PickerState {
    /// Whether an in-flight create prompt (either stage) is open.
    pub fn creating(&self) -> bool {
        self.create_prompt.is_some()
    }

    /// The current stage of the in-flight create prompt, if any.
    pub fn create_stage(&self) -> Option<CreateStage> {
        self.create_prompt.as_ref().map(|p| p.stage.clone())
    }

    /// The in-flight buffer for whichever stage is currently active.
    // Consumed by Task 4's phantom-row rendering, not yet wired.
    #[allow(dead_code)]
    pub fn create_buffer(&self) -> Option<&str> {
        self.create_prompt.as_ref().map(|p| match p.stage {
            CreateStage::SessionName => p.session_buffer.as_str(),
            CreateStage::WindowName => p.window_buffer.as_str(),
        })
    }

    /// `n` at session altitude: begin naming a session placed immediately
    /// above the row under the cursor (a window row acts on its parent
    /// session), joining that session's group. With no row at all (an empty
    /// picker), falls back to the end of the inbox.
    pub fn start_create_here(&mut self) {
        let placement = match self.cursor_session_name() {
            Some(name) => {
                let group = self.group_index_of(&name).unwrap_or(0);
                let order = self.effective_order(group);
                let member_index = order.iter().position(|n| n == &name).unwrap_or(order.len());
                CreatePlacement::AboveSelected { group, member_index }
            }
            None => CreatePlacement::EndOfGroup { group: self.inbox_index().unwrap_or(0) },
        };
        self.create_prompt = Some(CreatePrompt {
            stage: CreateStage::SessionName,
            session_buffer: String::new(),
            window_buffer: String::new(),
            placement,
        });
    }

    /// `⇧N` at group altitude: begin naming a session appended to the end
    /// of the highlighted group's block (identically the only slot for an
    /// empty group). A no-op if the group cursor is somehow out of range.
    pub fn start_create_in_group(&mut self) {
        if self.group_cursor >= self.groups.len() {
            return;
        }
        self.create_prompt = Some(CreatePrompt {
            stage: CreateStage::SessionName,
            session_buffer: String::new(),
            window_buffer: String::new(),
            placement: CreatePlacement::EndOfGroup { group: self.group_cursor },
        });
    }

    /// Push a character onto whichever stage's buffer is currently active.
    pub fn create_push(&mut self, c: char) {
        if let Some(p) = self.create_prompt.as_mut() {
            match p.stage {
                CreateStage::SessionName => p.session_buffer.push(c),
                CreateStage::WindowName => p.window_buffer.push(c),
            }
        }
    }

    /// Remove the last character from the active stage's buffer.
    pub fn create_backspace(&mut self) {
        if let Some(p) = self.create_prompt.as_mut() {
            match p.stage {
                CreateStage::SessionName => { p.session_buffer.pop(); }
                CreateStage::WindowName => { p.window_buffer.pop(); }
            }
        }
    }

    /// Delete the trailing word from the active stage's buffer (Ctrl-W convention).
    pub fn create_delete_word(&mut self) {
        if let Some(p) = self.create_prompt.as_mut() {
            let buf = match p.stage {
                CreateStage::SessionName => &mut p.session_buffer,
                CreateStage::WindowName => &mut p.window_buffer,
            };
            let trimmed = buf.trim_end_matches(char::is_whitespace);
            let cut = trimmed.trim_end_matches(|c: char| !c.is_whitespace());
            buf.truncate(cut.len());
        }
    }

    /// Clear the active stage's buffer entirely (Ctrl-U convention).
    pub fn create_clear(&mut self) {
        if let Some(p) = self.create_prompt.as_mut() {
            match p.stage {
                CreateStage::SessionName => p.session_buffer.clear(),
                CreateStage::WindowName => p.window_buffer.clear(),
            }
        }
    }

    /// `Esc` in the session-name stage: abort the whole prompt, discarding
    /// both buffers.
    pub fn create_cancel(&mut self) {
        self.create_prompt = None;
    }

    /// `Esc` in the window-name stage: step back to the session-name stage,
    /// discarding only the partial window-name buffer. A no-op in the
    /// session-name stage.
    pub fn create_back(&mut self) {
        if let Some(p) = self.create_prompt.as_mut() {
            if p.stage == CreateStage::WindowName {
                p.stage = CreateStage::SessionName;
                p.window_buffer.clear();
            }
        }
    }

    /// Commit whatever's in the active stage's buffer. In the session-name
    /// stage: an empty (or whitespace-only) name aborts the whole prompt
    /// (returns `None`, mirroring `take_rename_commit`'s empty-name guard);
    /// otherwise advances to the window-name stage (still returns `None`,
    /// prompt stays open). In the window-name stage: always returns
    /// `Some(PendingCreate)` and closes the prompt; an empty buffer yields
    /// `window_name: None` (tmux's own default naming takes over).
    pub fn create_commit(&mut self) -> Option<PendingCreate> {
        let stage = self.create_prompt.as_ref()?.stage.clone();
        match stage {
            CreateStage::SessionName => {
                let p = self.create_prompt.as_mut()?;
                if p.session_buffer.trim().is_empty() {
                    self.create_prompt = None;
                    return None;
                }
                p.stage = CreateStage::WindowName;
                None
            }
            CreateStage::WindowName => {
                let p = self.create_prompt.take()?;
                let session_name = p.session_buffer.trim().to_string();
                let window_name = {
                    let trimmed = p.window_buffer.trim();
                    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
                };
                Some(PendingCreate { session_name, window_name, placement: p.placement })
            }
        }
    }

    /// Write `done`'s membership into the group model per its placement
    /// (the actual `tmux new-session` call happens before this, in
    /// `main.rs`) and mark state dirty for persistence. Mirrors
    /// `reorder::cross_into_group`'s pattern of freezing a group's full
    /// effective order (persisted members plus any never-touched inbox
    /// fallback) before writing back to `members`, so an insertion above an
    /// implicit inbox member lands in the right visual spot rather than
    /// ahead of untouched fallback sessions.
    pub fn apply_create(&mut self, done: &PendingCreate) {
        match done.placement {
            CreatePlacement::AboveSelected { group, member_index } => {
                let mut order = self.effective_order(group);
                let idx = member_index.min(order.len());
                order.insert(idx, done.session_name.clone());
                self.groups[group].members = order;
            }
            CreatePlacement::EndOfGroup { group } => {
                let mut order = self.effective_order(group);
                order.push(done.session_name.clone());
                self.groups[group].members = order;
            }
        }
        self.dirty = true;
        self.focus_session(&done.session_name);
    }
}

#[cfg(test)]
mod tests {
    use crate::model::*;
    use crate::model::test_support::*;
    use crate::store::Config;

    #[test]
    fn session_name_commit_advances_to_window_stage() {
        let mut st = grouped_state();
        st.focus_session("a");
        st.start_create_here();
        assert!(st.creating());
        assert_eq!(st.create_stage(), Some(CreateStage::SessionName));
        for c in "newsess".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None, "advances, doesn't finish yet");
        assert_eq!(st.create_stage(), Some(CreateStage::WindowName));
        assert_eq!(st.create_buffer(), Some(""));
        assert!(st.creating(), "prompt stays open across the stage advance");
    }

    #[test]
    fn empty_session_name_aborts() {
        let mut st = grouped_state();
        st.focus_session("a");
        st.start_create_here();
        assert_eq!(st.create_commit(), None);
        assert!(!st.creating(), "empty name aborts the whole prompt");

        st.start_create_here();
        st.create_push(' '); // whitespace-only also aborts
        assert_eq!(st.create_commit(), None);
        assert!(!st.creating());
    }

    #[test]
    fn esc_in_window_stage_steps_back_preserving_session_name() {
        let mut st = grouped_state();
        st.focus_session("a");
        st.start_create_here();
        for c in "newsess".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None, "advances to WindowName");
        for c in "logs".chars() { st.create_push(c); }
        st.create_back();
        assert_eq!(st.create_stage(), Some(CreateStage::SessionName));
        assert_eq!(st.create_buffer(), Some("newsess"), "session name buffer preserved");
        assert!(st.creating(), "steps back, doesn't abort");
    }

    #[test]
    fn window_stage_empty_commit_yields_no_window_name() {
        let mut st = grouped_state();
        st.focus_session("a");
        st.start_create_here();
        for c in "newsess".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None);
        let pending = st.create_commit().expect("empty window name still commits");
        assert_eq!(pending.session_name, "newsess");
        assert_eq!(pending.window_name, None);
        assert!(!st.creating());
    }

    #[test]
    fn n_places_above_selected_in_its_group() {
        let mut st = state_with_two_groups(); // G1=[a,b], G2=[c]; residual d,e
        st.focus_session("b");
        st.start_create_here();
        for c in "x".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None);
        let pending = st.create_commit().unwrap();
        assert_eq!(
            pending.placement,
            CreatePlacement::AboveSelected { group: 0, member_index: 1 },
            "lands above b, which sits at index 1 of G1's effective order"
        );
    }

    #[test]
    fn n_from_window_row_uses_parent_session() {
        let mut st = grouped_state(); // G1=[a], G2=[b], INBOX=[c]
        st.focus_session("b");
        st.expand();
        st.move_cursor(1); // onto b's window row
        assert!(matches!(st.visible_rows()[st.cursor], Row::Window(_, _)), "precondition: cursor is on a window row");

        st.start_create_here();
        for c in "x".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None);
        let pending = st.create_commit().unwrap();
        assert_eq!(
            pending.placement,
            CreatePlacement::AboveSelected { group: 1, member_index: 0 },
            "acted on b, the window's parent session, in G2"
        );
    }

    #[test]
    fn n_on_empty_picker_falls_back_to_inbox_end() {
        let cfg = Config::default();
        let mut st = PickerState::build(Vec::new(), &cfg);
        let inbox = st.inbox_index().unwrap();

        st.start_create_here();
        for c in "x".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None);
        let pending = st.create_commit().unwrap();
        assert_eq!(pending.placement, CreatePlacement::EndOfGroup { group: inbox });
    }

    #[test]
    fn shift_n_appends_to_highlighted_group() {
        let mut st = grouped_state(); // G1=[a], G2=[b], INBOX=[c]
        st.enter_groups();
        st.group_move_cursor(1); // highlight G2
        st.start_create_in_group();
        assert!(st.creating());
        for c in "x".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None);
        let pending = st.create_commit().unwrap();
        assert_eq!(pending.placement, CreatePlacement::EndOfGroup { group: 1 });
    }

    #[test]
    fn shift_n_seeds_empty_group() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config {
            groups: vec![
                Group { name: "G1".into(), members: vec!["a".into()], ..Default::default() },
                Group { name: "EMPTY".into(), members: vec![], ..Default::default() },
            ],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.enter_groups();
        st.group_move_cursor(1); // highlight EMPTY
        assert_eq!(st.groups[st.group_cursor()].name, "EMPTY");
        st.start_create_in_group();
        assert!(st.creating(), "works identically for an empty group -- the only slot in it");
        for c in "x".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None);
        let pending = st.create_commit().unwrap();
        assert_eq!(pending.placement, CreatePlacement::EndOfGroup { group: 1 });
    }

    #[test]
    fn n_places_above_unlisted_fallback_member_in_the_inbox() {
        // state_with_two_groups: G1=[a,b], G2=[c]; d (created 4) and e
        // (created 5) are never listed in any group's `members` -- they
        // fall back to the auto-appended inbox purely via
        // group_index_of's fallback, oldest-created-first.
        let mut st = state_with_two_groups();
        let inbox = st.inbox_index().unwrap();
        assert_eq!(
            st.group_index_of("e"), Some(inbox),
            "precondition: e is an unlisted inbox fallback member, not an explicit member of any group"
        );

        st.focus_session("e");
        st.start_create_here();
        for c in "newsess".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None);
        let pending = st.create_commit().unwrap();
        assert_eq!(
            pending.placement,
            CreatePlacement::AboveSelected { group: inbox, member_index: 1 },
            "e sits at index 1 of the inbox's effective order (d, e)"
        );

        st.apply_create(&pending);
        assert_eq!(
            st.groups[inbox].members,
            vec!["d".to_string(), "newsess".to_string(), "e".to_string()],
            "inserted between the previously-unlisted fallback members d and e, freezing both into members"
        );
    }

    #[test]
    fn shift_n_appends_after_existing_inbox_overflow() {
        // Same fixture: d, e fall back to the inbox, unfrozen. Highlighting
        // the inbox itself at group altitude and appending must land the
        // new session after that overflow, not before it.
        let mut st = state_with_two_groups();
        let inbox = st.inbox_index().unwrap();
        st.enter_groups();
        st.group_cursor = inbox;

        st.start_create_in_group();
        for c in "newsess".chars() { st.create_push(c); }
        assert_eq!(st.create_commit(), None);
        let pending = st.create_commit().unwrap();
        assert_eq!(pending.placement, CreatePlacement::EndOfGroup { group: inbox });

        st.apply_create(&pending);
        assert_eq!(
            st.groups[inbox].members,
            vec!["d".to_string(), "e".to_string(), "newsess".to_string()],
            "new session appended after the now-frozen fallback overflow, not before it"
        );
    }

    #[test]
    fn apply_create_inserts_member_at_placement() {
        let mut st = state_with_two_groups(); // G1=[a,b], G2=[c]; residual d,e
        let pending = PendingCreate {
            session_name: "newsess".into(),
            window_name: None,
            placement: CreatePlacement::AboveSelected { group: 0, member_index: 1 },
        };
        st.apply_create(&pending);
        assert_eq!(
            st.groups[0].members,
            vec!["a".to_string(), "newsess".to_string(), "b".to_string()],
            "inserted directly above b, at its old position"
        );
        assert!(st.dirty);

        let mut st2 = grouped_state(); // G1=[a], G2=[b], INBOX=[c]
        let pending2 = PendingCreate {
            session_name: "second".into(),
            window_name: Some("logs".into()),
            placement: CreatePlacement::EndOfGroup { group: 1 },
        };
        st2.apply_create(&pending2);
        assert_eq!(st2.groups[1].members, vec!["b".to_string(), "second".to_string()]);
        assert!(st2.dirty);
    }
}
