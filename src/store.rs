use crate::model::{ColorPolicy, DefaultMode, Group, HEADER_COLORS};
use serde::Deserialize;
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
pub const CONFIG_VERSION: u32 = 2;

#[derive(Debug, Clone)]
pub struct Config {
    pub groups: Vec<Group>,
    pub manual_order: Vec<String>,
    pub dormant: Vec<String>,
    pub default_mode: DefaultMode,
    pub new_group_color_policy: ColorPolicy,
    pub static_color: String,
    pub active_palette: Vec<String>,
    pub attached_color: String,
    pub border_color: String,
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
            manual_order: Vec::new(),
            dormant: Vec::new(),
            default_mode: DefaultMode::default(),
            new_group_color_policy: ColorPolicy::default(),
            static_color: "cyan".to_string(),
            active_palette: default_active_palette(),
            attached_color: "cyan".to_string(),
            border_color: "cyan".to_string(),
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
}

#[derive(Deserialize, Default)]
struct RawSettings {
    #[serde(default)]
    default_mode: Option<String>,
    #[serde(default)]
    new_group_color_policy: Option<String>,
    #[serde(default)]
    static_color: Option<String>,
    #[serde(default)]
    active_palette: Option<Vec<String>>,
    #[serde(default)]
    attached_color: Option<String>,
    #[serde(default)]
    border_color: Option<String>,
}

#[derive(serde::Serialize)]
struct OutSettings {
    default_mode: String,
    new_group_color_policy: String,
    static_color: String,
    active_palette: Vec<String>,
    attached_color: String,
    border_color: String,
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
    manual_order: Vec<String>,
    // `sort` (v1 and earlier) is intentionally not modeled here: serde
    // ignores unknown fields by default, so a v1 file with `sort = "..."`
    // still loads cleanly, and the value is dropped on next save.
    #[serde(default)]
    dormant: Vec<String>,
    #[serde(default)]
    settings: RawSettings,
}

#[derive(serde::Serialize)]
struct OutGroup {
    name: String,
    members: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    color: String,
}

#[derive(serde::Serialize)]
struct OutConfig {
    config_version: u32,
    groups: Vec<OutGroup>,
    manual_order: Vec<String>,
    dormant: Vec<String>,
    settings: OutSettings,
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
        let groups = if raw.config_version < 1 && raw.groups.is_empty() && !raw.pinned.is_empty()
        {
            // v0 -> v1: single legacy `pinned` list becomes one PINNED group.
            vec![Group { name: "PINNED".into(), members: raw.pinned, color: String::new() }]
        } else {
            raw.groups
                .into_iter()
                .map(|g| Group { name: g.name, members: g.members, color: g.color })
                .collect()
        };
        let default_mode = raw
            .settings
            .default_mode
            .as_deref()
            .map(DefaultMode::from_config_str)
            .unwrap_or_default();
        let new_group_color_policy = raw
            .settings
            .new_group_color_policy
            .as_deref()
            .map(ColorPolicy::from_config_str)
            .unwrap_or_default();
        let static_color = raw.settings.static_color.unwrap_or_else(|| "cyan".to_string());
        let attached_color = raw.settings.attached_color.unwrap_or_else(|| "cyan".to_string());
        let border_color = raw.settings.border_color.unwrap_or_else(|| "cyan".to_string());
        let active_palette = raw
            .settings
            .active_palette
            .filter(|p| !p.is_empty())
            .unwrap_or_else(default_active_palette);
        Config {
            groups,
            manual_order: raw.manual_order,
            dormant: raw.dormant,
            default_mode,
            new_group_color_policy,
            static_color,
            active_palette,
            attached_color,
            border_color,
        }
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut dormant = self.dormant.clone();
        dormant.sort();
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
                })
                .collect(),
            manual_order: self.manual_order.clone(),
            dormant,
            settings: OutSettings {
                default_mode: self.default_mode.as_config_str().to_string(),
                new_group_color_policy: self.new_group_color_policy.as_config_str().to_string(),
                static_color: self.static_color.clone(),
                active_palette: self.active_palette.clone(),
                attached_color: self.attached_color.clone(),
                border_color: self.border_color.clone(),
            },
        };
        let body = toml::to_string(&out).map_err(io::Error::other)?;
        std::fs::write(path, body)
    }

    pub fn reconcile(&mut self, live_names: &[String]) -> bool {
        let is_live = |name: &String| live_names.iter().any(|n| n == name);
        let before: usize = self.groups.iter().map(|g| g.members.len()).sum::<usize>()
            + self.manual_order.len()
            + self.dormant.len();
        for g in &mut self.groups {
            g.members.retain(&is_live);
        }
        self.manual_order.retain(&is_live);
        self.dormant.retain(&is_live);
        let after: usize = self.groups.iter().map(|g| g.members.len()).sum::<usize>()
            + self.manual_order.len()
            + self.dormant.len();
        before != after
    }
}

pub fn config_path() -> PathBuf {
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x).join("smux").join("config.toml");
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config").join("smux").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let cfg = Config::load_from(Path::new("/nonexistent/smux/config.toml"));
        assert!(cfg.groups.is_empty());
        assert!(cfg.dormant.is_empty());
    }

    #[test]
    fn round_trips_dormant_sessions() {
        let dir = std::env::temp_dir().join(format!("smux-dormant-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            groups: vec![],
            manual_order: vec![],
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
    fn reconcile_drops_dead_dormant_entries() {
        let mut cfg = Config {
            groups: vec![],
            manual_order: vec![],
            dormant: vec!["a".into(), "gone".into()],
            ..Default::default()
        };
        let changed = cfg.reconcile(&["a".to_string()]);
        assert!(changed);
        assert_eq!(cfg.dormant, vec!["a".to_string()]);
    }

    #[test]
    fn load_then_save_round_trips_pins() {
        let dir = std::env::temp_dir().join(format!("smux-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "pinned = [\"pr-review\", \"my session\"]\n").unwrap();

        // Legacy pinned field migrates to a single PINNED group.
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 1);
        assert_eq!(cfg.groups[0].name, "PINNED");
        assert_eq!(
            cfg.groups[0].members,
            vec!["pr-review".to_string(), "my session".to_string()]
        );

        let out = dir.join("out.toml");
        cfg.save_to(&out).unwrap();
        let reloaded = Config::load_from(&out);
        assert_eq!(reloaded.groups, cfg.groups);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_manual_order() {
        let dir = std::env::temp_dir().join(format!("smux-manual-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "pinned = []\nmanual_order = [\"a\", \"my session\"]\n").unwrap();

        let cfg = Config::load_from(&path);
        assert_eq!(cfg.manual_order, vec!["a".to_string(), "my session".to_string()]);

        let out = dir.join("out.toml");
        cfg.save_to(&out).unwrap();
        let reloaded = Config::load_from(&out);
        assert_eq!(reloaded.manual_order, cfg.manual_order);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reconcile_drops_dead_manual_order_entries() {
        let mut cfg = Config {
            dormant: vec![], groups: vec![],
            manual_order: vec!["a".into(), "gone".into(), "b".into()],
            ..Default::default()
        };
        let live = vec!["a".to_string(), "b".to_string()];
        let changed = cfg.reconcile(&live);
        assert!(changed);
        assert_eq!(cfg.manual_order, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn legacy_pinned_migrates_to_single_group() {
        let dir = std::env::temp_dir().join(format!("smux-mig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "pinned = [\"a\", \"b\"]\nsort = \"activity\"\n").unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 1);
        assert_eq!(cfg.groups[0].name, "PINNED");
        assert_eq!(cfg.groups[0].members, vec!["a".to_string(), "b".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_stamps_current_config_version() {
        let dir = std::env::temp_dir().join(format!("smux-ver-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config { groups: vec![], manual_order: vec![], ..Default::default() };
        cfg.save_to(&path).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains(&format!("config_version = {CONFIG_VERSION}")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn v1_sort_field_is_dropped_on_load_and_resave() {
        let dir = std::env::temp_dir().join(format!("smux-sortdrop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "config_version = 1\ngroups = [{ name = \"PINNED\", members = [\"a\"] }]\nsort = \"activity\"\n",
        )
        .unwrap();

        // Loads without error despite the stale `sort` key.
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 1);
        assert_eq!(cfg.groups[0].name, "PINNED");

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
        let dir = std::env::temp_dir().join(format!("smux-nomig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            format!("config_version = {CONFIG_VERSION}\npinned = [\"a\"]\nsort = \"activity\"\n"),
        )
        .unwrap();
        let cfg = Config::load_from(&path);
        assert!(cfg.groups.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_version_ahead_of_current_loads_without_migration() {
        // A colleague on a newer smux writes a higher config_version than
        // this binary knows about; loading it must not panic or misfire an
        // old migration, just read the current-shape fields as-is.
        let dir = std::env::temp_dir().join(format!("smux-future-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "config_version = 99\ngroups = [{ name = \"PINNED\", members = [\"a\"] }]\nsort = \"activity\"\n",
        )
        .unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 1);
        assert_eq!(cfg.groups[0].name, "PINNED");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_named_groups() {
        let dir = std::env::temp_dir().join(format!("smux-grp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            dormant: vec![], groups: vec![
                Group { name: "CONFIG".into(), members: vec!["claude".into()], color: String::new() },
                Group { name: "TOOLS".into(), members: vec![], color: String::new() },
            ],
            manual_order: vec![],
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
            dormant: vec![], groups: vec![Group { name: "G".into(), members: vec!["a".into(), "gone".into()], color: String::new() }],
            manual_order: vec![],
            ..Default::default()
        };
        let live = vec!["a".to_string()];
        assert!(cfg.reconcile(&live));
        assert_eq!(cfg.groups[0].members, vec!["a".to_string()]);
        // Even if all members die, the group survives.
        assert!(cfg.reconcile(&[]));
        assert_eq!(cfg.groups.len(), 1);
        assert!(cfg.groups[0].members.is_empty());
    }

    #[test]
    fn default_config_seeds_settings_from_header_colors() {
        let cfg = Config::default();
        assert_eq!(cfg.default_mode, DefaultMode::Command);
        assert_eq!(cfg.new_group_color_policy, ColorPolicy::Rotate);
        assert_eq!(cfg.static_color, "cyan");
        assert_eq!(cfg.attached_color, "cyan");
        assert_eq!(cfg.border_color, "cyan");
        assert_eq!(
            cfg.active_palette,
            HEADER_COLORS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn legacy_config_without_settings_table_defaults_cleanly() {
        let dir = std::env::temp_dir().join(format!("smux-nosettings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        // A config_version=1 file from before [settings] existed.
        std::fs::write(&path, "config_version = 1\nsort = \"activity\"\n").unwrap();
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.default_mode, DefaultMode::Command);
        assert_eq!(cfg.new_group_color_policy, ColorPolicy::Rotate);
        assert_eq!(cfg.static_color, "cyan");
        assert_eq!(cfg.attached_color, "cyan");
        assert_eq!(cfg.border_color, "cyan");
        assert_eq!(
            cfg.active_palette,
            HEADER_COLORS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_active_palette_on_disk_falls_back_to_default() {
        // Guards the same invariant the settings-mode min-1 UI guard protects at
        // runtime: a hand-edited config can never load a zero-length palette.
        let dir = std::env::temp_dir().join(format!("smux-emptypal-{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("smux-settings-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            default_mode: DefaultMode::Search,
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
        assert_eq!(reloaded.new_group_color_policy, ColorPolicy::Static);
        assert_eq!(reloaded.static_color, "magenta");
        assert_eq!(reloaded.attached_color, "lightgreen");
        assert_eq!(reloaded.border_color, "yellow");
        assert_eq!(
            reloaded.active_palette,
            vec!["magenta".to_string(), "white".to_string()]
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
