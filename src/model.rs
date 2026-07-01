#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Activity,
    Created,
    Manual,
}

impl SortKey {
    pub fn from_config_str(s: &str) -> SortKey {
        match s {
            "created" => SortKey::Created,
            "manual" => SortKey::Manual,
            _ => SortKey::Activity,
        }
    }

    /// The next mode in the in-picker cycle: recency -> age -> manual -> recency.
    pub fn next(self) -> SortKey {
        match self {
            SortKey::Activity => SortKey::Created,
            SortKey::Created => SortKey::Manual,
            SortKey::Manual => SortKey::Activity,
        }
    }
}

/// Where the cursor starts when the picker opens. Like `SortKey`, this is a
/// swappable seam: change the single `INITIAL_FOCUS` constant below to pick a
/// policy without touching `build`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialFocus {
    /// Always start on the first row (top pinned/sorted session). Legacy
    /// behavior. Selected only by swapping `INITIAL_FOCUS`, so it is not
    /// constructed in the shipped binary; the allow keeps that intentional
    /// reserved variant from tripping the dead-code lint.
    #[allow(dead_code)]
    FirstRow,
    /// Start on the session the popup was launched from. Resolved precisely
    /// from `$TMUX` (passed in as `current`), falling back to the `attached`
    /// flag, then the first row.
    CurrentSession,
}

/// The active initial-focus policy. Swap this one constant to change behavior.
pub const INITIAL_FOCUS: InitialFocus = InitialFocus::CurrentSession;

/// Picker interaction mode. `Command` is the single-keystroke command UI;
/// `Search` routes typed characters into a fuzzy-filter query; `Groups` is the
/// full-screen group-management overlay. Which mode the picker launches in is
/// the `DEFAULT_MODE` seam below (cf. `INITIAL_FOCUS`), so a future
/// `default_mode` config key can select it without reworking the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Command,
    Search,
    Groups,
}

/// The mode the picker starts in. Swap this one constant (or later wire it to
/// config) to change the launch behavior.
pub const DEFAULT_MODE: Mode = Mode::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub index: u32,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub activity: i64,
    pub created: i64,
    pub attached: bool,
    pub windows: Vec<Window>,
}

/// Named header colors, in the order the color-flip cycles through them. These
/// are the 16-color ANSI names (never RGB) so headers inherit the user's
/// terminal theme; `magenta` is the purple in a Nord palette. An empty
/// `Group::color` means "use the positional default" (`HEADER_COLORS[index]`).
pub const HEADER_COLORS: [&str; 6] = ["cyan", "green", "yellow", "magenta", "blue", "red"];

/// A user-named, ordered collection of sessions that renders as its own
/// section above the residual `SESSIONS` bucket. Groups are durable: they
/// persist even when empty and are removed only by an explicit delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub members: Vec<String>,
    /// Header color name from `HEADER_COLORS`, or empty for the positional
    /// default. Set explicitly by the color-flip so it survives reordering.
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    SwitchSession(String),
    SwitchWindow(String, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Session(usize),
    Window(usize, usize),
}

use crate::store::Config;
use std::collections::HashSet;

pub struct PickerState {
    all: Vec<Session>,
    pub groups: Vec<Group>,
    pub manual_order: Vec<String>,
    pub sort: SortKey,
    expanded: HashSet<String>,
    pub cursor: usize,
    pub dirty: bool,
    pub mode: Mode,
    pub query: String,
    search_cursor: usize,
    /// Cursor position within the group list in `Mode::Groups`.
    pub group_cursor: usize,
    /// In-flight rename buffer; `Some` while a rename is in progress.
    pub group_edit: Option<String>,
}

fn sort_value(s: &Session, key: SortKey) -> i64 {
    match key {
        SortKey::Activity => s.activity,
        // Manual never reaches here (`ordered` branches before sorting), but the
        // match must be exhaustive; created is the sensible fallthrough.
        SortKey::Created | SortKey::Manual => s.created,
    }
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
        let mut state = PickerState {
            all: sessions,
            groups: config.groups.clone(),
            manual_order: config.manual_order.clone(),
            sort: config.sort,
            expanded: HashSet::new(),
            cursor: 0,
            dirty: false,
            mode: DEFAULT_MODE,
            query: String::new(),
            search_cursor: 0,
            group_cursor: 0,
            group_edit: None,
        };
        state.apply_initial_focus(focus, current);
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

    /// The index of the named group that owns `name`, if any. A session belongs to
    /// at most one group (the first match wins if config lists it twice).
    pub fn group_index_of(&self, name: &str) -> Option<usize> {
        self.groups
            .iter()
            .position(|g| g.members.iter().any(|m| m == name))
    }

    /// Whether `name` sits in any named group (vs. the residual `SESSIONS` bucket).
    pub fn is_grouped(&self, name: &str) -> bool {
        self.group_index_of(name).is_some()
    }

    /// Group id for each entry of `ordered()`: `Some(group_index)` for a grouped
    /// session, `None` for the residual bucket. Parallel to `ordered()` so the UI
    /// can emit a section header wherever this value changes.
    pub fn ordered_group_ids(&self) -> Vec<Option<usize>> {
        self.ordered()
            .iter()
            .map(|s| self.group_index_of(&s.name))
            .collect()
    }

    pub fn ordered(&self) -> Vec<&Session> {
        let mut out: Vec<&Session> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for g in &self.groups {
            for name in &g.members {
                if seen.contains(name.as_str()) {
                    continue; // guard against a session listed in two groups
                }
                if let Some(sess) = self.all.iter().find(|s| &s.name == name) {
                    out.push(sess);
                    seen.insert(name.as_str());
                }
            }
        }
        let mut rest: Vec<&Session> = self
            .all
            .iter()
            .filter(|s| !self.is_grouped(&s.name))
            .collect();
        if self.sort == SortKey::Manual {
            // Manually placed sessions first (in saved order, skipping any that
            // are now grouped or gone), then everything unlisted by created
            // ascending so the newest session sinks to the bottom.
            rest.sort_by(|a, b| {
                let rank = |s: &Session| {
                    self.manual_order
                        .iter()
                        .position(|n| n == &s.name)
                        .map(|p| (0, p as i64))
                        .unwrap_or((1, s.created))
                };
                rank(a).cmp(&rank(b)).then(a.name.cmp(&b.name))
            });
        } else {
            rest.sort_by(|a, b| {
                sort_value(b, self.sort)
                    .cmp(&sort_value(a, self.sort))
                    .then(a.name.cmp(&b.name))
            });
        }
        out.extend(rest);
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
        let len = self.visible_rows().len() as i32;
        if len == 0 {
            self.cursor = 0;
            return;
        }
        let next = (self.cursor as i32 + delta).clamp(0, len - 1);
        self.cursor = next as usize;
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
        }
    }

    pub fn collapse(&mut self) {
        if let Some(name) = self.cursor_session_name() {
            self.expanded.remove(&name);
            self.focus_session(&name);
        }
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

    /// Move the session under the cursor by `delta` rows, crossing group boundaries
    /// when needed: out of a group into the one above/below, or into/out of the
    /// residual SESSIONS bucket. Clamps silently at the very top and bottom.
    pub fn move_row(&mut self, delta: i32) {
        let name = match self.cursor_session_name() { Some(n) => n, None => return };
        match self.group_index_of(&name) {
            Some(gi) => self.move_grouped(gi, &name, delta),
            None => self.move_residual(&name, delta),
        }
    }

    /// Move a session that currently lives in named group `gi`.
    fn move_grouped(&mut self, gi: usize, name: &str, delta: i32) {
        let pos = match self.groups[gi].members.iter().position(|m| m == name) {
            Some(p) => p,
            None => return,
        };
        let last = self.groups[gi].members.len().saturating_sub(1);
        if delta < 0 {
            if pos > 0 {
                self.groups[gi].members.swap(pos, pos - 1);
            } else if gi > 0 {
                self.groups[gi].members.remove(pos);
                self.groups[gi - 1].members.push(name.to_string());
            } else {
                return; // very top: clamp
            }
        } else if pos < last {
            self.groups[gi].members.swap(pos, pos + 1);
        } else if gi + 1 < self.groups.len() {
            self.groups[gi].members.remove(pos);
            self.groups[gi + 1].members.insert(0, name.to_string());
        } else {
            // bottom of the last group: drop into the residual bucket
            self.groups[gi].members.remove(pos);
            if self.sort == SortKey::Manual {
                self.manual_order.retain(|n| n != name);
                self.manual_order.insert(0, name.to_string());
            }
        }
        self.dirty = true;
        self.focus_session(name);
    }

    /// Move a session that currently lives in the residual `SESSIONS` bucket.
    fn move_residual(&mut self, name: &str, delta: i32) {
        let residual: Vec<String> = self
            .ordered()
            .iter()
            .filter(|s| !self.is_grouped(&s.name))
            .map(|s| s.name.clone())
            .collect();
        let ri = match residual.iter().position(|n| n == name) {
            Some(r) => r,
            None => return,
        };
        if delta < 0 && ri == 0 {
            if let Some(last) = self.groups.last_mut() {
                self.manual_order.retain(|n| n != name);
                last.members.push(name.to_string());
                self.dirty = true;
                self.focus_session(name);
            }
        } else if delta > 0 && ri + 1 == residual.len() {
            // residual bottom: clamp
        } else {
            // interior residual reorder: manual mode only (unchanged behavior)
            if self.sort == SortKey::Manual {
                self.move_unpinned(delta);
            }
        }
    }

    /// Reorder an ungrouped session within the manual order (Manual mode only).
    /// The first move freezes the current ungrouped display order into
    /// `manual_order` so the swap is well-defined even before anything was moved.
    fn move_unpinned(&mut self, delta: i32) {
        let name = match self.cursor_session_name() {
            Some(n) => n,
            None => return,
        };
        if self.is_grouped(&name) {
            return;
        }
        let mut frozen: Vec<String> = self
            .ordered()
            .iter()
            .filter(|s| !self.is_grouped(&s.name))
            .map(|s| s.name.clone())
            .collect();
        let pos = match frozen.iter().position(|p| p == &name) {
            Some(p) => p as i32,
            None => return,
        };
        let target = pos + delta;
        if target < 0 || target >= frozen.len() as i32 {
            return; // clamped at an edge: leave order untouched
        }
        frozen.swap(pos as usize, target as usize);
        self.manual_order = frozen;
        self.dirty = true;
        self.focus_session(&name);
    }

    /// Advance the sort mode to the next in the cycle and mark the state dirty so
    /// the choice persists.
    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.dirty = true;
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

    /// Number of live sessions in the residual `SESSIONS` bucket (ungrouped).
    pub fn residual_count(&self) -> usize {
        self.all.iter().filter(|s| !self.is_grouped(&s.name)).count()
    }

    /// Move the group cursor by `delta`, clamped to the valid range.
    pub fn group_move_cursor(&mut self, delta: i32) {
        let len = self.groups.len() as i32;
        if len == 0 { self.group_cursor = 0; return; }
        self.group_cursor = (self.group_cursor as i32 + delta).clamp(0, len - 1) as usize;
    }

    /// Reorder the selected group among the named groups (clamped at the ends).
    pub fn group_reorder(&mut self, delta: i32) {
        let gc = self.group_cursor;
        let target = gc as i32 + delta;
        if target < 0 || target >= self.groups.len() as i32 { return; }
        self.groups.swap(gc, target as usize);
        self.group_cursor = target as usize;
        self.dirty = true;
    }

    /// Append a new empty group after the last named group and begin naming it.
    pub fn group_new(&mut self) {
        self.groups.push(Group {
            name: String::new(),
            members: Vec::new(),
            color: String::new(),
        });
        self.group_cursor = self.groups.len() - 1;
        self.group_edit = Some(String::new());
    }

    /// Advance the selected group's header color to the next in `HEADER_COLORS`,
    /// wrapping around. Starts from the group's effective color (its explicit
    /// name, or the positional default) and stores the result explicitly so it
    /// no longer shifts when groups are reordered.
    pub fn group_cycle_color(&mut self) {
        let gi = self.group_cursor;
        if gi >= self.groups.len() {
            return;
        }
        let current = if self.groups[gi].color.is_empty() {
            HEADER_COLORS[gi % HEADER_COLORS.len()]
        } else {
            self.groups[gi].color.as_str()
        };
        let idx = HEADER_COLORS.iter().position(|c| *c == current).unwrap_or(0);
        self.groups[gi].color = HEADER_COLORS[(idx + 1) % HEADER_COLORS.len()].to_string();
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

    /// Delete the selected group; its members fall back into the residual bucket.
    pub fn group_delete(&mut self) {
        if self.group_cursor >= self.groups.len() { return; }
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
        let len = self.search_results().len() as i32;
        if len == 0 {
            self.search_cursor = 0;
            return;
        }
        let next = (self.search_cursor as i32 + delta).clamp(0, len - 1);
        self.search_cursor = next as usize;
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

    /// Switch action for the session at 1-based display number `n` (grouped #1
    /// down, stable regardless of what is expanded). `None` if out of range.
    pub fn action_for_session_number(&self, n: usize) -> Option<Action> {
        if n == 0 {
            return None;
        }
        self.ordered()
            .get(n - 1)
            .map(|s| Action::SwitchSession(s.name.clone()))
    }

    /// Move the cursor to the session at 1-based display number `n` (grouped #1
    /// down, the same stable order as `action_for_session_number`) and expand it
    /// so its windows show. Unlike a plain-digit switch, this only relocates the
    /// highlight and reveals windows so one can be picked; it does not switch.
    /// No-op if `n` is 0 or out of range.
    pub fn focus_session_number(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        if let Some(name) = self.ordered().get(n - 1).map(|s| s.name.clone()) {
            self.expanded.insert(name.clone());
            self.focus_session(&name);
        }
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
        if let Some(name) = focus {
            self.focus_session(&name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Config;

    fn s(name: &str, activity: i64, created: i64) -> Session {
        Session {
            name: name.into(),
            activity,
            created,
            attached: false,
            windows: vec![Window { index: 0, name: "w".into(), active: true }],
        }
    }

    #[test]
    fn sort_key_parses_with_default_fallback() {
        assert_eq!(SortKey::from_config_str("created"), SortKey::Created);
        assert_eq!(SortKey::from_config_str("activity"), SortKey::Activity);
        assert_eq!(SortKey::from_config_str("garbage"), SortKey::Activity);
        assert_eq!(SortKey::default(), SortKey::Activity);
    }

    #[test]
    fn initial_focus_prefers_precise_current_over_attached() {
        // Ordered top is "a"; "b" carries the attached flag, but the precise
        // current (from $TMUX) is "c" -- the precise signal must win.
        let mut sessions = vec![s("a", 30, 1), s("b", 20, 2), s("c", 10, 3)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::CurrentSession, Some("c"));
        assert_eq!(state.cursor_session_name().as_deref(), Some("c"));
    }

    #[test]
    fn initial_focus_current_falls_back_to_attached_flag() {
        // No precise current; the attached flag ("b") is the fallback.
        let mut sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::CurrentSession, None);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn initial_focus_first_row_ignores_current_and_attached() {
        let mut sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        sessions[1].attached = true;
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state =
            PickerState::build_with_focus(sessions, &cfg, InitialFocus::FirstRow, Some("b"));
        assert_eq!(state.cursor, 0);
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
    }

    #[test]
    fn initial_focus_current_falls_back_to_first_row_when_nothing_matches() {
        let sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn refocus_current_moves_to_named_session_and_no_ops_on_none() {
        let sessions = vec![s("a", 30, 1), s("b", 10, 2)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg); // no attached -> row 0 ("a")
        state.refocus_current(Some("b"));
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
        state.refocus_current(None); // no-op
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
    }

    #[test]
    fn ordered_lists_groups_in_order_then_residual_by_activity() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2), s("c", 20, 3), s("d", 40, 4)];
        let cfg = Config {
            groups: vec![
                Group { name: "CONFIG".into(), members: vec!["c".into()], color: String::new() },
                Group { name: "TOOLS".into(), members: vec!["a".into()], color: String::new() },
            ],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        // groups first in config order (c, then a), residual by activity desc (d 40, b 30)
        assert_eq!(names, vec!["c", "a", "d", "b"]);
        assert!(state.is_grouped("c"));
        assert!(state.is_grouped("a"));
        assert!(!state.is_grouped("b"));
        assert_eq!(state.group_index_of("a"), Some(1));
        assert_eq!(state.group_index_of("b"), None);
    }

    #[test]
    fn ordered_unpinned_by_created_when_configured() {
        let sessions = vec![s("a", 10, 100), s("b", 30, 50)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Created };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        // created desc: a (100) before b (50)
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn ordered_breaks_ties_by_name_ascending() {
        let sessions = vec![s("zebra", 50, 1), s("apple", 50, 2)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        // both have activity 50, so sort by name ascending: apple before zebra
        assert_eq!(names, vec!["apple", "zebra"]);
    }

    #[test]
    fn expand_reveals_windows_and_cursor_moves_over_them() {
        let mut sessions = vec![s("a", 10, 1), s("b", 5, 2)];
        sessions[0].windows = vec![
            Window { index: 0, name: "e".into(), active: true },
            Window { index: 1, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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

        // Clamp at bottom.
        state.move_cursor(5);
        assert_eq!(state.cursor, 3);
        // Clamp at top.
        state.move_cursor(-99);
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn selected_action_session_vs_window() {
        let mut sessions = vec![s("a", 30, 1)];
        sessions[0].windows = vec![
            Window { index: 0, name: "e".into(), active: true },
            Window { index: 3, name: "l".into(), active: false },
        ];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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
            groups: vec![Group { name: "PINNED".into(), members: vec!["c".into()], color: String::new() }],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
        let mut state = PickerState::build(sessions, &cfg); // order: c, b, a

        assert_eq!(state.action_for_session_number(1), Some(Action::SwitchSession("c".into())));
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("b".into())));
        assert_eq!(state.action_for_session_number(3), Some(Action::SwitchSession("a".into())));
        assert_eq!(state.action_for_session_number(0), None);
        assert_eq!(state.action_for_session_number(4), None);

        // Numbers are stable even when a session is expanded (no renumbering).
        state.expand(); // expands "c" (cursor at top)
        assert_eq!(state.action_for_session_number(2), Some(Action::SwitchSession("b".into())));
        assert_eq!(state.action_for_session_number(3), Some(Action::SwitchSession("a".into())));
    }

    #[test]
    fn focus_session_number_moves_cursor_without_switching() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2), s("c", 20, 3)];
        let cfg = Config {
            groups: vec![Group { name: "PINNED".into(), members: vec!["c".into()], color: String::new() }],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
        let mut state = PickerState::build(sessions, &cfg); // order: c, b, a

        state.focus_session_number(3); // -> a
        assert_eq!(state.cursor_session_name().as_deref(), Some("a"));
        assert!(state.is_expanded("a"), "focused session expands");
        state.focus_session_number(1); // -> c
        assert_eq!(state.cursor_session_name().as_deref(), Some("c"));
        assert!(state.is_expanded("c"), "focused session expands");

        // Zero and out-of-range are no-ops (cursor stays put).
        state.focus_session_number(0);
        assert_eq!(state.cursor_session_name().as_deref(), Some("c"));
        state.focus_session_number(9);
        assert_eq!(state.cursor_session_name().as_deref(), Some("c"));

        // Focusing does not switch or dirty state.
        assert!(!state.dirty);
    }

    #[test]
    fn sort_key_parses_manual_and_cycles() {
        assert_eq!(SortKey::from_config_str("manual"), SortKey::Manual);
        assert_eq!(SortKey::Activity.next(), SortKey::Created);
        assert_eq!(SortKey::Created.next(), SortKey::Manual);
        assert_eq!(SortKey::Manual.next(), SortKey::Activity);
    }

    #[test]
    fn ordered_manual_empty_list_is_created_ascending() {
        // No manual placements yet: ungrouped read oldest -> newest (created asc),
        // so a freshly created session naturally lands at the bottom.
        let sessions = vec![s("a", 99, 3), s("b", 99, 1), s("c", 99, 2)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Manual };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["b", "c", "a"]); // created 1, 2, 3
    }

    #[test]
    fn ordered_manual_lists_then_remaining_excluding_pinned() {
        let sessions = vec![s("a", 1, 10), s("b", 1, 20), s("c", 1, 30), s("d", 1, 40)];
        // d is in a PINNED group (and also wrongly listed in manual_order to prove it is
        // filtered out of the manual tail); c then a are the manual placements;
        // b is unlisted and falls in after, by created asc.
        let cfg = Config {
            groups: vec![Group { name: "PINNED".into(), members: vec!["d".into()], color: String::new() }],
            manual_order: vec!["d".into(), "c".into(), "a".into()],
            sort: SortKey::Manual,
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
            groups: vec![],
            manual_order: vec!["mid".into(), "old".into()],
            sort: SortKey::Manual,
        };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["mid", "old", "x"]);
    }

    #[test]
    fn move_row_unpinned_in_manual_freezes_then_swaps_and_dirties() {
        // Manual + empty list => base order is created asc: a(1), b(2), c(3).
        let sessions = vec![s("a", 9, 1), s("b", 9, 2), s("c", 9, 3)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Manual };
        let mut state = PickerState::build(sessions, &cfg);

        state.focus_session("b");
        state.move_row(-1); // move b up past a
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["b", "a", "c"]);
        // The full ungrouped order is frozen into manual_order on the first move.
        assert_eq!(
            state.manual_order,
            vec!["b".to_string(), "a".to_string(), "c".to_string()]
        );
        assert!(state.dirty);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));

        // Moving up at the top is a clamped no-op.
        state.dirty = false;
        state.move_row(-1);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
        assert!(!state.dirty);
    }

    #[test]
    fn move_row_unpinned_is_noop_outside_manual_mode() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("b");
        state.move_row(1);
        assert!(!state.dirty);
        assert!(state.manual_order.is_empty());
    }

    #[test]
    fn move_row_on_pinned_reorders_pins_in_any_mode() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config {
            groups: vec![Group { name: "PINNED".into(), members: vec!["a".into(), "b".into()], color: String::new() }],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("a");
        state.move_row(1);
        assert_eq!(state.groups[0].members, vec!["b".to_string(), "a".to_string()]);
        assert!(state.dirty);
    }

    #[test]
    fn cycle_sort_advances_mode_and_dirties() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg);
        state.cycle_sort();
        assert_eq!(state.sort, SortKey::Created);
        assert!(state.dirty);
        state.cycle_sort();
        assert_eq!(state.sort, SortKey::Manual);
        state.cycle_sort();
        assert_eq!(state.sort, SortKey::Activity);
    }

    #[test]
    fn default_mode_is_command() {
        let sessions = vec![s("a", 30, 1)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);
        assert_eq!(state.mode, Mode::Command);
        assert!(state.query.is_empty());
    }

    #[test]
    fn search_results_empty_query_is_normal_order() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let state = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = state.search_results().iter().map(|s| s.name.as_str()).collect();
        // Same as ordered(): activity desc -> b, a
        assert_eq!(names, vec!["b", "a"]);
    }

    #[test]
    fn search_results_filters_and_ranks_by_query() {
        // "prr" matches pr-review (p,r,-,r) tightly and provision not at all
        // (only one 'r'), so pr-review must rank first and scratch is excluded.
        let sessions = vec![s("provision", 1, 1), s("pr-review", 1, 2), s("scratch", 1, 3)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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
    fn query_change_resets_to_top_and_move_clamps() {
        let sessions = vec![s("alpha", 1, 1), s("alto", 1, 2), s("alarm", 1, 3)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_push('a');
        state.search_push('l');

        state.search_move(1);
        state.search_push('a'); // query changed -> back to top
        assert_eq!(state.search_cursor(), 0, "query change resets to top");

        // Clamp at the bottom: a big move never exceeds the last match.
        state.search_move(99);
        let n = state.search_results().len();
        assert!(state.search_cursor() < n.max(1));
    }

    #[test]
    fn search_selected_action_switches_to_highlighted() {
        let sessions = vec![s("provision", 1, 1), s("pr-review", 1, 2)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
        let mut state = PickerState::build(sessions, &cfg);
        state.enter_search();
        state.search_push('z');
        state.search_push('z');
        assert_eq!(state.search_selected_action(), None);
    }

    #[test]
    fn search_backspace_shrinks_query_and_clears_to_empty() {
        let sessions = vec![s("api-gateway", 30, 1), s("web", 20, 2)];
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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
        let cfg = Config { groups: vec![], manual_order: vec![], sort: SortKey::Activity };
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

    fn grouped_state() -> PickerState {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("c", 1, 3)];
        let cfg = Config {
            groups: vec![
                Group { name: "G1".into(), members: vec!["a".into()], color: String::new() },
                Group { name: "G2".into(), members: vec!["b".into()], color: String::new() },
            ],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
        PickerState::build(sessions, &cfg)
    }

    #[test]
    fn group_new_appends_empty_and_starts_rename() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_new();
        assert_eq!(st.groups.len(), 3);
        assert_eq!(st.groups[2].name, "");
        assert!(st.groups[2].members.is_empty());
        assert_eq!(st.group_cursor(), 2);
        assert!(st.group_editing());
        for c in "TOOLS".chars() { st.group_edit_push(c); }
        st.group_commit_rename();
        assert_eq!(st.groups[2].name, "TOOLS");
        assert!(!st.group_editing());
        assert!(st.dirty);
    }

    #[test]
    fn group_new_then_cancel_discards() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_new();
        st.group_cancel_rename();
        assert_eq!(st.groups.len(), 2);
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
    fn group_delete_spills_members_to_residual() {
        let mut st = grouped_state();
        st.enter_groups(); // cursor on G1 (member a)
        st.group_delete();
        assert_eq!(st.groups.len(), 1);
        assert_eq!(st.groups[0].name, "G2");
        assert!(!st.is_grouped("a")); // a fell into the residual
        assert!(st.dirty);
    }

    #[test]
    fn residual_count_excludes_grouped() {
        let st = grouped_state(); // a,b grouped; c residual
        assert_eq!(st.residual_count(), 1);
    }

    #[test]
    fn group_new_leaves_color_positional_and_cycle_pins_explicit() {
        let mut st = grouped_state();
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
    fn enter_and_exit_groups_toggles_mode() {
        let mut st = grouped_state();
        assert_eq!(st.mode, Mode::Command);
        st.enter_groups();
        assert_eq!(st.mode, Mode::Groups);
        st.exit_groups();
        assert_eq!(st.mode, Mode::Command);
    }

    fn state_with_two_groups() -> PickerState {
        // groups: G1=[a,b], G2=[c]; residual d,e by activity (d 40 > e 30)
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("c", 1, 3), s("d", 40, 4), s("e", 30, 5)];
        let cfg = Config {
            groups: vec![
                Group { name: "G1".into(), members: vec!["a".into(), "b".into()], color: String::new() },
                Group { name: "G2".into(), members: vec!["c".into()], color: String::new() },
            ],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
        PickerState::build(sessions, &cfg)
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
    fn move_up_at_very_top_clamps() {
        let mut st = state_with_two_groups();
        st.focus_session("a"); // top of first group
        st.move_row(-1);
        assert_eq!(st.groups[0].members, vec!["a".to_string(), "b".to_string()]);
        assert!(!st.dirty);
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
        assert!(!st.is_grouped("c"));
    }

    #[test]
    fn move_up_from_residual_top_joins_last_group() {
        let mut st = state_with_two_groups();
        st.focus_session("d"); // residual top (activity 40)
        st.move_row(-1);
        assert_eq!(st.groups[1].members, vec!["c".to_string(), "d".to_string()]);
        assert!(st.is_grouped("d"));
    }

    #[test]
    fn move_down_at_residual_bottom_clamps() {
        let mut st = state_with_two_groups();
        st.focus_session("e"); // residual bottom
        st.move_row(1);
        assert!(!st.is_grouped("e"));
        assert!(!st.dirty);
    }

    #[test]
    fn ordered_group_ids_track_sections() {
        let st = state_with_two_groups(); // G1=[a,b], G2=[c], residual d,e
        assert_eq!(
            st.ordered_group_ids(),
            vec![Some(0), Some(0), Some(1), None, None]
        );
    }
}
