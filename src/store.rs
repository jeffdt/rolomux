use crate::model::SortKey;
use serde::Deserialize;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub pinned: Vec<String>,
    pub manual_order: Vec<String>,
    pub sort: SortKey,
}

#[derive(Deserialize, Default)]
struct RawConfig {
    #[serde(default)]
    pinned: Vec<String>,
    #[serde(default)]
    manual_order: Vec<String>,
    #[serde(default)]
    sort: Option<String>,
}

#[derive(serde::Serialize)]
struct OutConfig {
    pinned: Vec<String>,
    manual_order: Vec<String>,
    sort: String,
}

impl Config {
    pub fn load_from(path: &Path) -> Config {
        let raw: RawConfig = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        Config {
            pinned: raw.pinned,
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
            pinned: self.pinned.clone(),
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
        let before = (self.pinned.len(), self.manual_order.len());
        self.pinned.retain(&is_live);
        self.manual_order.retain(&is_live);
        before != (self.pinned.len(), self.manual_order.len())
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
        assert!(cfg.pinned.is_empty());
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

        let cfg = Config::load_from(&path);
        assert_eq!(cfg.pinned, vec!["pr-review".to_string(), "my session".to_string()]);
        assert_eq!(cfg.sort, SortKey::Created);

        let out = dir.join("out.toml");
        cfg.save_to(&out).unwrap();
        let reloaded = Config::load_from(&out);
        assert_eq!(reloaded.pinned, cfg.pinned);
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
            pinned: vec![],
            manual_order: vec!["a".into(), "gone".into(), "b".into()],
            sort: SortKey::Manual,
        };
        let live = vec!["a".to_string(), "b".to_string()];
        let changed = cfg.reconcile(&live);
        assert!(changed);
        assert_eq!(cfg.manual_order, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn reconcile_drops_dead_pins_and_reports_change() {
        let mut cfg = Config {
            pinned: vec!["a".into(), "gone".into(), "b".into()],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
        let live = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let changed = cfg.reconcile(&live);
        assert!(changed);
        assert_eq!(cfg.pinned, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn reconcile_no_change_when_all_pins_live() {
        let mut cfg = Config {
            pinned: vec!["a".into(), "b".into()],
            manual_order: vec![],
            sort: SortKey::Activity,
        };
        let live = vec!["a".to_string(), "b".to_string()];
        assert!(!cfg.reconcile(&live));
        assert_eq!(cfg.pinned, vec!["a".to_string(), "b".to_string()]);
    }
}
