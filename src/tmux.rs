use crate::model::{Action, Session, Window};
use std::io;
use std::process::Command;

pub const FMT: &str = "#{session_name}\x1f#{session_activity}\x1f#{session_created}\x1f#{session_attached}\x1f#{window_index}\x1f#{window_name}\x1f#{window_active}\x1f#{session_id}";

/// Result of a single gather: the sessions plus the name of the session the
/// popup was launched from (when it can be resolved from `$TMUX`).
#[derive(Debug, Clone, Default)]
pub struct Gathered {
    pub sessions: Vec<Session>,
    pub current: Option<String>,
}

impl Gathered {
    /// `(session_name, tmux #{session_id})` for every live session, e.g.
    /// `("work", "$3")`. Feeds `Config::reconcile`, which uses the id to
    /// recover group, dormant, and expanded state across a plain tmux
    /// rename (issue #38): it's stable across a rename within a running
    /// tmux server even though the name isn't.
    pub fn session_ids(&self) -> Vec<(String, String)> {
        self.sessions.iter().map(|s| (s.name.clone(), s.id.clone())).collect()
    }
}

pub trait Tmux {
    fn gather(&self) -> Gathered;
    fn switch_session(&self, name: &str) -> io::Result<()>;
    fn select_window(&self, name: &str, index: u32) -> io::Result<()>;
    fn rename_session(&self, old: &str, new: &str) -> io::Result<()>;
    fn rename_window(&self, session: &str, index: u32, new: &str) -> io::Result<()>;
    fn swap_window(&self, session: &str, a: u32, b: u32) -> io::Result<()>;
    fn move_window(&self, src_session: &str, src_index: u32, dst_session: &str, dst_anchor_index: u32, before: bool) -> io::Result<()>;
    fn new_placeholder_window(&self, session: &str) -> io::Result<()>;
    fn kill_session(&self, name: &str) -> io::Result<()>;
    fn kill_window(&self, session: &str, index: u32) -> io::Result<()>;
    fn detach_on_destroy_off(&self, session: &str) -> bool;
    /// The (session, stable window id) the invoking client is currently
    /// attached to and viewing, resolved implicitly against "this client"
    /// the same way `switch_session` already resolves an implicit target --
    /// works identically whether rolomux is running inside a popup or a
    /// plain pane. `None` if it can't be resolved.
    fn attached_window(&self) -> Option<(String, String)>;
    /// Where a stable tmux window id (`@N`, from `attached_window`)
    /// currently lives on the server, if it still exists -- used to
    /// relocate a window whose index (or session) may have shifted as a
    /// side effect of a swap/move it wasn't even involved in.
    fn locate_window(&self, window_id: &str) -> Option<(String, u32)>;
}

pub struct RealTmux {
    /// The server socket rolomux was launched from (`$TMUX`'s first field). `None`
    /// when rolomux runs outside tmux, in which case tmux's default socket is used.
    socket: Option<String>,
}

impl RealTmux {
    /// Bind to the tmux server rolomux was launched from, resolved from `$TMUX`.
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

    fn rename_session(&self, old: &str, new: &str) -> io::Result<()> {
        self.command()
            .args(["rename-session", "-t", old, new])
            .status()
            .map(|_| ())
    }

    fn rename_window(&self, session: &str, index: u32, new: &str) -> io::Result<()> {
        let target = format!("{session}:{index}");
        self.command()
            .args(["rename-window", "-t", &target, new])
            .status()
            .map(|_| ())
    }

    fn swap_window(&self, session: &str, a: u32, b: u32) -> io::Result<()> {
        let src = format!("{session}:{a}");
        let dst = format!("{session}:{b}");
        self.command()
            .args(["swap-window", "-d", "-s", &src, "-t", &dst])
            .status()
            .map(|_| ())
    }

    fn move_window(&self, src_session: &str, src_index: u32, dst_session: &str, dst_anchor_index: u32, before: bool) -> io::Result<()> {
        let src = format!("{src_session}:{src_index}");
        let anchor = format!("{dst_session}:{dst_anchor_index}");
        let flag = if before { "-b" } else { "-a" };
        // `-d`: without it, the incoming window steals "current" status in
        // the destination session -- if an attached client happens to be
        // looking at that session, its view visibly jumps to the newly
        // arrived window even though it wasn't the one being moved.
        // Verified empirically against a live tmux 3.7b (swap-window
        // already carried `-d` for the analogous reason; move-window had
        // been missed).
        self.command()
            .args(["move-window", "-d", flag, "-s", &src, "-t", &anchor])
            .status()
            .map(|_| ())
    }

    fn new_placeholder_window(&self, session: &str) -> io::Result<()> {
        self.command()
            .args(["new-window", "-d", "-t", session, "-n", "(empty)"])
            .status()
            .map(|_| ())
    }

    fn kill_session(&self, name: &str) -> io::Result<()> {
        self.command()
            .args(["kill-session", "-t", name])
            .status()
            .map(|_| ())
    }

    fn kill_window(&self, session: &str, index: u32) -> io::Result<()> {
        let target = format!("{session}:{index}");
        self.command()
            .args(["kill-window", "-t", &target])
            .status()
            .map(|_| ())
    }

    fn detach_on_destroy_off(&self, session: &str) -> bool {
        let session_scoped = self
            .command()
            .args(["show-options", "-t", session, "detach-on-destroy"])
            .output()
            .ok();
        if let Some(v) = session_scoped
            .as_ref()
            .and_then(|o| parse_detach_on_destroy(&String::from_utf8_lossy(&o.stdout)))
        {
            return v;
        }
        let global = self.command().args(["show-options", "-g", "detach-on-destroy"]).output().ok();
        global
            .as_ref()
            .and_then(|o| parse_detach_on_destroy(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or(false)
    }

    fn attached_window(&self) -> Option<(String, String)> {
        let out = self
            .command()
            .args(["display-message", "-p", "#{session_name}\x1f#{window_id}"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_attached_window(&String::from_utf8_lossy(&out.stdout))
    }

    fn locate_window(&self, window_id: &str) -> Option<(String, u32)> {
        let filter = format!("#{{==:#{{window_id}},{window_id}}}");
        let out = self
            .command()
            .args(["list-windows", "-a", "-f", &filter, "-F", "#{session_name}\x1f#{window_index}"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_located_window(&String::from_utf8_lossy(&out.stdout))
    }
}

/// Parses `display-message -p "#{session_name}\x1f#{window_id}"` output.
/// Pure so it's unit-testable without a live tmux.
pub fn parse_attached_window(output: &str) -> Option<(String, String)> {
    let mut parts = output.trim().splitn(2, '\u{1f}');
    let session = parts.next()?.to_string();
    let window_id = parts.next()?.to_string();
    if session.is_empty() || window_id.is_empty() {
        None
    } else {
        Some((session, window_id))
    }
}

/// Parses `list-windows -F "#{session_name}\x1f#{window_index}"` output,
/// taking the first line only -- a window-id filter should match at most
/// one line across the whole server. Pure so it's unit-testable without a
/// live tmux.
pub fn parse_located_window(output: &str) -> Option<(String, u32)> {
    let line = output.lines().next()?;
    let mut parts = line.splitn(2, '\u{1f}');
    let session = parts.next()?.to_string();
    let index: u32 = parts.next()?.trim().parse().ok()?;
    Some((session, index))
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

/// Parses `show-options ... detach-on-destroy` output (e.g.
/// `"detach-on-destroy off\n"`) into whether it's explicitly `off`. `None`
/// means the query produced no output at all -- which is exactly what a
/// session with no local override prints at session scope (verified against
/// a live tmux 3.7b); callers fall back to the global query in that case.
/// Pure (output passed in) so it's unit-testable.
pub fn parse_detach_on_destroy(output: &str) -> Option<bool> {
    let line = output.trim();
    if line.is_empty() {
        return None;
    }
    let value = line.rsplit(' ').next()?;
    Some(value == "off")
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
/// unit separator rolomux uses in its format as the literal 4-character octal
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
                id: f[7].to_string(),
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
#[derive(Default)]
pub(crate) struct FakeTmux {
    pub calls: std::cell::RefCell<Vec<String>>,
    gathered: std::cell::RefCell<Gathered>,
    detach_on_destroy_off: std::cell::Cell<bool>,
    attached_window: std::cell::RefCell<Option<(String, String)>>,
    located_windows: std::cell::RefCell<std::collections::HashMap<String, (String, u32)>>,
}

#[cfg(test)]
impl FakeTmux {
    pub fn with_gather(gathered: Gathered) -> Self {
        FakeTmux {
            calls: std::cell::RefCell::new(Vec::new()),
            gathered: std::cell::RefCell::new(gathered),
            detach_on_destroy_off: std::cell::Cell::new(false),
            attached_window: std::cell::RefCell::new(None),
            located_windows: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }

    pub fn with_detach_on_destroy_off(self, off: bool) -> Self {
        self.detach_on_destroy_off.set(off);
        self
    }

    /// Configure what `attached_window()` returns -- the (session, window
    /// id) the invoking client was on before a move.
    pub fn with_attached_window(self, session: &str, window_id: &str) -> Self {
        *self.attached_window.borrow_mut() = Some((session.to_string(), window_id.to_string()));
        self
    }

    /// Configure what `locate_window(window_id)` returns -- where that
    /// window lives after the move.
    pub fn with_located_window(self, window_id: &str, session: &str, index: u32) -> Self {
        self.located_windows.borrow_mut().insert(window_id.to_string(), (session.to_string(), index));
        self
    }
}

#[cfg(test)]
impl Tmux for FakeTmux {
    fn gather(&self) -> Gathered {
        self.gathered.borrow().clone()
    }
    fn switch_session(&self, name: &str) -> std::io::Result<()> {
        self.calls.borrow_mut().push(format!("switch:{name}"));
        Ok(())
    }
    fn select_window(&self, name: &str, index: u32) -> std::io::Result<()> {
        self.calls.borrow_mut().push(format!("select:{name}:{index}"));
        Ok(())
    }
    fn rename_session(&self, old: &str, new: &str) -> std::io::Result<()> {
        self.calls.borrow_mut().push(format!("rename-session:{old}:{new}"));
        Ok(())
    }
    fn rename_window(&self, session: &str, index: u32, new: &str) -> std::io::Result<()> {
        self.calls.borrow_mut().push(format!("rename-window:{session}:{index}:{new}"));
        Ok(())
    }
    fn swap_window(&self, session: &str, a: u32, b: u32) -> std::io::Result<()> {
        self.calls.borrow_mut().push(format!("swap-window:{session}:{a}:{b}"));
        Ok(())
    }
    fn move_window(&self, src_session: &str, src_index: u32, dst_session: &str, dst_anchor_index: u32, before: bool) -> std::io::Result<()> {
        let dir = if before { "before" } else { "after" };
        self.calls
            .borrow_mut()
            .push(format!("move-window:{src_session}:{src_index}:{dst_session}:{dst_anchor_index}:{dir}"));
        Ok(())
    }
    fn new_placeholder_window(&self, session: &str) -> std::io::Result<()> {
        self.calls.borrow_mut().push(format!("new-window:{session}"));
        Ok(())
    }
    fn kill_session(&self, name: &str) -> std::io::Result<()> {
        self.calls.borrow_mut().push(format!("kill-session:{name}"));
        Ok(())
    }
    fn kill_window(&self, session: &str, index: u32) -> std::io::Result<()> {
        self.calls.borrow_mut().push(format!("kill-window:{session}:{index}"));
        Ok(())
    }
    fn detach_on_destroy_off(&self, _session: &str) -> bool {
        self.detach_on_destroy_off.get()
    }
    fn attached_window(&self) -> Option<(String, String)> {
        self.attached_window.borrow().clone()
    }
    fn locate_window(&self, window_id: &str) -> Option<(String, u32)> {
        self.located_windows.borrow().get(window_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Action;

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
                Session { id: "$3".into(),
                    name: "work".into(),
                    activity: 100,
                    created: 10,
                    attached: true,
                    windows: vec![
                        Window { index: 0, name: "editor".into(), active: true },
                        Window { index: 1, name: "my logs".into(), active: false },
                    ],
                },
                Session { id: "$8".into(),
                    name: "scratch".into(),
                    activity: 50,
                    created: 5,
                    attached: false,
                    windows: vec![Window { index: 0, name: "shell".into(), active: true }],
                },
            ]
        );
    }

    #[test]
    fn parse_windows_populates_one_session_id_per_session() {
        let raw = "\
work\u{1f}100\u{1f}10\u{1f}1\u{1f}0\u{1f}editor\u{1f}1\u{1f}$3
work\u{1f}100\u{1f}10\u{1f}1\u{1f}1\u{1f}my logs\u{1f}0\u{1f}$3
scratch\u{1f}50\u{1f}5\u{1f}0\u{1f}0\u{1f}shell\u{1f}1\u{1f}$8
";
        let sessions = parse_windows(raw);
        assert_eq!(sessions.len(), 2, "two sessions, ids collapse per-session not per-window");
        assert_eq!(sessions[0].id, "$3");
        assert_eq!(sessions[1].id, "$8");
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
        let sessions = parse_windows(&normalize_separators(raw));
        assert_eq!(sessions.len(), 1);
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

    #[test]
    fn fake_tmux_records_rename_session_call() {
        let t = FakeTmux::default();
        t.rename_session("old", "new").unwrap();
        assert_eq!(*t.calls.borrow(), vec!["rename-session:old:new"]);
    }

    #[test]
    fn fake_tmux_records_rename_window_call() {
        let t = FakeTmux::default();
        t.rename_window("work", 2, "logs").unwrap();
        assert_eq!(*t.calls.borrow(), vec!["rename-window:work:2:logs"]);
    }

    #[test]
    fn fake_tmux_with_gather_returns_configured_snapshot() {
        let t = FakeTmux::with_gather(Gathered {
            sessions: vec![Session {
                id: "$3".into(),
                name: "work".into(),
                activity: 0,
                created: 0,
                attached: true,
                windows: vec![],
            }],
            current: Some("work".to_string()),
        });
        let g = t.gather();
        assert_eq!(g.current.as_deref(), Some("work"));
        assert_eq!(g.sessions[0].id, "$3");
    }

    #[test]
    fn parse_detach_on_destroy_reads_off() {
        assert_eq!(parse_detach_on_destroy("detach-on-destroy off\n"), Some(true));
    }

    #[test]
    fn parse_detach_on_destroy_reads_on() {
        assert_eq!(parse_detach_on_destroy("detach-on-destroy on\n"), Some(false));
    }

    #[test]
    fn parse_detach_on_destroy_empty_output_is_none() {
        // A session with no local override prints nothing at session scope
        // (verified empirically against a live tmux 3.7b) -- callers must
        // fall back to the global query, not assume "on".
        assert_eq!(parse_detach_on_destroy(""), None);
        assert_eq!(parse_detach_on_destroy("\n"), None);
    }

    #[test]
    fn parse_attached_window_reads_session_and_window_id() {
        assert_eq!(
            parse_attached_window("work\u{1f}@42\n"),
            Some(("work".to_string(), "@42".to_string()))
        );
    }

    #[test]
    fn parse_attached_window_missing_fields_is_none() {
        assert_eq!(parse_attached_window(""), None);
        assert_eq!(parse_attached_window("work\u{1f}"), None);
    }

    #[test]
    fn parse_located_window_reads_first_matching_line() {
        assert_eq!(
            parse_located_window("beta\u{1f}0\n"),
            Some(("beta".to_string(), 0))
        );
    }

    #[test]
    fn parse_located_window_empty_output_is_none() {
        assert_eq!(parse_located_window(""), None);
    }

    #[test]
    fn fake_tmux_records_swap_and_move_and_placeholder_calls() {
        let t = FakeTmux::with_gather(Gathered::default());
        t.swap_window("work", 2, 1).unwrap();
        t.move_window("alpha", 1, "beta", 0, true).unwrap();
        t.move_window("alpha", 1, "beta", 3, false).unwrap();
        t.new_placeholder_window("alpha").unwrap();
        assert_eq!(
            *t.calls.borrow(),
            vec![
                "swap-window:work:2:1".to_string(),
                "move-window:alpha:1:beta:0:before".to_string(),
                "move-window:alpha:1:beta:3:after".to_string(),
                "new-window:alpha".to_string(),
            ]
        );
    }

    #[test]
    fn fake_tmux_detach_on_destroy_off_defaults_false_and_is_settable() {
        let t = FakeTmux::with_gather(Gathered::default());
        assert!(!t.detach_on_destroy_off("any"));
        let t2 = FakeTmux::with_gather(Gathered::default()).with_detach_on_destroy_off(true);
        assert!(t2.detach_on_destroy_off("any"));
    }
}
