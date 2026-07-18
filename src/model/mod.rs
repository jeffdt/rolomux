mod types;
pub use types::*;

mod reorder;
pub use reorder::WindowMove;
use reorder::PendingWindowMove;

mod rename;
pub use rename::{PendingRename, RenameTarget};

mod settings;
pub use settings::SettingsRow;

use crate::store::Config;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct PickerState {
    all: Vec<Session>,
    pub groups: Vec<Group>,
    expanded: HashSet<String>,
    dormant: HashSet<String>,
    focus_mode: bool,
    pub cursor: usize,
    pub dirty: bool,
    pub mode: Mode,
    pub query: String,
    search_cursor: usize,
    /// Cursor position within the group list in `Mode::Groups`.
    pub group_cursor: usize,
    /// In-flight rename buffer; `Some` while a rename is in progress.
    pub group_edit: Option<String>,
    /// In-flight session/window rename buffer; `Some` while a rename is in progress.
    rename_edit: Option<String>,
    /// In-flight window-move confirmation, armed when a press would destroy
    /// a session; `Some` until the same-direction key repeats it or any
    /// other key clears it.
    pending_window_move: Option<PendingWindowMove>,
    /// One-shot flag set when `group_reorder` refuses to move the inbox;
    /// cleared by any other group-mode input, mirroring
    /// `pending_window_move`'s clear-on-any-other-key lifecycle.
    group_reorder_blocked: bool,
    pub default_mode: DefaultMode,
    pub number_dormant_sessions: bool,
    pub remember_expanded_sessions: bool,
    pub clear_dormant_on_attach: bool,
    pub session_metric: SessionMetric,
    pub new_group_position: NewGroupPosition,
    pub new_group_color_policy: ColorPolicy,
    pub static_color: String,
    pub active_palette: Vec<String>,
    pub attached_color: String,
    pub border_color: String,
    settings_cursor: usize,
    palette_expanded: bool,
    attached_color_expanded: bool,
    border_color_expanded: bool,
}

impl PickerState {
    pub fn build(sessions: Vec<Session>, config: &Config) -> PickerState {
        Self::build_with_focus(sessions, config, INITIAL_FOCUS, None)
    }

    /// Like `build`, but with an explicit initial-focus policy and current
    /// session. `build` calls this with `INITIAL_FOCUS` and no precise current
    /// (the `attached` flag is the fallback); tests use it to exercise each
    /// policy and the precise-current path directly.
    fn build_with_focus(
        sessions: Vec<Session>,
        config: &Config,
        focus: InitialFocus,
        current: Option<&str>,
    ) -> PickerState {
        let mut groups = config.groups.clone();
        ensure_single_inbox(&mut groups);
        ensure_inbox_last(&mut groups);
        let mut state = PickerState {
            all: sessions,
            groups,
            expanded: if config.remember_expanded_sessions {
                config.expanded.iter().cloned().collect()
            } else {
                HashSet::new()
            },
            dormant: config.dormant.iter().cloned().collect(),
            focus_mode: config.focus_mode,
            cursor: 0,
            dirty: false,
            mode: config.default_mode.as_mode(),
            query: String::new(),
            search_cursor: 0,
            group_cursor: 0,
            group_edit: None,
            rename_edit: None,
            pending_window_move: None,
            group_reorder_blocked: false,
            default_mode: config.default_mode,
            number_dormant_sessions: config.number_dormant_sessions,
            remember_expanded_sessions: config.remember_expanded_sessions,
            clear_dormant_on_attach: config.clear_dormant_on_attach,
            session_metric: config.session_metric,
            new_group_position: config.new_group_position,
            new_group_color_policy: config.new_group_color_policy,
            static_color: config.static_color.clone(),
            active_palette: config.active_palette.clone(),
            attached_color: config.attached_color.clone(),
            border_color: config.border_color.clone(),
            settings_cursor: 0,
            palette_expanded: false,
            attached_color_expanded: false,
            border_color_expanded: false,
        };
        state.apply_clear_dormant_on_attach();
        state.apply_initial_focus(focus, current);
        state
    }

    /// Like `build`, but seeds the transient expand set from `expanded`
    /// instead of from `config.expanded`/`remember_expanded_sessions`. Used
    /// to rebuild the picker after a mid-run session/window rename, so a
    /// session expanded this run (even with "remember expanded" off)
    /// survives the rebuild under its possibly-new name.
    pub fn build_with_expanded(sessions: Vec<Session>, config: &Config, expanded: Vec<String>) -> PickerState {
        let mut state = Self::build_with_focus(sessions, config, INITIAL_FOCUS, None);
        state.expanded = expanded.into_iter().collect();
        state
    }

    /// Refine the initial cursor with the precise current-session name resolved
    /// from `$TMUX` (which `build` can't see). Only applies under the
    /// `CurrentSession` policy, so swapping `INITIAL_FOCUS` to `FirstRow` is
    /// still honored. Called by `main` right after `build`.
    pub fn refocus_current(&mut self, current: Option<&str>) {
        if let (InitialFocus::CurrentSession, Some(name)) = (INITIAL_FOCUS, current) {
            self.focus_session(name);
        }
    }

    /// Place the cursor according to `focus`. For `CurrentSession`, prefer the
    /// precise `current` name (resolved from `$TMUX`), then the `attached`
    /// flag, then leave it on the first row (the `cursor: 0` default).
    fn apply_initial_focus(&mut self, focus: InitialFocus, current: Option<&str>) {
        if let InitialFocus::CurrentSession = focus {
            let target = current
                .map(str::to_string)
                .or_else(|| self.all.iter().find(|s| s.attached).map(|s| s.name.clone()));
            if let Some(name) = target {
                self.focus_session(&name);
            }
        }
    }

    /// The index of the group that owns `name`: either the group whose
    /// `members` literally lists it, or -- if no group does -- the inbox
    /// group, which absorbs anything not explicitly filed elsewhere. A
    /// session belongs to at most one *explicit* group (first match wins if
    /// config lists it twice); this only returns `None` if the inbox
    /// invariant somehow doesn't hold, which `ensure_single_inbox` prevents.
    pub fn group_index_of(&self, name: &str) -> Option<usize> {
        self.groups
            .iter()
            .position(|g| g.members.iter().any(|m| m == name))
            .or_else(|| self.inbox_index())
    }

    /// Live sessions currently attributed to group `gi` (via `group_index_of`,
    /// so an inbox group's count includes fallback members it hasn't
    /// persisted into `members` yet, not just its explicit list).
    pub fn group_session_count(&self, gi: usize) -> usize {
        self.all.iter().filter(|s| self.group_index_of(&s.name) == Some(gi)).count()
    }

    /// Sessions that fall back to inbox group `gi` (via `group_index_of`)
    /// and aren't excluded by `is_excluded`, sorted oldest-created-first.
    /// Shared by the move logic below (`effective_order`, which excludes
    /// this group's own already-persisted members) and by `ordered()`'s
    /// restructuring in Task 4 (which excludes whatever's already been
    /// placed in the output so far) -- both need "the inbox's virtual,
    /// never-persisted tail," and should not duplicate this filter/sort.
    fn inbox_overflow(&self, gi: usize, mut is_excluded: impl FnMut(&str) -> bool) -> Vec<&Session> {
        let mut rest: Vec<&Session> = self
            .all
            .iter()
            .filter(|s| !is_excluded(&s.name) && self.group_index_of(&s.name) == Some(gi))
            .collect();
        rest.sort_by(|a, b| a.created.cmp(&b.created).then(a.name.cmp(&b.name)));
        rest
    }

    /// The index of the one group flagged `inbox: true`. `PickerState`
    /// always has exactly one after construction (see `build_with_focus`),
    /// so this is only `None` before that invariant is established.
    pub fn inbox_index(&self) -> Option<usize> {
        self.groups.iter().position(|g| g.inbox)
    }

    /// Group id for each entry of `ordered()`. Parallel to `ordered()` so the
    /// UI can emit a section header wherever this value changes. Always
    /// resolvable now -- every session belongs to some group, the inbox at
    /// worst -- so unlike the pre-issue-#23 residual bucket there is no
    /// `None` case left to represent.
    pub fn ordered_group_ids(&self) -> Vec<usize> {
        self.ordered()
            .iter()
            .map(|s| self.group_index_of(&s.name).unwrap_or(0))
            .collect()
    }

    pub fn ordered(&self) -> Vec<&Session> {
        let mut out: Vec<&Session> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for (gi, g) in self.groups.iter().enumerate() {
            for name in &g.members {
                if seen.contains(name.as_str()) {
                    continue; // guard against a session listed in two groups
                }
                if let Some(sess) = self.all.iter().find(|s| &s.name == name) {
                    out.push(sess);
                    seen.insert(name.as_str());
                }
            }
            if g.inbox {
                // Sessions nobody has explicitly filed anywhere: they belong
                // to this block too (via group_index_of's fallback), but
                // aren't in `members` yet. Render them right after this
                // group's real members, oldest-created first, wherever this
                // group currently sits -- not appended at the very end of
                // the whole list.
                for sess in self.inbox_overflow(gi, |name| seen.contains(name)) {
                    out.push(sess);
                    seen.insert(sess.name.as_str());
                }
            }
        }
        if self.focus_mode {
            out.retain(|s| !self.dormant.contains(&s.name) || s.attached);
        }
        out
    }

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

    pub fn visible_rows(&self) -> Vec<Row> {
        let ordered = self.ordered();
        let mut rows = Vec::new();
        for (si, sess) in ordered.iter().enumerate() {
            rows.push(Row::Session(si));
            if self.expanded.contains(&sess.name) {
                for wi in 0..sess.windows.len() {
                    rows.push(Row::Window(si, wi));
                }
            }
        }
        rows
    }

    pub fn move_cursor(&mut self, delta: i32) {
        self.cursor = move_index_with_edge_wrap(self.cursor, delta, self.visible_rows().len());
    }

    fn cursor_ordered_index(&self) -> Option<usize> {
        let rows = self.visible_rows();
        rows.get(self.cursor).map(|r| match r {
            Row::Session(si) => *si,
            Row::Window(si, _) => *si,
        })
    }

    pub fn cursor_session_name(&self) -> Option<String> {
        let si = self.cursor_ordered_index()?;
        self.ordered().get(si).map(|s| s.name.clone())
    }

    pub fn is_expanded(&self, name: &str) -> bool {
        self.expanded.contains(name)
    }

    pub fn expand(&mut self) {
        if let Some(name) = self.cursor_session_name() {
            self.expanded.insert(name);
            if self.remember_expanded_sessions {
                self.dirty = true;
            }
        }
    }

    pub fn collapse(&mut self) {
        if let Some(name) = self.cursor_session_name() {
            self.expanded.remove(&name);
            if self.remember_expanded_sessions {
                self.dirty = true;
            }
            self.focus_session(&name);
        }
    }

    /// Whether `name` is marked dormant. When dormant sessions are shown, they
    /// are dimmed but otherwise fully normal; `focus_mode` is the only filter
    /// that removes them from the picker, except for the attached session,
    /// which always stays visible even while dormant (see `is_attached`).
    pub fn is_dormant(&self, name: &str) -> bool {
        self.dormant.contains(name)
    }

    /// Whether `name` is the session the current tmux client is attached to.
    fn is_attached(&self, name: &str) -> bool {
        self.all.iter().any(|s| s.name == name && s.attached)
    }

    pub fn focus_mode(&self) -> bool {
        self.focus_mode
    }

    /// Count of dormant sessions actually hidden by focus mode -- excludes
    /// the attached session, which stays visible even while dormant.
    pub fn hidden_dormant_count(&self) -> usize {
        if !self.focus_mode {
            return 0;
        }
        self.all.iter().filter(|s| self.is_dormant(&s.name) && !s.attached).count()
    }

    fn session_visible(&self, name: &str) -> bool {
        !self.focus_mode || !self.is_dormant(name) || self.is_attached(name)
    }

    /// Toggle whether focus mode (hiding dormant sessions, and any group left
    /// with nothing visible) is on. The filter is persisted as a preference so
    /// it survives closing and reopening the popup, same as the dormant set
    /// itself.
    pub fn toggle_focus_mode(&mut self) {
        let command_focus = self.cursor_session_name();
        let search_focus = self.search_cursor_name();
        self.focus_mode = !self.focus_mode;
        self.dirty = true;
        if let Some(name) = command_focus.as_deref().filter(|name| self.session_visible(name)) {
            self.focus_session(name);
        } else {
            self.clamp_cursor_to_visible_rows();
        }
        if let Some(name) = search_focus.as_deref() {
            if let Some(i) = self.search_results().iter().position(|s| s.name == name) {
                self.search_cursor = i;
            } else {
                self.clamp_search_cursor_to_results();
            }
        } else {
            self.clamp_search_cursor_to_results();
        }
    }

    /// Applied once at construction: if `clear_dormant_on_attach` is on,
    /// drop dormant status for every session that is both attached and
    /// dormant. This is the opt-in cleanup; `is_attached`'s always-visible
    /// exemption in `ordered`/`session_visible` is the always-on safety net
    /// that applies regardless of this setting.
    fn apply_clear_dormant_on_attach(&mut self) {
        if !self.clear_dormant_on_attach {
            return;
        }
        let names: Vec<String> = self
            .all
            .iter()
            .filter(|s| s.attached && self.dormant.contains(&s.name))
            .map(|s| s.name.clone())
            .collect();
        if names.is_empty() {
            return;
        }
        for name in names {
            self.dormant.remove(&name);
        }
        self.dirty = true;
    }

    fn clamp_cursor_to_visible_rows(&mut self) {
        let len = self.visible_rows().len();
        if len == 0 {
            self.cursor = 0;
        } else {
            self.cursor = self.cursor.min(len - 1);
        }
    }

    fn clamp_search_cursor_to_results(&mut self) {
        let len = self.search_results().len();
        if len == 0 {
            self.search_cursor = 0;
        } else {
            self.search_cursor = self.search_cursor.min(len - 1);
        }
    }

    /// Toggle dormant status for the session under the cursor. Resolves
    /// through an expanded window row to its parent session, same as
    /// `cursor_session_name` already does for other per-session commands.
    pub fn toggle_dormant(&mut self) {
        if let Some(name) = self.cursor_session_name() {
            if !self.dormant.remove(&name) {
                self.dormant.insert(name.clone());
            }
            self.dirty = true;
            if self.session_visible(&name) {
                self.focus_session(&name);
            } else {
                self.clamp_cursor_to_visible_rows();
                self.clamp_search_cursor_to_results();
            }
        }
    }

    /// Sorted snapshot of every dormant session name.
    pub fn dormant_list(&self) -> Vec<String> {
        let mut v: Vec<String> = self.dormant.iter().cloned().collect();
        v.sort();
        v
    }

    pub fn expanded_list(&self) -> Vec<String> {
        let mut v: Vec<String> = self.expanded.iter().cloned().collect();
        v.sort();
        v
    }

    /// Copy the current mutable picker state into the persisted config model.
    pub fn apply_to_config(&self, config: &mut Config) {
        config.groups = self.groups.clone();
        config.dormant = self.dormant_list();
        config.focus_mode = self.focus_mode();
        config.default_mode = self.default_mode;
        config.number_dormant_sessions = self.number_dormant_sessions;
        config.new_group_position = self.new_group_position;
        config.new_group_color_policy = self.new_group_color_policy;
        config.static_color = self.static_color.clone();
        config.active_palette = self.active_palette.clone();
        config.attached_color = self.attached_color.clone();
        config.border_color = self.border_color.clone();
        config.remember_expanded_sessions = self.remember_expanded_sessions;
        config.clear_dormant_on_attach = self.clear_dormant_on_attach;
        config.session_metric = self.session_metric;
        config.expanded = self.expanded_list();
    }

    pub fn focus_session(&mut self, name: &str) {
        let rows = self.visible_rows();
        let ordered = self.ordered();
        for (i, r) in rows.iter().enumerate() {
            if let Row::Session(si) = r {
                if ordered[*si].name == name {
                    self.cursor = i;
                    return;
                }
            }
        }
    }

    /// Focus a specific window row by session name and stable tmux window
    /// index (unlike a window's name, its index survives a rename). Requires
    /// the session to already be expanded, which holds after a window rename
    /// since only a visible window row can be renamed in the first place.
    pub fn focus_window(&mut self, session: &str, index: u32) {
        let rows = self.visible_rows();
        let ordered = self.ordered();
        for (i, r) in rows.iter().enumerate() {
            if let Row::Window(si, wi) = r {
                if ordered[*si].name == session && ordered[*si].windows[*wi].index == index {
                    self.cursor = i;
                    return;
                }
            }
        }
    }

    /// Force `name` into the expanded set regardless of its prior state --
    /// used after a cross-session window move lands in a session that may
    /// not already be expanded.
    pub fn expand_session(&mut self, name: &str) {
        self.expanded.insert(name.to_string());
        if self.remember_expanded_sessions {
            self.dirty = true;
        }
    }

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
        self.groups.swap(gc, target);
        self.group_cursor = target;
        self.dirty = true;
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

    pub fn selected_action(&self) -> Option<Action> {
        let rows = self.visible_rows();
        let ordered = self.ordered();
        match rows.get(self.cursor)? {
            Row::Session(si) => {
                Some(Action::SwitchSession(ordered[*si].name.clone()))
            }
            Row::Window(si, wi) => {
                let sess = ordered[*si];
                Some(Action::SwitchWindow(sess.name.clone(), sess.windows[*wi].index))
            }
        }
    }

    fn numbered_order(&self) -> Vec<&Session> {
        self.ordered()
            .into_iter()
            .filter(|s| self.number_dormant_sessions || !self.is_dormant(&s.name))
            .collect()
    }

    /// Switch action for the session at 1-based display number `n` (grouped #1
    /// down, stable regardless of what is expanded). Visible dormant sessions
    /// participate only when `number_dormant_sessions` is enabled. `None` if
    /// out of range.
    pub fn action_for_session_number(&self, n: usize) -> Option<Action> {
        if n == 0 {
            return None;
        }
        self.numbered_order()
            .get(n - 1)
            .map(|s| Action::SwitchSession(s.name.clone()))
    }

    /// Expand every session, or collapse all if everything is already expanded.
    /// Keeps the cursor on the same session.
    pub fn toggle_all(&mut self) {
        let focus = self.cursor_session_name();
        if self.expanded.len() >= self.all.len() {
            self.expanded.clear();
        } else {
            self.expanded = self.all.iter().map(|s| s.name.clone()).collect();
        }
        if self.remember_expanded_sessions {
            self.dirty = true;
        }
        if let Some(name) = focus {
            self.focus_session(&name);
        }
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

/// Move within a bounded list, wrapping one-row cursor steps at the edges.
fn move_index_with_edge_wrap(index: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let max = len - 1;
    let index = index.min(max);
    if delta == -1 && index == 0 {
        max
    } else if delta == 1 && index == max {
        0
    } else {
        (index as i32 + delta).clamp(0, max as i32) as usize
    }
}

/// Shared constructors for the `model` submodule test suites. `pub(crate)` and
/// `#[cfg(test)]` so every module's own `mod tests` can build fixtures the same
/// way without redefining them.
#[cfg(test)]
pub(crate) mod test_support {
    use super::{Group, PickerState, Session, Window};
    use crate::store::Config;

    pub fn s(name: &str, activity: i64, created: i64) -> Session {
        Session { id: String::new(),
            name: name.into(),
            activity,
            created,
            attached: false,
            windows: vec![Window { index: 0, name: "w".into(), active: true }],
        }
    }

    pub fn win(index: u32, name: &str) -> Window {
        Window { index, name: name.into(), active: false }
    }

    pub fn session_with_windows(name: &str, created: i64, windows: Vec<Window>) -> Session {
        Session { id: String::new(), name: name.into(), activity: created, created, attached: false, windows }
    }

    pub fn state_with_two_groups() -> PickerState {
        // groups: G1=[a,b], G2=[c]; residual d,e by activity (d 40 > e 30)
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("c", 1, 3), s("d", 40, 4), s("e", 30, 5)];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "G1".into(), members: vec!["a".into(), "b".into()], color: String::new(), ..Default::default() },
                Group { name: "G2".into(), members: vec!["c".into()], color: String::new(), ..Default::default() },
            ],
            ..Default::default()
        };
        PickerState::build(sessions, &cfg)
    }

    pub fn grouped_state() -> PickerState {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("c", 1, 3)];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "G1".into(), members: vec!["a".into()], color: String::new(), ..Default::default() },
                Group { name: "G2".into(), members: vec!["b".into()], color: String::new(), ..Default::default() },
            ],
            ..Default::default()
        };
        PickerState::build(sessions, &cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::store::Config;

    #[test]
    fn expand_session_marks_dirty_only_when_remembering() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.expand_session("a");
        assert!(st.is_expanded("a"));
        assert!(!st.dirty); // remember_expanded_sessions defaults to false
    }

    #[test]
    fn group_index_of_falls_back_to_inbox_for_unlisted_sessions() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2)];
        let cfg = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() }],
            ..Default::default()
        };
        let st = PickerState::build(sessions, &cfg);
        assert_eq!(st.group_index_of("a"), Some(0));
        assert_eq!(st.group_index_of("b"), st.inbox_index());
        assert_ne!(st.inbox_index(), Some(0));
    }

    #[test]
    fn group_session_count_includes_inbox_fallback_members() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("c", 1, 3)];
        let cfg = Config {
            groups: vec![
                Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() },
                Group { name: "INBOX".into(), members: vec!["b".into()], inbox: true, ..Default::default() },
            ],
            ..Default::default()
        };
        let st = PickerState::build(sessions, &cfg);
        assert_eq!(st.group_session_count(0), 1); // WORK: just "a"
        assert_eq!(st.group_session_count(1), 2); // INBOX: persisted "b" + fallback "c"
    }

    #[test]
    fn all_named_colors_has_sixteen_unique_entries() {
        assert_eq!(ALL_NAMED_COLORS.len(), 16);
        let mut sorted = ALL_NAMED_COLORS.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 16, "no duplicate color names");
    }

    #[test]
    fn initial_focus_prefers_precise_current_over_attached() {
        // Ordered top is "a"; "b" carries the attached flag, but the precise
        // current (from $TMUX) is "c" -- the precise signal must win.
        let mut sessions = vec![s("a", 30, 1), s("b", 20, 2), s("c", 10, 3)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], ..Default::default() };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::CurrentSession, Some("c"));
        assert_eq!(state.cursor_session_name().as_deref(), Some("c"));
    }

    #[test]
    fn initial_focus_current_falls_back_to_attached_flag() {
        // No precise current; the attached flag ("b") is the fallback.
        let mut sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], ..Default::default() };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::CurrentSession, None);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn initial_focus_first_row_ignores_current_and_attached() {
        let mut sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], ..Default::default() };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::FirstRow, Some("b"));
        assert_eq!(state.cursor, 0);
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
    }

    #[test]
    fn initial_focus_current_falls_back_to_first_row_when_nothing_matches() {
        let sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::CurrentSession, None);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
    }

    #[test]
    fn build_defaults_to_current_focus_via_attached_fallback() {
        // Canary for the shipped INITIAL_FOCUS default; update if it is swapped.
        let mut sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn refocus_current_moves_to_named_session_and_no_ops_on_none() {
        let sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg); // no attached -> row 0 ("a")
        state.refocus_current(Some("b"));
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
        state.refocus_current(None); // no-op
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn ordered_lists_groups_in_order_then_residual_by_created() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2), s("c", 20, 3), s("d", 40, 4)];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "CONFIG".into(), members: vec!["c".into()], color: String::new(), ..Default::default() },
                Group { name: "TOOLS".into(), members: vec!["a".into()], color: String::new(), ..Default::default() },
            ],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        // groups first in config order (c, then a), residual unlisted by created asc (b 2, d 4)
        assert_eq!(names, vec!["c", "a", "b", "d"]);
    }

    #[test]
    fn ordered_breaks_ties_by_name_ascending() {
        let sessions = vec![s("zebra", 50, 5), s("apple", 50, 5)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        // both unranked with the same created time, so sort by name ascending: apple before zebra
        assert_eq!(names, vec!["apple", "zebra"]);
    }

    #[test]
    fn expand_reveals_windows_and_cursor_moves_over_them() {
        let mut sessions = vec![s("a", 10, 1), s("b", 5, 2)];
        sessions[0].windows = vec![
            Window { index: 0, name: "e".into(), active: true },
            Window { index: 1, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        // Collapsed: two session rows only.
        assert_eq!(state.visible_rows().len(), 2);

        // Cursor on "a" (first), expand it.
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
        state.expand();
        assert!(state.is_expanded("a"));
        assert!(!state.is_expanded("b"));
        assert_eq!(state.visible_rows().len(), 4); // a, a:0, a:1, b

        // Move down twice -> still within a's windows / onto b.
        state.move_cursor(1);
        state.move_cursor(1);
        assert!(matches!(state.visible_rows()[state.cursor], Row::Window(0, 1)));

        // Large jumps still land on the edge.
        state.move_cursor(5);
        assert_eq!(state.cursor, 3);
        // Moving past either edge wraps.
        state.move_cursor(1);
        assert_eq!(state.cursor, 0);
        state.move_cursor(-1);
        assert_eq!(state.cursor, 3);
    }

    #[test]
    fn selected_action_session_vs_window() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].windows = vec![
            Window { index: 0, name: "e".into(), active: true },
            Window { index: 3, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        // On the session row.
        assert_eq!(state.selected_action(), Some(Action::SwitchSession("a".into())));

        // Expand and move onto the second window (tmux index 3).
        state.expand();
        state.move_cursor(2);
        assert_eq!(state.selected_action(), Some(Action::SwitchWindow("a".into(), 3)));
    }

    #[test]
    fn action_for_session_number_uses_stable_pinned_first_order() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2), s("c", 20, 3)];
        let cfg = Config {
            dormant: vec![], groups: vec![Group { name: "PINNED".into(), members: vec!["c".into()], color: String::new(), ..Default::default() }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg); // order: c, a, b (a/b unranked by created asc)

        assert_eq!(state.action_for_session_number(1), Some(Action::SwitchSession("c".into())));
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("a".into())));
        assert_eq!(state.action_for_session_number(3), Some(Action::SwitchSession("b".into())));
        assert_eq!(state.action_for_session_number(0), None);
        assert_eq!(state.action_for_session_number(4), None);

        // Numbers are stable even when a session is expanded (no renumbering).
        state.expand(); // expands "c" (cursor at top)
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("a".into())));
        assert_eq!(state.action_for_session_number(3), Some(Action::SwitchSession("b".into())));
    }

    #[test]
    fn action_for_session_number_extends_past_nine() {
        let sessions: Vec<Session> = (1..=12).map(|i| s(&format!("s{i}"), 0, i as i64)).collect();
        let cfg = Config::default();
        let state = PickerState::build(sessions, &cfg);

        assert_eq!(state.action_for_session_number(10), Some(Action::SwitchSession("s10".into())));
        assert_eq!(state.action_for_session_number(11), Some(Action::SwitchSession("s11".into())));
        assert_eq!(state.action_for_session_number(12), Some(Action::SwitchSession("s12".into())));
        assert_eq!(state.action_for_session_number(13), None);
    }

    #[test]
    fn dormant_sessions_can_be_skipped_by_jump_numbering() {
        let sessions = vec![s("alpha", 10, 1), s("beta", 20, 2), s("gamma", 30, 3)];
        let cfg = Config {
            dormant: vec!["beta".into()],
            number_dormant_sessions: false,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);

        assert_eq!(state.action_for_session_number(1), Some(Action::SwitchSession("alpha".into())));
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("gamma".into())));
        assert_eq!(state.action_for_session_number(3), None);
    }

    #[test]
    fn hidden_dormant_sessions_are_never_jump_numbered() {
        let sessions = vec![s("alpha", 10, 1), s("beta", 20, 2), s("gamma", 30, 3)];
        let cfg = Config {
            dormant: vec!["beta".into()],
            focus_mode: true,
            number_dormant_sessions: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);

        assert_eq!(state.action_for_session_number(1), Some(Action::SwitchSession("alpha".into())));
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("gamma".into())));
        assert_eq!(state.action_for_session_number(3), None);
    }

    #[test]
    fn ordered_manual_empty_list_is_created_ascending() {
        // No manual placements yet: ungrouped read oldest -> newest (created asc),
        // so a freshly created session naturally lands at the bottom.
        let sessions = vec![s("a", 99, 3), s("b", 99, 1), s("c", 99, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["b", "c", "a"]); // created 1, 2, 3
    }

    #[test]
    fn ordered_manual_lists_then_remaining_excluding_pinned() {
        let sessions = vec![s("a", 1, 10), s("b", 1, 20), s("c", 1, 30), s("d", 1, 40)];
        // d is in a PINNED group (and also wrongly listed in inbox to prove it is
        // filtered out of the inbox tail); c then a are the inbox placements;
        // b is unlisted and falls in after, by created asc.
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "PINNED".into(), members: vec!["d".into()], ..Default::default() },
                Group { name: "INBOX".into(), members: vec!["d".into(), "c".into(), "a".into()], inbox: true, ..Default::default() }
            ],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["d", "c", "a", "b"]);
    }

    #[test]
    fn ordered_manual_new_session_sinks_to_bottom() {
        // "x" is the newest (highest created) and unlisted -> appears last.
        let sessions = vec![s("old", 1, 1), s("mid", 1, 2), s("x", 1, 99)];
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "INBOX".into(), members: vec!["mid".into(), "old".into()], inbox: true, ..Default::default() }
            ],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["mid", "old", "x"]);
    }

    #[test]
    fn default_mode_is_command() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        assert_eq!(state.mode, Mode::Command);
        assert!(state.query.is_empty());
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
    fn toggle_all_expands_then_collapses_keeping_focus() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        assert_eq!(state.visible_rows().len(), 2); // both collapsed

        state.toggle_all(); // expand all -> 2 sessions + 2 windows
        assert!(state.is_expanded("a"));
        assert!(state.is_expanded("b"));
        assert_eq!(state.visible_rows().len(), 4);
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));

        state.toggle_all(); // collapse all
        assert!(!state.is_expanded("a"));
        assert!(!state.is_expanded("b"));
        assert_eq!(state.visible_rows().len(), 2);
    }

    #[test]
    fn toggle_dormant_flips_and_dirties() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg); // cursor starts on "a"

        assert!(!state.is_dormant("a"));
        state.toggle_dormant();
        assert!(state.is_dormant("a"));
        assert!(state.dirty);

        state.dirty = false;
        state.toggle_dormant();
        assert!(!state.is_dormant("a"));
        assert!(state.dirty);
    }

    #[test]
    fn dormant_loads_from_config() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config { groups: vec![], dormant: vec!["a".into()], ..Default::default() };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.is_dormant("a"));
    }

    #[test]
    fn focus_mode_loads_from_config() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            focus_mode: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.focus_mode());
        assert_eq!(state.hidden_dormant_count(), 1);
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["b"]);
    }

    #[test]
    fn attached_dormant_session_stays_visible_in_focus_mode() {
        let mut sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        sessions[1].attached = true;
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into(), "b".into()],
            focus_mode: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            visible,
            vec!["b"],
            "attached dormant session stays visible; non-attached dormant session stays hidden"
        );
        assert_eq!(
            state.hidden_dormant_count(),
            1,
            "only the non-attached dormant session counts toward the hidden count"
        );
    }

    #[test]
    fn session_visible_true_for_attached_dormant_session_in_focus_mode() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].attached = true;
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            focus_mode: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.session_visible("a"));
    }

    #[test]
    fn toggle_dormant_on_attached_session_keeps_cursor_focused_in_focus_mode() {
        let mut sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        sessions[0].attached = true;
        let cfg = Config { groups: vec![], focus_mode: true, ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg); // cursor starts on "a" (attached)
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));

        state.toggle_dormant(); // marks "a" dormant while it's the attached session
        assert!(state.is_dormant("a"));
        assert_eq!(
            state.cursor_session_name().as_deref(),
            Some("a"),
            "cursor stays on the attached session even though it's now dormant"
        );
    }

    #[test]
    fn build_clears_dormant_flag_for_attached_session_when_setting_on() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].attached = true;
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            clear_dormant_on_attach: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(!state.is_dormant("a"), "attaching to a dormant session clears its dormant flag when the setting is on");
        assert!(state.dirty, "the cleared flag is marked dirty so it flushes to config");
    }

    #[test]
    fn build_leaves_dormant_flag_when_clear_on_attach_setting_is_off() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].attached = true;
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            clear_dormant_on_attach: false,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.is_dormant("a"), "dormant flag stays untouched when the setting is off");
        assert!(!state.dirty, "nothing changed, so build shouldn't mark state dirty");
    }

    #[test]
    fn build_leaves_dormant_flag_for_non_attached_session_even_with_setting_on() {
        let sessions = vec![s("a", 30, 1)]; // not attached
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            clear_dormant_on_attach: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.is_dormant("a"), "only the attached session's dormant flag is cleared");
    }

    #[test]
    fn toggle_dormant_on_expanded_window_row_affects_parent_session() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].windows = vec![
            Window { index: 0, name: "e".into(), active: true },
            Window { index: 1, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        state.expand();
        state.move_cursor(1); // land on the first window row
        assert!(matches!(state.visible_rows()[state.cursor], Row::Window(0, 0)));

        state.toggle_dormant();
        assert!(state.is_dormant("a"), "toggling on a window row affects its parent session");
    }

    #[test]
    fn dormant_list_is_sorted_snapshot() {
        let sessions = vec![s("charlie", 1, 1), s("alpha", 1, 2), s("bravo", 1, 3)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("charlie");
        state.toggle_dormant();
        state.focus_session("alpha");
        state.toggle_dormant();
        assert_eq!(state.dormant_list(), vec!["alpha".to_string(), "charlie".to_string()]);
    }

    #[test]
    fn build_seeds_expanded_from_config_only_when_remembering() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2)];

        let cfg_off = Config {
            remember_expanded_sessions: false,
            expanded: vec!["a".to_string()],
            ..Default::default()
        };
        let state_off = PickerState::build(sessions.clone(), &cfg_off);
        assert!(!state_off.is_expanded("a"), "off: config's expanded list is ignored");

        let cfg_on = Config {
            remember_expanded_sessions: true,
            expanded: vec!["a".to_string()],
            ..Default::default()
        };
        let state_on = PickerState::build(sessions, &cfg_on);
        assert!(state_on.is_expanded("a"), "on: config's expanded list is restored");
        assert!(!state_on.is_expanded("b"));
    }

    #[test]
    fn expand_and_collapse_only_mark_dirty_when_remembering() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default(); // remember_expanded_sessions: false
        let mut state = PickerState::build(sessions, &cfg);
        state.expand();
        assert!(state.is_expanded("a"));
        assert!(!state.dirty, "off: expand does not persist");
        state.collapse();
        assert!(!state.dirty, "off: collapse does not persist");

        state.remember_expanded_sessions = true;
        state.expand();
        assert!(state.dirty, "on: expand persists");
        state.dirty = false;
        state.collapse();
        assert!(state.dirty, "on: collapse persists");
    }

    #[test]
    fn toggle_all_only_marks_dirty_when_remembering() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2)];
        let cfg = Config::default();
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_all();
        assert!(!state.dirty, "off: toggle_all does not persist");

        state.dirty = false;
        state.remember_expanded_sessions = true;
        state.toggle_all();
        assert!(state.dirty, "on: toggle_all persists");
    }

    #[test]
    fn expanded_list_is_sorted_snapshot() {
        let sessions = vec![s("charlie", 1, 1), s("alpha", 1, 2), s("bravo", 1, 3)];
        let cfg = Config { groups: vec![], remember_expanded_sessions: true, ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("charlie");
        state.expand();
        state.focus_session("alpha");
        state.expand();
        assert_eq!(state.expanded_list(), vec!["alpha".to_string(), "charlie".to_string()]);
    }

    #[test]
    fn toggle_focus_mode_filters_command_and_search_and_dirties() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2), s("gamma", 1, 3)];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        assert!(state.is_dormant("beta"));
        let shown: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(shown, vec!["alpha", "beta", "gamma"]);

        state.toggle_focus_mode();
        assert!(state.focus_mode());
        assert_eq!(state.hidden_dormant_count(), 1);
        assert!(state.dirty, "entering focus mode persists the preference");
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["alpha", "gamma"]);

        state.enter_search();
        state.search_push('b');
        assert!(state.search_results().is_empty(), "hidden dormant sessions are absent from search");
        state.search_clear();
        let search_visible: Vec<&str> = state.search_results().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(search_visible, vec!["alpha", "gamma"]);

        state.toggle_focus_mode();
        assert!(!state.focus_mode());
        let restored: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(restored, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn focus_mode_clamps_cursor_when_selected_session_disappears() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("beta");
        assert_eq!(state.cursor_session_name().as_deref(), Some("beta"));

        state.toggle_focus_mode();

        assert_eq!(state.cursor_session_name().as_deref(), Some("alpha"));
        assert_eq!(state.visible_rows().len(), 1);
    }

    #[test]
    fn toggling_dormant_while_filter_is_active_hides_the_session() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_focus_mode();
        assert_eq!(state.cursor_session_name().as_deref(), Some("alpha"));

        state.toggle_dormant();

        assert!(state.is_dormant("alpha"));
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["beta"]);
        assert_eq!(state.cursor_session_name().as_deref(), Some("beta"));
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
    fn group_new_defaults_to_bottom() {
        let mut st = grouped_state();
        assert_eq!(st.new_group_position, NewGroupPosition::Bottom);
        st.enter_groups();
        st.group_new();
        assert_eq!(st.groups[2].name, "", "default position lands above the inbox");
        assert!(st.groups[3].inbox);
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
    fn group_reorder_swaps_named_groups() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_reorder(1); // move G1 down
        assert_eq!(st.groups[0].name, "G2");
        assert_eq!(st.groups[1].name, "G1");
        assert!(st.dirty);
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
    fn residual_count_excludes_grouped() {
        let st = grouped_state(); // a,b grouped; c residual
        assert_eq!(st.group_session_count(st.inbox_index().unwrap()), 1);
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
    fn group_cycle_color_uses_the_customized_active_palette() {
        let mut st = grouped_state();
        st.active_palette = vec!["white".to_string(), "black".to_string()];
        st.enter_groups(); // cursor on group 0
        st.group_cycle_color();
        // group 0's positional default is active_palette[0 % 2] = "white"; flip advances to "black".
        assert_eq!(st.groups[0].color, "black");
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
    fn build_seeds_mode_and_fields_from_config_default_mode() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config {
            default_mode: DefaultMode::Search,
            number_dormant_sessions: false,
            new_group_color_policy: ColorPolicy::Static,
            static_color: "red".to_string(),
            active_palette: vec!["red".to_string(), "white".to_string()],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert_eq!(state.mode, Mode::Search, "startup mode follows config.default_mode");
        assert_eq!(state.default_mode, DefaultMode::Search);
        assert!(!state.number_dormant_sessions);
        assert_eq!(state.new_group_color_policy, ColorPolicy::Static);
        assert_eq!(state.static_color, "red");
        assert_eq!(state.active_palette, vec!["red".to_string(), "white".to_string()]);
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
    fn enter_and_exit_groups_toggles_mode() {
        let mut st = grouped_state();
        assert_eq!(st.mode, Mode::Command);
        st.enter_groups();
        assert_eq!(st.mode, Mode::Groups);
        st.exit_groups();
        assert_eq!(st.mode, Mode::Command);
    }

    #[test]
    fn build_normalizes_a_stored_inbox_position_back_to_the_trailing_slot() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("new", 1, 3)];
        // A config saved before issue #111 pinned the inbox down (or a
        // hand-edited TOML) might store the inbox anywhere; `build` must
        // normalize it back to the trailing slot via `ensure_inbox_last`.
        let cfg = Config {
            groups: vec![
                Group { name: "INBOX".into(), members: vec!["b".into()], inbox: true, ..Default::default() },
                Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        let st = PickerState::build(sessions, &cfg);
        assert_eq!(st.groups.last().map(|g| g.name.as_str()), Some("INBOX"));
        assert_eq!(st.groups.first().map(|g| g.name.as_str()), Some("WORK"));
        let names: Vec<&str> = st.ordered().iter().map(|s| s.name.as_str()).collect();
        // "new" is never explicitly listed anywhere, so it renders inside the
        // inbox's own (now always-trailing) block, right after "b".
        assert_eq!(names, vec!["a", "b", "new"]);
    }

    #[test]
    fn ordered_group_ids_are_never_none() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2)];
        let cfg = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() }],
            ..Default::default()
        };
        let st = PickerState::build(sessions, &cfg);
        let ids = st.ordered_group_ids();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], 0); // "a" in WORK
        assert_eq!(ids[1], st.inbox_index().unwrap()); // "b" falls back to inbox
    }

    #[test]
    fn ordered_group_ids_track_sections() {
        let st = state_with_two_groups(); // G1=[a,b], G2=[c], residual d,e (falls back to inbox)
        let inbox = st.inbox_index().unwrap();
        assert_eq!(
            st.ordered_group_ids(),
            vec![0, 0, 1, inbox, inbox]
        );
    }

    #[test]
    fn apply_to_config_persists_settings_preferences() {
        let dir = std::env::temp_dir().join(format!("rolomux-state-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let mut cfg = Config::default();
        let mut st = PickerState::build(vec![s("a", 1, 1)], &cfg);
        st.attached_color = "magenta".to_string();
        st.border_color = "yellow".to_string();
        st.default_mode = DefaultMode::Search;
        st.number_dormant_sessions = false;
        st.focus_mode = true;
        st.new_group_position = NewGroupPosition::Bottom;
        st.new_group_color_policy = ColorPolicy::Static;
        st.static_color = "white".to_string();
        st.active_palette = vec!["red".to_string(), "white".to_string()];
        st.remember_expanded_sessions = true;
        st.clear_dormant_on_attach = true;
        st.session_metric = SessionMetric::Age;

        st.apply_to_config(&mut cfg);
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);

        assert_eq!(reloaded.attached_color, "magenta");
        assert_eq!(reloaded.border_color, "yellow");
        assert_eq!(reloaded.default_mode, DefaultMode::Search);
        assert!(!reloaded.number_dormant_sessions);
        assert!(reloaded.focus_mode);
        assert_eq!(reloaded.new_group_position, NewGroupPosition::Bottom);
        assert_eq!(reloaded.new_group_color_policy, ColorPolicy::Static);
        assert_eq!(reloaded.static_color, "white");
        assert_eq!(reloaded.active_palette, vec!["red".to_string(), "white".to_string()]);
        assert!(reloaded.remember_expanded_sessions);
        assert!(reloaded.clear_dormant_on_attach);
        assert_eq!(reloaded.session_metric, SessionMetric::Age);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pick_random_color_selects_by_seed_modulo_palette_len() {
        let palette = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(pick_random_color(&palette, 0), "a");
        assert_eq!(pick_random_color(&palette, 1), "b");
        assert_eq!(pick_random_color(&palette, 2), "c");
        assert_eq!(pick_random_color(&palette, 3), "a", "wraps");
    }

    #[test]
    fn pick_random_color_empty_palette_returns_empty_string() {
        assert_eq!(pick_random_color(&[], 42), "");
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
    fn build_with_expanded_seeds_expand_set_regardless_of_remember_setting() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        let cfg = Config { remember_expanded_sessions: false, ..Default::default() };
        let st = PickerState::build_with_expanded(sessions, &cfg, vec!["alpha".to_string()]);
        assert!(st.is_expanded("alpha"));
        assert!(!st.is_expanded("beta"));
    }

    #[test]
    fn build_with_expanded_ignores_config_expanded_field() {
        let sessions = vec![s("alpha", 1, 1)];
        let cfg = Config {
            remember_expanded_sessions: true,
            expanded: vec!["alpha".to_string()],
            ..Default::default()
        };
        // Even though config.expanded lists "alpha", the explicit (empty) override wins.
        let st = PickerState::build_with_expanded(sessions, &cfg, vec![]);
        assert!(!st.is_expanded("alpha"));
    }
}
