//! Pure data types, enums, constants, and small free helpers shared across the
//! `model` submodules. No `PickerState` methods live here.

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

/// Governs the `focus_mode` boolean a fresh popup starts with. `Remember`
/// (default) seeds it from the persisted `Config::focus_mode`, exactly like
/// every prior version of this picker; `Always`/`Never` override that with a
/// fixed value regardless of what was last saved. Same 3-state-cycle shape as
/// `SessionMetric`: `next()` only, no distinct "previous".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartFocusMode {
    #[default]
    Remember,
    Always,
    Never,
}

// Not yet constructed by any caller outside this module's own tests; Task 3
// wires PickerState/Config to it, at which point this allow should come out.
#[allow(dead_code)]
impl StartFocusMode {
    pub fn from_config_str(s: &str) -> StartFocusMode {
        match s {
            "always" => StartFocusMode::Always,
            "never" => StartFocusMode::Never,
            _ => StartFocusMode::Remember,
        }
    }

    pub fn as_config_str(self) -> &'static str {
        match self {
            StartFocusMode::Remember => "remember",
            StartFocusMode::Always => "always",
            StartFocusMode::Never => "never",
        }
    }

    pub fn next(self) -> StartFocusMode {
        match self {
            StartFocusMode::Remember => StartFocusMode::Always,
            StartFocusMode::Always => StartFocusMode::Never,
            StartFocusMode::Never => StartFocusMode::Remember,
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

/// Governs the color of the `●` marking a session's active window
/// (`window_item`). `Static` uses a single fixed color (`Config::dot_color`,
/// cycled via `c` same as `attached_color`/`border_color`); `Group` inherits
/// the color of the session's own group header instead, so the dot recolors
/// per-section like the gutter bar already does. Only two values, so `next`
/// covers both `h` and `l` -- same shape as `NewGroupPosition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DotColorMode {
    #[default]
    Static,
    Group,
}

impl DotColorMode {
    pub fn from_config_str(s: &str) -> DotColorMode {
        match s {
            "group" => DotColorMode::Group,
            _ => DotColorMode::Static,
        }
    }

    pub fn as_config_str(self) -> &'static str {
        match self {
            DotColorMode::Static => "static",
            DotColorMode::Group => "group",
        }
    }

    pub fn next(self) -> DotColorMode {
        match self {
            DotColorMode::Static => DotColorMode::Group,
            DotColorMode::Group => DotColorMode::Static,
        }
    }
}

/// Governs whether the footer's key-shortcut legend renders on every frame
/// or stays hidden until the transient `?` toggle (`PickerState::toggle_shortcuts`)
/// reveals it for the rest of the current popup's lifetime. Only two values,
/// so `next` covers both `h` and `l` -- same shape as `NewGroupPosition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShortcutVisibility {
    #[default]
    Always,
    OnDemand,
}

impl ShortcutVisibility {
    pub fn from_config_str(s: &str) -> ShortcutVisibility {
        match s {
            "on_demand" => ShortcutVisibility::OnDemand,
            _ => ShortcutVisibility::Always,
        }
    }

    pub fn as_config_str(self) -> &'static str {
        match self {
            ShortcutVisibility::Always => "always",
            ShortcutVisibility::OnDemand => "on_demand",
        }
    }

    pub fn next(self) -> ShortcutVisibility {
        match self {
            ShortcutVisibility::Always => ShortcutVisibility::OnDemand,
            ShortcutVisibility::OnDemand => ShortcutVisibility::Always,
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

/// The curated set of glyphs a user can cycle the inbox group's header icon
/// through (`PickerState::settings_step_left`/`settings_step_right` on the
/// `SettingsRow::InboxIcon` row). Index 0 is the historical hardcoded
/// default, kept first so upgrading users see no visual change until they
/// opt in. Every glyph was checked against Unicode's East Asian Width
/// property to confirm single-column rendering (the fixed-width header-rule
/// padding in `ui::group_label_width`/`header_item` assumes one column per
/// `char`), and against emoji-presentation to keep this app emoji-free.
pub const INBOX_ICONS: [&str; 21] = [
    "⊛", "☆", "❆", "❁", "❃", "❋", "❦", "⟡", "⌂", "⌖", "⍟", "⎈", "⦿", "∆",
    "⊕", "∅", "⧉", "♤", "♡", "♢", "♧",
];

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn color_policy_parses_with_rotate_fallback() {
        assert_eq!(ColorPolicy::from_config_str("random"), ColorPolicy::Random);
        assert_eq!(ColorPolicy::from_config_str("static"), ColorPolicy::Static);
        assert_eq!(ColorPolicy::from_config_str("rotate"), ColorPolicy::Rotate);
        assert_eq!(ColorPolicy::from_config_str("garbage"), ColorPolicy::Rotate);
        assert_eq!(ColorPolicy::default(), ColorPolicy::Rotate);
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
    fn default_mode_parses_with_command_fallback() {
        assert_eq!(DefaultMode::from_config_str("search"), DefaultMode::Search);
        assert_eq!(DefaultMode::from_config_str("command"), DefaultMode::Command);
        assert_eq!(DefaultMode::from_config_str("garbage"), DefaultMode::Command);
        assert_eq!(DefaultMode::default(), DefaultMode::Command);
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
    fn session_metric_parses_with_recency_fallback() {
        assert_eq!(SessionMetric::from_config_str("age"), SessionMetric::Age);
        assert_eq!(SessionMetric::from_config_str("hidden"), SessionMetric::Hidden);
        assert_eq!(SessionMetric::from_config_str("recency"), SessionMetric::Recency);
        assert_eq!(SessionMetric::from_config_str("garbage"), SessionMetric::Recency);
        assert_eq!(SessionMetric::default(), SessionMetric::Recency);
    }

    #[test]
    fn start_focus_mode_next_cycles_through_all_three_and_wraps() {
        assert_eq!(StartFocusMode::Remember.next(), StartFocusMode::Always);
        assert_eq!(StartFocusMode::Always.next(), StartFocusMode::Never);
        assert_eq!(StartFocusMode::Never.next(), StartFocusMode::Remember);
        assert_eq!(StartFocusMode::Remember.as_config_str(), "remember");
        assert_eq!(StartFocusMode::Always.as_config_str(), "always");
        assert_eq!(StartFocusMode::Never.as_config_str(), "never");
    }

    #[test]
    fn start_focus_mode_parses_with_remember_fallback() {
        assert_eq!(StartFocusMode::from_config_str("always"), StartFocusMode::Always);
        assert_eq!(StartFocusMode::from_config_str("never"), StartFocusMode::Never);
        assert_eq!(StartFocusMode::from_config_str("remember"), StartFocusMode::Remember);
        assert_eq!(StartFocusMode::from_config_str("garbage"), StartFocusMode::Remember);
        assert_eq!(StartFocusMode::default(), StartFocusMode::Remember);
    }

    #[test]
    fn dot_color_mode_next_toggles_and_round_trips_config_str() {
        assert_eq!(DotColorMode::Static.next(), DotColorMode::Group);
        assert_eq!(DotColorMode::Group.next(), DotColorMode::Static);
        assert_eq!(DotColorMode::Static.as_config_str(), "static");
        assert_eq!(DotColorMode::Group.as_config_str(), "group");
        assert_eq!(DotColorMode::default(), DotColorMode::Static);
    }

    #[test]
    fn dot_color_mode_parses_with_static_fallback() {
        assert_eq!(DotColorMode::from_config_str("group"), DotColorMode::Group);
        assert_eq!(DotColorMode::from_config_str("static"), DotColorMode::Static);
        assert_eq!(DotColorMode::from_config_str("garbage"), DotColorMode::Static);
    }

    #[test]
    fn shortcut_visibility_next_toggles_and_round_trips_config_str() {
        assert_eq!(ShortcutVisibility::Always.next(), ShortcutVisibility::OnDemand);
        assert_eq!(ShortcutVisibility::OnDemand.next(), ShortcutVisibility::Always);
        assert_eq!(ShortcutVisibility::Always.as_config_str(), "always");
        assert_eq!(ShortcutVisibility::OnDemand.as_config_str(), "on_demand");
        assert_eq!(ShortcutVisibility::default(), ShortcutVisibility::Always);
    }

    #[test]
    fn shortcut_visibility_parses_with_always_fallback() {
        assert_eq!(ShortcutVisibility::from_config_str("on_demand"), ShortcutVisibility::OnDemand);
        assert_eq!(ShortcutVisibility::from_config_str("always"), ShortcutVisibility::Always);
        assert_eq!(ShortcutVisibility::from_config_str("garbage"), ShortcutVisibility::Always);
    }
}
