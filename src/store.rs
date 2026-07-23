use crate::model::{
    AttachedColorMode, ColorPolicy, DefaultMode, DotColorMode, Group, HEADER_COLORS, NewGroupPosition, SessionMetric,
    ShortcutVisibility, StartFocusMode, ensure_inbox_last, ensure_single_inbox,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

// Bump whenever the on-disk schema changes in a way that isn't already
// tolerated by serde defaults (i.e. a rename or a semantic change, not a
// plain new-field addition). Add the migration step in `Config::migrate`
// and a matching test. See AGENTS.md "Configuration".
//
// v1 -> v2: dropped `sort` (issue #15, sort-mode cycling removed). No
// transform needed: the field is simply no longer read or written, and no
// existing data (`groups`, `manual_order`) is affected.
//
// v2 -> v3: the residual `manual_order` list is folded into `groups` as a
// real, flagged `inbox` group (issue #23). `manual_order` becomes
// migration-input-only, exactly as `pinned` was for v0 -> v1.
//
// v3 -> v4: `hide_dormant` renamed to `focus_mode` (issue #100), same
// boolean, no semantic change. `hide_dormant` becomes migration-input-only.
pub const CONFIG_VERSION: u32 = 4;

/// A single window-level dormant entry: `session`/`index` locate it the
/// same way `SwitchWindow`/`focus_window` address any window, and `id`
/// (tmux `#{window_id}`) is what `reconcile` uses to recover the entry
/// across an index reuse (closing a window frees its index; the next
/// created window can reuse it) or a `move-window` to a different
/// session. `#[serde(default)]` on `id` so a hand-edited entry missing it
/// loads cleanly (and self-heals: reconcile treats an empty id as "not
/// found live" and drops the entry next launch).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DormantWindow {
    pub session: String,
    pub index: u32,
    #[serde(default)]
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub groups: Vec<Group>,
    pub dormant: Vec<String>,
    pub dormant_windows: Vec<DormantWindow>,
    pub focus_mode: bool,
    pub default_mode: DefaultMode,
    pub number_dormant_sessions: bool,
    pub clear_dormant_on_attach: bool,
    pub new_group_position: NewGroupPosition,
    pub new_group_color_policy: ColorPolicy,
    pub static_color: String,
    pub active_palette: Vec<String>,
    pub attached_color: String,
    pub attached_color_mode: AttachedColorMode,
    pub border_color: String,
    pub border_color_policy: ColorPolicy,
    pub inbox_icon: String,
    pub remember_expanded_sessions: bool,
    pub expanded: Vec<String>,
    pub session_metric: SessionMetric,
    pub start_focus_mode: StartFocusMode,
    pub dot_color_mode: DotColorMode,
    pub dot_color: String,
    pub shortcut_color: String,
    pub shortcut_visibility: ShortcutVisibility,
    /// Last-known tmux `session_id` (e.g. `"$3"`) for every name currently
    /// tracked in a group's `members`, in `dormant`, or in `expanded`. Used
    /// by `reconcile` to recover tracking across a plain tmux rename
    /// (issue #38) — untracked sessions are never recorded here, since
    /// there's nothing to recover for them.
    pub session_ids: HashMap<String, String>,
}

/// The active palette a fresh `Config` starts with, and the fallback when a
/// loaded config has no (or an empty) `active_palette`.
fn default_active_palette() -> Vec<String> {
    HEADER_COLORS.iter().map(|s| s.to_string()).collect()
}

impl Default for Config {
    fn default() -> Config {
        Config {
            groups: Vec::new(),
            dormant: Vec::new(),
            dormant_windows: Vec::new(),
            focus_mode: false,
            default_mode: DefaultMode::default(),
            number_dormant_sessions: true,
            clear_dormant_on_attach: false,
            new_group_position: NewGroupPosition::default(),
            new_group_color_policy: ColorPolicy::default(),
            static_color: "cyan".to_string(),
            active_palette: default_active_palette(),
            attached_color: "green".to_string(),
            attached_color_mode: AttachedColorMode::default(),
            border_color: "green".to_string(),
            border_color_policy: ColorPolicy::Static,
            inbox_icon: "⊛".to_string(),
            remember_expanded_sessions: false,
            expanded: Vec::new(),
            session_metric: SessionMetric::default(),
            start_focus_mode: StartFocusMode::default(),
            dot_color_mode: DotColorMode::default(),
            dot_color: "green".to_string(),
            shortcut_color: "gray".to_string(),
            shortcut_visibility: ShortcutVisibility::default(),
            session_ids: HashMap::new(),
        }
    }
}

#[derive(serde::Deserialize)]
struct RawGroup {
    name: String,
    #[serde(default)]
    members: Vec<String>,
    #[serde(default)]
    color: String,
    #[serde(default)]
    inbox: bool,
}

#[derive(Deserialize, Default)]
struct RawSettings {
    #[serde(default)]
    default_mode: Option<String>,
    #[serde(default)]
    number_dormant_sessions: Option<bool>,
    #[serde(default)]
    clear_dormant_on_attach: Option<bool>,
    #[serde(default)]
    new_group_position: Option<String>,
    #[serde(default)]
    new_group_color_policy: Option<String>,
    #[serde(default)]
    static_color: Option<String>,
    #[serde(default)]
    active_palette: Option<Vec<String>>,
    #[serde(default)]
    attached_color: Option<String>,
    #[serde(default)]
    attached_color_mode: Option<String>,
    #[serde(default)]
    border_color: Option<String>,
    #[serde(default)]
    border_color_policy: Option<String>,
    #[serde(default)]
    inbox_icon: Option<String>,
    #[serde(default)]
    remember_expanded_sessions: Option<bool>,
    #[serde(default)]
    session_metric: Option<String>,
    #[serde(default)]
    start_focus_mode: Option<String>,
    #[serde(default)]
    dot_color_mode: Option<String>,
    #[serde(default)]
    dot_color: Option<String>,
    #[serde(default)]
    shortcut_color: Option<String>,
    #[serde(default)]
    shortcut_visibility: Option<String>,
}

#[derive(serde::Serialize)]
struct OutSettings {
    default_mode: String,
    number_dormant_sessions: bool,
    clear_dormant_on_attach: bool,
    new_group_position: String,
    new_group_color_policy: String,
    static_color: String,
    active_palette: Vec<String>,
    attached_color: String,
    attached_color_mode: String,
    border_color: String,
    border_color_policy: String,
    inbox_icon: String,
    remember_expanded_sessions: bool,
    session_metric: String,
    start_focus_mode: String,
    dot_color_mode: String,
    dot_color: String,
    shortcut_color: String,
    shortcut_visibility: String,
}

#[derive(Deserialize, Default)]
struct RawConfig {
    // Absent on any file written before config_version existed; defaults to
    // 0, which is treated as "pre-versioning legacy schema".
    #[serde(default)]
    config_version: u32,
    #[serde(default)]
    pinned: Vec<String>, // migration input only, superseded by `groups` at version 1
    #[serde(default)]
    groups: Vec<RawGroup>,
    #[serde(default)]
    manual_order: Vec<String>, // migration input only, superseded by an `inbox`-flagged group at version 3
    // `sort` (v1 and earlier) is intentionally not modeled here: serde
    // ignores unknown fields by default, so a v1 file with `sort = "..."`
    // still loads cleanly, and the value is dropped on next save.
    #[serde(default)]
    dormant: Vec<String>,
    #[serde(default)]
    dormant_windows: Vec<DormantWindow>,
    // migration input only, superseded by `focus_mode` at version 4
    #[serde(default)]
    hide_dormant: bool,
    #[serde(default)]
    focus_mode: bool,
    #[serde(default)]
    settings: RawSettings,
    #[serde(default)]
    expanded: Vec<String>,
    #[serde(default)]
    session_ids: HashMap<String, String>,
}

#[derive(serde::Serialize)]
struct OutGroup {
    name: String,
    members: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    color: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    inbox: bool,
}

#[derive(serde::Serialize)]
struct OutConfig {
    config_version: u32,
    groups: Vec<OutGroup>,
    dormant: Vec<String>,
    dormant_windows: Vec<DormantWindow>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    focus_mode: bool,
    settings: OutSettings,
    expanded: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    session_ids: BTreeMap<String, String>,
}

impl Config {
    pub fn load_from(path: &Path) -> Config {
        let raw: RawConfig = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        Config::migrate(raw)
    }

    // Applies every migration step the loaded file hasn't already been
    // through, gated on `raw.config_version`. Each step should be additive
    // and idempotent-safe within its own version guard so re-running
    // `load_from` on an already-migrated file is a no-op.
    fn migrate(raw: RawConfig) -> Config {
        let mut groups = if raw.config_version < 1 && raw.groups.is_empty() && !raw.pinned.is_empty()
        {
            // v0 -> v1: single legacy `pinned` list becomes one PINNED group.
            vec![Group { name: "PINNED".into(), members: raw.pinned, ..Default::default() }]
        } else {
            raw.groups
                .into_iter()
                .map(|g| Group { name: g.name, members: g.members, color: g.color, inbox: g.inbox })
                .collect()
        };
        if raw.config_version < 3 && !groups.iter().any(|g| g.inbox) {
            // v2 -> v3: manual_order becomes a real, flagged inbox group.
            groups.push(Group {
                name: "INBOX".into(),
                members: raw.manual_order,
                color: "cyan".into(),
                inbox: true,
            });
        }
        ensure_single_inbox(&mut groups);
        ensure_inbox_last(&mut groups);
        let default_mode = raw
            .settings
            .default_mode
            .as_deref()
            .map(DefaultMode::from_config_str)
            .unwrap_or_default();
        let new_group_position = raw
            .settings
            .new_group_position
            .as_deref()
            .map(NewGroupPosition::from_config_str)
            .unwrap_or_default();
        let new_group_color_policy = raw
            .settings
            .new_group_color_policy
            .as_deref()
            .map(ColorPolicy::from_config_str)
            .unwrap_or_default();
        let static_color = raw.settings.static_color.unwrap_or_else(|| "cyan".to_string());
        let attached_color = raw.settings.attached_color.unwrap_or_else(|| "green".to_string());
        let attached_color_mode = raw
            .settings
            .attached_color_mode
            .as_deref()
            .map(AttachedColorMode::from_config_str)
            .unwrap_or_default();
        let border_color = raw.settings.border_color.unwrap_or_else(|| "green".to_string());
        let border_color_policy = raw
            .settings
            .border_color_policy
            .as_deref()
            .map(ColorPolicy::from_config_str)
            .unwrap_or(ColorPolicy::Static);
        let inbox_icon = raw.settings.inbox_icon.unwrap_or_else(|| "⊛".to_string());
        let active_palette = raw
            .settings
            .active_palette
            .filter(|p| !p.is_empty())
            .unwrap_or_else(default_active_palette);
        let session_metric = raw
            .settings
            .session_metric
            .as_deref()
            .map(SessionMetric::from_config_str)
            .unwrap_or_default();
        let start_focus_mode = raw
            .settings
            .start_focus_mode
            .as_deref()
            .map(StartFocusMode::from_config_str)
            .unwrap_or_default();
        let dot_color_mode = raw
            .settings
            .dot_color_mode
            .as_deref()
            .map(DotColorMode::from_config_str)
            .unwrap_or_default();
        let dot_color = raw.settings.dot_color.unwrap_or_else(|| "green".to_string());
        let shortcut_color = raw.settings.shortcut_color.unwrap_or_else(|| "gray".to_string());
        let shortcut_visibility = raw
            .settings
            .shortcut_visibility
            .as_deref()
            .map(ShortcutVisibility::from_config_str)
            .unwrap_or_default();
        let focus_mode = if raw.config_version < 4 { raw.hide_dormant } else { raw.focus_mode };
        Config {
            groups,
            dormant: raw.dormant,
            dormant_windows: raw.dormant_windows,
            focus_mode,
            default_mode,
            number_dormant_sessions: raw.settings.number_dormant_sessions.unwrap_or(true),
            clear_dormant_on_attach: raw.settings.clear_dormant_on_attach.unwrap_or(false),
            new_group_position,
            new_group_color_policy,
            static_color,
            active_palette,
            attached_color,
            attached_color_mode,
            border_color,
            border_color_policy,
            inbox_icon,
            remember_expanded_sessions: raw.settings.remember_expanded_sessions.unwrap_or(false),
            expanded: raw.expanded,
            session_metric,
            start_focus_mode,
            dot_color_mode,
            dot_color,
            shortcut_color,
            shortcut_visibility,
            session_ids: raw.session_ids,
        }
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut dormant = self.dormant.clone();
        dormant.sort();
        let mut dormant_windows = self.dormant_windows.clone();
        dormant_windows.sort_by(|a, b| a.session.cmp(&b.session).then(a.index.cmp(&b.index)));
        let mut expanded = self.expanded.clone();
        expanded.sort();
        let session_ids: BTreeMap<String, String> = self.session_ids.clone().into_iter().collect();
        let out = OutConfig {
            config_version: CONFIG_VERSION,
            groups: self
                .groups
                .iter()
                .filter(|g| !g.name.is_empty())
                .map(|g| OutGroup {
                    name: g.name.clone(),
                    members: g.members.clone(),
                    color: g.color.clone(),
                    inbox: g.inbox,
                })
                .collect(),
            dormant,
            dormant_windows,
            focus_mode: self.focus_mode,
            settings: OutSettings {
                default_mode: self.default_mode.as_config_str().to_string(),
                number_dormant_sessions: self.number_dormant_sessions,
                clear_dormant_on_attach: self.clear_dormant_on_attach,
                new_group_position: self.new_group_position.as_config_str().to_string(),
                new_group_color_policy: self.new_group_color_policy.as_config_str().to_string(),
                static_color: self.static_color.clone(),
                active_palette: self.active_palette.clone(),
                attached_color: self.attached_color.clone(),
                attached_color_mode: self.attached_color_mode.as_config_str().to_string(),
                border_color: self.border_color.clone(),
                border_color_policy: self.border_color_policy.as_config_str().to_string(),
                inbox_icon: self.inbox_icon.clone(),
                remember_expanded_sessions: self.remember_expanded_sessions,
                session_metric: self.session_metric.as_config_str().to_string(),
                start_focus_mode: self.start_focus_mode.as_config_str().to_string(),
                dot_color_mode: self.dot_color_mode.as_config_str().to_string(),
                dot_color: self.dot_color.clone(),
                shortcut_color: self.shortcut_color.clone(),
                shortcut_visibility: self.shortcut_visibility.as_config_str().to_string(),
            },
            expanded,
            session_ids,
        };
        let body = toml::to_string(&out).map_err(io::Error::other)?;
        std::fs::write(path, body)
    }

    fn tracked_names(&self) -> HashSet<String> {
        let mut names: HashSet<String> = HashSet::new();
        for g in &self.groups {
            names.extend(g.members.iter().cloned());
        }
        names.extend(self.dormant.iter().cloned());
        names.extend(self.expanded.iter().cloned());
        names
    }

    pub fn reconcile(&mut self, live: &[(String, String)], live_windows: &[(String, u32, String)]) -> bool {
        let live_by_name: HashMap<&str, &str> =
            live.iter().map(|(n, i)| (n.as_str(), i.as_str())).collect();
        let live_by_id: HashMap<&str, &str> =
            live.iter().map(|(n, i)| (i.as_str(), n.as_str())).collect();

        // A tracked name that's gone dark: if its last-known id now belongs to
        // a different live name, that's a plain-tmux rename, not a dead
        // session -- carry the tracking forward under the new name.
        let mut renames: HashMap<String, String> = HashMap::new();
        for name in self.tracked_names() {
            if live_by_name.contains_key(name.as_str()) {
                continue;
            }
            if let Some(id) = self.session_ids.get(&name) {
                if let Some(&new_name) = live_by_id.get(id.as_str()) {
                    if new_name != name {
                        renames.insert(name, new_name.to_string());
                    }
                }
            }
        }

        let before: usize = self.groups.iter().map(|g| g.members.len()).sum::<usize>()
            + self.dormant.len()
            + self.expanded.len();

        for g in &mut self.groups {
            for m in &mut g.members {
                if let Some(new_name) = renames.get(m) {
                    *m = new_name.clone();
                }
            }
        }
        for name in &mut self.dormant {
            if let Some(new_name) = renames.get(name) {
                *name = new_name.clone();
            }
        }
        for name in &mut self.expanded {
            if let Some(new_name) = renames.get(name) {
                *name = new_name.clone();
            }
        }

        let is_live = |name: &String| live_by_name.contains_key(name.as_str());
        for g in &mut self.groups {
            g.members.retain(&is_live);
            dedup_preserving_order(&mut g.members);
        }
        self.dormant.retain(&is_live);
        dedup_preserving_order(&mut self.dormant);
        self.expanded.retain(&is_live);
        dedup_preserving_order(&mut self.expanded);

        let after: usize = self.groups.iter().map(|g| g.members.len()).sum::<usize>()
            + self.dormant.len()
            + self.expanded.len();

        let mut new_ids: HashMap<String, String> = HashMap::new();
        for name in self.tracked_names() {
            if let Some(&id) = live_by_name.get(name.as_str()) {
                new_ids.insert(name, id.to_string());
            }
        }
        let ids_changed = new_ids != self.session_ids;
        self.session_ids = new_ids;

        let windows_before = self.dormant_windows.clone();
        let mut live_window_by_loc: HashMap<(&str, u32), &str> = HashMap::new();
        for (name, idx, id) in live_windows {
            live_window_by_loc.insert((name.as_str(), *idx), id.as_str());
        }
        let mut new_dormant_windows: Vec<DormantWindow> = Vec::new();
        for w in &self.dormant_windows {
            let still_at_recorded_spot =
                live_window_by_loc.get(&(w.session.as_str(), w.index)) == Some(&w.id.as_str());
            if still_at_recorded_spot {
                new_dormant_windows.push(w.clone());
                continue;
            }
            if let Some((new_session, new_index, _)) =
                live_windows.iter().find(|(_, _, id)| id == &w.id)
            {
                new_dormant_windows.push(DormantWindow {
                    session: new_session.clone(),
                    index: *new_index,
                    id: w.id.clone(),
                });
            }
            // else: the id isn't live anywhere -- the window is genuinely gone, drop it.
        }
        self.dormant_windows = new_dormant_windows;
        let windows_changed = self.dormant_windows != windows_before;

        before != after || !renames.is_empty() || ids_changed || windows_changed
    }
}

fn dedup_preserving_order(v: &mut Vec<String>) {
    let mut seen = HashSet::new();
    v.retain(|x| seen.insert(x.clone()));
}

pub fn config_path() -> PathBuf {
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x).join("rolomux").join("config.toml");
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config").join("rolomux").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let cfg = Config::load_from(Path::new("/nonexistent/rolomux/config.toml"));
        assert_eq!(cfg.groups.len(), 1);
        assert!(cfg.groups[0].inbox);
        assert!(cfg.dormant.is_empty());
        assert!(!cfg.focus_mode);
        assert!(cfg.number_dormant_sessions);
    }

    #[test]
    fn round_trips_dormant_sessions() {
        let dir = std::env::temp_dir().join(format!("rolomux-dormant-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            groups: vec![],
            dormant: vec!["zebra".into(), "alpha".into()],
            ..Default::default()
        };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        // Saved sorted for a stable diff, regardless of insertion order.
        assert_eq!(reloaded.dormant, vec!["alpha".to_string(), "zebra".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn session_ids_round_trips_through_toml() {
        let dir = std::env::temp_dir().join(format!("rolomux-sessionids-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            session_ids: HashMap::from([("work".to_string(), "$3".to_string())]),
            ..Default::default()
        };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert_eq!(reloaded.session_ids.get("work"), Some(&"$3".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_config_without_session_ids_defaults_to_empty_map() {
        let dir = std::env::temp_dir().join(format!("rolomux-nosessionids-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "config_version = 3\n").unwrap();
        let cfg = Config::load_from(&path);
        assert!(cfg.session_ids.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_session_ids_are_omitted_from_saved_toml() {
        let dir = std::env::temp_dir().join(format!("rolomux-emptysessionids-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        Config::default().save_to(&path).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(!written.contains("session_ids"), "empty map should be skipped: {written}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_focus_mode_preference() {
        let dir = std::env::temp_dir().join(format!("rolomux-focus-mode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config { focus_mode: true, ..Default::default() };
        cfg.save_to(&path).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("focus_mode = true"));
        let reloaded = Config::load_from(&path);
        assert!(reloaded.focus_mode);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hide_dormant_migrates_to_focus_mode_preserving_value() {
        let dir = std::env::temp_dir().join(format!("rolomux-mig-v3-focus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "config_version = 3\nhide_dormant = true\n").unwrap();
        let cfg = Config::load_from(&path);
        assert!(cfg.focus_mode, "legacy hide_dormant value carries into focus_mode");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn current_version_config_ignores_stray_legacy_hide_dormant_key() {
        let dir = std::env::temp_dir().join(format!("rolomux-nomig-focus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            format!("config_version = {CONFIG_VERSION}\nhide_dormant = true\nfocus_mode = false\n"),
        )
        .unwrap();
        let cfg = Config::load_from(&path);
        assert!(!cfg.focus_mode, "current-version file reads focus_mode, not the stale legacy key");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reconcile_drops_dead_dormant_entries() {
        let mut cfg = Config {
            groups: vec![],
            dormant: vec!["a".into(), "gone".into()],
            ..Default::default()
        };
        let changed = cfg.reconcile(&live_ids(&["a"]), &[]);
        assert!(changed);
        assert_eq!(cfg.dormant, vec!["a".to_string()]);
    }

    #[test]
    fn load_then_save_round_trips_pins() {
        let dir = std::env::temp_dir().join(format!("rolomux-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "pinned = [\"pr-review\", \"my session\"]\n").unwrap();

        // Legacy pinned field migrates to a single PINNED group, plus a synthesized INBOX.
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 2);
        assert_eq!(cfg.groups[0].name, "PINNED");
        assert_eq!(
            cfg.groups[0].members,
            vec!["pr-review".to_string(), "my session".to_string()]
        );
        assert!(cfg.groups[1].inbox);

        let out = dir.join("out.toml");
        cfg.save_to(&out).unwrap();
        let reloaded = Config::load_from(&out);
        assert_eq!(reloaded.groups, cfg.groups);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_manual_order() {
        let dir = std::env::temp_dir().join(format!("rolomux-manual-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "pinned = []\nmanual_order = [\"a\", \"my session\"]\n").unwrap();

        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 1);
        assert!(cfg.groups[0].inbox);
        assert_eq!(cfg.groups[0].members, vec!["a".to_string(), "my session".to_string()]);

        let out = dir.join("out.toml");
        cfg.save_to(&out).unwrap();
        let reloaded = Config::load_from(&out);
        assert_eq!(reloaded.groups[0].members, cfg.groups[0].members);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reconcile_drops_dead_manual_order_entries() {
        let mut cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "INBOX".into(), members: vec!["a".into(), "gone".into(), "b".into()], inbox: true, ..Default::default() }
            ],
            ..Default::default()
        };
        let live = live_ids(&["a", "b"]);
        let changed = cfg.reconcile(&live, &[]);
        assert!(changed);
        assert_eq!(cfg.groups[0].members, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn legacy_pinned_migrates_to_single_group() {
        let dir = std::env::temp_dir().join(format!("rolomux-mig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "pinned = [\"a\", \"b\"]\nsort = \"activity\"\n").unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 2);
        assert_eq!(cfg.groups[0].name, "PINNED");
        assert_eq!(cfg.groups[0].members, vec!["a".to_string(), "b".to_string()]);
        assert!(cfg.groups[1].inbox);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_stamps_current_config_version() {
        let dir = std::env::temp_dir().join(format!("rolomux-ver-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config { groups: vec![], ..Default::default() };
        cfg.save_to(&path).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains(&format!("config_version = {CONFIG_VERSION}")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn v1_sort_field_is_dropped_on_load_and_resave() {
        let dir = std::env::temp_dir().join(format!("rolomux-sortdrop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "config_version = 1\ngroups = [{ name = \"PINNED\", members = [\"a\"] }]\nsort = \"activity\"\n",
        )
        .unwrap();

        // Loads without error despite the stale `sort` key, plus synthesizes INBOX.
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 2);
        assert_eq!(cfg.groups[0].name, "PINNED");
        assert!(cfg.groups[1].inbox);

        // Next save no longer writes `sort` at all.
        let out = dir.join("out.toml");
        cfg.save_to(&out).unwrap();
        let written = std::fs::read_to_string(&out).unwrap();
        assert!(!written.contains("sort"), "sort field should be dropped: {written}");
        assert!(written.contains(&format!("config_version = {CONFIG_VERSION}")));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migration_does_not_rerun_once_versioned() {
        // A file already stamped at the current version is never re-migrated,
        // even if it happens to still carry a stale `pinned` list (e.g. from
        // manual editing). Version gating, not field presence, decides.
        // However, ensure_single_inbox still runs and synthesizes INBOX if needed.
        let dir = std::env::temp_dir().join(format!("rolomux-nomig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            format!("config_version = {CONFIG_VERSION}\npinned = [\"a\"]\nsort = \"activity\"\n"),
        )
        .unwrap();
        let cfg = Config::load_from(&path);
        // No PINNED group (v-current files ignore `pinned`), but INBOX is synthesized.
        assert_eq!(cfg.groups.len(), 1);
        assert!(cfg.groups[0].inbox);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_version_ahead_of_current_loads_without_migration() {
        // A colleague on a newer rolomux writes a higher config_version than
        // this binary knows about; loading it must not panic or misfire an
        // old migration, just read the current-shape fields as-is, plus ensure_single_inbox.
        let dir = std::env::temp_dir().join(format!("rolomux-future-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "config_version = 99\ngroups = [{ name = \"PINNED\", members = [\"a\"] }]\nsort = \"activity\"\n",
        )
        .unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 2);
        assert_eq!(cfg.groups[0].name, "PINNED");
        assert!(cfg.groups[1].inbox);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_named_groups() {
        let dir = std::env::temp_dir().join(format!("rolomux-grp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "CONFIG".into(), members: vec!["claude".into()], color: String::new(), ..Default::default() },
                Group { name: "TOOLS".into(), members: vec![], color: String::new(), ..Default::default() },
                Group { name: "INBOX".into(), members: vec![], inbox: true, ..Default::default() },
            ],
            ..Default::default()
        };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert_eq!(reloaded.groups, cfg.groups);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reconcile_drops_dead_members_but_keeps_empty_group() {
        let mut cfg = Config {
            dormant: vec![], groups: vec![Group { name: "G".into(), members: vec!["a".into(), "gone".into()], color: String::new(), ..Default::default() }],
            ..Default::default()
        };
        let live = live_ids(&["a"]);
        assert!(cfg.reconcile(&live, &[]));
        assert_eq!(cfg.groups[0].members, vec!["a".to_string()]);
        // Even if all members die, the group survives.
        assert!(cfg.reconcile(&[], &[]));
        assert_eq!(cfg.groups.len(), 1);
        assert!(cfg.groups[0].members.is_empty());
    }

    #[test]
    fn default_config_seeds_settings_from_header_colors() {
        let cfg = Config::default();
        assert_eq!(cfg.default_mode, DefaultMode::Command);
        assert!(cfg.number_dormant_sessions);
        assert_eq!(cfg.new_group_position, NewGroupPosition::Bottom);
        assert_eq!(cfg.new_group_color_policy, ColorPolicy::Rotate);
        assert_eq!(cfg.static_color, "cyan");
        assert_eq!(cfg.attached_color, "green");
        assert_eq!(cfg.border_color, "green");
        assert_eq!(
            cfg.active_palette,
            HEADER_COLORS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn legacy_config_without_settings_table_defaults_cleanly() {
        let dir = std::env::temp_dir().join(format!("rolomux-nosettings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        // A config_version=1 file from before [settings] existed.
        std::fs::write(&path, "config_version = 1\nsort = \"activity\"\n").unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.default_mode, DefaultMode::Command);
        assert!(cfg.number_dormant_sessions);
        assert_eq!(cfg.new_group_position, NewGroupPosition::Bottom);
        assert_eq!(cfg.new_group_color_policy, ColorPolicy::Rotate);
        assert_eq!(cfg.static_color, "cyan");
        assert_eq!(cfg.attached_color, "green");
        assert_eq!(cfg.border_color, "green");
        assert_eq!(
            cfg.active_palette,
            HEADER_COLORS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migrates_v2_manual_order_into_a_flagged_inbox_group() {
        let dir = std::env::temp_dir().join(format!("rolomux-mig-v2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
config_version = 2
manual_order = ["scratch", "misc-session"]

[[groups]]
name = "WORK"
members = ["proj-a"]
color = "green"
"#,
        )
        .unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 2);
        assert_eq!(cfg.groups[0].name, "WORK");
        assert!(!cfg.groups[0].inbox);
        assert_eq!(cfg.groups[1].name, "INBOX");
        assert!(cfg.groups[1].inbox);
        assert_eq!(cfg.groups[1].members, vec!["scratch".to_string(), "misc-session".to_string()]);
        assert_eq!(cfg.groups[1].color, "cyan");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn v3_file_with_no_flagged_inbox_is_repaired_not_remigrated() {
        let dir = std::env::temp_dir().join(format!("rolomux-mig-v3-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
config_version = 3

[[groups]]
name = "WORK"
members = ["proj-a"]
"#,
        )
        .unwrap();
        let cfg = Config::load_from(&path);
        // No re-migration from manual_order (there is none); the missing-inbox
        // repair still runs and appends a fresh one rather than promoting WORK.
        assert_eq!(cfg.groups.len(), 2);
        assert_eq!(cfg.groups[0].name, "WORK");
        assert!(!cfg.groups[0].inbox);
        assert_eq!(cfg.groups[1].name, "INBOX");
        assert!(cfg.groups[1].inbox);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn v3_file_with_two_flagged_inboxes_keeps_the_first() {
        let dir = std::env::temp_dir().join(format!("rolomux-mig-v3-dup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
config_version = 3

[[groups]]
name = "FIRST"
inbox = true

[[groups]]
name = "SECOND"
inbox = true
"#,
        )
        .unwrap();
        let cfg = Config::load_from(&path);
        // "FIRST" keeps the inbox flag (ensure_single_inbox), then
        // ensure_inbox_last relocates it to the trailing slot.
        assert_eq!(cfg.groups[0].name, "SECOND");
        assert!(!cfg.groups[0].inbox);
        assert_eq!(cfg.groups[1].name, "FIRST");
        assert!(cfg.groups[1].inbox);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_then_load_round_trips_inbox_flag_and_omits_it_for_ordinary_groups() {
        let dir = std::env::temp_dir().join(format!("rolomux-inbox-roundtrip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            groups: vec![
                Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() },
                Group { name: "INBOX".into(), members: vec!["b".into()], inbox: true, ..Default::default() },
            ],
            ..Default::default()
        };
        cfg.save_to(&path).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(!body.contains("manual_order"), "manual_order should no longer be written");
        assert!(body.contains("inbox = true"));
        // Ordinary group's block has no `inbox` key at all.
        let work_block_end = body.find("[[groups]]").unwrap();
        let second_block = body[work_block_end + 10..].find("[[groups]]").map(|i| i + work_block_end + 10);
        let work_block = &body[work_block_end..second_block.unwrap_or(body.len())];
        assert!(!work_block.contains("inbox"));

        let reloaded = Config::load_from(&path);
        assert_eq!(reloaded.groups.len(), 2);
        assert!(!reloaded.groups[0].inbox);
        assert!(reloaded.groups[1].inbox);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_active_palette_on_disk_falls_back_to_default() {
        // Guards the same invariant the settings-mode min-1 UI guard protects at
        // runtime: a hand-edited config can never load a zero-length palette.
        let dir = std::env::temp_dir().join(format!("rolomux-emptypal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "config_version = 1\nsort = \"activity\"\n\n[settings]\nactive_palette = []\n",
        )
        .unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(
            cfg.active_palette,
            HEADER_COLORS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_settings_table() {
        let dir = std::env::temp_dir().join(format!("rolomux-settings-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            default_mode: DefaultMode::Search,
            number_dormant_sessions: false,
            new_group_position: NewGroupPosition::Bottom,
            new_group_color_policy: ColorPolicy::Static,
            static_color: "magenta".to_string(),
            active_palette: vec!["magenta".to_string(), "white".to_string()],
            attached_color: "lightgreen".to_string(),
            border_color: "yellow".to_string(),
            ..Default::default()
        };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert_eq!(reloaded.default_mode, DefaultMode::Search);
        assert!(!reloaded.number_dormant_sessions);
        assert_eq!(reloaded.new_group_position, NewGroupPosition::Bottom);
        assert_eq!(reloaded.new_group_color_policy, ColorPolicy::Static);
        assert_eq!(reloaded.static_color, "magenta");
        assert_eq!(reloaded.attached_color, "lightgreen");
        assert_eq!(reloaded.border_color, "yellow");
        assert_eq!(reloaded.inbox_icon, "⊛");
        assert_eq!(
            reloaded.active_palette,
            vec!["magenta".to_string(), "white".to_string()]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn default_config_has_remember_expanded_sessions_off_and_empty_expanded() {
        let cfg = Config::default();
        assert!(!cfg.remember_expanded_sessions);
        assert!(cfg.expanded.is_empty());
    }

    #[test]
    fn round_trips_remember_expanded_sessions_and_expanded_list() {
        let dir = std::env::temp_dir().join(format!("rolomux-expanded-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            remember_expanded_sessions: true,
            expanded: vec!["zebra".into(), "alpha".into()],
            ..Default::default()
        };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert!(reloaded.remember_expanded_sessions);
        // Saved sorted for a stable diff, regardless of insertion order.
        assert_eq!(reloaded.expanded, vec!["alpha".to_string(), "zebra".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reconcile_drops_dead_expanded_entries() {
        let mut cfg = Config {
            groups: vec![],
            expanded: vec!["a".into(), "gone".into()],
            ..Default::default()
        };
        let changed = cfg.reconcile(&live_ids(&["a"]), &[]);
        assert!(changed);
        assert_eq!(cfg.expanded, vec!["a".to_string()]);
    }

    #[test]
    fn default_config_has_clear_dormant_on_attach_off() {
        let cfg = Config::default();
        assert!(!cfg.clear_dormant_on_attach);
    }

    #[test]
    fn round_trips_clear_dormant_on_attach() {
        let dir = std::env::temp_dir().join(format!("rolomux-clear-dormant-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config { clear_dormant_on_attach: true, ..Default::default() };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert!(reloaded.clear_dormant_on_attach);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn default_config_has_inbox_icon_of_circled_asterisk() {
        let cfg = Config::default();
        assert_eq!(cfg.inbox_icon, "⊛");
    }

    #[test]
    fn round_trips_inbox_icon() {
        let dir = std::env::temp_dir().join(format!("rolomux-inbox-icon-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config { inbox_icon: "♧".to_string(), ..Default::default() };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert_eq!(reloaded.inbox_icon, "♧");
        std::fs::remove_dir_all(&dir).ok();
    }

    fn live_ids(names: &[&str]) -> Vec<(String, String)> {
        names.iter().map(|n| (n.to_string(), format!("id-{n}"))).collect()
    }

    fn live_windows(entries: &[(&str, u32, &str)]) -> Vec<(String, u32, String)> {
        entries.iter().map(|(s, i, id)| (s.to_string(), *i, id.to_string())).collect()
    }

    #[test]
    fn reconcile_detects_rename_via_session_id_and_preserves_group_membership() {
        let mut cfg = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["old-name".into()], ..Default::default() }],
            session_ids: HashMap::from([("old-name".to_string(), "$3".to_string())]),
            ..Default::default()
        };
        let changed = cfg.reconcile(&[("new-name".to_string(), "$3".to_string())], &[]);
        assert!(changed);
        assert_eq!(cfg.groups[0].members, vec!["new-name".to_string()]);
        assert_eq!(cfg.session_ids.get("new-name"), Some(&"$3".to_string()));
        assert!(!cfg.session_ids.contains_key("old-name"));
    }

    #[test]
    fn reconcile_detects_rename_via_session_id_and_preserves_dormant_status() {
        let mut cfg = Config {
            dormant: vec!["old-name".into()],
            session_ids: HashMap::from([("old-name".to_string(), "$5".to_string())]),
            ..Default::default()
        };
        let changed = cfg.reconcile(&[("new-name".to_string(), "$5".to_string())], &[]);
        assert!(changed);
        assert_eq!(cfg.dormant, vec!["new-name".to_string()]);
    }

    #[test]
    fn reconcile_detects_rename_via_session_id_and_preserves_expanded_status() {
        let mut cfg = Config {
            expanded: vec!["old-name".into()],
            session_ids: HashMap::from([("old-name".to_string(), "$7".to_string())]),
            ..Default::default()
        };
        let changed = cfg.reconcile(&[("new-name".to_string(), "$7".to_string())], &[]);
        assert!(changed);
        assert_eq!(cfg.expanded, vec!["new-name".to_string()]);
    }

    #[test]
    fn reconcile_still_drops_dead_session_with_no_matching_live_id() {
        let mut cfg = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["gone".into()], ..Default::default() }],
            session_ids: HashMap::from([("gone".to_string(), "$9".to_string())]),
            ..Default::default()
        };
        // "other" is live but its id doesn't match "gone"'s last-known id, so
        // this is a dead session, not a detected rename.
        let changed = cfg.reconcile(&live_ids(&["other"]), &[]);
        assert!(changed);
        assert!(cfg.groups[0].members.is_empty());
        assert!(cfg.session_ids.is_empty());
    }

    #[test]
    fn reconcile_prunes_session_ids_for_names_no_longer_tracked() {
        let mut cfg = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["a".into()], ..Default::default() }],
            session_ids: HashMap::from([
                ("a".to_string(), "$1".to_string()),
                ("stale".to_string(), "$2".to_string()),
            ]),
            ..Default::default()
        };
        assert!(cfg.reconcile(&live_ids(&["a"]), &[]));
        assert_eq!(cfg.session_ids.len(), 1);
        assert_eq!(cfg.session_ids.get("a"), Some(&"id-a".to_string()));
        assert!(!cfg.session_ids.contains_key("stale"));
    }

    #[test]
    fn reconcile_deduplicates_when_rename_target_collides_with_existing_tracked_entry() {
        let mut cfg = Config {
            groups: vec![Group {
                name: "WORK".into(),
                members: vec!["foo".into(), "oldbar".into()],
                ..Default::default()
            }],
            session_ids: HashMap::from([
                ("foo".to_string(), "$1".to_string()),
                ("oldbar".to_string(), "$2".to_string()),
            ]),
            ..Default::default()
        };
        // "$2" died; "foo"/"$1" was renamed to "oldbar" via plain tmux.
        cfg.reconcile(&[("oldbar".to_string(), "$1".to_string())], &[]);
        assert_eq!(cfg.groups[0].members, vec!["oldbar".to_string()]);
    }

    #[test]
    fn reconcile_drops_a_dormant_window_entry_whose_window_closed() {
        let mut cfg = Config {
            dormant_windows: vec![
                DormantWindow { session: "work".into(), index: 0, id: "@1".into() },
                DormantWindow { session: "work".into(), index: 1, id: "@2".into() },
            ],
            ..Default::default()
        };
        // Window @2 (index 1) closed; @1 is still live at the same spot.
        let changed = cfg.reconcile(
            &live_ids(&["work"]),
            &live_windows(&[("work", 0, "@1")]),
        );
        assert!(changed);
        assert_eq!(cfg.dormant_windows, vec![DormantWindow { session: "work".into(), index: 0, id: "@1".into() }]);
    }

    #[test]
    fn reconcile_relocates_a_dormant_window_entry_whose_index_was_reused() {
        // @2 used to be at index 1; it's now at index 3 (something closed
        // and index 1 got reused by an unrelated new window with a
        // different id) -- the entry must follow its id, not stay pinned
        // to the stale index.
        let mut cfg = Config {
            dormant_windows: vec![DormantWindow { session: "work".into(), index: 1, id: "@2".into() }],
            ..Default::default()
        };
        let changed = cfg.reconcile(
            &live_ids(&["work"]),
            &live_windows(&[("work", 1, "@99"), ("work", 3, "@2")]),
        );
        assert!(changed);
        assert_eq!(cfg.dormant_windows, vec![DormantWindow { session: "work".into(), index: 3, id: "@2".into() }]);
    }

    #[test]
    fn reconcile_relocates_a_dormant_window_entry_moved_to_a_different_session() {
        let mut cfg = Config {
            dormant_windows: vec![DormantWindow { session: "work".into(), index: 0, id: "@7".into() }],
            ..Default::default()
        };
        let changed = cfg.reconcile(
            &live_ids(&["work", "personal"]),
            &live_windows(&[("personal", 2, "@7")]),
        );
        assert!(changed);
        assert_eq!(cfg.dormant_windows, vec![DormantWindow { session: "personal".into(), index: 2, id: "@7".into() }]);
    }

    #[test]
    fn reconcile_leaves_dormant_windows_untouched_when_nothing_changed() {
        let mut cfg = Config {
            dormant_windows: vec![DormantWindow { session: "work".into(), index: 0, id: "@1".into() }],
            ..Default::default()
        };
        let changed = cfg.reconcile(&live_ids(&["work"]), &live_windows(&[("work", 0, "@1")]));
        assert!(!changed);
        assert_eq!(cfg.dormant_windows, vec![DormantWindow { session: "work".into(), index: 0, id: "@1".into() }]);
    }

    #[test]
    fn legacy_config_without_expanded_settings_defaults_cleanly() {
        let dir = std::env::temp_dir().join(format!("rolomux-noexpanded-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        // A config_version=1 file from before this setting existed.
        std::fs::write(&path, "config_version = 1\nsort = \"activity\"\n").unwrap();
        let cfg = Config::load_from(&path);
        assert!(!cfg.remember_expanded_sessions);
        assert!(cfg.expanded.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn default_config_has_session_metric_recency() {
        let cfg = Config::default();
        assert_eq!(cfg.session_metric, SessionMetric::Recency);
    }

    #[test]
    fn round_trips_session_metric() {
        let dir = std::env::temp_dir().join(format!("rolomux-session-metric-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config { session_metric: SessionMetric::Age, ..Default::default() };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert_eq!(reloaded.session_metric, SessionMetric::Age);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_config_without_session_metric_defaults_to_recency() {
        let dir = std::env::temp_dir().join(format!("rolomux-nometric-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "config_version = 3\n").unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.session_metric, SessionMetric::Recency);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn default_config_has_start_focus_mode_remember() {
        let cfg = Config::default();
        assert_eq!(cfg.start_focus_mode, StartFocusMode::Remember);
    }

    #[test]
    fn round_trips_start_focus_mode() {
        let dir = std::env::temp_dir().join(format!("rolomux-start-focus-mode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config { start_focus_mode: StartFocusMode::Always, ..Default::default() };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert_eq!(reloaded.start_focus_mode, StartFocusMode::Always);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_config_without_start_focus_mode_defaults_to_remember() {
        let dir = std::env::temp_dir().join(format!("rolomux-nostartfocus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "config_version = 4\n").unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.start_focus_mode, StartFocusMode::Remember);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_attached_color_mode() {
        let dir = std::env::temp_dir().join(format!("rolomux-attached-color-mode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config { attached_color_mode: AttachedColorMode::Match, ..Default::default() };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert_eq!(reloaded.attached_color_mode, AttachedColorMode::Match);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn border_color_policy_defaults_to_static_on_a_fresh_config() {
        let cfg = Config::default();
        assert_eq!(cfg.border_color_policy, ColorPolicy::Static);
    }

    #[test]
    fn legacy_config_without_border_color_policy_defaults_to_static() {
        let dir = std::env::temp_dir().join(format!("rolomux-noborderpolicy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "config_version = 4\n").unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.border_color_policy, ColorPolicy::Static);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_border_color_policy() {
        let dir = std::env::temp_dir().join(format!("rolomux-border-color-policy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config { border_color_policy: ColorPolicy::Rotate, ..Default::default() };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert_eq!(reloaded.border_color_policy, ColorPolicy::Rotate);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_config_without_attached_color_mode_defaults_to_static() {
        let dir = std::env::temp_dir().join(format!("rolomux-noattachedmode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "config_version = 4\n").unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.attached_color_mode, AttachedColorMode::Static);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_dormant_windows() {
        let dir = std::env::temp_dir().join(format!("rolomux-dormant-windows-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            dormant_windows: vec![
                DormantWindow { session: "work".into(), index: 2, id: "@5".into() },
                DormantWindow { session: "scratch".into(), index: 0, id: "@1".into() },
            ],
            ..Default::default()
        };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert_eq!(
            reloaded.dormant_windows,
            vec![
                DormantWindow { session: "scratch".into(), index: 0, id: "@1".into() },
                DormantWindow { session: "work".into(), index: 2, id: "@5".into() },
            ],
            "round-trips through save/load, sorted by (session, index) for a stable diff"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_dormant_windows_field_defaults_to_empty() {
        let dir = std::env::temp_dir().join(format!("rolomux-no-dormant-windows-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, format!("config_version = {CONFIG_VERSION}\n")).unwrap();
        let cfg = Config::load_from(&path);
        assert!(cfg.dormant_windows.is_empty(), "a config written before this feature loads cleanly");
        std::fs::remove_dir_all(&dir).ok();
    }
}
