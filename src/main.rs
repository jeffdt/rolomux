mod debug;
mod input;
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
use model::{KillTarget, Mode, PendingRename, PickerState, RenameTarget, Row, WindowMove};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout};
use std::time::Duration;
use tmux::{apply_action, RealTmux, Tmux};
use input::{map_group_key, map_key, map_search_key, map_settings_key, GroupInput, Input, SearchInput, SettingsInput};
use ui::draw;

const HELP: &str = "\
rolomux - a fast tmux session picker

Usage:
  rolomux            Launch the picker (intended via `tmux popup -E`)
  rolomux --version  Print version and exit
  rolomux --help     Print this help and exit

Bind it in ~/.tmux.conf, e.g.:
  bind S display-popup -E -B -w 84 -h 60% \"exec rolomux\"";

/// Poll interval used only while a swap-flash indicator is in flight (see
/// `event_loop`), so the bright-to-dim fade redraws without a keypress.
const SWAP_INDICATOR_TICK: Duration = Duration::from_millis(50);

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
    if config.reconcile(&gathered.session_ids()) {
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
    if config.reconcile(&gathered.session_ids()) {
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
        commit_window_move(mv, false, delta, tmux, config, path, state);
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
                            LastWindowRisk::Ejects => commit_window_move(mv, true, delta, tmux, config, path, state),
                        }
                    }
                    _ => commit_window_move(mv, false, delta, tmux, config, path, state),
                }
            }
        }
        None => {}
    }
}

/// Apply a planned window move: flush pending config changes, issue the
/// tmux mutation(s) (a placeholder window first when `with_placeholder` is
/// true), re-gather, reconcile, rebuild state, and refocus -- the same
/// shape as `commit_rename`. `delta` is the triggering `⇧J`/`⇧K` press,
/// needed only to flash the swap indicator in the right direction after the
/// state rebuild below (see `PickerState::set_window_swap`).
fn commit_window_move(
    mv: WindowMove,
    with_placeholder: bool,
    delta: i32,
    tmux: &dyn Tmux,
    config: &mut store::Config,
    path: &std::path::Path,
    state: &mut PickerState,
) {
    let expanded_snapshot = state.expanded_list();
    let attached = tmux.attached_window();

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

    // Neither swap-window nor move-window reliably leave the invoking
    // client's actual view alone -- swap-window's -s operand becomes the
    // session's current window regardless of -d, and this class of side
    // effect isn't worth chasing flag-by-flag (verified empirically: a
    // completely uninvolved third window can lose "current" status just
    // from two *other* windows being swapped). So instead of preventing
    // it, unconditionally restore whatever window the client was actually
    // on before this mutation, located by its stable id. This covers both
    // "an uninvolved window lost focus" (relocates to the same spot) and
    // "the client's own window was the one that moved" (relocates to its
    // new session, so the client follows it there).
    if let Some((_, window_id)) = &attached {
        if let Some((session, index)) = tmux.locate_window(window_id) {
            let _ = tmux.select_window(&session, index);
            let _ = tmux.switch_session(&session);
        }
    }

    let gathered = tmux.gather();
    if config.reconcile(&gathered.session_ids()) {
        let _ = config.save_to(path);
    }

    *state = PickerState::build_with_expanded(gathered.sessions, config, expanded_snapshot);

    match &mv {
        // swap-window exchanges the two indices, so the window that was
        // under the cursor (originally at a_index) now lives at b_index.
        WindowMove::SwapWithin { session, a_index, b_index } => {
            state.focus_window(session, *b_index);
            state.set_window_swap(session, *b_index, *a_index, delta);
        }
        // move-window -b/-a place the incoming window at the anchor's own
        // index (before) or one past it (after); it never keeps its
        // original source index.
        WindowMove::CrossSession { dst_session, dst_anchor_index, before, .. } => {
            let new_index = if *before { *dst_anchor_index } else { dst_anchor_index + 1 };
            state.expand_session(dst_session);
            state.focus_window(dst_session, new_index);
            state.set_window_cross(dst_session, new_index, delta);
        }
    }
}

/// Dispatch an `x` press. First press classifies the risk (reusing
/// `last_window_risk`, the same helper the `⇧J`/`⇧K` last-window-move path
/// uses) and arms a confirm; a second press commits it.
fn handle_kill(
    state: &mut PickerState,
    tmux: &dyn Tmux,
    config: &mut store::Config,
    path: &std::path::Path,
) {
    if let Some(target) = state.take_confirmed_kill() {
        commit_kill(target, tmux, config, path, state);
        return;
    }

    let Some(target) = state.plan_kill() else { return };
    let risky = match &target {
        KillTarget::Session(name) => {
            let attached = state.ordered().iter().find(|s| s.name == *name).map(|s| s.attached).unwrap_or(false);
            matches!(last_window_risk(tmux, name, attached), LastWindowRisk::Ejects)
        }
        KillTarget::Window { session, index } => {
            let sess = state.ordered().into_iter().find(|s| s.name == *session);
            let is_only_window = sess.map(|s| s.windows.len() == 1 && s.windows[0].index == *index).unwrap_or(false);
            if is_only_window {
                let attached = sess.map(|s| s.attached).unwrap_or(false);
                matches!(last_window_risk(tmux, session, attached), LastWindowRisk::Ejects)
            } else {
                false
            }
        }
    };
    state.arm_kill(target, risky);
}

/// Apply a confirmed kill: flush pending config changes, issue the tmux
/// kill, re-gather, reconcile (this is what drops the dead session out of
/// every group's members, `dormant`, and `expanded`), rebuild state. No
/// explicit refocus afterward -- unlike rename/move there's no "where did
/// it go" target, so the rebuilt state's default cursor placement is correct
/// as-is.
fn commit_kill(
    target: KillTarget,
    tmux: &dyn Tmux,
    config: &mut store::Config,
    path: &std::path::Path,
    state: &mut PickerState,
) {
    if state.dirty {
        state.apply_to_config(config);
        let _ = config.save_to(path);
    }

    match &target {
        KillTarget::Session(name) => {
            let _ = tmux.kill_session(name);
        }
        KillTarget::Window { session, index } => {
            let _ = tmux.kill_window(session, *index);
        }
    }

    let gathered = tmux.gather();
    if config.reconcile(&gathered.session_ids()) {
        let _ = config.save_to(path);
    }

    *state = PickerState::build(gathered.sessions, config);
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

        // While a swap-flash indicator is in flight, wake up on a short
        // tick instead of blocking forever on the next keypress, so the
        // bright-to-dim fade (and eventual disappearance) redraws on its
        // own. Once it clears, this reverts to a plain blocking read --
        // zero idle CPU cost outside the ~1s window after a ⇧J/⇧K press.
        let event = if state.swap_indicator_active() {
            if event::poll(SWAP_INDICATOR_TICK)? { Some(event::read()?) } else { None }
        } else {
            Some(event::read()?)
        };
        state.tick_swap_indicator();
        let Some(event) = event else { continue };

        if let Event::Key(key) = event {
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
                            SearchInput::Up | SearchInput::Down | SearchInput::Expand | SearchInput::Collapse | SearchInput::ToggleFocusMode | SearchInput::None => {}
                        }
                    } else {
                        let input = map_key(key);
                        let had_pending_window_move = state.pending_window_move_warning().is_some();
                        let had_pending_kill = state.pending_kill_warning().is_some();
                        if !matches!(input, Input::MoveUp | Input::MoveDown) {
                            state.clear_pending_window_move();
                        }
                        if !matches!(input, Input::Kill) {
                            state.clear_pending_kill();
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
                            Input::ToggleShortcuts => state.toggle_shortcuts(),
                            Input::Rename => state.start_rename(),
                            Input::Kill => handle_kill(state, tmux, config, path),
                            Input::Select => return Ok(state.selected_action()),
                            Input::Switch(n) => {
                                if let Some(action) = state.action_for_session_number(n) {
                                    return Ok(Some(action));
                                }
                            }
                            // A pending window-move confirm absorbs Quit as
                            // a cancel-back-to-command-mode instead of
                            // closing the picker -- matches the existing
                            // Esc/q "back out one level" convention already
                            // used by Search/Groups/Settings mode, rather
                            // than surprising the user by quitting the
                            // whole picker out from under them.
                            Input::Quit => {
                                if !had_pending_window_move && !had_pending_kill {
                                    return Ok(None);
                                }
                            }
                            Input::None => {}
                        }
                    }
                }
                Mode::Search => match map_search_key(key) {
                    SearchInput::Char(c) => state.search_push(c),
                    SearchInput::Backspace => state.search_backspace(),
                    SearchInput::DeleteWord => state.search_delete_word(),
                    SearchInput::Clear => state.search_clear(),
                    SearchInput::Expand => state.search_expand(),
                    SearchInput::Collapse => state.search_collapse(),
                    SearchInput::Up => state.search_move(-1),
                    SearchInput::Down => state.search_move(1),
                    SearchInput::Select => {
                        if let Some(action) = state.search_selected_action() {
                            return Ok(Some(action));
                        }
                    }
                    SearchInput::Exit => state.exit_search(),
                    SearchInput::ToggleFocusMode => state.toggle_focus_mode(),
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
                            SearchInput::Up | SearchInput::Down | SearchInput::Expand | SearchInput::Collapse | SearchInput::ToggleFocusMode | SearchInput::None => {}
                        }
                    } else {
                        let input = map_group_key(key);
                        if !matches!(input, GroupInput::MoveUp | GroupInput::MoveDown) {
                            state.clear_group_reorder_warning();
                        }
                        match input {
                            GroupInput::Up => state.group_move_cursor(-1),
                            GroupInput::Down => state.group_move_cursor(1),
                            GroupInput::MoveUp => state.group_reorder(-1),
                            GroupInput::MoveDown => state.group_reorder(1),
                            GroupInput::New => state.group_new(),
                            GroupInput::Rename => state.group_start_rename(),
                            GroupInput::CycleColor => state.group_cycle_color(),
                            GroupInput::Delete => state.group_delete(),
                            GroupInput::ToggleShortcuts => state.toggle_shortcuts(),
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
                    SettingsInput::ToggleShortcuts => state.toggle_shortcuts(),
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
        Session { id: String::new(),
            name: name.into(),
            activity: 1,
            created: 1,
            attached: false,
            windows: vec![Window { index: 0, name: "w".into(), active: true }],
        }
    }

    fn sess_id(name: &str, id: &str) -> Session {
        Session { id: id.into(), ..sess(name) }
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
            sessions: vec![sess_id("new-name", "$3")],
            current: None,
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
            sessions: vec![sess_id("new-name", "$3"), sess_id("kept", "$9")],
            current: None,
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
        let sessions = vec![Session { id: String::new(), name: "host".into(), activity: 1, created: 1, attached: false, windows: windows_before }];
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
            sessions: vec![Session { id: String::new(), name: "host".into(), activity: 1, created: 1, attached: false, windows: windows_after }],
            current: None,
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
        let sessions = vec![Session { id: String::new(), name: "work".into(), activity: 1, created: 1, attached: false, windows: windows_before }];
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
            sessions: vec![Session { id: String::new(), name: "work".into(), activity: 1, created: 1, attached: false, windows: windows_after }],
            current: None,
        });

        handle_move(-1, &mut state, &tmux, &mut config, &path);

        assert_eq!(*tmux.calls.borrow(), vec!["swap-window:work:1:0".to_string()]);
        assert_eq!(state.selected_action(), Some(crate::model::Action::SwitchWindow("work".into(), 0)));
        assert_eq!(
            state.window_swap_marker("work", 0),
            Some((crate::model::SwapDirection::Up, true)),
            "the moved window (now at 0) flashes up"
        );
        assert_eq!(
            state.window_swap_marker("work", 1),
            Some((crate::model::SwapDirection::Down, true)),
            "the bumped neighbor (now at 1) flashes down"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_window_move_restores_the_clients_actual_attached_window_after_an_unrelated_swap() {
        let dir = std::env::temp_dir().join(format!("rolomux-window-move-restore-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config::default();

        let windows_before = vec![
            Window { index: 0, name: "a".into(), active: false },
            Window { index: 1, name: "b".into(), active: false },
            Window { index: 2, name: "c".into(), active: true }, // the client's real window, uninvolved
        ];
        let sessions = vec![Session { id: String::new(), name: "work".into(), activity: 1, created: 1, attached: true, windows: windows_before }];
        let mut state = PickerState::build(sessions, &config);
        state.expand();
        state.move_cursor(1); // lands on "a" (wi = 0)

        // swap-window's -s operand ("a") steals "current" regardless of -d
        // -- verified empirically -- so "c" shows as no longer active in
        // this post-move gather even though it was never touched.
        let windows_after = vec![
            Window { index: 0, name: "b".into(), active: false },
            Window { index: 1, name: "a".into(), active: false },
            Window { index: 2, name: "c".into(), active: false },
        ];
        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![Session { id: String::new(), name: "work".into(), activity: 1, created: 1, attached: true, windows: windows_after }],
            current: None,
        })
        .with_attached_window("work", "@9")
        .with_located_window("@9", "work", 2); // "c" is still at index 2, untouched by the swap

        handle_move(1, &mut state, &tmux, &mut config, &path); // move "a" down, swaps with "b"

        assert_eq!(
            *tmux.calls.borrow(),
            vec![
                "swap-window:work:0:1".to_string(),
                "select:work:2".to_string(),
                "switch:work".to_string(),
            ]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_window_move_follows_the_client_when_it_was_on_the_window_that_moved() {
        let dir = std::env::temp_dir().join(format!("rolomux-window-move-follow-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config {
            groups: vec![Group { name: "ONLY".into(), members: vec!["alpha".into(), "beta".into()], inbox: true, ..Default::default() }],
            ..Default::default()
        };

        let sessions = vec![
            Session { id: String::new(),
                name: "alpha".into(), activity: 1, created: 1, attached: true,
                windows: vec![
                    Window { index: 0, name: "a1".into(), active: true },
                    Window { index: 1, name: "a2".into(), active: false },
                ],
            },
            Session { id: String::new(),
                name: "beta".into(), activity: 2, created: 2, attached: false,
                windows: vec![Window { index: 0, name: "b1".into(), active: true }],
            },
        ];
        let mut state = PickerState::build(sessions, &config);
        state.focus_session("alpha");
        state.expand();
        state.move_cursor(2); // lands on "a2" (wi = 1, last window)

        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![
                Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![Window { index: 0, name: "a1".into(), active: true }] },
                Session { id: String::new(),
                    name: "beta".into(), activity: 2, created: 2, attached: true,
                    windows: vec![
                        Window { index: 0, name: "a2".into(), active: true },
                        Window { index: 1, name: "b1".into(), active: false },
                    ],
                },
            ],
            current: None,
        })
        .with_attached_window("alpha", "@42") // the client was on "a2" before the move
        .with_located_window("@42", "beta", 0); // "a2" now lives in beta at index 0

        handle_move(1, &mut state, &tmux, &mut config, &path); // move "a2" down, crossing into beta

        assert_eq!(
            *tmux.calls.borrow(),
            vec![
                "move-window:alpha:1:beta:0:before".to_string(),
                "select:beta:0".to_string(),
                "switch:beta".to_string(),
            ]
        );

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
            Session { id: String::new(),
                name: "alpha".into(), activity: 1, created: 1, attached: false,
                windows: vec![
                    Window { index: 0, name: "a1".into(), active: true },
                    Window { index: 1, name: "a2".into(), active: false },
                ],
            },
            Session { id: String::new(),
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
                Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![Window { index: 0, name: "a1".into(), active: true }] },
                Session { id: String::new(),
                    name: "beta".into(), activity: 2, created: 2, attached: false,
                    windows: vec![
                        Window { index: 0, name: "a2".into(), active: false },
                        Window { index: 1, name: "b1".into(), active: true },
                    ],
                },
            ],
            current: None,
        });

        handle_move(1, &mut state, &tmux, &mut config, &path);

        assert_eq!(*tmux.calls.borrow(), vec!["move-window:alpha:1:beta:0:before".to_string()]);
        assert!(state.is_expanded("beta"));
        assert_eq!(state.selected_action(), Some(crate::model::Action::SwitchWindow("beta".into(), 0)));
        assert_eq!(
            state.window_swap_marker("beta", 0),
            Some((crate::model::SwapDirection::Down, true)),
            "the window that crossed into beta flashes down"
        );

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
            Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: vec![Window { index: 0, name: "a1".into(), active: true }] },
            Session { id: String::new(), name: "beta".into(), activity: 2, created: 2, attached: false, windows: vec![Window { index: 0, name: "b1".into(), active: true }] },
        ];
        let mut state = PickerState::build(sessions, &config);
        state.focus_session("alpha");
        state.expand();
        state.move_cursor(1); // the only window in alpha

        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![Session { id: String::new(),
                name: "beta".into(), activity: 2, created: 2, attached: false,
                windows: vec![
                    Window { index: 0, name: "b1".into(), active: true },
                    Window { index: 1, name: "a1".into(), active: false },
                ],
            }],
            current: None,
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
            Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: true, windows: vec![Window { index: 0, name: "a1".into(), active: true }] },
            Session { id: String::new(), name: "beta".into(), activity: 2, created: 2, attached: false, windows: vec![Window { index: 0, name: "b1".into(), active: true }] },
        ];
        let mut state = PickerState::build(sessions, &config);
        state.focus_session("alpha");
        state.expand();
        state.move_cursor(1); // the only window in alpha, which is attached

        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![
                Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: true, windows: vec![Window { index: 0, name: "(empty)".into(), active: true }] },
                Session { id: String::new(),
                    name: "beta".into(), activity: 2, created: 2, attached: false,
                    windows: vec![
                        Window { index: 0, name: "b1".into(), active: true },
                        Window { index: 1, name: "a1".into(), active: false },
                    ],
                },
            ],
            current: None,
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

    #[test]
    fn handle_kill_arms_then_confirms_a_session_kill() {
        let dir = std::env::temp_dir().join(format!("rolomux-kill-session-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config {
            groups: vec![Group { name: "WORK".into(), members: vec!["alpha".into(), "beta".into()], ..Default::default() }],
            ..Default::default()
        };
        let sessions = vec![sess("alpha"), sess("beta")];
        let mut state = PickerState::build(sessions, &config);
        state.focus_session("alpha");

        let tmux = FakeTmux::with_gather(Gathered { sessions: vec![sess("beta")], current: None });

        handle_kill(&mut state, &tmux, &mut config, &path);
        assert!(tmux.calls.borrow().is_empty(), "first press only arms, no tmux call yet");
        assert!(state.pending_kill_warning().is_some());

        handle_kill(&mut state, &tmux, &mut config, &path);
        assert_eq!(*tmux.calls.borrow(), vec!["kill-session:alpha".to_string()]);
        assert!(state.pending_kill_warning().is_none());
        assert_eq!(config.groups[0].members, vec!["beta".to_string()], "dead session scrubbed from group membership");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handle_kill_confirms_a_window_kill_on_a_non_last_window() {
        let dir = std::env::temp_dir().join(format!("rolomux-kill-window-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config::default();
        let windows_before = vec![
            Window { index: 0, name: "editor".into(), active: true },
            Window { index: 1, name: "logs".into(), active: false },
        ];
        let sessions = vec![Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: windows_before }];
        let mut state = PickerState::build(sessions, &config);
        state.expand();
        state.move_cursor(2); // rows: [session, window 0 "editor", window 1 "logs"]

        let windows_after = vec![Window { index: 0, name: "editor".into(), active: true }];
        let tmux = FakeTmux::with_gather(Gathered {
            sessions: vec![Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: false, windows: windows_after }],
            current: None,
        });

        handle_kill(&mut state, &tmux, &mut config, &path); // arm
        assert!(!state.pending_kill_warning().unwrap().contains("exit tmux"), "non-last window is never risky");
        handle_kill(&mut state, &tmux, &mut config, &path); // confirm

        assert_eq!(*tmux.calls.borrow(), vec!["kill-window:alpha:1".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handle_kill_warns_of_ejection_when_killing_the_attached_session() {
        let dir = std::env::temp_dir().join(format!("rolomux-kill-session-risky-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config::default();
        let sessions = vec![Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: true, windows: vec![Window { index: 0, name: "w".into(), active: true }] }];
        let mut state = PickerState::build(sessions, &config);

        let tmux = FakeTmux::with_gather(Gathered { sessions: vec![], current: None });
        // FakeTmux's detach_on_destroy_off defaults to false ("on" / risky).

        handle_kill(&mut state, &tmux, &mut config, &path); // arm
        let warning = state.pending_kill_warning().expect("armed");
        assert!(warning.contains("exit tmux"), "attached session with detach-on-destroy on is risky: {warning}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handle_kill_on_the_only_window_of_an_attached_session_is_risky() {
        let dir = std::env::temp_dir().join(format!("rolomux-kill-only-window-risky-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config::default();
        let sessions = vec![Session { id: String::new(), name: "alpha".into(), activity: 1, created: 1, attached: true, windows: vec![Window { index: 0, name: "w".into(), active: true }] }];
        let mut state = PickerState::build(sessions, &config);
        state.expand();
        state.move_cursor(1); // the only window in alpha

        let tmux = FakeTmux::with_gather(Gathered { sessions: vec![], current: None });

        handle_kill(&mut state, &tmux, &mut config, &path); // arm
        let warning = state.pending_kill_warning().expect("armed");
        assert!(warning.contains("exit tmux"), "killing the only window of an attached session is risky: {warning}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn handle_kill_on_an_unattached_session_is_never_risky_even_with_detach_on_destroy_on() {
        let dir = std::env::temp_dir().join(format!("rolomux-kill-unattached-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut config = Config::default();
        let sessions = vec![sess("alpha")];
        let mut state = PickerState::build(sessions, &config);

        let tmux = FakeTmux::with_gather(Gathered { sessions: vec![], current: None });

        handle_kill(&mut state, &tmux, &mut config, &path); // arm
        let warning = state.pending_kill_warning().expect("armed");
        assert!(!warning.contains("exit tmux"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
