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
use model::{Mode, PickerState};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout};
use tmux::{apply_action, RealTmux, Tmux};
use ui::{draw, map_group_key, map_key, map_search_key, GroupInput, Input, SearchInput};

const HELP: &str = "\
smux - a fast tmux session picker

Usage:
  smux            Launch the picker (intended via `tmux popup -E`)
  smux --version  Print version and exit
  smux --help     Print this help and exit

Bind it in ~/.tmux.conf, e.g.:
  bind S display-popup -E -B -w 84 -h 60% \"exec smux\"";

fn main() -> io::Result<()> {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "-V" | "--version" => {
                println!("smux {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(());
            }
            other => {
                eprintln!("smux: unknown argument '{other}'\n\n{HELP}");
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
    if config.reconcile(&live) {
        let _ = config.save_to(&path);
    }

    let mut state = PickerState::build(gathered.sessions, &config);
    state.refocus_current(gathered.current.as_deref());
    if state.visible_rows().is_empty() {
        debug::log(|| "exit: no visible rows (empty gather) -> returning immediately".into());
        return Ok(()); // nothing to pick
    }

    let action = run_ui(&mut state)?;

    if state.dirty {
        config.groups = state.groups.clone();
        config.manual_order = state.manual_order.clone();
        config.sort = state.sort;
        let _ = config.save_to(&path);
    }

    if let Some(action) = action {
        let _ = apply_action(&tmux, &action);
    }
    Ok(())
}

fn run_ui(state: &mut PickerState) -> io::Result<Option<model::Action>> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(out))?;

    let result = event_loop(&mut terminal, state);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut PickerState,
) -> io::Result<Option<model::Action>> {
    loop {
        terminal.draw(|f| draw(f, state))?;
        if let Event::Key(key) = event::read()? {
            if key.kind != event::KeyEventKind::Press {
                continue;
            }
            match state.mode {
                Mode::Command => match map_key(key) {
                    Input::Up => state.move_cursor(-1),
                    Input::Down => state.move_cursor(1),
                    Input::Expand => state.expand(),
                    Input::Collapse => state.collapse(),
                    Input::ToggleAll => state.toggle_all(),
                    Input::EnterGroups => state.enter_groups(),
                    Input::MoveUp => state.move_row(-1),
                    Input::MoveDown => state.move_row(1),
                    Input::CycleSort => state.cycle_sort(),
                    Input::EnterSearch => state.enter_search(),
                    Input::Select => return Ok(state.selected_action()),
                    Input::Switch(n) => {
                        if let Some(action) = state.action_for_session_number(n) {
                            return Ok(Some(action));
                        }
                    }
                    Input::Focus(n) => state.focus_session_number(n),
                    Input::Quit => return Ok(None),
                    Input::None => {}
                },
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
            }
        }
    }
}
