mod debug;
mod model;
mod search;
mod store;
mod tmux;
mod ui;

use crossterm::event::{self, Event};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::execute;
use model::{Mode, PendingRename, PickerState, RenameTarget, Row, WindowMove};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout};
use tmux::{apply_action, RealTmux, Tmux};
use ui::{draw, map_group_key, map_key, map_search_key, map_settings_key, GroupInput, Input, SearchInput, SettingsInput};

const HELP: &str = "\
rolomux - a fast tmux session picker

Usage:
  rolomux            Launch the picker (intended via `tmux popup -E`)
  rolomux --version  Print version and exit
  rolomux --help     Print this help and exit

Bind it in ~/.tmux.conf, e.g.:
  bind S display-popup -E -B -w 84 -h 60% \"exec rolomux\"";

fn main() -> io::Result<()> {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-V" | "--version" => {
                println!("rolomux {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(());
            }
            other => {
                eprintln!("rolomux: unknown argument '{other}'\n\n{HELP}");
                std::process::exit(2);
            }
        }
    }

    debug::log(|| {
        format!(
            "start: version={} TMUX={:?} XDG_CONFIG_HOME={:?}",
            env!("CARGO_PKG_VERSION"),
            std::env::var("TMUX").ok(),
            std::env::var("XDG_CONFIG_HOME").ok(),
        )
    });

    let tmux = RealTmux::new();
    let gathered = tmux.gather();
    let live: Vec<String> = gathered.sessions.iter().map(|s| s.name.clone()).collect();

    let path = store::config_path();
    let mut config = store::Config::load_from(&path);
    if config.reconcile(&gathered.ids) {
        let _ = config.save_to(&path);
    }

    let mut state = PickerState::build(gathered.sessions, &config);
    state.refocus_current(gathered.current.as_deref());
    if live.is_empty() {
        debug::log(|| "exit: no live sessions -> returning immediately".into());
        return Ok(()); // nothing to pick
    }

    let action = run_ui(&mut state, &tmux, &mut config, &path)?;

    if state.dirty {
        state.apply_to_config(&mut config);
        let _ = config.save_to(&path);
    }

    if let Some(action) = action {
        let _ = apply_action(&tmux, &action);
    }
    Ok(())
}

fn run_ui(
    state: &mut PickerState,
    tmux: &dyn Tmux,
    config: &mut store::Config,
    path: &std::path::Path,
) -> io::Result<Option<model::Action>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;

    let result = event_loop(&mut terminal, state, tmux, config, path);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

/// Apply a committed session/window rename: flush any already-pending in-run
/// config changes first (so they aren't lost when `state` gets rebuilt below),
/// call the real tmux rename, re-gather from tmux, let the existing
/// `Config::reconcile` carry group/dormant membership across the id-matched
/// rename, rebuild the picker state with the (possibly old-name-to-new-name
/// remapped) expand set preserved, and refocus the cursor on the renamed row.
fn commit_rename(
    pending: PendingRename,
    tmux: &dyn Tmux,
    config: &mut store::Config,
    path: &std::path::Path,
    state: &mut PickerState,
) {
    let mut expanded_snapshot = state.expanded_list();
    if let RenameTarget::Session(old_name) = &pending.target {
        if let Some(pos) = expanded_snapshot.iter().position(|n| n == old_name) {
            expanded_snapshot[pos] = pending.new_name.clone();
        }
    }

    if state.dirty {
        state.apply_to_config(config);
        let _ = config.save_to(path);
    }

    match &pending.target {
        RenameTarget::Session(old_name) => {
            let _ = tmux.rename_session(old_name, &pending.new_name);
        }
        RenameTarget::Window(session, index) => {
            let _ = tmux.rename_window(session, *index, &pending.new_name);
        }
    }

    let gathered = tmux.gather();
    if config.reconcile(&gathered.ids) {
        let _ = config.save_to(path);
    }

    *state = PickerState::build_with_expanded(gathered.sessions, config, expanded_snapshot);
    match &pending.target {
        RenameTarget::Session(_) => state.focus_session(&pending.new_name),
        RenameTarget::Window(session, index) => state.focus_window(session, *index),
    }
}

enum LastWindowRisk {
    Safe,
    Ejects,
}

fn last_window_risk(tmux: &dyn Tmux, session: &str, attached: bool) -> LastWindowRisk {
    if !attached || tmux.detach_on_destroy_off(session) {
        LastWindowRisk::Safe
    } else {
        LastWindowRisk::Ejects
    }
}

/// Dispatch a `⇧J`/`⇧K` press. On a session row, unchanged session-level
/// reorder. On a window row, plans and (immediately, or after a one-press
/// confirm for the destructive last-window case) commits a `WindowMove`.
fn handle_move(
    delta: i32,
    state: &mut PickerState,
    tmux: &dyn Tmux,
    config: &mut store::Config,
    path: &std::path::Path,
) {
    if let Some(mv) = state.take_confirmed_window_move(delta) {
        commit_window_move(mv, false, tmux, config, path, state);
        return;
    }

    let rows = state.visible_rows();
    match rows.get(state.cursor) {
        Some(Row::Session(_)) => state.move_row(delta),
        Some(Row::Window(_, _)) => {
            if let Some(mv) = state.plan_window_move(delta) {
                match &mv {
                    WindowMove::CrossSession { kills_source: true, src_session, src_attached, .. } => {
                        match last_window_risk(tmux, src_session, *src_attached) {
                            LastWindowRisk::Safe => state.arm_window_move(mv, delta),
                            LastWindowRisk::Ejects => commit_window_move(mv, true, tmux, config, path, state),
                        }
                    }
                    _ => commit_window_move(mv, false, tmux, config, path, state),
                }
            }
        }
        None => {}
    }
}

/// Apply a planned window move: flush pending config changes, issue the
/// tmux mutation(s) (a placeholder window first when `with_placeholder` is
/// true), re-gather, reconcile, rebuild state, and refocus -- the same
/// shape as `commit_rename`.
fn commit_window_move(
    mv: WindowMove,
    with_placeholder: bool,
    tmux: &dyn Tmux,
    config: &mut store::Config,
    path: &std::path::Path,
    state: &mut PickerState,
) {
    let expanded_snapshot = state.expanded_list();

    if state.dirty {
        state.apply_to_config(config);
        let _ = config.save_to(path);
    }

    match &mv {
        WindowMove::SwapWithin { session, a_index, b_index } => {
            let _ = tmux.swap_window(session, *a_index, *b_index);
        }
        WindowMove::CrossSession { src_session, window_index, dst_session, dst_anchor_index, before, .. } => {
            if with_placeholder {
                let _ = tmux.new_placeholder_window(src_session);
            }
            let _ = tmux.move_window(src_session, *window_index, dst_session, *dst_anchor_index, *before);
        }
    }

    let gathered = tmux.gather();
    if config.reconcile(&gathered.ids) {
        let _ = config.save_to(path);
    }

    *state = PickerState::build_with_expanded(gathered.sessions, config, expanded_snapshot);

    match &mv {
        // swap-window exchanges the two indices, so the window that was
        // under the cursor (originally at a_index) now lives at b_index.
        WindowMove::SwapWithin { session, b_index, .. } => state.focus_window(session, *b_index),
        // move-window -b/-a place the incoming window at the anchor's own
        // index (before) or one past it (after); it never keeps its
        // original source index.
        WindowMove::CrossSession { dst_session, dst_anchor_index, before, .. } => {
            let new_index = if *before { *dst_anchor_index } else { dst_anchor_index + 1 };
            state.expand_session(dst_session);
            state.focus_window(dst_session, new_index);
        }
    }
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut PickerState,
    tmux: &dyn Tmux,
    config: &mut store::Config,
    path: &std::path::Path,
) -> io::Result<Option<model::Action>> {
    loop {
        terminal.draw(|f| draw(f, state))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != event::KeyEventKind::Press {
                continue;
            }
            match state.mode {
                Mode::Command => {
                    if state.renaming() {
                        match map_search_key(key) {
                            SearchInput::Char(c) => state.rename_edit_push(c),
                            SearchInput::Backspace => state.rename_edit_backspace(),
                            SearchInput::DeleteWord => state.rename_edit_delete_word(),
                            SearchInput::Clear => state.rename_edit_clear(),
                            SearchInput::Select => {
                                if let Some(pending) = state.take_rename_commit() {
                                    commit_rename(pending, tmux, config, path, state);
                                }
                            }
                            SearchInput::Exit => state.cancel_rename(),
                            SearchInput::Up | SearchInput::Down | SearchInput::None => {}
                        }
                    } else {
                        let input = map_key(key);
                        if !matches!(input, Input::MoveUp | Input::MoveDown) {
                            state.clear_pending_window_move();
                        }
                        match input {
                            Input::Up => state.move_cursor(-1),
                            Input::Down => state.move_cursor(1),
                            Input::Expand => state.expand(),
                            Input::Collapse => state.collapse(),
                            Input::ToggleAll => state.toggle_all(),
                            Input::EnterGroups => state.enter_groups(),
                            Input::EnterSettings => state.enter_settings(),
                            Input::MoveUp => handle_move(-1, state, tmux, config, path),
                            Input::MoveDown => handle_move(1, state, tmux, config, path),
                            Input::EnterSearch => state.enter_search(),
                            Input::ToggleDormant => state.toggle_dormant(),
                            Input::ToggleFocusMode => state.toggle_focus_mode(),
                            Input::Rename => state.start_rename(),
                            Input::Select => return Ok(state.selected_action()),
                            Input::Switch(n) => {
                                if let Some(action) = state.action_for_session_number(n) {
                                    return Ok(Some(action));
                                }
                            }
                            Input::Quit => return Ok(None),
                            Input::None => {}
                        }
                    }
                }
                Mode::Search => match map_search_key(key) {
                    SearchInput::Char(c) => state.search_push(c),
                    SearchInput::Backspace => state.search_backspace(),
                    SearchInput::DeleteWord => state.search_delete_word(),
                    SearchInput::Clear => state.search_clear(),
                    SearchInput::Up => state.search_move(-1),
                    SearchInput::Down => state.search_move(1),
                    SearchInput::Select => {
                        if let Some(action) = state.search_selected_action() {
                            return Ok(Some(action));
                        }
                    }
                    SearchInput::Exit => state.exit_search(),
                    SearchInput::None => {}
                },
                Mode::Groups => {
                    if state.group_editing() {
                        match map_search_key(key) {
                            SearchInput::Char(c) => state.group_edit_push(c),
                            SearchInput::Backspace => state.group_edit_backspace(),
                            SearchInput::DeleteWord => state.group_edit_delete_word(),
                            SearchInput::Clear => state.group_edit_clear(),
                            SearchInput::Select => state.group_commit_rename(),
                            SearchInput::Exit => state.group_cancel_rename(),
                            SearchInput::Up | SearchInput::Down | SearchInput::None => {}
                        }
                    } else {
                        match map_group_key(key) {
                            GroupInput::Up => state.group_move_cursor(-1),
                            GroupInput::Down => state.group_move_cursor(1),
                            GroupInput::MoveUp => state.group_reorder(-1),
                            GroupInput::MoveDown => state.group_reorder(1),
                            GroupInput::New => state.group_new(),
                            GroupInput::Rename => state.group_start_rename(),
                            GroupInput::CycleColor => state.group_cycle_color(),
                            GroupInput::Delete => state.group_delete(),
                            GroupInput::Exit => state.exit_groups(),
                            GroupInput::None => {}
                        }
                    }
                }
                Mode::Settings => match map_settings_key(key) {
                    SettingsInput::Up => state.settings_move_cursor(-1),
                    SettingsInput::Down => state.settings_move_cursor(1),
                    SettingsInput::Left => state.settings_step_left(),
                    SettingsInput::Right => state.settings_step_right(),
                    SettingsInput::Activate => state.settings_activate(),
                    SettingsInput::CycleColor => state.settings_cycle_color(),
                    SettingsInput::Exit => state.exit_settings(),
                    SettingsInput::None => {}
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Group, Session, Window};
    use crate::store::Config;
    use crate::tmux::{FakeTmux, Gathered};
    use std::collections::HashMap;

    fn sess(name: &str) -> Session {
        Session {
            name: name.into(),
            activity: 1,
            created: 1,
            attached: false,
            windows: vec![Window { index: 0, name: "w".into(), active: true }],
        }
    }

    #[test]
    fn commit_rename_renames_via_tmux_and_preserves_group_membership() {
        let dir = std::env::temp_dir().join(format!("rolomux-commit-rename-{}", std::process::id()));
        let path = dir.join("config.toml");

        let mut config = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["old-name".into()], ..Default::default() }],
            session_ids: HashMap::from([("old-name".to_string(), "$3".to_string())]),
            ..Default::default()
        };

        let sessions = vec![sess("old-name")];
        let mut state = PickerState::build(sessions, &config);
        state.start_rename();
        state.rename_edit_clear();
        for c in "new-name".chars() { state.rename_edit_push(c); }
        let pending = state.take_rename_commit().expect("changed name commits");

        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![sess("new-name")],
            current: None,
            ids: vec![("new-name".to_string(), "$3".to_string())],
        });

        commit_rename(pending, &tmux, &mut config, &path, &mut state);

        assert_eq!(*tmux.calls.borrow(), vec!["rename-session:old-name:new-name"]);
        assert_eq!(config.groups[0].members, vec!["new-name".to_string()]);
        assert_eq!(state.cursor_session_name(), Some("new-name".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_rename_flushes_pending_dirty_changes_before_rebuilding() {
        let dir = std::env::temp_dir().join(format!("rolomux-commit-rename-dirty-{}", std::process::id()));
        let path = dir.join("config.toml");

        let mut config = Config {
            session_ids: HashMap::from([
                ("old-name".to_string(), "$3".to_string()),
                ("kept".to_string(), "$9".to_string()),
            ]),
            ..Default::default()
        };

        let sessions = vec![sess("old-name"), sess("kept")];
        let mut state = PickerState::build(sessions, &config);
        state.focus_session("kept");
        state.toggle_dormant(); // marks "kept" dormant; sets state.dirty, not yet flushed to config
        assert!(state.dirty, "toggle_dormant marks the state dirty");
        state.focus_session("old-name");
        state.start_rename();
        state.rename_edit_clear();
        for c in "new-name".chars() { state.rename_edit_push(c); }
        let pending = state.take_rename_commit().expect("changed name commits");

        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![sess("new-name"), sess("kept")],
            current: None,
            ids: vec![("new-name".to_string(), "$3".to_string()), ("kept".to_string(), "$9".to_string())],
        });

        commit_rename(pending, &tmux, &mut config, &path, &mut state);

        assert_eq!(config.dormant, vec!["kept".to_string()], "pending dormant toggle survived the rebuild");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_rename_window_focuses_the_renamed_window_row_not_the_session() {
        let dir = std::env::temp_dir().join(format!("rolomux-commit-rename-window-{}", std::process::id()));
        let path = dir.join("config.toml");

        let mut config = Config {
            session_ids: HashMap::from([("host".to_string(), "$3".to_string())]),
            ..Default::default()
        };

        let windows_before = vec![
            Window { index: 0, name: "editor".into(), active: true },
            Window { index: 1, name: "old-window".into(), active: false },
        ];
        let sessions = vec![Session { name: "host".into(), activity: 1, created: 1, attached: false, windows: windows_before }];
        let mut state = PickerState::build(sessions, &config);
        state.expand();
        state.move_cursor(2); // rows are [session, window 0 "editor", window 1 "old-window"]
        state.start_rename();
        state.rename_edit_clear();
        for c in "new-window".chars() { state.rename_edit_push(c); }
        let pending = state.take_rename_commit().expect("changed name commits");

        let windows_after = vec![
            Window { index: 0, name: "editor".into(), active: true },
            Window { index: 1, name: "new-window".into(), active: false },
        ];
        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![Session { name: "host".into(), activity: 1, created: 1, attached: false, windows: windows_after }],
            current: None,
            ids: vec![("host".to_string(), "$3".to_string())],
        });

        commit_rename(pending, &tmux, &mut config, &path, &mut state);

        assert_eq!(*tmux.calls.borrow(), vec!["rename-window:host:1:new-window"]);
        assert_eq!(
            state.selected_action(),
            Some(crate::model::Action::SwitchWindow("host".to_string(), 1)),
            "cursor should land back on the renamed window row, not its session"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handle_move_swaps_windows_within_a_session_via_tmux() {
        let dir = std::env::temp_dir().join(format!("rolomux-window-move-swap-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config::default();

        let windows_before = vec![
            Window { index: 0, name: "a".into(), active: true },
            Window { index: 1, name: "b".into(), active: false },
        ];
        let sessions = vec![Session { name: "work".into(), activity: 1, created: 1, attached: false, windows: windows_before }];
        let mut state = PickerState::build(sessions, &config);
        state.expand();
        state.move_cursor(2); // rows: [session, window 0 "a", window 1 "b"]; lands on "b"

        // swap-window exchanges the two indices -- "b" (was at 1) ends up
        // at 0, "a" (was at 0) ends up at 1 -- verified empirically.
        let windows_after = vec![
            Window { index: 0, name: "b".into(), active: false },
            Window { index: 1, name: "a".into(), active: true },
        ];
        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![Session { name: "work".into(), activity: 1, created: 1, attached: false, windows: windows_after }],
            current: None,
            ids: vec![("work".to_string(), "$1".to_string())],
        });

        handle_move(-1, &mut state, &tmux, &mut config, &path);

        assert_eq!(*tmux.calls.borrow(), vec!["swap-window:work:1:0".to_string()]);
        assert_eq!(state.selected_action(), Some(crate::model::Action::SwitchWindow("work".into(), 0)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handle_move_crosses_into_the_adjacent_session_and_expands_it() {
        let dir = std::env::temp_dir().join(format!("rolomux-window-move-cross-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config {
            groups: vec![Group { name: "ONLY".into(), members: vec!["alpha".into(), "beta".into()], inbox: true, ..Default::default() }],
            ..Default::default()
        };

        let sessions = vec![
            Session {
                name: "alpha".into(), activity: 1, created: 1, attached: false,
                windows: vec![
                    Window { index: 0, name: "a1".into(), active: true },
                    Window { index: 1, name: "a2".into(), active: false },
                ],
            },
            Session {
                name: "beta".into(), activity: 2, created: 2, attached: false,
                windows: vec![Window { index: 0, name: "b1".into(), active: true }],
            },
        ];
        let mut state = PickerState::build(sessions, &config);
        state.focus_session("alpha");
        state.expand();
        state.move_cursor(2); // rows: [alpha, a1, a2, beta]; lands on "a2" (last window)

        // move-window -b places the incoming window at the anchor's own
        // index (0) and shifts the anchor to 1 -- verified empirically.
        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![
                Session { name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![Window { index: 0, name: "a1".into(), active: true }] },
                Session {
                    name: "beta".into(), activity: 2, created: 2, attached: false,
                    windows: vec![
                        Window { index: 0, name: "a2".into(), active: false },
                        Window { index: 1, name: "b1".into(), active: true },
                    ],
                },
            ],
            current: None,
            ids: vec![("alpha".to_string(), "$1".to_string()), ("beta".to_string(), "$2".to_string())],
        });

        handle_move(1, &mut state, &tmux, &mut config, &path);

        assert_eq!(*tmux.calls.borrow(), vec!["move-window:alpha:1:beta:0:before".to_string()]);
        assert!(state.is_expanded("beta"));
        assert_eq!(state.selected_action(), Some(crate::model::Action::SwitchWindow("beta".into(), 0)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handle_move_arms_then_commits_the_last_window_of_an_unattached_session() {
        let dir = std::env::temp_dir().join(format!("rolomux-window-move-arm-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config {
            groups: vec![Group { name: "ONLY".into(), members: vec!["alpha".into(), "beta".into()], inbox: true, ..Default::default() }],
            ..Default::default()
        };

        let sessions = vec![
            Session { name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![Window { index: 0, name: "a1".into(), active: true }] },
            Session { name: "beta".into(), activity: 2, created: 2, attached: false, windows: vec![Window { index: 0, name: "b1".into(), active: true }] },
        ];
        let mut state = PickerState::build(sessions, &config);
        state.focus_session("alpha");
        state.expand();
        state.move_cursor(1); // the only window in alpha

        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![Session {
                name: "beta".into(), activity: 2, created: 2, attached: false,
                windows: vec![
                    Window { index: 0, name: "b1".into(), active: true },
                    Window { index: 1, name: "a1".into(), active: false },
                ],
            }],
            current: None,
            ids: vec![("beta".to_string(), "$2".to_string())],
        });

        handle_move(-1, &mut state, &tmux, &mut config, &path);
        assert!(tmux.calls.borrow().is_empty(), "first press only arms, no tmux call yet");
        assert!(state.pending_window_move_warning().is_some());

        handle_move(-1, &mut state, &tmux, &mut config, &path);
        assert_eq!(*tmux.calls.borrow(), vec!["move-window:alpha:0:beta:0:after".to_string()]);
        assert!(state.pending_window_move_warning().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handle_move_uses_a_placeholder_when_the_source_session_is_attached_and_would_eject() {
        let dir = std::env::temp_dir().join(format!("rolomux-window-move-placeholder-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config {
            groups: vec![Group { name: "ONLY".into(), members: vec!["alpha".into(), "beta".into()], inbox: true, ..Default::default() }],
            ..Default::default()
        };

        let sessions = vec![
            Session { name: "alpha".into(), activity: 1, created: 1, attached: true, windows: vec![Window { index: 0, name: "a1".into(), active: true }] },
            Session { name: "beta".into(), activity: 2, created: 2, attached: false, windows: vec![Window { index: 0, name: "b1".into(), active: true }] },
        ];
        let mut state = PickerState::build(sessions, &config);
        state.focus_session("alpha");
        state.expand();
        state.move_cursor(1); // the only window in alpha, which is attached

        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![
                Session { name: "alpha".into(), activity: 1, created: 1, attached: true, windows: vec![Window { index: 0, name: "(empty)".into(), active: true }] },
                Session {
                    name: "beta".into(), activity: 2, created: 2, attached: false,
                    windows: vec![
                        Window { index: 0, name: "b1".into(), active: true },
                        Window { index: 1, name: "a1".into(), active: false },
                    ],
                },
            ],
            current: None,
            ids: vec![("alpha".to_string(), "$1".to_string()), ("beta".to_string(), "$2".to_string())],
        });
        // FakeTmux's detach_on_destroy_off defaults to false ("on" / risky).

        handle_move(-1, &mut state, &tmux, &mut config, &path);

        assert_eq!(
            *tmux.calls.borrow(),
            vec!["new-window:alpha".to_string(), "move-window:alpha:0:beta:0:after".to_string()]
        );
        assert!(state.pending_window_move_warning().is_none(), "no confirmation needed once a placeholder makes the move safe");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
