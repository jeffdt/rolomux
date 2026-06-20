mod model;
mod store;
mod tmux;
mod ui;

use crossterm::event::{self, Event};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::execute;
use model::PickerState;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout};
use tmux::{apply_action, RealTmux, Tmux};
use ui::{draw, map_key, Input};

const HELP: &str = "\
smux - a fast tmux session picker

Usage:
  smux            Launch the picker (intended via `tmux popup -E`)
  smux --version  Print version and exit
  smux --help     Print this help and exit

Bind it in ~/.tmux.conf, e.g.:
  bind S display-popup -E -w 80% -h 80% \"exec smux\"";

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

    let tmux = RealTmux;
    let sessions = tmux.gather();
    let live: Vec<String> = sessions.iter().map(|s| s.name.clone()).collect();

    let path = store::config_path();
    let mut config = store::Config::load_from(&path);
    if config.reconcile(&live) {
        let _ = config.save_to(&path);
    }

    let mut state = PickerState::build(sessions, &config);
    if state.visible_rows().is_empty() {
        return Ok(()); // nothing to pick
    }

    let action = run_ui(&mut state)?;

    if state.dirty {
        config.pinned = state.pinned.clone();
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
            match map_key(key) {
                Input::Up => state.move_cursor(-1),
                Input::Down => state.move_cursor(1),
                Input::Expand => state.expand(),
                Input::Collapse => state.collapse(),
                Input::ToggleAll => state.toggle_all(),
                Input::Pin => state.toggle_pin(),
                Input::MoveUp => state.move_pinned(-1),
                Input::MoveDown => state.move_pinned(1),
                Input::Select => return Ok(state.selected_action()),
                Input::Switch(n) => {
                    if let Some(action) = state.action_for_session_number(n) {
                        return Ok(Some(action));
                    }
                }
                Input::Focus(n) => state.focus_session_number(n),
                Input::Quit => return Ok(None),
                Input::None => {}
            }
        }
    }
}
