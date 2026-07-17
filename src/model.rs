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

/// Governs which timestamp the session-row metadata column reflects. A
/// 3-state cycle (unlike `DefaultMode`'s 2-state toggle), so `h`, `l`, and
/// `Enter`/`Space` on the Session Metadata settings row all call `next()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionMetric {
    #[default]
    Recency,
    Age,
    Hidden,
}

impl SessionMetric {
    pub fn from_config_str(s: &str) -> SessionMetric {
        match s {
            "age" => SessionMetric::Age,
            "hidden" => SessionMetric::Hidden,
            _ => SessionMetric::Recency,
        }
    }

    pub fn as_config_str(self) -> &'static str {
        match self {
            SessionMetric::Recency => "recency",
            SessionMetric::Age => "age",
            SessionMetric::Hidden => "hidden",
        }
    }

    pub fn next(self) -> SessionMetric {
        match self {
            SessionMetric::Recency => SessionMetric::Age,
            SessionMetric::Age => SessionMetric::Hidden,
            SessionMetric::Hidden => SessionMetric::Recency,
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

/// Governs where `PickerState::group_new` inserts a freshly created group in
/// `groups`. The inbox always occupies the trailing slot (see
/// `ensure_inbox_last`), so `Bottom` means immediately above the inbox, not
/// the absolute end of the vector. Never retroactively moves any existing
/// group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewGroupPosition {
    Top,
    #[default]
    Bottom,
}

impl NewGroupPosition {
    pub fn from_config_str(s: &str) -> NewGroupPosition {
        match s {
            "top" => NewGroupPosition::Top,
            _ => NewGroupPosition::Bottom,
        }
    }

    pub fn as_config_str(self) -> &'static str {
        match self {
            NewGroupPosition::Top => "top",
            NewGroupPosition::Bottom => "bottom",
        }
    }

    /// Only two values, so a single `next` covers both `h` and `l` -- unlike
    /// `ColorPolicy`'s three-way cycle, there's no separate `prev` direction.
    pub fn next(self) -> NewGroupPosition {
        match self {
            NewGroupPosition::Top => NewGroupPosition::Bottom,
            NewGroupPosition::Bottom => NewGroupPosition::Top,
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
    /// The tmux `#{session_id}` (e.g. `"$3"`), stable across a plain tmux
    /// rename within a running server even though `name` isn't -- used by
    /// `Config::reconcile` to recover group, dormant, and expanded state
    /// across such a rename (issue #38). Empty when unknown (e.g. a
    /// synthetic `Session` built outside a real tmux gather).
    pub id: String,
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

/// Guarantees the inbox group -- wherever it's flagged -- sits at the very
/// end of `groups`. The inbox can still be renamed and recolored like any
/// other group, but its position is fixed: it can't be reordered via
/// `PickerState::group_reorder`, so this is the self-healing counterpart to
/// that guard for any config where the inbox landed elsewhere on disk (e.g.
/// hand-edited TOML, or one saved by issue #23's now-reverted "inbox moves
/// freely" behavior). Always call after `ensure_single_inbox`, which
/// guarantees there's exactly one flagged group to relocate. A no-op if the
/// inbox is already last.
pub fn ensure_inbox_last(groups: &mut Vec<Group>) {
    if let Some(i) = groups.iter().position(|g| g.inbox) {
        if i != groups.len() - 1 {
            let inbox = groups.remove(i);
            groups.push(inbox);
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

/// A planned window-level `⇧J`/`⇧K` action, computed by `plan_window_move`.
/// Pure data -- no tmux call happens until `main.rs` commits it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowMove {
    /// Swap two windows within the same session (`tmux swap-window`).
    SwapWithin { session: String, a_index: u32, b_index: u32 },
    /// Move a window into an adjacent session (`tmux move-window`).
    /// `before: true` inserts before `dst_anchor_index` (landing first),
    /// `false` inserts after it (landing last). `kills_source` is true iff
    /// `window_index` is the only window left in `src_session` -- moving it
    /// away would destroy that session. `src_attached` is `src_session`'s
    /// `Session::attached` flag, carried through so callers don't need to
    /// re-look it up.
    CrossSession {
        src_session: String,
        window_index: u32,
        dst_session: String,
        dst_anchor_index: u32,
        before: bool,
        kills_source: bool,
        src_attached: bool,
    },
}

/// What's armed by `arm_window_move`, and which direction confirms it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingWindowMove {
    mv: WindowMove,
    delta: i32,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRow {
    DefaultMode,
    DormantNumbering,
    RememberExpanded,
    SessionMetric,
    ClearDormantOnAttach,
    NewGroupPosition,
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

impl SettingsRow {
    /// A short, single-line explanation of what this setting does, shown
    /// on the Settings footer's description line. Child/option rows
    /// (individual color choices) reuse their parent setting's text since
    /// the option itself (a named color, a checkbox) is self-explanatory.
    pub fn description(&self) -> &'static str {
        match self {
            SettingsRow::DefaultMode => {
                "Whether the picker opens in Command mode or straight into Search."
            }
            SettingsRow::DormantNumbering => {
                "Whether visible dormant sessions get jump numbers (1-20)."
            }
            SettingsRow::RememberExpanded => {
                "When on, expand/collapse state persists across popups."
            }
            SettingsRow::SessionMetric => {
                "Whether the row's trailing timestamp shows Recency, Age, or is Hidden."
            }
            SettingsRow::ClearDormantOnAttach => {
                "When on, attaching to a dormant session automatically clears its dormant flag."
            }
            SettingsRow::NewGroupPosition => {
                "Where a newly created group is inserted: Top of the list, or Bottom (just above the inbox)."
            }
            SettingsRow::AttachedColor | SettingsRow::AttachedColorOption(_) => {
                "Highlight color for the session your tmux client is attached to."
            }
            SettingsRow::BorderColor | SettingsRow::BorderColorOption(_) => {
                "rolomux's own border frame color."
            }
            SettingsRow::ColorPolicy => {
                "How a new group picks its header color: Rotate, Random, or Static."
            }
            SettingsRow::Palette | SettingsRow::PaletteColor(_) => {
                "Which of the 16 terminal colors are in rotation for new group headers."
            }
        }
    }
}

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

    /// Arm a confirm: the next matching-direction `MoveUp`/`MoveDown` will
    /// commit `mv` instead of re-planning; anything else clears it.
    pub fn arm_window_move(&mut self, mv: WindowMove, delta: i32) {
        self.pending_window_move = Some(PendingWindowMove { mv, delta });
    }

    /// Consume and return the armed move if `delta` matches what was armed;
    /// otherwise leaves the arm untouched (a different direction doesn't
    /// confirm it) and returns `None`.
    pub fn take_confirmed_window_move(&mut self, delta: i32) -> Option<WindowMove> {
        match &self.pending_window_move {
            Some(p) if p.delta == delta => self.pending_window_move.take().map(|p| p.mv),
            _ => None,
        }
    }

    /// Drop any armed confirm with no side effect. Called for every input
    /// other than `MoveUp`/`MoveDown`.
    pub fn clear_pending_window_move(&mut self) {
        self.pending_window_move = None;
    }

    /// The footer warning to show while a confirm is armed, or `None`.
    pub fn pending_window_move_warning(&self) -> Option<&'static str> {
        self.pending_window_move.as_ref().map(|p| {
            if p.delta < 0 {
                "⇧K again to move last window — closes session · Esc cancels"
            } else {
                "⇧J again to move last window — closes session · Esc cancels"
            }
        })
    }

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

    /// Move the session under the cursor by `delta` rows, crossing group
    /// boundaries into the group above/below when at an edge -- the inbox
    /// group included, since it's just another entry in `self.groups` now.
    /// Wraps around at the very top and bottom of the whole list (top wraps
    /// to the end of the last group, bottom wraps to the front of the first
    /// group) rather than clamping.
    /// Compute what a window-row `⇧J`/`⇧K` press should do, without doing
    /// it. Returns `None` when the cursor isn't on a window row, or when
    /// there is truly nowhere for the window to go (a single session with a
    /// single window). Never mutates `self` -- the actual tmux call and
    /// state rebuild happen in `main.rs`, mirroring how rename works.
    pub fn plan_window_move(&self, delta: i32) -> Option<WindowMove> {
        let rows = self.visible_rows();
        let (si, wi) = match rows.get(self.cursor) {
            Some(Row::Window(si, wi)) => (*si, *wi),
            _ => return None,
        };
        let ordered = self.ordered();
        let sess = ordered[si];

        if delta < 0 && wi > 0 {
            let neighbor = &sess.windows[wi - 1];
            return Some(WindowMove::SwapWithin {
                session: sess.name.clone(),
                a_index: sess.windows[wi].index,
                b_index: neighbor.index,
            });
        }
        if delta > 0 && wi + 1 < sess.windows.len() {
            let neighbor = &sess.windows[wi + 1];
            return Some(WindowMove::SwapWithin {
                session: sess.name.clone(),
                a_index: sess.windows[wi].index,
                b_index: neighbor.index,
            });
        }

        // At the edge of this session's own window list: cross into the
        // adjacent session in the flat visible order, wrapping around the
        // whole list -- unless this is the only session on screen, in
        // which case there is nowhere else to go.
        if ordered.len() <= 1 {
            return None;
        }
        let dst_si = if delta < 0 {
            (si + ordered.len() - 1) % ordered.len()
        } else {
            (si + 1) % ordered.len()
        };
        let dst = ordered[dst_si];
        let (dst_anchor_index, before) = if delta < 0 {
            // Moving up and out: land at the *last* slot of the session above.
            (dst.windows.last().expect("a session always has >= 1 window").index, false)
        } else {
            // Moving down and out: land at the *first* slot of the session below.
            (dst.windows[0].index, true)
        };
        Some(WindowMove::CrossSession {
            src_session: sess.name.clone(),
            window_index: sess.windows[wi].index,
            dst_session: dst.name.clone(),
            dst_anchor_index,
            before,
            kills_source: sess.windows.len() == 1,
            src_attached: sess.attached,
        })
    }

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

    /// Move the session at the top of its group up into the group above,
    /// wrapping around to the *last* group when this is already the first
    /// group -- there is no longer a clamp at the very top of the list.
    fn move_up(&mut self, gi: usize, name: &str) {
        let order = self.effective_order(gi);
        let pos = match order.iter().position(|n| n == name) { Some(p) => p, None => return };
        if let Some(prev) = self.previous_visible_position(&order, pos) {
            self.commit_swap(gi, order, pos, prev, name);
            return;
        }
        let dest_gi = (gi + self.groups.len() - 1) % self.groups.len();
        self.cross_into_group(gi, dest_gi, name, true);
    }

    /// Move the session at the bottom of its group down into the group
    /// below, wrapping around to the *first* group when this is already the
    /// last group -- there is no longer a clamp at the very bottom of the list.
    fn move_down(&mut self, gi: usize, name: &str) {
        let order = self.effective_order(gi);
        let pos = match order.iter().position(|n| n == name) { Some(p) => p, None => return };
        if let Some(next) = self.next_visible_position(&order, pos) {
            self.commit_swap(gi, order, pos, next, name);
            return;
        }
        let dest_gi = (gi + 1) % self.groups.len();
        self.cross_into_group(gi, dest_gi, name, false);
    }

    /// Move `name` out of `src_gi` and into `dest_gi`, landing at the end
    /// (`append: true`) or the front (`append: false`) of `dest_gi`'s full
    /// rendered content -- not just its already-persisted `members` prefix.
    ///
    /// This must freeze `dest_gi`'s *effective* order (persisted members
    /// plus any never-touched inbox fallback overflow) before inserting,
    /// not just push/insert onto the raw `members` list: `members` always
    /// renders before fallback overflow in `ordered()`, so a naive
    /// `members.push` would land `name` ahead of any untouched fallback
    /// sessions instead of after them, when `dest_gi` is an inbox with
    /// fallback content still unfrozen. The defensive `retain` after
    /// computing `dest_order` is required, not optional: once `name` is
    /// removed from `src_gi.members`, `group_index_of(name)` falls back to
    /// the inbox, so if `dest_gi` is itself the inbox, `effective_order`
    /// may already include `name` via that same fallback -- without the
    /// retain, `name` would be double-inserted.
    fn cross_into_group(&mut self, src_gi: usize, dest_gi: usize, name: &str, append: bool) {
        self.groups[src_gi].members.retain(|m| m != name);
        let mut dest_order = self.effective_order(dest_gi);
        dest_order.retain(|n| n != name);
        if append {
            dest_order.push(name.to_string());
        } else {
            dest_order.insert(0, name.to_string());
        }
        self.groups[dest_gi].members = dest_order;
        self.dirty = true;
        self.focus_session(name);
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
        let mut rows = vec![
            SettingsRow::DefaultMode,
            SettingsRow::DormantNumbering,
            SettingsRow::RememberExpanded,
            SettingsRow::SessionMetric,
            SettingsRow::ClearDormantOnAttach,
            SettingsRow::NewGroupPosition,
            SettingsRow::AttachedColor,
        ];
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

    fn toggle_dormant_numbering(&mut self) {
        self.number_dormant_sessions = !self.number_dormant_sessions;
        self.dirty = true;
    }

    fn toggle_remember_expanded_sessions(&mut self) {
        self.remember_expanded_sessions = !self.remember_expanded_sessions;
        self.dirty = true;
    }

    fn toggle_clear_dormant_on_attach(&mut self) {
        self.clear_dormant_on_attach = !self.clear_dormant_on_attach;
        self.dirty = true;
    }

    fn toggle_new_group_position(&mut self) {
        self.new_group_position = self.new_group_position.next();
        self.dirty = true;
    }

    fn cycle_session_metric(&mut self) {
        self.session_metric = self.session_metric.next();
        self.dirty = true;
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
            SettingsRow::DormantNumbering => self.toggle_dormant_numbering(),
            SettingsRow::RememberExpanded => self.toggle_remember_expanded_sessions(),
            SettingsRow::SessionMetric => self.cycle_session_metric(),
            SettingsRow::ClearDormantOnAttach => self.toggle_clear_dormant_on_attach(),
            SettingsRow::NewGroupPosition => self.toggle_new_group_position(),
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
            SettingsRow::DormantNumbering => self.toggle_dormant_numbering(),
            SettingsRow::RememberExpanded => self.toggle_remember_expanded_sessions(),
            SettingsRow::SessionMetric => self.cycle_session_metric(),
            SettingsRow::ClearDormantOnAttach => self.toggle_clear_dormant_on_attach(),
            SettingsRow::NewGroupPosition => self.toggle_new_group_position(),
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
            SettingsRow::DefaultMode
            | SettingsRow::DormantNumbering
            | SettingsRow::RememberExpanded
            | SettingsRow::SessionMetric
            | SettingsRow::ClearDormantOnAttach
            | SettingsRow::NewGroupPosition
            | SettingsRow::ColorPolicy => self.settings_step_right(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Config;

    fn s(name: &str, activity: i64, created: i64) -> Session {
        Session { id: String::new(),
            name: name.into(),
            activity,
            created,
            attached: false,
            windows: vec![Window { index: 0, name: "w".into(), active: true }],
        }
    }

    fn win(index: u32, name: &str) -> Window {
        Window { index, name: name.into(), active: false }
    }

    fn session_with_windows(name: &str, created: i64, windows: Vec<Window>) -> Session {
        Session { id: String::new(), name: name.into(), activity: created, created, attached: false, windows }
    }

    #[test]
    fn plan_window_move_swaps_with_previous_window_in_same_session() {
        let sessions = vec![session_with_windows("work", 1, vec![win(0, "a"), win(1, "b"), win(2, "c")])];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("work");
        st.expand();
        st.move_cursor(3); // rows: [work, a, b, c]; lands on "c" (wi = 2)
        assert_eq!(
            st.plan_window_move(-1),
            Some(WindowMove::SwapWithin { session: "work".into(), a_index: 2, b_index: 1 })
        );
    }

    #[test]
    fn plan_window_move_swaps_with_next_window_in_same_session() {
        let sessions = vec![session_with_windows("work", 1, vec![win(0, "a"), win(1, "b"), win(2, "c")])];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("work");
        st.expand();
        st.move_cursor(1); // rows: [work, a, b, c]; lands on "a" (wi = 0)
        assert_eq!(
            st.plan_window_move(1),
            Some(WindowMove::SwapWithin { session: "work".into(), a_index: 0, b_index: 1 })
        );
    }

    #[test]
    fn plan_window_move_up_from_first_window_crosses_into_session_above_last_slot() {
        let sessions = vec![
            session_with_windows("alpha", 1, vec![win(0, "a1"), win(3, "a2")]),
            session_with_windows("beta", 2, vec![win(0, "b1")]),
        ];
        let cfg = Config {
            groups: vec![Group {
                name: "ONLY".into(),
                members: vec!["alpha".into(), "beta".into()],
                inbox: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("beta");
        st.expand();
        st.move_cursor(1); // rows: [alpha, a1, a2, beta, b1]; lands on "b1" (wi = 0, only window)
        assert_eq!(
            st.plan_window_move(-1),
            Some(WindowMove::CrossSession {
                src_session: "beta".into(),
                window_index: 0,
                dst_session: "alpha".into(),
                dst_anchor_index: 3,
                before: false,
                kills_source: true,
                src_attached: false,
            })
        );
    }

    #[test]
    fn plan_window_move_down_from_last_window_crosses_into_session_below_first_slot() {
        let sessions = vec![
            session_with_windows("alpha", 1, vec![win(0, "a1"), win(1, "a2")]),
            session_with_windows("beta", 2, vec![win(0, "b1"), win(2, "b2")]),
        ];
        let cfg = Config {
            groups: vec![Group {
                name: "ONLY".into(),
                members: vec!["alpha".into(), "beta".into()],
                inbox: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("alpha");
        st.expand();
        st.move_cursor(2); // rows: [alpha, a1, a2, beta]; lands on "a2" (wi = 1, last window)
        assert_eq!(
            st.plan_window_move(1),
            Some(WindowMove::CrossSession {
                src_session: "alpha".into(),
                window_index: 1,
                dst_session: "beta".into(),
                dst_anchor_index: 0,
                before: true,
                kills_source: false,
                src_attached: false,
            })
        );
    }

    #[test]
    fn plan_window_move_wraps_from_first_session_up_to_last_session_last_slot() {
        let sessions = vec![
            session_with_windows("alpha", 1, vec![win(0, "a1")]),
            session_with_windows("beta", 2, vec![win(0, "b1"), win(1, "b2")]),
        ];
        let cfg = Config {
            groups: vec![Group {
                name: "ONLY".into(),
                members: vec!["alpha".into(), "beta".into()],
                inbox: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("alpha");
        st.expand();
        st.move_cursor(1); // rows: [alpha, a1, beta, b1, b2]; lands on "a1" (wi = 0, only window)
        assert_eq!(
            st.plan_window_move(-1),
            Some(WindowMove::CrossSession {
                src_session: "alpha".into(),
                window_index: 0,
                dst_session: "beta".into(), // wraps around to the last session on screen
                dst_anchor_index: 1,
                before: false,
                kills_source: true,
                src_attached: false,
            })
        );
    }

    #[test]
    fn plan_window_move_is_none_when_only_one_session_with_one_window() {
        let sessions = vec![session_with_windows("solo", 1, vec![win(0, "only")])];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("solo");
        st.expand();
        st.move_cursor(1); // the only window
        assert_eq!(st.plan_window_move(-1), None);
        assert_eq!(st.plan_window_move(1), None);
    }

    #[test]
    fn plan_window_move_is_none_when_cursor_is_on_a_session_row() {
        let sessions = vec![session_with_windows("solo", 1, vec![win(0, "only"), win(1, "other")])];
        let cfg = Config::default();
        let st = PickerState::build(sessions, &cfg); // cursor defaults onto the session row
        assert_eq!(st.plan_window_move(-1), None);
    }

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
    fn arm_and_take_confirmed_window_move_round_trips_on_matching_direction() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        let mv = WindowMove::SwapWithin { session: "a".into(), a_index: 0, b_index: 1 };
        st.arm_window_move(mv.clone(), -1);
        assert!(st.pending_window_move_warning().is_some());
        assert_eq!(st.take_confirmed_window_move(1), None, "wrong direction doesn't confirm");
        assert!(st.pending_window_move_warning().is_some(), "still armed after a non-matching direction");
        assert_eq!(st.take_confirmed_window_move(-1), Some(mv), "matching direction confirms and consumes it");
        assert!(st.pending_window_move_warning().is_none());
    }

    #[test]
    fn clear_pending_window_move_drops_the_arm() {
        let sessions = vec![s("a", 1, 1)];
        let cfg = Config::default();
        let mut st = PickerState::build(sessions, &cfg);
        let mv = WindowMove::SwapWithin { session: "a".into(), a_index: 0, b_index: 1 };
        st.arm_window_move(mv, -1);
        st.clear_pending_window_move();
        assert!(st.pending_window_move_warning().is_none());
        assert_eq!(st.take_confirmed_window_move(-1), None);
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
    fn session_metric_parses_with_recency_fallback() {
        assert_eq!(SessionMetric::from_config_str("age"), SessionMetric::Age);
        assert_eq!(SessionMetric::from_config_str("hidden"), SessionMetric::Hidden);
        assert_eq!(SessionMetric::from_config_str("recency"), SessionMetric::Recency);
        assert_eq!(SessionMetric::from_config_str("garbage"), SessionMetric::Recency);
        assert_eq!(SessionMetric::default(), SessionMetric::Recency);
    }

    #[test]
    fn session_metric_next_cycles_through_all_three_and_wraps() {
        assert_eq!(SessionMetric::Recency.next(), SessionMetric::Age);
        assert_eq!(SessionMetric::Age.next(), SessionMetric::Hidden);
        assert_eq!(SessionMetric::Hidden.next(), SessionMetric::Recency);
        assert_eq!(SessionMetric::Recency.as_config_str(), "recency");
        assert_eq!(SessionMetric::Age.as_config_str(), "age");
        assert_eq!(SessionMetric::Hidden.as_config_str(), "hidden");
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

        // Moving up at the top now wraps to the bottom of the (single) group.
        state.dirty = false;
        state.move_row(-1);
        let names: Vec<&str> = state.ordered().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["a", "c", "b"]);
        assert_eq!(state.cursor_session_name().as_deref(), Some("b"));
        assert!(state.dirty);
    }

    #[test]
    fn move_row_unpinned_at_residual_bottom_wraps_to_front() {
        let sessions = vec![s("a", 30, 1), s("b", 20, 2)];
        let cfg = Config { groups: vec![], ..Default::default() };
        let mut state = PickerState::build(sessions, &cfg);
        state.focus_session("b");
        state.move_row(1);
        assert!(state.dirty);
        assert_eq!(
            state.groups[state.inbox_index().unwrap()].members,
            vec!["b".to_string(), "a".to_string()]
        );
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
        state.toggle_focus_mode();
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
    fn settings_step_left_and_right_toggle_new_group_position() {
        let mut st = grouped_state();
        assert_eq!(st.new_group_position, NewGroupPosition::Bottom);
        st.settings_move_cursor(5); // NewGroupPosition row (after ClearDormantOnAttach)
        assert_eq!(st.current_settings_row(), SettingsRow::NewGroupPosition);
        st.settings_step_right();
        assert_eq!(st.new_group_position, NewGroupPosition::Top);
        st.settings_step_right();
        assert_eq!(st.new_group_position, NewGroupPosition::Bottom, "only two values, so right also wraps");
        st.settings_step_left();
        assert_eq!(st.new_group_position, NewGroupPosition::Top);
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
    fn move_up_at_very_top_wraps_to_last_group_end() {
        let mut st = state_with_two_groups();
        st.focus_session("a"); // top of first group == top of the whole visible list
        st.move_row(-1);
        assert_eq!(st.groups[0].members, vec!["b".to_string()]);
        let inbox = st.inbox_index().unwrap();
        // inbox (the last group here) has untouched fallback members d, e;
        // "a" must land after them, not before -- this is the case that
        // needs the effective-order-aware cross_into_group helper below,
        // not a naive `.members.push`.
        assert_eq!(
            st.groups[inbox].members,
            vec!["d".to_string(), "e".to_string(), "a".to_string()]
        );
        assert_eq!(st.cursor_session_name().as_deref(), Some("a"));
        assert!(st.dirty);
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
    fn move_down_at_residual_bottom_wraps_to_first_group_front() {
        let mut st = state_with_two_groups();
        st.focus_session("e"); // bottom of inbox == bottom of the whole visible list
        st.move_row(1);
        assert_eq!(
            st.groups[0].members,
            vec!["e".to_string(), "a".to_string(), "b".to_string()]
        );
        assert_eq!(st.cursor_session_name().as_deref(), Some("e"));
        assert!(st.dirty);
    }

    #[test]
    fn move_wraps_within_a_single_group_when_only_one_group_exists() {
        let sessions = vec![s("a", 1, 1), s("b", 1, 2), s("c", 1, 3)];
        let cfg = Config {
            groups: vec![Group {
                name: "ONLY".into(),
                members: vec!["a".into(), "b".into(), "c".into()],
                inbox: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut st = PickerState::build(sessions, &cfg);
        st.focus_session("a"); // top of the only group
        st.move_row(-1);
        assert_eq!(
            st.groups[0].members,
            vec!["b".to_string(), "c".to_string(), "a".to_string()]
        );
        assert_eq!(st.cursor_session_name().as_deref(), Some("a"));
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
    fn settings_visible_rows_collapsed_shows_ten_rows_in_order() {
        let st = settings_state();
        assert_eq!(
            st.settings_visible_rows(),
            vec![
                SettingsRow::DefaultMode,
                SettingsRow::DormantNumbering,
                SettingsRow::RememberExpanded,
                SettingsRow::SessionMetric,
                SettingsRow::ClearDormantOnAttach,
                SettingsRow::NewGroupPosition,
                SettingsRow::AttachedColor,
                SettingsRow::BorderColor,
                SettingsRow::ColorPolicy,
                SettingsRow::Palette,
            ]
        );
    }

    #[test]
    fn settings_toggle_remember_expanded_in_either_direction() {
        let mut st = settings_state();
        st.settings_move_cursor(2); // RememberExpanded
        assert!(!st.remember_expanded_sessions);
        st.settings_step_right();
        assert!(st.remember_expanded_sessions);
        st.settings_step_left();
        assert!(!st.remember_expanded_sessions);
        st.settings_activate();
        assert!(st.remember_expanded_sessions);
        assert!(st.dirty);
    }

    #[test]
    fn settings_cycles_session_metric_through_all_three_states() {
        let mut st = settings_state();
        st.settings_move_cursor(3); // SessionMetric
        assert_eq!(st.session_metric, SessionMetric::Recency);
        st.settings_step_right();
        assert_eq!(st.session_metric, SessionMetric::Age);
        st.settings_step_right();
        assert_eq!(st.session_metric, SessionMetric::Hidden);
        st.settings_step_left(); // h also advances a 3-state cycle, same as l
        assert_eq!(st.session_metric, SessionMetric::Recency, "wraps back via next()");
        st.settings_activate();
        assert_eq!(st.session_metric, SessionMetric::Age);
        assert!(st.dirty);
    }

    #[test]
    fn attached_and_border_color_default_to_green() {
        let st = settings_state();
        assert_eq!(st.attached_color, "green");
        assert_eq!(st.border_color, "green");
    }

    #[test]
    fn attached_color_expands_and_collapses_via_step_right_and_left() {
        let mut st = settings_state();
        st.settings_move_cursor(6); // row 6: AttachedColor
        assert_eq!(st.settings_visible_rows().len(), 10);
        st.settings_step_right();
        assert_eq!(st.settings_visible_rows().len(), 10 + 16, "expanded into 16 options");
        assert_eq!(
            st.settings_visible_rows()[st.settings_cursor()],
            SettingsRow::AttachedColorOption(2),
            "cursor lands on the currently selected color (green, index 2), not row 0"
        );
        st.settings_step_left();
        assert_eq!(st.settings_visible_rows().len(), 10, "collapsed back");
        assert_eq!(st.settings_cursor(), 6, "cursor returned to the AttachedColor row");
    }

    #[test]
    fn border_color_expands_and_collapses_via_step_right_and_left() {
        let mut st = settings_state();
        st.settings_move_cursor(7); // row 7: BorderColor
        st.settings_step_right();
        assert_eq!(st.settings_visible_rows().len(), 10 + 16);
        assert_eq!(
            st.settings_visible_rows()[st.settings_cursor()],
            SettingsRow::BorderColorOption(2),
            "cursor lands on the currently selected color (green, index 2)"
        );
        st.settings_step_left();
        assert_eq!(st.settings_visible_rows().len(), 10);
        assert_eq!(st.settings_cursor(), 7, "cursor returned to the BorderColor row");
    }

    #[test]
    fn activate_on_an_attached_color_option_commits_and_collapses() {
        let mut st = settings_state();
        st.settings_move_cursor(6); // AttachedColor
        st.settings_step_right(); // expand, cursor lands on index 2 (green)
        st.settings_move_cursor(-1); // step to index 1 ("red")
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::AttachedColorOption(1));
        st.settings_activate();
        assert_eq!(st.attached_color, "red");
        assert!(st.dirty);
        assert_eq!(st.settings_visible_rows().len(), 10, "collapsed after committing");
        assert_eq!(st.settings_cursor(), 6, "cursor returned to the AttachedColor row");
    }

    #[test]
    fn activate_on_a_border_color_option_commits_and_collapses() {
        let mut st = settings_state();
        st.settings_move_cursor(7); // BorderColor
        st.settings_step_right(); // expand, cursor lands on index 2 (green)
        st.settings_move_cursor(1); // step to index 3 ("yellow")
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::BorderColorOption(3));
        st.settings_activate();
        assert_eq!(st.border_color, "yellow");
        assert!(st.dirty);
        assert_eq!(st.settings_cursor(), 7, "cursor returned to the BorderColor row");
    }

    #[test]
    fn h_on_an_attached_color_option_collapses_without_changing_the_value() {
        let mut st = settings_state();
        st.settings_move_cursor(6);
        st.settings_step_right();
        st.settings_move_cursor(-1); // onto "red"
        st.settings_step_left(); // cancel, not activate
        assert_eq!(st.attached_color, "green", "unchanged: h cancels rather than commits");
        assert_eq!(st.settings_cursor(), 6);
    }

    #[test]
    fn expanding_and_collapsing_palette_still_refocuses_correctly_with_other_sections_expanded() {
        // Regression guard for the dynamic collapse-cursor refactor: Palette's
        // own index is no longer fixed at 2 once AttachedColor/BorderColor can
        // also expand above it.
        let mut st = settings_state();
        st.settings_move_cursor(6);
        st.settings_step_right(); // expand AttachedColor: 16 rows now sit between it and BorderColor/ColorPolicy/Palette
        st.settings_move_cursor(-1);
        st.settings_step_left(); // collapse AttachedColor again, back to the 10-row layout
        assert_eq!(st.settings_visible_rows().len(), 10);
        st.settings_move_cursor(9); // Palette, still at index 9
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::Palette);
        st.settings_step_right(); // expand Palette
        st.settings_move_cursor(1); // first PaletteColor child
        st.settings_step_left(); // collapse
        assert_eq!(st.settings_cursor(), 9, "Palette collapse still lands on index 9");
    }

    #[test]
    fn settings_move_cursor_wraps_between_first_and_last_row() {
        let mut st = settings_state();
        assert_eq!(st.settings_cursor(), 0);
        st.settings_move_cursor(-1);
        assert_eq!(st.settings_cursor(), 9, "moving up from the top wraps to bottom");
        st.settings_move_cursor(1);
        assert_eq!(st.settings_cursor(), 0, "moving down from the bottom wraps to top");
        st.settings_move_cursor(1);
        assert_eq!(st.settings_cursor(), 1);
        st.settings_move_cursor(99);
        assert_eq!(st.settings_cursor(), 9, "large jumps still land on the edge");
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
    fn settings_toggle_dormant_numbering_in_either_direction() {
        let mut st = settings_state();
        st.settings_move_cursor(1); // DormantNumbering
        assert!(st.number_dormant_sessions);
        st.settings_step_right();
        assert!(!st.number_dormant_sessions);
        st.settings_step_left();
        assert!(st.number_dormant_sessions);
        st.settings_activate();
        assert!(!st.number_dormant_sessions);
        assert!(st.dirty);
    }

    #[test]
    fn step_cycles_color_policy_forward_and_backward() {
        let mut st = settings_state();
        st.settings_move_cursor(8); // row 8: ColorPolicy
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
        st.settings_move_cursor(9); // row 9: Palette
        assert!(!st.palette_expanded());
        st.settings_step_right();
        assert!(st.palette_expanded());
        assert_eq!(st.settings_visible_rows().len(), 10 + 16);
        st.settings_step_left();
        assert!(!st.palette_expanded());
        assert_eq!(st.settings_visible_rows().len(), 10);
    }

    #[test]
    fn step_left_on_a_palette_color_row_collapses_and_refocuses_the_parent() {
        let mut st = settings_state();
        st.settings_move_cursor(9);
        st.settings_step_right(); // expand
        st.settings_move_cursor(1); // onto the first PaletteColor child
        assert_eq!(st.settings_visible_rows()[st.settings_cursor()], SettingsRow::PaletteColor(0));
        st.settings_step_left();
        assert!(!st.palette_expanded());
        assert_eq!(st.settings_cursor(), 9, "cursor returns to the Palette row");
    }

    #[test]
    fn activate_toggles_a_palette_color_off() {
        let mut st = settings_state();
        st.settings_move_cursor(9);
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
        st.settings_move_cursor(9);
        st.settings_step_right();
        let cyan_idx = st.settings_palette_rows().iter().position(|(n, _)| n == "cyan").unwrap();
        st.settings_move_cursor(1 + cyan_idx as i32); // the only active color
        st.settings_activate();
        assert_eq!(st.active_palette, vec!["cyan".to_string()], "guard: last active color stays");
    }

    #[test]
    fn activate_reactivates_an_inactive_color_at_its_canonical_position() {
        let mut st = settings_state(); // active: cyan, green, yellow, magenta, blue, red
        st.settings_move_cursor(9);
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
        st.settings_move_cursor(9);
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
        st.settings_move_cursor(8); // ColorPolicy row, policy still Rotate
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
        st.settings_move_cursor(8);
        st.settings_step_right(); st.settings_step_right(); // -> Static
        st.settings_move_cursor(-8); // back to DefaultMode row
        st.settings_cycle_color();
        assert_eq!(st.static_color, "cyan", "cursor must be on a color row");
    }

    #[test]
    fn static_color_persists_across_policy_switches() {
        let mut st = settings_state();
        st.settings_move_cursor(8);
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
        st.settings_move_cursor(6); // AttachedColor, collapsed
        st.settings_cycle_color();
        assert_eq!(st.attached_color, "yellow", "green -> yellow, next in ALL_NAMED_COLORS");
        assert!(st.dirty);
        assert_eq!(st.settings_visible_rows().len(), 10, "stays collapsed");
    }

    #[test]
    fn c_key_quick_cycles_border_color_without_expanding() {
        let mut st = settings_state();
        st.settings_move_cursor(7); // BorderColor, collapsed
        st.settings_cycle_color();
        assert_eq!(st.border_color, "yellow");
        assert!(st.dirty);
        assert_eq!(st.settings_visible_rows().len(), 10, "stays collapsed");
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

    #[test]
    fn ensure_inbox_last_relocates_a_leading_inbox_to_the_end() {
        let mut groups = vec![
            Group { name: "INBOX".into(), inbox: true, ..Default::default() },
            Group { name: "WORK".into(), ..Default::default() },
        ];
        ensure_inbox_last(&mut groups);
        assert_eq!(groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), vec!["WORK", "INBOX"]);
        assert!(groups[1].inbox);
    }

    #[test]
    fn ensure_inbox_last_relocates_a_middle_inbox_to_the_end() {
        let mut groups = vec![
            Group { name: "WORK".into(), ..Default::default() },
            Group { name: "INBOX".into(), inbox: true, ..Default::default() },
            Group { name: "PLAY".into(), ..Default::default() },
        ];
        ensure_inbox_last(&mut groups);
        assert_eq!(
            groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
            vec!["WORK", "PLAY", "INBOX"]
        );
    }

    #[test]
    fn ensure_inbox_last_is_a_noop_when_already_last() {
        let mut groups = vec![
            Group { name: "WORK".into(), ..Default::default() },
            Group { name: "INBOX".into(), inbox: true, ..Default::default() },
        ];
        let before = groups.clone();
        ensure_inbox_last(&mut groups);
        assert_eq!(groups, before);
    }

    #[test]
    fn ensure_inbox_last_is_a_noop_on_an_empty_or_unflagged_list() {
        let mut groups: Vec<Group> = vec![];
        ensure_inbox_last(&mut groups); // never panics on empty input
        assert!(groups.is_empty());

        let mut groups = vec![Group { name: "WORK".into(), ..Default::default() }];
        ensure_inbox_last(&mut groups); // no flagged group to relocate
        assert_eq!(groups[0].name, "WORK");
    }

    #[test]
    fn settings_row_description_describes_default_mode() {
        assert_eq!(
            SettingsRow::DefaultMode.description(),
            "Whether the picker opens in Command mode or straight into Search."
        );
    }

    #[test]
    fn settings_row_description_child_rows_reuse_parent_text() {
        assert_eq!(
            SettingsRow::AttachedColorOption(0).description(),
            SettingsRow::AttachedColor.description()
        );
        assert_eq!(
            SettingsRow::BorderColorOption(0).description(),
            SettingsRow::BorderColor.description()
        );
        assert_eq!(
            SettingsRow::PaletteColor(0).description(),
            SettingsRow::Palette.description()
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
