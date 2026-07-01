use crate::model::{Action, Session, Window};
use std::io;
use std::process::Command;

pub const FMT: &str = "#{session_name}\x1f#{session_activity}\x1f#{session_created}\x1f#{session_attached}\x1f#{window_index}\x1f#{window_name}\x1f#{window_active}\x1f#{session_id}";

/// Result of a single gather: the sessions plus the name of the session the
/// popup was launched from (when it can be resolved from `$TMUX`).
pub struct Gathered {
    pub sessions: Vec<Session>,
    pub current: Option<String>,
}

pub trait Tmux {
    fn gather(&self) -> Gathered;
    fn switch_session(&self, name: &str) -> io::Result<()>;
    fn select_window(&self, name: &str, index: u32) -> io::Result<()>;
}

pub struct RealTmux {
    /// The server socket smux was launched from (`$TMUX`'s first field). `None`
    /// when smux runs outside tmux, in which case tmux's default socket is used.
    socket: Option<String>,
}

impl RealTmux {
    /// Bind to the tmux server smux was launched from, resolved from `$TMUX`.
    /// Without this, every subprocess would talk to tmux's *default* socket, so
    /// a picker launched from a non-default socket would see the wrong server's
    /// sessions (or none) and switch-client would target the wrong server.
    pub fn new() -> Self {
        RealTmux { socket: tmux_socket(std::env::var("TMUX").ok().as_deref()) }
    }

    /// A `tmux` invocation already pointed at the launching server via `-S`.
    fn command(&self) -> Command {
        let mut c = Command::new("tmux");
        if let Some(sock) = &self.socket {
            c.arg("-S").arg(sock);
        }
        c
    }
}

impl Default for RealTmux {
    fn default() -> Self {
        Self::new()
    }
}

impl Tmux for RealTmux {
    fn gather(&self) -> Gathered {
        let out = self
            .command()
            .args(["list-windows", "-a", "-F", FMT])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let lossy = String::from_utf8_lossy(&o.stdout);
                let raw = normalize_separators(&lossy);
                let raw = raw.as_ref();
                let sessions = parse_windows(raw);
                let current = current_session(raw, std::env::var("TMUX").ok().as_deref());
                crate::debug::log(|| {
                    format!(
                        "gather: ok socket={:?} status=0 stdout_bytes={} stdout_lines={} sessions={} current={:?}",
                        self.socket,
                        o.stdout.len(),
                        raw.lines().count(),
                        sessions.len(),
                        current,
                    )
                });
                // A running tmux server can't have zero sessions, so parsing
                // zero out of non-empty stdout means the lines didn't match
                // FMT's expected 8-field shape. Log a preview to diagnose
                // field-report crashes without needing raw output relayed by hand.
                if sessions.is_empty() && !raw.trim().is_empty() {
                    crate::debug::log(|| {
                        let preview: String = raw.chars().take(400).collect();
                        format!("gather: parsed zero sessions from non-empty stdout, raw preview: {preview:?}")
                    });
                }
                Gathered { sessions, current }
            }
            Ok(o) => {
                crate::debug::log(|| {
                    format!(
                        "gather: tmux exited non-zero socket={:?} status={:?} stderr={:?}",
                        self.socket,
                        o.status.code(),
                        String::from_utf8_lossy(&o.stderr).trim(),
                    )
                });
                Gathered { sessions: Vec::new(), current: None }
            }
            Err(e) => {
                crate::debug::log(|| format!("gather: failed to spawn tmux: {e} (is tmux on PATH for this process?)"));
                Gathered { sessions: Vec::new(), current: None }
            }
        }
    }

    fn switch_session(&self, name: &str) -> io::Result<()> {
        self.command()
            .args(["switch-client", "-t", name])
            .status()
            .map(|_| ())
    }

    fn select_window(&self, name: &str, index: u32) -> io::Result<()> {
        let target = format!("{name}:{index}");
        self.command()
            .args(["select-window", "-t", &target])
            .status()
            .map(|_| ())
    }
}

/// Extract the tmux server socket path from `$TMUX` (its first comma-separated
/// field, e.g. `/tmp/tmux-501/default`). Returns `None` when `$TMUX` is absent
/// or empty so callers fall back to tmux's default socket. Pure (env passed in)
/// so it is unit-testable, mirroring `current_session`.
pub fn tmux_socket(tmux_env: Option<&str>) -> Option<String> {
    let sock = tmux_env?.split(',').next()?.trim();
    if sock.is_empty() {
        None
    } else {
        Some(sock.to_string())
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

/// Resolve the session the popup was launched from by matching the session-id
/// field of `$TMUX` (its 3rd comma-separated component, e.g. `7`) against the
/// `#{session_id}` column (e.g. `$7`) in the gather output. Returns `None` when
/// `$TMUX` is absent or too short, or nothing matches; callers then fall back
/// to the `attached` flag. Pure (env passed in) so it is unit-testable.
pub fn current_session(raw: &str, tmux_env: Option<&str>) -> Option<String> {
    let id_num = tmux_env?.split(',').nth(2)?.trim();
    if id_num.is_empty() {
        return None;
    }
    for line in raw.lines() {
        let f: Vec<&str> = line.split('\u{1f}').collect();
        if f.len() == 8 && f[7].trim_start_matches('$') == id_num {
            return Some(f[0].to_string());
        }
    }
    None
}

/// Normalize the field separator in `-F` output. tmux 3.5 renders the `0x1F`
/// unit separator smux uses in its format as the literal 4-character octal
/// escape `\037` instead of the raw control byte; left as-is, every line becomes
/// a single unsplittable field and the picker sees zero sessions. Convert the
/// escape back to the real separator. Records stay newline-separated either way.
/// Borrows (no allocation) for tmux versions that already emit the raw byte.
pub fn normalize_separators(raw: &str) -> std::borrow::Cow<'_, str> {
    if raw.contains("\\037") {
        std::borrow::Cow::Owned(raw.replace("\\037", "\u{1f}"))
    } else {
        std::borrow::Cow::Borrowed(raw)
    }
}

pub fn parse_windows(raw: &str) -> Vec<Session> {
    let mut sessions: Vec<Session> = Vec::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\u{1f}').collect();
        if f.len() != 8 {
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
        // Trailing field is #{session_id}.
        let raw = "\
work\u{1f}100\u{1f}10\u{1f}1\u{1f}0\u{1f}editor\u{1f}1\u{1f}$3
work\u{1f}100\u{1f}10\u{1f}1\u{1f}1\u{1f}my logs\u{1f}0\u{1f}$3
scratch\u{1f}50\u{1f}5\u{1f}0\u{1f}0\u{1f}shell\u{1f}1\u{1f}$8
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

    const SAMPLE: &str = "\
work\u{1f}100\u{1f}10\u{1f}1\u{1f}0\u{1f}editor\u{1f}1\u{1f}$3
scratch\u{1f}50\u{1f}5\u{1f}0\u{1f}0\u{1f}shell\u{1f}1\u{1f}$8
";

    #[test]
    fn normalize_separators_handles_tmux35_octal_escape() {
        // tmux 3.5 emits the 0x1F field separator as the literal escape `\037`
        // (backslash-zero-three-seven), with records still newline-separated.
        // This is the exact shape from a real 3.5a field report.
        let escaped =
            "0\\0371782948598\\0371782748885\\0371\\0371\\0372.1.198\\0371\\037$0\n\
             0\\0371782948598\\0371782748885\\0371\\0372\\037zsh\\0370\\037$0\n";
        let normalized = normalize_separators(escaped);
        assert!(normalized.contains('\u{1f}'), "escape converted to real separator");
        assert!(!normalized.contains("\\037"), "no literal escape remains");

        let sessions = parse_windows(&normalized);
        assert_eq!(sessions.len(), 1, "the two window lines fold into one session");
        assert_eq!(sessions[0].name, "0");
        assert!(sessions[0].attached);
        assert_eq!(sessions[0].windows.len(), 2);
        assert_eq!(sessions[0].windows[0].name, "2.1.198");
        assert_eq!(sessions[0].windows[1].name, "zsh");
    }

    #[test]
    fn normalize_separators_passes_raw_byte_form_through_unallocated() {
        // tmux versions that emit the raw 0x1F byte are borrowed, not copied.
        let raw = "work\u{1f}100\u{1f}10\u{1f}1\u{1f}0\u{1f}editor\u{1f}1\u{1f}$3";
        assert!(matches!(normalize_separators(raw), std::borrow::Cow::Borrowed(_)));
        assert_eq!(parse_windows(&normalize_separators(raw)).len(), 1);
    }

    #[test]
    fn current_session_matches_tmux_env_session_id() {
        // $TMUX = socket,pid,session-id -> "8" should map to scratch ($8).
        let env = "/tmp/tmux-501/default,32102,8";
        assert_eq!(current_session(SAMPLE, Some(env)).as_deref(), Some("scratch"));
    }

    #[test]
    fn tmux_socket_extracts_first_field_or_none() {
        // $TMUX = socket,pid,session-id -> the socket is the first field.
        assert_eq!(
            tmux_socket(Some("/tmp/tmux-501/default,32102,7")).as_deref(),
            Some("/tmp/tmux-501/default")
        );
        // A non-default socket (e.g. `tmux -L work`) is honored verbatim.
        assert_eq!(
            tmux_socket(Some("/private/tmp/tmux-501/work,111,2")).as_deref(),
            Some("/private/tmp/tmux-501/work")
        );
        // Absent or empty $TMUX -> None, so callers use tmux's default socket.
        assert_eq!(tmux_socket(None), None);
        assert_eq!(tmux_socket(Some("")), None);
        assert_eq!(tmux_socket(Some(",123,4")), None);
    }

    #[test]
    fn current_session_none_when_env_missing_or_no_match() {
        assert_eq!(current_session(SAMPLE, None), None);
        // session id 99 is not present
        assert_eq!(current_session(SAMPLE, Some("sock,123,99")), None);
        // too few comma fields
        assert_eq!(current_session(SAMPLE, Some("sock,123")), None);
    }

    #[derive(Default)]
    struct FakeTmux {
        calls: RefCell<Vec<String>>,
    }
    impl Tmux for FakeTmux {
        fn gather(&self) -> Gathered {
            Gathered { sessions: Vec::new(), current: None }
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
