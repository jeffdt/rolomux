#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DefaultMode {
    #[default]
    Command,
    Search,
}

impl DefaultMode {
    pub fn from_config_str(s: &str) -> DefaultMode {
        match s {
            "search" => DefaultMode::Search,
            _ => DefaultMode::Command,
        }
    }

    pub fn as_config_str(self) -> &'static str {
        match self {
            DefaultMode::Command => "command",
            DefaultMode::Search => "search",
        }
    }

    /// The other value. A 2-state cycle, so `h`, `l`, and `Enter`/`Space` on
    /// the Default Mode settings row all call this: there is no distinct
    /// "previous".
    pub fn next(self) -> DefaultMode {
        match self {
            DefaultMode::Command => DefaultMode::Search,
            DefaultMode::Search => DefaultMode::Command,
        }
    }

    pub fn as_mode(self) -> Mode {
        match self {
            DefaultMode::Command => Mode::Command,
            DefaultMode::Search => Mode::Search,
        }
    }
}

/// Governs the header color assigned when a new group is created
/// (`PickerState::group_new`). Never retroactively recolors existing groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorPolicy {
    #[default]
    Rotate,
    Random,
    Static,
}

impl ColorPolicy {
    pub fn from_config_str(s: &str) -> ColorPolicy {
        match s {
            "random" => ColorPolicy::Random,
            "static" => ColorPolicy::Static,
            _ => ColorPolicy::Rotate,
        }
    }

    pub fn as_config_str(self) -> &'static str {
        match self {
            ColorPolicy::Rotate => "rotate",
            ColorPolicy::Random => "random",
            ColorPolicy::Static => "static",
        }
    }

    pub fn next(self) -> ColorPolicy {
        match self {
            ColorPolicy::Rotate => ColorPolicy::Random,
            ColorPolicy::Random => ColorPolicy::Static,
            ColorPolicy::Static => ColorPolicy::Rotate,
        }
    }

    pub fn prev(self) -> ColorPolicy {
        match self {
            ColorPolicy::Rotate => ColorPolicy::Static,
            ColorPolicy::Random => ColorPolicy::Rotate,
            ColorPolicy::Static => ColorPolicy::Random,
        }
    }
}

/// All 16 named ANSI terminal colors (never RGB), in a fixed canonical order.
/// Backs the settings palette checklist and the Static-policy color cycle.
pub const ALL_NAMED_COLORS: [&str; 16] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "gray",
    "darkgray", "lightred", "lightgreen", "lightyellow", "lightblue",
    "lightmagenta", "lightcyan", "white",
];

/// Where the cursor starts when the picker opens. This is a swappable seam:
/// change the single `INITIAL_FOCUS` constant below to pick a policy without
/// touching `build`.
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
/// full-screen group-management overlay; `Settings` is the full-screen
/// settings overlay. Which mode the picker launches in is governed by the
/// persisted `default_mode` preference (`Config::default_mode`, of type
/// `DefaultMode`), read once at `build` time -- see `DefaultMode::as_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Command,
    Search,
    Groups,
    Settings,
}

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

/// The default active color palette, seeded into a fresh `Config` when no
/// `[settings]` table is present on disk (`store::Config::default`). Also
/// the historical positional-default order. Named ANSI colors only (never
/// RGB) so headers inherit the user's terminal theme.
pub const HEADER_COLORS: [&str; 6] = ["cyan", "green", "yellow", "magenta", "blue", "red"];

/// A user-named, ordered collection of sessions that renders as its own
/// section. One group is always the inbox (see `inbox` below), which
/// receives sessions not explicitly listed anywhere else. Groups are
/// durable: they persist even when empty and are removed only by an
/// explicit delete (the inbox group can't be deleted at all).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub members: Vec<String>,
    /// Header color name from `HEADER_COLORS`, or empty for the positional
    /// default. Set explicitly by the color-flip so it survives reordering.
    pub color: String,
    /// Whether this is the one group that receives sessions not explicitly
    /// listed anywhere else. Exactly one group has this set to `true` at
    /// all times -- see `ensure_single_inbox`. Never toggled by any UI
    /// action other than migration/repair: there is no "make this the
    /// inbox" command.
    pub inbox: bool,
}

/// Guarantees "exactly one group has `inbox: true`" after loading or
/// building from any source. If none do, always synthesizes and appends a
/// fresh empty `INBOX` group -- never repurposes an existing named group,
/// since silently flipping someone's real group would be a far more
/// surprising repair than adding an empty one. If more than one do (only
/// reachable via a hand-edited TOML), keeps the first and clears the rest.
pub fn ensure_single_inbox(groups: &mut Vec<Group>) {
    let flagged: Vec<usize> = groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.inbox)
        .map(|(i, _)| i)
        .collect();
    match flagged.len() {
        0 => groups.push(Group { name: "INBOX".into(), inbox: true, ..Default::default() }),
        1 => {}
        _ => {
            for &i in &flagged[1..] {
                groups[i].inbox = false;
            }
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRow {
    DefaultMode,
    AttachedColor,
    /// Index into `ALL_NAMED_COLORS`.
    AttachedColorOption(usize),
    BorderColor,
    /// Index into `ALL_NAMED_COLORS`.
    BorderColorOption(usize),
    ColorPolicy,
    Palette,
    /// Index into `PickerState::settings_palette_rows()`'s display order.
    PaletteColor(usize),
}

use crate::store::Config;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct PickerState {
    all: Vec<Session>,
    pub groups: Vec<Group>,
    expanded: HashSet<String>,
    dormant: HashSet<String>,
    hide_dormant: bool,
    pub cursor: usize,
    pub dirty: bool,
    pub mode: Mode,
    pub query: String,
    search_cursor: usize,
    /// Cursor position within the group list in `Mode::Groups`.
    pub group_cursor: usize,
    /// In-flight rename buffer; `Some` while a rename is in progress.
    pub group_edit: Option<String>,
    pub default_mode: DefaultMode,
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
        let mut state = PickerState {
            all: sessions,
            groups,
            expanded: HashSet::new(),
            dormant: config.dormant.iter().cloned().collect(),
            hide_dormant: config.hide_dormant,
            cursor: 0,
            dirty: false,
            mode: config.default_mode.as_mode(),
            query: String::new(),
            search_cursor: 0,
            group_cursor: 0,
            group_edit: None,
            default_mode: config.default_mode,
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
        if self.hide_dormant {
            out.retain(|s| !self.dormant.contains(&s.name));
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
        }
    }

    pub fn collapse(&mut self) {
        if let Some(name) = self.cursor_session_name() {
            self.expanded.remove(&name);
            self.focus_session(&name);
        }
    }

    /// Whether `name` is marked dormant. When dormant sessions are shown, they
    /// are dimmed but otherwise fully normal; `hide_dormant` is the only filter
    /// that removes them from the picker.
    pub fn is_dormant(&self, name: &str) -> bool {
        self.dormant.contains(name)
    }

    pub fn hiding_dormant(&self) -> bool {
        self.hide_dormant
    }

    pub fn dormant_count(&self) -> usize {
        self.all.iter().filter(|s| self.is_dormant(&s.name)).count()
    }

    pub fn hidden_dormant_count(&self) -> usize {
        if self.hide_dormant { self.dormant_count() } else { 0 }
    }

    fn session_visible(&self, name: &str) -> bool {
        !self.hide_dormant || !self.is_dormant(name)
    }

    /// Toggle whether dormant sessions are hidden from the picker. The filter
    /// is persisted as a preference so it survives closing and reopening the
    /// popup, same as the dormant set itself.
    pub fn toggle_dormant_visibility(&mut self) {
        let command_focus = self.cursor_session_name();
        let search_focus = self.search_cursor_name();
        self.hide_dormant = !self.hide_dormant;
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

    /// Move the session under the cursor by `delta` rows, crossing group
    /// boundaries into the group above/below when at an edge -- the inbox
    /// group included, since it's just another entry in `self.groups` now.
    /// Clamps silently at the very top and bottom of the whole list.
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

    fn move_up(&mut self, gi: usize, name: &str) {
        let order = self.effective_order(gi);
        let pos = match order.iter().position(|n| n == name) { Some(p) => p, None => return };
        if let Some(prev) = self.previous_visible_position(&order, pos) {
            self.commit_swap(gi, order, pos, prev, name);
        } else if gi > 0 {
            self.groups[gi].members.retain(|m| m != name);
            self.groups[gi - 1].members.push(name.to_string());
            self.dirty = true;
            self.focus_session(name);
        }
        // else: top of the whole list, clamp silently.
    }

    fn move_down(&mut self, gi: usize, name: &str) {
        let order = self.effective_order(gi);
        let pos = match order.iter().position(|n| n == name) { Some(p) => p, None => return };
        if let Some(next) = self.next_visible_position(&order, pos) {
            self.commit_swap(gi, order, pos, next, name);
        } else if gi + 1 < self.groups.len() {
            self.groups[gi].members.retain(|m| m != name);
            self.groups[gi + 1].members.insert(0, name.to_string());
            self.dirty = true;
            self.focus_session(name);
        }
        // else: bottom of the whole list, clamp silently.
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

    /// Enter the full-screen settings overlay.
    pub fn enter_settings(&mut self) {
        self.mode = Mode::Settings;
    }

    /// Leave settings mode back to session command mode.
    pub fn exit_settings(&mut self) {
        self.mode = Mode::Command;
    }

    /// The current cursor position within the settings rows (Task 4).
    pub fn settings_cursor(&self) -> usize {
        self.settings_cursor
    }

    /// Whether the color-palette checklist is currently expanded (Task 4).
    pub fn palette_expanded(&self) -> bool {
        self.palette_expanded
    }

    /// Whether the Attached session color picker is currently expanded.
    pub fn attached_color_expanded(&self) -> bool {
        self.attached_color_expanded
    }

    /// Whether the Border color picker is currently expanded.
    pub fn border_color_expanded(&self) -> bool {
        self.border_color_expanded
    }

    /// The flat, ordered list of settings rows currently on screen. Three
    /// expandable sections (Attached session color, Border color, Color
    /// palette) each splice their child rows in directly below themselves
    /// while expanded, same shape as the original Palette/PaletteColor
    /// pattern.
    pub fn settings_visible_rows(&self) -> Vec<SettingsRow> {
        let mut rows = vec![SettingsRow::DefaultMode, SettingsRow::AttachedColor];
        if self.attached_color_expanded {
            for i in 0..ALL_NAMED_COLORS.len() {
                rows.push(SettingsRow::AttachedColorOption(i));
            }
        }
        rows.push(SettingsRow::BorderColor);
        if self.border_color_expanded {
            for i in 0..ALL_NAMED_COLORS.len() {
                rows.push(SettingsRow::BorderColorOption(i));
            }
        }
        rows.push(SettingsRow::ColorPolicy);
        rows.push(SettingsRow::Palette);
        if self.palette_expanded {
            for i in 0..ALL_NAMED_COLORS.len() {
                rows.push(SettingsRow::PaletteColor(i));
            }
        }
        rows
    }

    /// All 16 named colors in fixed `ALL_NAMED_COLORS` canonical order, each
    /// paired with whether it's currently active. The order never changes as
    /// colors are toggled, so checking/unchecking a color never reshuffles
    /// the list.
    pub fn settings_palette_rows(&self) -> Vec<(String, bool)> {
        ALL_NAMED_COLORS
            .iter()
            .map(|name| (name.to_string(), self.active_palette.iter().any(|c| c == name)))
            .collect()
    }

    /// Move the settings cursor by `delta`, wrapping between the first and last row.
    pub fn settings_move_cursor(&mut self, delta: i32) {
        self.settings_cursor = move_index_with_edge_wrap(
            self.settings_cursor,
            delta,
            self.settings_visible_rows().len(),
        );
    }

    /// The settings row the cursor currently sits on.
    fn current_settings_row(&self) -> SettingsRow {
        let rows = self.settings_visible_rows();
        rows[self.settings_cursor.min(rows.len().saturating_sub(1))]
    }

    /// Place the cursor on `target`, found by scanning a freshly rebuilt
    /// `settings_visible_rows()`. Needed because expanding one section can
    /// shift every row below it by up to 16 positions, so a fixed index (as
    /// the codebase used before this section existed) is no longer safe.
    /// Falls back to row 0 if `target` isn't present (should not happen for
    /// any caller here, but never panics).
    fn focus_settings_row(&mut self, target: SettingsRow) {
        let rows = self.settings_visible_rows();
        self.settings_cursor = rows.iter().position(|r| *r == target).unwrap_or(0);
    }

    /// Expand the Attached session color picker with the cursor starting on
    /// the currently selected color, not row 0 -- opening the picker always
    /// lands on the current value, like a standard radio picker.
    fn expand_attached_color(&mut self) {
        self.attached_color_expanded = true;
        let idx = ALL_NAMED_COLORS.iter().position(|c| *c == self.attached_color).unwrap_or(0);
        self.focus_settings_row(SettingsRow::AttachedColorOption(idx));
    }

    /// Same as `expand_attached_color`, for Border color.
    fn expand_border_color(&mut self) {
        self.border_color_expanded = true;
        let idx = ALL_NAMED_COLORS.iter().position(|c| *c == self.border_color).unwrap_or(0);
        self.focus_settings_row(SettingsRow::BorderColorOption(idx));
    }

    /// Commit `idx` as the new attached-session color, collapse, and return
    /// the cursor to the parent row.
    fn select_attached_color(&mut self, idx: usize) {
        self.attached_color = ALL_NAMED_COLORS[idx].to_string();
        self.attached_color_expanded = false;
        self.dirty = true;
        self.focus_settings_row(SettingsRow::AttachedColor);
    }

    /// Same as `select_attached_color`, for Border color.
    fn select_border_color(&mut self, idx: usize) {
        self.border_color = ALL_NAMED_COLORS[idx].to_string();
        self.border_color_expanded = false;
        self.dirty = true;
        self.focus_settings_row(SettingsRow::BorderColor);
    }

    /// `h` on the current settings row: step Default Mode / Color Policy
    /// backward, collapse an expanded section, or (from inside an expanded
    /// section's child row) cancel -- collapse without changing the value --
    /// and jump back to that section's parent row.
    pub fn settings_step_left(&mut self) {
        match self.current_settings_row() {
            SettingsRow::DefaultMode => {
                self.default_mode = self.default_mode.next();
                self.dirty = true;
            }
            SettingsRow::AttachedColor => self.attached_color_expanded = false,
            SettingsRow::AttachedColorOption(_) => {
                self.attached_color_expanded = false;
                self.focus_settings_row(SettingsRow::AttachedColor);
            }
            SettingsRow::BorderColor => self.border_color_expanded = false,
            SettingsRow::BorderColorOption(_) => {
                self.border_color_expanded = false;
                self.focus_settings_row(SettingsRow::BorderColor);
            }
            SettingsRow::ColorPolicy => {
                self.new_group_color_policy = self.new_group_color_policy.prev();
                self.dirty = true;
            }
            SettingsRow::Palette => self.palette_expanded = false,
            SettingsRow::PaletteColor(_) => {
                self.palette_expanded = false;
                self.focus_settings_row(SettingsRow::Palette);
            }
        }
    }

    /// `l` on the current settings row: step Default Mode / Color Policy
    /// forward, or expand a section (Attached session color, Border color,
    /// Color palette). A no-op on an already-expanded section's child row --
    /// there is nothing further to expand, and selection there happens via
    /// `Enter`/`Space` (`settings_activate`), not `l`.
    pub fn settings_step_right(&mut self) {
        match self.current_settings_row() {
            SettingsRow::DefaultMode => {
                self.default_mode = self.default_mode.next();
                self.dirty = true;
            }
            SettingsRow::AttachedColor => self.expand_attached_color(),
            SettingsRow::AttachedColorOption(_) => {}
            SettingsRow::BorderColor => self.expand_border_color(),
            SettingsRow::BorderColorOption(_) => {}
            SettingsRow::ColorPolicy => {
                self.new_group_color_policy = self.new_group_color_policy.next();
                self.dirty = true;
            }
            SettingsRow::Palette => self.palette_expanded = true,
            SettingsRow::PaletteColor(_) => {}
        }
    }

    /// `Enter`/`Space` on the current settings row: steps Default Mode /
    /// Color Policy forward (same as `l`), expands a collapsed color section
    /// (same as `l`), commits the color under the cursor on an expanded
    /// section's child row (radio-select: pick one, collapse), or toggles a
    /// palette color's active state (checkbox: pick many, stays expanded).
    pub fn settings_activate(&mut self) {
        match self.current_settings_row() {
            SettingsRow::DefaultMode | SettingsRow::ColorPolicy => self.settings_step_right(),
            SettingsRow::AttachedColor => self.expand_attached_color(),
            SettingsRow::AttachedColorOption(idx) => self.select_attached_color(idx),
            SettingsRow::BorderColor => self.expand_border_color(),
            SettingsRow::BorderColorOption(idx) => self.select_border_color(idx),
            SettingsRow::Palette => {}
            SettingsRow::PaletteColor(_) => self.settings_toggle_palette_color(),
        }
    }

    /// The palette-checklist index under the cursor, if the cursor is
    /// currently on a `PaletteColor` row. Shared by the palette mutation
    /// methods below so they resolve "which color" the same way `h`/`l`/`Enter`
    /// resolve "which row".
    fn current_palette_color_idx(&self) -> Option<usize> {
        match self.current_settings_row() {
            SettingsRow::PaletteColor(i) => Some(i),
            _ => None,
        }
    }

    /// Toggle the palette color under the cursor active/inactive. `active_palette`
    /// is kept in `ALL_NAMED_COLORS` canonical order on every mutation, so
    /// rotation/cycle order always matches the checklist's fixed display order
    /// and toggling a color never reshuffles the list. Guarded: the last
    /// active color can never be deactivated (several resolution paths
    /// divide/index by `active_palette.len()`).
    fn settings_toggle_palette_color(&mut self) {
        let idx = match self.current_palette_color_idx() {
            Some(i) => i,
            None => return,
        };
        let name = ALL_NAMED_COLORS[idx];
        if self.active_palette.iter().any(|c| c == name) {
            if self.active_palette.len() <= 1 {
                return;
            }
            self.active_palette.retain(|c| c != name);
        } else {
            self.active_palette.push(name.to_string());
            self.active_palette
                .sort_by_key(|c| ALL_NAMED_COLORS.iter().position(|n| n == c).unwrap_or(usize::MAX));
        }
        self.dirty = true;
    }

    /// Step `current` forward one position through `ALL_NAMED_COLORS`,
    /// wrapping from white back to black. Shared by every raw single-color
    /// cycle in Settings so the wrap-around index logic lives in exactly one
    /// place.
    fn cycle_named_color(current: &str) -> String {
        let idx = ALL_NAMED_COLORS.iter().position(|c| *c == current).unwrap_or(0);
        ALL_NAMED_COLORS[(idx + 1) % ALL_NAMED_COLORS.len()].to_string()
    }

    /// `c`: cycle the current row's raw color value forward through all 16
    /// named colors. Applies to the Color Policy row only while its policy
    /// is Static (the nested `static_color`), and to the two standalone
    /// color rows (`attached_color`, `border_color`) whether collapsed or
    /// expanded. A no-op everywhere else, so `c` never surprises a row that
    /// isn't a raw color picker.
    pub fn settings_cycle_color(&mut self) {
        match self.current_settings_row() {
            SettingsRow::ColorPolicy if self.new_group_color_policy == ColorPolicy::Static => {
                self.static_color = Self::cycle_named_color(&self.static_color);
                self.dirty = true;
            }
            SettingsRow::AttachedColor => {
                self.attached_color = Self::cycle_named_color(&self.attached_color);
                self.dirty = true;
            }
            SettingsRow::BorderColor => {
                self.border_color = Self::cycle_named_color(&self.border_color);
                self.dirty = true;
            }
            _ => {}
        }
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
    pub fn group_reorder(&mut self, delta: i32) {
        let gc = self.group_cursor;
        let target = gc as i32 + delta;
        if target < 0 || target >= self.groups.len() as i32 { return; }
        self.groups.swap(gc, target as usize);
        self.group_cursor = target as usize;
        self.dirty = true;
    }

    /// Append a new empty group after the last named group and begin naming it.
    /// The header color is resolved from the current new-group-color policy:
    /// Rotate leaves it unset (positional default, resolved at render/cycle
    /// time from the live active palette); Random picks once now from the
    /// active palette; Static uses the configured static color. Neither Random
    /// nor Static retroactively touch any other group.
    pub fn group_new(&mut self) {
        let color = match self.new_group_color_policy {
            ColorPolicy::Rotate => String::new(),
            ColorPolicy::Random => pick_random_color(&self.active_palette, random_seed()),
            ColorPolicy::Static => self.static_color.clone(),
        };
        self.groups.push(Group { name: String::new(), members: Vec::new(), color, inbox: false });
        self.group_cursor = self.groups.len() - 1;
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
    fn default_mode_parses_with_command_fallback() {
        assert_eq!(DefaultMode::from_config_str("search"), DefaultMode::Search);
        assert_eq!(DefaultMode::from_config_str("command"), DefaultMode::Command);
        assert_eq!(DefaultMode::from_config_str("garbage"), DefaultMode::Command);
        assert_eq!(DefaultMode::default(), DefaultMode::Command);
    }

    #[test]
    fn default_mode_next_toggles_and_maps_to_mode() {
        assert_eq!(DefaultMode::Command.next(), DefaultMode::Search);
        assert_eq!(DefaultMode::Search.next(), DefaultMode::Command);
        assert_eq!(DefaultMode::Command.as_mode(), Mode::Command);
        assert_eq!(DefaultMode::Search.as_mode(), Mode::Search);
        assert_eq!(DefaultMode::Command.as_config_str(), "command");
        assert_eq!(DefaultMode::Search.as_config_str(), "search");
    }

    #[test]
    fn color_policy_parses_with_rotate_fallback() {
        assert_eq!(ColorPolicy::from_config_str("random"), ColorPolicy::Random);
        assert_eq!(ColorPolicy::from_config_str("static"), ColorPolicy::Static);
        assert_eq!(ColorPolicy::from_config_str("rotate"), ColorPolicy::Rotate);
        assert_eq!(ColorPolicy::from_config_str("garbage"), ColorPolicy::Rotate);
        assert_eq!(ColorPolicy::default(), ColorPolicy::Rotate);
    }

    #[test]
    fn color_policy_cycles_forward_and_backward() {
        assert_eq!(ColorPolicy::Rotate.next(), ColorPolicy::Random);
        assert_eq!(ColorPolicy::Random.next(), ColorPolicy::Static);
        assert_eq!(ColorPolicy::Static.next(), ColorPolicy::Rotate);
        assert_eq!(ColorPolicy::Rotate.prev(), ColorPolicy::Static);
        assert_eq!(ColorPolicy::Static.prev(), ColorPolicy::Random);
        assert_eq!(ColorPolicy::Random.prev(), ColorPolicy::Rotate);
        assert_eq!(ColorPolicy::Rotate.as_config_str(), "rotate");
        assert_eq!(ColorPolicy::Random.as_config_str(), "random");
        assert_eq!(ColorPolicy::Static.as_config_str(), "static");
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
    fn focus_session_number_moves_cursor_without_switching() {
        let sessions = vec![s("a", 10, 1), s("b", 30, 2), s("c", 20, 3)];
        let cfg = Config {
            dormant: vec![], groups: vec![Group { name: "PINNED".into(), members: vec!["c".into()], color: String::new(), ..Default::default() }],
            ..Default::default()
        };
        let mut state = PickerState::build(sessions, &cfg); // order: c, a, b (a/b unranked by created asc)

        state.focus_session_number(3); // -> b
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
        assert!(state.is_expanded("b"), "focused session expands");
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

        // Moving up at the top is a clamped no-op.
        state.dirty = false;
        state.move_row(-1);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
        assert!(!state.dirty);
    }

    #[test]
    fn move_row_unpinned_at_residual_bottom_is_noop() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("b");
        state.move_row(1);
        assert!(!state.dirty);
        assert!(state.groups[state.inbox_index().unwrap()].members.is_empty());
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
    fn hide_dormant_loads_from_config() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config {
            groups: vec![],
            dormant: vec!["a".into()],
            hide_dormant: true,
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert!(state.hiding_dormant());
        assert_eq!(state.hidden_dormant_count(), 1);
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["b"]);
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
    fn toggle_dormant_visibility_filters_command_and_search_and_dirties() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2), s("gamma", 1, 3)];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);

        assert_eq!(state.dormant_count(), 1);
        let shown: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(shown, vec!["alpha", "beta", "gamma"]);

        state.toggle_dormant_visibility();
        assert!(state.hiding_dormant());
        assert_eq!(state.hidden_dormant_count(), 1);
        assert!(state.dirty, "hiding dormant sessions persists the preference");
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["alpha", "gamma"]);

        state.enter_search();
        state.search_push('b');
        assert!(state.search_results().is_empty(), "hidden dormant sessions are absent from search");
        state.search_clear();
        let search_visible: Vec<&str> = state.search_results().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(search_visible, vec!["alpha", "gamma"]);

        state.toggle_dormant_visibility();
        assert!(!state.hiding_dormant());
        let restored: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(restored, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn hiding_dormant_clamps_cursor_when_selected_session_disappears() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        let cfg = Config { groups: vec![], dormant: vec!["beta".into()], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("beta");
        assert_eq!(state.cursor_session_name().as_deref(), Some("beta"));

        state.toggle_dormant_visibility();

        assert_eq!(state.cursor_session_name().as_deref(), Some("alpha"));
        assert_eq!(state.visible_rows().len(), 1);
    }

    #[test]
    fn toggling_dormant_while_filter_is_active_hides_the_session() {
        let sessions = vec![s("alpha", 1, 1), s("beta", 1, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.toggle_dormant_visibility();
        assert_eq!(state.cursor_session_name().as_deref(), Some("alpha"));

        state.toggle_dormant();

        assert!(state.is_dormant("alpha"));
        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["beta"]);
        assert_eq!(state.cursor_session_name().as_deref(), Some("beta"));
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
        state.toggle_dormant_visibility();
        state.focus_session("alpha");

        state.move_row(1);

        let visible: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(visible, vec!["gamma", "alpha"]);
        let all_members = &state.groups[state.inbox_index().unwrap()].members;
        assert_eq!(all_members, &vec!["gamma".to_string(), "beta".to_string(), "alpha".to_string()]);
    }

    fn grouped_state() -> PickerState {
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

    #[test]
    fn group_new_appends_empty_and_starts_rename() {
        let mut st = grouped_state();
        st.enter_groups();
        st.group_new();
        assert_eq!(st.groups.len(), 4);
        assert_eq!(st.groups[3].name, "");
        assert!(st.groups[3].members.is_empty());
        assert_eq!(st.group_cursor(), 3);
        assert!(st.group_editing());
        for c in "TOOLS".chars() { st.group_edit_push(c); }
        st.group_commit_rename();
        assert_eq!(st.groups[3].name, "TOOLS");
        assert!(!st.group_editing());
        assert!(st.dirty);
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
        st.enter_groups();
        st.group_new(); // empty color -> positional default (HEADER_COLORS[index])
        assert!(st.groups[3].color.is_empty(), "new group defaults to positional color");

        st.dirty = false;
        // Cursor is on the new group (index 3); its positional color is
        // HEADER_COLORS[3] ("magenta"), so a flip advances to "blue".
        st.group_cycle_color();
        assert_eq!(st.groups[3].color, "blue");
        assert!(st.dirty, "flipping a color dirties state");

        // Cycling wraps around the palette back to the start.
        st.groups[3].color = HEADER_COLORS[HEADER_COLORS.len() - 1].to_string();
        st.group_cycle_color();
        assert_eq!(st.groups[3].color, HEADER_COLORS[0]);
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
            new_group_color_policy: ColorPolicy::Static,
            static_color: "red".to_string(),
            active_palette: vec!["red".to_string(), "white".to_string()],
            ..Default::default()
        };
        let state = PickerState::build(sessions, &cfg);
        assert_eq!(state.mode, Mode::Search, "startup mode follows config.default_mode");
        assert_eq!(state.default_mode, DefaultMode::Search);
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
    fn enter_and_exit_settings_toggles_mode() {
        let mut st = grouped_state();
        assert_eq!(st.mode, Mode::Command);
        st.enter_settings();
        assert_eq!(st.mode, Mode::Settings);
        st.exit_settings();
        assert_eq!(st.mode, Mode::Command);
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
    fn ordered_places_unassigned_sessions_inside_inbox_block_wherever_it_sits() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("new", 1, 3)];
        // Inbox is first in `groups` (as if the user dragged it to the top);
        // WORK is second. "new" is never explicitly listed anywhere.
        let cfg = Config {
            groups: vec![
                Group { name: "INBOX".into(), members: vec!["b".into()], inbox: true, ..Default::default() },
                Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() },
            ],
            ..Default::default()
        };
        let st = PickerState::build(sessions, &cfg);
        let names: Vec<&str> = st.ordered().iter().map(|s| s.name.as_str()).collect();
        // "new" renders right after "b" (inbox's own block), not at the very
        // end of the whole list after WORK.
        assert_eq!(names, vec!["b", "new", "a"]);
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

    fn state_with_two_groups() -> PickerState {
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
    fn move_down_at_residual_bottom_clamps() {
        let mut st = state_with_two_groups();
        st.focus_session("e"); // residual bottom
        st.move_row(1);
        assert_eq!(st.group_index_of("e"), st.inbox_index());
        assert!(!st.dirty);
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

    fn settings_state() -> PickerState {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default();
        PickerState::build(sessions, &cfg)
    }

    #[test]
    fn settings_palette_rows_lists_all_sixteen_in_fixed_canonical_order() {
        let st = settings_state(); // default active_palette = HEADER_COLORS: cyan,green,yellow,magenta,blue,red
        let rows = st.settings_palette_rows();
        assert_eq!(rows.len(), 16);
        // Order never changes with active/inactive status: it's always
        // ALL_NAMED_COLORS canonical order, so a toggle never reshuffles it.
        assert_eq!(&rows[0], &("black".to_string(), false));
        assert_eq!(&rows[1], &("red".to_string(), true));
        assert_eq!(&rows[2], &("green".to_string(), true));
        assert_eq!(&rows[3], &("yellow".to_string(), true));
        assert_eq!(&rows[4], &("blue".to_string(), true));
        assert_eq!(&rows[5], &("magenta".to_string(), true));
        assert_eq!(&rows[6], &("cyan".to_string(), true));
        assert_eq!(&rows[7], &("gray".to_string(), false));
    }

    #[test]
    fn settings_visible_rows_collapsed_shows_five_rows_in_order() {
        let st = settings_state();
        assert_eq!(
            st.settings_visible_rows(),
            vec![
                SettingsRow::DefaultMode,
                SettingsRow::AttachedColor,
                SettingsRow::BorderColor,
                SettingsRow::ColorPolicy,
                SettingsRow::Palette,
            ]
        );
    }

    #[test]
    fn attached_and_border_color_default_to_cyan() {
        let st = settings_state();
        assert_eq!(st.attached_color, "cyan");
        assert_eq!(st.border_color, "cyan");
    }

    #[test]
    fn attached_color_expands_and_collapses_via_step_right_and_left() {
        let mut st = settings_state();
        st.settings_move_cursor(1); // row 1: AttachedColor
        assert_eq!(st.settings_visible_rows().len(), 5);
        st.settings_step_right();
        assert_eq!(st.settings_visible_rows().len(), 5 + 16, "expanded into 16 options");
        assert_eq!(
            st.settings_visible_rows()[st.settings_cursor()],
            SettingsRow::AttachedColorOption(6),
            "cursor lands on the currently selected color (cyan, index 6), not row 0"
        );
        st.settings_step_left();
        assert_eq!(st.settings_visible_rows().len(), 5, "collapsed back");
        assert_eq!(st.settings_cursor(), 1, "cursor returned to the AttachedColor row");
    }

    #[test]
    fn border_color_expands_and_collapses_via_step_right_and_left() {
        let mut st = settings_state();
        st.settings_move_cursor(2); // row 2: BorderColor
        st.settings_step_right();
        assert_eq!(st.settings_visible_rows().len(), 5 + 16);
        assert_eq!(
            st.settings_visible_rows()[st.settings_cursor()],
            SettingsRow::BorderColorOption(6),
            "cursor lands on the currently selected color (cyan, index 6)"
        );
        st.settings_step_left();
        assert_eq!(st.settings_visible_rows().len(), 5);
        assert_eq!(st.settings_cursor(), 2, "cursor returned to the BorderColor row");
    }

    #[test]
    fn activate_on_an_attached_color_option_commits_and_collapses() {
        let mut st = settings_state();
        st.settings_move_cursor(1); // AttachedColor
        st.settings_step_right(); // expand, cursor lands on index 6 (cyan)
        st.settings_move_cursor(-1); // step to index 5 ("magenta")
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::AttachedColorOption(5));
        st.settings_activate();
        assert_eq!(st.attached_color, "magenta");
        assert!(st.dirty);
        assert_eq!(st.settings_visible_rows().len(), 5, "collapsed after committing");
        assert_eq!(st.settings_cursor(), 1, "cursor returned to the AttachedColor row");
    }

    #[test]
    fn activate_on_a_border_color_option_commits_and_collapses() {
        let mut st = settings_state();
        st.settings_move_cursor(2); // BorderColor
        st.settings_step_right();
        st.settings_move_cursor(-5); // index 6 -> index 1 ("red")
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::BorderColorOption(1));
        st.settings_activate();
        assert_eq!(st.border_color, "red");
        assert!(st.dirty);
        assert_eq!(st.settings_cursor(), 2, "cursor returned to the BorderColor row");
    }

    #[test]
    fn h_on_an_attached_color_option_collapses_without_changing_the_value() {
        let mut st = settings_state();
        st.settings_move_cursor(1);
        st.settings_step_right();
        st.settings_move_cursor(-1); // onto "magenta"
        st.settings_step_left(); // cancel, not activate
        assert_eq!(st.attached_color, "cyan", "unchanged: h cancels rather than commits");
        assert_eq!(st.settings_cursor(), 1);
    }

    #[test]
    fn expanding_and_collapsing_palette_still_refocuses_correctly_with_other_sections_expanded() {
        // Regression guard for the dynamic collapse-cursor refactor: Palette's
        // own index is no longer fixed at 2 once AttachedColor/BorderColor can
        // also expand above it.
        let mut st = settings_state();
        st.settings_move_cursor(1);
        st.settings_step_right(); // expand AttachedColor: 16 rows now sit between it and BorderColor/ColorPolicy/Palette
        st.settings_move_cursor(-1);
        st.settings_step_left(); // collapse AttachedColor again, back to the 5-row layout
        assert_eq!(st.settings_visible_rows().len(), 5);
        st.settings_move_cursor(4); // Palette, still at index 4
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::Palette);
        st.settings_step_right(); // expand Palette
        st.settings_move_cursor(1); // first PaletteColor child
        st.settings_step_left(); // collapse
        assert_eq!(st.settings_cursor(), 4, "Palette collapse still lands on index 4");
    }

    #[test]
    fn settings_move_cursor_wraps_between_first_and_last_row() {
        let mut st = settings_state();
        assert_eq!(st.settings_cursor(), 0);
        st.settings_move_cursor(-1);
        assert_eq!(st.settings_cursor(), 4, "moving up from the top wraps to bottom");
        st.settings_move_cursor(1);
        assert_eq!(st.settings_cursor(), 0, "moving down from the bottom wraps to top");
        st.settings_move_cursor(1);
        assert_eq!(st.settings_cursor(), 1);
        st.settings_move_cursor(99);
        assert_eq!(st.settings_cursor(), 4, "large jumps still land on the edge");
    }

    #[test]
    fn step_cycles_default_mode_in_either_direction() {
        let mut st = settings_state(); // cursor on row 0, DefaultMode
        assert_eq!(st.default_mode, DefaultMode::Command);
        st.settings_step_right();
        assert_eq!(st.default_mode, DefaultMode::Search);
        st.settings_step_right();
        assert_eq!(st.default_mode, DefaultMode::Command, "2-state cycle wraps");
        st.settings_step_left();
        assert_eq!(st.default_mode, DefaultMode::Search, "h also flips a 2-state toggle");
        assert!(st.dirty);
    }

    #[test]
    fn activate_also_cycles_default_mode_forward() {
        let mut st = settings_state();
        st.settings_activate();
        assert_eq!(st.default_mode, DefaultMode::Search);
    }

    #[test]
    fn step_cycles_color_policy_forward_and_backward() {
        let mut st = settings_state();
        st.settings_move_cursor(3); // row 3: ColorPolicy
        assert_eq!(st.new_group_color_policy, ColorPolicy::Rotate);
        st.settings_step_right();
        assert_eq!(st.new_group_color_policy, ColorPolicy::Random);
        st.settings_step_right();
        assert_eq!(st.new_group_color_policy, ColorPolicy::Static);
        st.settings_step_right();
        assert_eq!(st.new_group_color_policy, ColorPolicy::Rotate, "wraps forward");
        st.settings_step_left();
        assert_eq!(st.new_group_color_policy, ColorPolicy::Static, "wraps backward");
    }

    #[test]
    fn palette_expands_and_collapses_via_step_right_and_left() {
        let mut st = settings_state();
        st.settings_move_cursor(4); // row 4: Palette
        assert!(!st.palette_expanded());
        st.settings_step_right();
        assert!(st.palette_expanded());
        assert_eq!(st.settings_visible_rows().len(), 5 + 16);
        st.settings_step_left();
        assert!(!st.palette_expanded());
        assert_eq!(st.settings_visible_rows().len(), 5);
    }

    #[test]
    fn step_left_on_a_palette_color_row_collapses_and_refocuses_the_parent() {
        let mut st = settings_state();
        st.settings_move_cursor(4);
        st.settings_step_right(); // expand
        st.settings_move_cursor(1); // onto the first PaletteColor child
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::PaletteColor(0));
        st.settings_step_left();
        assert!(!st.palette_expanded());
        assert_eq!(st.settings_cursor(), 4, "cursor returns to the Palette row");
    }

    #[test]
    fn activate_toggles_a_palette_color_off() {
        let mut st = settings_state();
        st.settings_move_cursor(4);
        st.settings_step_right(); // expand
        let cyan_idx = st.settings_palette_rows().iter().position(|(n, _)| n == "cyan").unwrap();
        st.settings_move_cursor(1 + cyan_idx as i32); // descend onto the "cyan" child row
        assert_eq!(st.settings_palette_rows()[cyan_idx], ("cyan".to_string(), true));
        st.settings_activate();
        assert!(!st.active_palette.contains(&"cyan".to_string()));
        assert!(st.dirty);
    }

    #[test]
    fn activate_cannot_deactivate_the_last_active_color() {
        let mut st = settings_state();
        st.active_palette = vec!["cyan".to_string()];
        st.settings_move_cursor(4);
        st.settings_step_right();
        let cyan_idx = st.settings_palette_rows().iter().position(|(n, _)| n == "cyan").unwrap();
        st.settings_move_cursor(1 + cyan_idx as i32); // the only active color
        st.settings_activate();
        assert_eq!(st.active_palette, vec!["cyan".to_string()], "guard: last active color stays");
    }

    #[test]
    fn activate_reactivates_an_inactive_color_at_its_canonical_position() {
        let mut st = settings_state(); // active: cyan, green, yellow, magenta, blue, red
        st.settings_move_cursor(4);
        st.settings_step_right();
        let black_idx = st.settings_palette_rows().iter().position(|(n, _)| n == "black").unwrap();
        st.settings_move_cursor(1 + black_idx as i32); // descend onto the "black" child row
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::PaletteColor(black_idx));
        st.settings_activate();
        assert!(st.active_palette.contains(&"black".to_string()));
        // "black" is first in ALL_NAMED_COLORS canonical order, so reactivating
        // it inserts it at the front of active_palette, not the end: rotation
        // order always matches the checklist's fixed display order.
        assert_eq!(
            st.active_palette.first(),
            Some(&"black".to_string()),
            "newly activated color is inserted at its canonical position"
        );
    }

    #[test]
    fn toggling_a_color_never_reorders_the_checklist() {
        let mut st = settings_state();
        st.settings_move_cursor(4);
        st.settings_step_right(); // expand
        let before: Vec<String> =
            st.settings_palette_rows().into_iter().map(|(n, _)| n).collect();
        let cyan_idx = before.iter().position(|n| n == "cyan").unwrap();
        st.settings_move_cursor(1 + cyan_idx as i32);
        st.settings_activate(); // toggle cyan off
        let after: Vec<String> = st.settings_palette_rows().into_iter().map(|(n, _)| n).collect();
        assert_eq!(before, after, "deactivating a color must not move any row");
    }

    #[test]
    fn static_color_defaults_to_cyan() {
        let st = settings_state();
        assert_eq!(st.static_color, "cyan");
    }

    #[test]
    fn c_key_only_cycles_static_color_when_policy_is_static() {
        let mut st = settings_state();
        st.settings_move_cursor(3); // ColorPolicy row, policy still Rotate
        st.settings_cycle_color();
        assert_eq!(st.static_color, "cyan", "no-op: policy is not Static");

        st.settings_step_right(); // Rotate -> Random
        st.settings_cycle_color();
        assert_eq!(st.static_color, "cyan", "no-op: policy is Random, not Static");

        st.settings_step_right(); // Random -> Static
        assert_eq!(st.new_group_color_policy, ColorPolicy::Static);
        st.settings_cycle_color();
        assert_eq!(st.static_color, "gray", "cycles to the next of all 16 named colors after cyan");
        assert!(st.dirty);
    }

    #[test]
    fn c_key_is_a_noop_off_a_color_row() {
        let mut st = settings_state();
        st.settings_move_cursor(3);
        st.settings_step_right(); st.settings_step_right(); // -> Static
        st.settings_move_cursor(-3); // back to DefaultMode row
        st.settings_cycle_color();
        assert_eq!(st.static_color, "cyan", "cursor must be on a color row");
    }

    #[test]
    fn static_color_persists_across_policy_switches() {
        let mut st = settings_state();
        st.settings_move_cursor(3);
        st.settings_step_right(); st.settings_step_right(); // -> Static
        st.settings_cycle_color(); // cyan -> gray
        assert_eq!(st.static_color, "gray");
        st.settings_step_right(); // Static -> Rotate
        assert_eq!(st.static_color, "gray", "not cleared by switching away from Static");
        st.settings_step_right(); st.settings_step_right(); // Random -> Static
        assert_eq!(st.static_color, "gray", "round-trips back without loss");
    }

    #[test]
    fn c_key_quick_cycles_attached_color_without_expanding() {
        let mut st = settings_state();
        st.settings_move_cursor(1); // AttachedColor, collapsed
        st.settings_cycle_color();
        assert_eq!(st.attached_color, "gray", "cyan -> gray, next in ALL_NAMED_COLORS");
        assert!(st.dirty);
        assert_eq!(st.settings_visible_rows().len(), 5, "stays collapsed");
    }

    #[test]
    fn c_key_quick_cycles_border_color_without_expanding() {
        let mut st = settings_state();
        st.settings_move_cursor(2); // BorderColor, collapsed
        st.settings_cycle_color();
        assert_eq!(st.border_color, "gray");
        assert!(st.dirty);
        assert_eq!(st.settings_visible_rows().len(), 5, "stays collapsed");
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
        st.enter_groups();
        st.group_new();
        assert!(st.groups.last().unwrap().color.is_empty(), "unchanged Rotate behavior");
    }

    #[test]
    fn group_new_under_static_policy_uses_the_configured_static_color() {
        let mut st = grouped_state();
        st.new_group_color_policy = ColorPolicy::Static;
        st.static_color = "magenta".to_string();
        st.enter_groups();
        st.group_new();
        assert_eq!(st.groups.last().unwrap().color, "magenta");
    }

    #[test]
    fn group_new_under_random_policy_picks_from_the_active_palette() {
        let mut st = grouped_state();
        st.new_group_color_policy = ColorPolicy::Random;
        st.enter_groups();
        st.group_new();
        let picked = st.groups.last().unwrap().color.clone();
        assert!(
            st.active_palette.contains(&picked),
            "random pick must come from the active palette"
        );
    }

    #[test]
    fn ensure_single_inbox_appends_fresh_when_none_flagged() {
        let mut groups = vec![
            Group { name: "G1".into(), members: vec!["a".into()], ..Default::default() },
            Group { name: "G2".into(), members: vec!["b".into()], ..Default::default() },
        ];
        ensure_single_inbox(&mut groups);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[2].name, "INBOX");
        assert!(groups[2].inbox);
        assert!(groups[2].members.is_empty());
        // Existing groups are never repurposed.
        assert!(!groups[0].inbox);
        assert!(!groups[1].inbox);
    }

    #[test]
    fn ensure_single_inbox_appends_fresh_when_groups_empty() {
        let mut groups: Vec<Group> = vec![];
        ensure_single_inbox(&mut groups);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].inbox);
        assert_eq!(groups[0].name, "INBOX");
    }

    #[test]
    fn ensure_single_inbox_is_a_noop_when_exactly_one_flagged() {
        let mut groups = vec![
            Group { name: "G1".into(), ..Default::default() },
            Group { name: "MINE".into(), inbox: true, members: vec!["x".into()], ..Default::default() },
        ];
        ensure_single_inbox(&mut groups);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[1].name, "MINE");
        assert!(groups[1].inbox);
    }

    #[test]
    fn ensure_single_inbox_keeps_first_and_clears_rest_when_multiple_flagged() {
        let mut groups = vec![
            Group { name: "FIRST".into(), inbox: true, ..Default::default() },
            Group { name: "SECOND".into(), inbox: true, ..Default::default() },
        ];
        ensure_single_inbox(&mut groups);
        assert!(groups[0].inbox);
        assert!(!groups[1].inbox);
    }
}
