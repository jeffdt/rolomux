use crate::model::{Group, SortKey};
use serde::Deserialize;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub groups: Vec<Group>,
    pub manual_order: Vec<String>,
    pub sort: SortKey,
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
struct RawConfig {
    #[serde(default)]
    pinned: Vec<String>, // legacy migration input only
    #[serde(default)]
    groups: Vec<RawGroup>,
    #[serde(default)]
    manual_order: Vec<String>,
    #[serde(default)]
    sort: Option<String>,
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
    groups: Vec<OutGroup>,
    manual_order: Vec<String>,
    sort: String,
}

impl Config {
    pub fn load_from(path: &Path) -> Config {
        let raw: RawConfig = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        let groups = if raw.groups.is_empty() && !raw.pinned.is_empty() {
            vec![Group { name: "PINNED".into(), members: raw.pinned, color: String::new() }]
        } else {
            raw.groups
                .into_iter()
                .map(|g| Group { name: g.name, members: g.members, color: g.color })
                .collect()
        };
        Config {
            groups,
            manual_order: raw.manual_order,
            sort: raw
                .sort
                .map(|s| SortKey::from_config_str(&s))
                .unwrap_or_default(),
        }
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let out = OutConfig {
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
            sort: match self.sort {
                SortKey::Activity => "activity".into(),
                SortKey::Created => "created".into(),
                SortKey::Manual => "manual".into(),
            },
        };
        let body = toml::to_string(&out).map_err(io::Error::other)?;
        std::fs::write(path, body)
    }

    pub fn reconcile(&mut self, live_names: &[String]) -> bool {
        let is_live = |name: &String| live_names.iter().any(|n| n == name);
        let before: usize =
            self.groups.iter().map(|g| g.members.len()).sum::<usize>() + self.manual_order.len();
        for g in &mut self.groups {
            g.members.retain(&is_live);
        }
        self.manual_order.retain(&is_live);
        let after: usize =
            self.groups.iter().map(|g| g.members.len()).sum::<usize>() + self.manual_order.len();
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
        assert_eq!(cfg.sort, SortKey::Activity);
    }

    #[test]
    fn load_then_save_round_trips_pins_and_sort() {
        let dir = std::env::temp_dir().join(format!("smux-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "pinned = [\"pr-review\", \"my session\"]\nsort = \"created\"\n",
        )
        .unwrap();

        // Legacy pinned field migrates to a single PINNED group.
        let cfg = Config::load_from(&path);
        assert_eq!(cfg.groups.len(), 1);
        assert_eq!(cfg.groups[0].name, "PINNED");
        assert_eq!(
            cfg.groups[0].members,
            vec!["pr-review".to_string(), "my session".to_string()]
        );
        assert_eq!(cfg.sort, SortKey::Created);

        let out = dir.join("out.toml");
        cfg.save_to(&out).unwrap();
        let reloaded = Config::load_from(&out);
        assert_eq!(reloaded.groups, cfg.groups);
        assert_eq!(reloaded.sort, SortKey::Created);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_manual_sort_and_order() {
        let dir = std::env::temp_dir().join(format!("smux-manual-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "pinned = []\nmanual_order = [\"a\", \"my session\"]\nsort = \"manual\"\n",
        )
        .unwrap();

        let cfg = Config::load_from(&path);
        assert_eq!(cfg.sort, SortKey::Manual);
        assert_eq!(cfg.manual_order, vec!["a".to_string(), "my session".to_string()]);

        let out = dir.join("out.toml");
        cfg.save_to(&out).unwrap();
        let reloaded = Config::load_from(&out);
        assert_eq!(reloaded.sort, SortKey::Manual);
        assert_eq!(reloaded.manual_order, cfg.manual_order);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reconcile_drops_dead_manual_order_entries() {
        let mut cfg = Config {
            groups: vec![],
            manual_order: vec!["a".into(), "gone".into(), "b".into()],
            sort: SortKey::Manual,
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
    fn round_trips_named_groups() {
        let dir = std::env::temp_dir().join(format!("smux-grp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = Config {
            groups: vec![
                Group { name: "CONFIG".into(), members: vec!["claude".into()], color: String::new() },
                Group { name: "TOOLS".into(), members: vec![], color: String::new() },
            ],
            manual_order: vec![],
            sort: SortKey::Manual,
        };
        cfg.save_to(&path).unwrap();
        let reloaded = Config::load_from(&path);
        assert_eq!(reloaded.groups, cfg.groups);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reconcile_drops_dead_members_but_keeps_empty_group() {
        let mut cfg = Config {
            groups: vec![Group { name: "G".into(), members: vec!["a".into(), "gone".into()], color: String::new() }],
            manual_order: vec![],
            sort: SortKey::Manual,
        };
        let live = vec!["a".to_string()];
        assert!(cfg.reconcile(&live));
        assert_eq!(cfg.groups[0].members, vec!["a".to_string()]);
        // Even if all members die, the group survives.
        assert!(cfg.reconcile(&[]));
        assert_eq!(cfg.groups.len(), 1);
        assert!(cfg.groups[0].members.is_empty());
    }
}
