use crate::model::{Action, Session, Window};
use std::io;
use std::process::Command;

pub const FMT: &str = "#{session_name}\x1f#{session_activity}\x1f#{session_created}\x1f#{session_attached}\x1f#{window_index}\x1f#{window_name}\x1f#{window_active}";

pub trait Tmux {
    fn gather(&self) -> Vec<Session>;
    fn switch_session(&self, name: &str) -> io::Result<()>;
    fn select_window(&self, name: &str, index: u32) -> io::Result<()>;
}

pub struct RealTmux;

impl Tmux for RealTmux {
    fn gather(&self) -> Vec<Session> {
        let out = Command::new("tmux")
            .args(["list-windows", "-a", "-F", FMT])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                parse_windows(&String::from_utf8_lossy(&o.stdout))
            }
            _ => Vec::new(),
        }
    }

    fn switch_session(&self, name: &str) -> io::Result<()> {
        Command::new("tmux")
            .args(["switch-client", "-t", name])
            .status()
            .map(|_| ())
    }

    fn select_window(&self, name: &str, index: u32) -> io::Result<()> {
        let target = format!("{name}:{index}");
        Command::new("tmux")
            .args(["select-window", "-t", &target])
            .status()
            .map(|_| ())
    }
}

pub fn apply_action(t: &dyn Tmux, action: &Action) -> io::Result<()> {
    match action {
        Action::SwitchSession(name) => t.switch_session(name),
        Action::SwitchWindow(name, index) => {
            t.switch_session(name)?;
            t.select_window(name, *index)
        }
    }
}

pub fn parse_windows(raw: &str) -> Vec<Session> {
    let mut sessions: Vec<Session> = Vec::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\u{1f}').collect();
        if f.len() != 7 {
            continue;
        }
        let name = f[0].to_string();
        let window = Window {
            index: f[4].parse().unwrap_or(0),
            name: f[5].to_string(),
            active: f[6] == "1",
        };
        if let Some(s) = sessions.iter_mut().find(|s| s.name == name) {
            s.windows.push(window);
        } else {
            sessions.push(Session {
                name,
                activity: f[1].parse().unwrap_or(0),
                created: f[2].parse().unwrap_or(0),
                attached: f[3] == "1",
                windows: vec![window],
            });
        }
    }
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Action;
    use std::cell::RefCell;

    #[test]
    fn parses_two_sessions_grouping_windows_in_order() {
        // Fields separated by the unit separator \x1f; one line per window.
        let raw = "\
work\u{1f}100\u{1f}10\u{1f}1\u{1f}0\u{1f}editor\u{1f}1
work\u{1f}100\u{1f}10\u{1f}1\u{1f}1\u{1f}my logs\u{1f}0
scratch\u{1f}50\u{1f}5\u{1f}0\u{1f}0\u{1f}shell\u{1f}1
";
        let sessions = parse_windows(raw);
        assert_eq!(
            sessions,
            vec![
                Session {
                    name: "work".into(),
                    activity: 100,
                    created: 10,
                    attached: true,
                    windows: vec![
                        Window { index: 0, name: "editor".into(), active: true },
                        Window { index: 1, name: "my logs".into(), active: false },
                    ],
                },
                Session {
                    name: "scratch".into(),
                    activity: 50,
                    created: 5,
                    attached: false,
                    windows: vec![Window { index: 0, name: "shell".into(), active: true }],
                },
            ]
        );
    }

    #[derive(Default)]
    struct FakeTmux {
        calls: RefCell<Vec<String>>,
    }
    impl Tmux for FakeTmux {
        fn gather(&self) -> Vec<Session> {
            Vec::new()
        }
        fn switch_session(&self, name: &str) -> std::io::Result<()> {
            self.calls.borrow_mut().push(format!("switch:{name}"));
            Ok(())
        }
        fn select_window(&self, name: &str, index: u32) -> std::io::Result<()> {
            self.calls.borrow_mut().push(format!("select:{name}:{index}"));
            Ok(())
        }
    }

    #[test]
    fn apply_switch_session_calls_switch_only() {
        let t = FakeTmux::default();
        apply_action(&t, &Action::SwitchSession("work".into())).unwrap();
        assert_eq!(*t.calls.borrow(), vec!["switch:work"]);
    }

    #[test]
    fn apply_switch_window_switches_then_selects() {
        let t = FakeTmux::default();
        apply_action(&t, &Action::SwitchWindow("work".into(), 2)).unwrap();
        assert_eq!(*t.calls.borrow(), vec!["switch:work", "select:work:2"]);
    }
}
