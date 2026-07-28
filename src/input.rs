//! Keyboard input decoding: maps crossterm key events to the mode-specific
//! Input / SearchInput / AltitudeInput / SettingsInput command enums the event
//! loop dispatches on. Pure functions, no rendering or model state.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Up,
    Down,
    Expand,
    Collapse,
    ToggleAll,
    Select,
    Switch(usize),
    EnterGroups,
    EnterSettings,
    MoveUp,
    MoveDown,
    EnterSearch,
    ToggleDormant,
    UndormantSession,
    UndormantAll,
    ToggleFocusMode,
    OpenHelp,
    Rename,
    QuickCreate,
    NewSession,
    Kill,
    Quit,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchInput {
    Char(char),
    Backspace,
    DeleteWord,
    Clear,
    Expand,
    Collapse,
    Up,
    Down,
    Select,
    Exit,
    ToggleFocusMode,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AltitudeInput {
    Up,
    Down,
    MoveUp,
    MoveDown,
    New,
    Rename,
    CycleColor,
    Delete,
    DescendInto,
    Descend,
    EnterSearch,
    Switch(usize),
    OpenHelp,
    NewSessionInGroup,
    Quit,
    None,
}

/// Key mapping for group altitude while NOT editing a name. During an
/// inline rename the loop routes keys through `map_search_key` instead.
///
/// Mirrors `map_key`'s verbs one level up: `Enter` descends into a group's
/// sessions (`DescendInto`) rather than renaming, `x` deletes a group (same
/// key as session-mode Kill) leaving `d` reserved/unmapped, and `g`/`Esc`
/// descend back to session mode while `q` quits the picker outright -- the
/// same digit-decoding as `map_key` reaches groups 1-20. `⇧N` starts a
/// session-create prompt appended to the highlighted group (mirroring `n`'s
/// session-altitude counterpart), distinct from plain `n`'s existing
/// "new group" command.
pub fn map_altitude_key(key: KeyEvent) -> AltitudeInput {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Char('J') | KeyCode::Down if shift => AltitudeInput::MoveDown,
        KeyCode::Char('K') | KeyCode::Up if shift => AltitudeInput::MoveUp,
        KeyCode::Char('j') | KeyCode::Down => AltitudeInput::Down,
        KeyCode::Char('k') | KeyCode::Up => AltitudeInput::Up,
        KeyCode::Char('N') if shift => AltitudeInput::NewSessionInGroup,
        KeyCode::Char('n') => AltitudeInput::New,
        KeyCode::Char('r') => AltitudeInput::Rename,
        KeyCode::Enter => AltitudeInput::DescendInto,
        KeyCode::Char('c') => AltitudeInput::CycleColor,
        KeyCode::Char('x') => AltitudeInput::Delete,
        KeyCode::Char('/') => AltitudeInput::EnterSearch,
        KeyCode::Char('?') => AltitudeInput::OpenHelp,
        KeyCode::Esc | KeyCode::Char('g') => AltitudeInput::Descend,
        KeyCode::Char('q') => AltitudeInput::Quit,
        KeyCode::Char(c @ '1'..='9') if alt => AltitudeInput::Switch(10 + (c as usize - '0' as usize)),
        KeyCode::Char('0') if alt => AltitudeInput::Switch(20),
        KeyCode::Char(c @ '1'..='9') => AltitudeInput::Switch(c as usize - '0' as usize),
        KeyCode::Char('0') => AltitudeInput::Switch(10),
        _ => AltitudeInput::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsInput {
    Up,
    Down,
    Left,
    Right,
    Jump(usize),
    Activate,
    CycleColor,
    OpenHelp,
    Exit,
    None,
}

/// Key mapping for settings mode. `,` exits (mirroring how it also enters,
/// same as `g` for Groups mode), alongside the usual `q`/`Esc`. The palette
/// checklist has a fixed display order (`ALL_NAMED_COLORS` canonical order),
/// so there is no reorder key here (unlike Groups mode's `⇧JK`).
pub fn map_settings_key(key: KeyEvent) -> SettingsInput {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => SettingsInput::Down,
        KeyCode::Char('k') | KeyCode::Up => SettingsInput::Up,
        KeyCode::Char('l') | KeyCode::Right => SettingsInput::Right,
        KeyCode::Char('h') | KeyCode::Left => SettingsInput::Left,
        KeyCode::Enter | KeyCode::Char(' ') => SettingsInput::Activate,
        KeyCode::Char('c') => SettingsInput::CycleColor,
        KeyCode::Char('?') => SettingsInput::OpenHelp,
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(',') => SettingsInput::Exit,
        KeyCode::Char(c @ '1'..='9') if alt => SettingsInput::Jump(10 + (c as usize - '0' as usize)),
        KeyCode::Char('0') if alt => SettingsInput::Jump(20),
        KeyCode::Char(c @ '1'..='9') => SettingsInput::Jump(c as usize - '0' as usize),
        KeyCode::Char('0') => SettingsInput::Jump(10),
        _ => SettingsInput::None,
    }
}

/// Key mapping while in search mode. Printable characters (including digits)
/// build the query; movement uses arrows plus the fzf/vim Ctrl pairs.
/// Expand/collapse reuse the arrow keys (`→`/`←`) plus `Ctrl-l`/`Ctrl-h` as a
/// vim-style alias -- bare `l`/`h` stay query text, since they're ordinary
/// letters someone might be typing to filter. Confirmed via a throwaway
/// crossterm probe that `Ctrl-h` arrives as `Char('h')`+`CONTROL`, distinct
/// from a plain `Backspace` keypress (no modifiers) -- no collision, unlike
/// the digit case documented in AGENTS.md.
///
/// Note: under the legacy (non-kitty) encoding some terminals deliver Ctrl-j as
/// Enter, in which case it selects rather than moving down. Arrows, Ctrl-n,
/// Ctrl-p, and Ctrl-k are the reliable movement keys; Ctrl-j is mapped for
/// terminals that can distinguish it.
///
/// `Ctrl-f` toggles focus mode (same reasoning as `Ctrl-l`/`Ctrl-h`: bare
/// `f` stays query text, so the Ctrl form carries the command).
pub fn map_search_key(key: KeyEvent) -> SearchInput {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Esc => SearchInput::Exit,
        KeyCode::Enter => SearchInput::Select,
        KeyCode::Backspace if alt => SearchInput::DeleteWord,
        KeyCode::Backspace => SearchInput::Backspace,
        KeyCode::Right => SearchInput::Expand,
        KeyCode::Left => SearchInput::Collapse,
        KeyCode::Up => SearchInput::Up,
        KeyCode::Down => SearchInput::Down,
        KeyCode::Char('w') if ctrl => SearchInput::DeleteWord,
        KeyCode::Char('u') if ctrl => SearchInput::Clear,
        KeyCode::Char('l') if ctrl => SearchInput::Expand,
        KeyCode::Char('h') if ctrl => SearchInput::Collapse,
        KeyCode::Char('p') | KeyCode::Char('k') if ctrl => SearchInput::Up,
        KeyCode::Char('n') | KeyCode::Char('j') if ctrl => SearchInput::Down,
        KeyCode::Char('f') if ctrl => SearchInput::ToggleFocusMode,
        KeyCode::Char(_) if ctrl => SearchInput::None,
        KeyCode::Char(c) => SearchInput::Char(c),
        _ => SearchInput::None,
    }
}

pub fn map_key(key: KeyEvent) -> Input {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('K') | KeyCode::Up if shift => Input::MoveUp,
        KeyCode::Char('J') | KeyCode::Down if shift => Input::MoveDown,
        KeyCode::Char('D') if shift => Input::UndormantAll,
        KeyCode::Char('N') if shift => Input::QuickCreate,
        KeyCode::Char('x') => Input::Kill,
        KeyCode::Char('j') | KeyCode::Down => Input::Down,
        KeyCode::Char('k') | KeyCode::Up => Input::Up,
        KeyCode::Char('n') => Input::NewSession,
        KeyCode::Char('r') => Input::Rename,
        KeyCode::Char('l') | KeyCode::Right => Input::Expand,
        KeyCode::Char('h') | KeyCode::Left => Input::Collapse,
        KeyCode::Char('f') => Input::ToggleFocusMode,
        KeyCode::Char('z') => Input::ToggleAll,
        KeyCode::Enter => Input::Select,
        KeyCode::Char('g') => Input::EnterGroups,
        KeyCode::Char(',') => Input::EnterSettings,
        KeyCode::Char('/') => Input::EnterSearch,
        KeyCode::Char('d') if ctrl => Input::UndormantSession,
        KeyCode::Char('d') => Input::ToggleDormant,
        KeyCode::Char('?') => Input::OpenHelp,
        KeyCode::Char(c @ '1'..='9') if alt => Input::Switch(10 + (c as usize - '0' as usize)),
        KeyCode::Char('0') if alt => Input::Switch(20),
        KeyCode::Char(c @ '1'..='9') => Input::Switch(c as usize - '0' as usize),
        KeyCode::Char('0') => Input::Switch(10),
        KeyCode::Char('q') | KeyCode::Esc => Input::Quit,
        _ => Input::None,
    }
}

/// Whether `key` closes the shortcuts-overlay while it's open: `?` (the same
/// key that opened it), `Esc`, or `q`. Checked once at the top of the event
/// loop, ahead of the mode-specific key maps, so every other key is a no-op
/// while the overlay is showing (issue #156).
pub fn is_help_dismiss_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};


    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }
    fn alt(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT)
    }
    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn comma_enters_settings_from_command_mode() {
        assert_eq!(map_key(key(KeyCode::Char(','))), Input::EnterSettings);
    }

    #[test]
    fn altitude_keymap_same_verbs_one_level_up() {
        assert_eq!(map_altitude_key(key(KeyCode::Char('r'))), AltitudeInput::Rename);
        assert_eq!(map_altitude_key(key(KeyCode::Enter)), AltitudeInput::DescendInto);
        assert_eq!(map_altitude_key(key(KeyCode::Char('x'))), AltitudeInput::Delete);
        assert_eq!(map_altitude_key(key(KeyCode::Char('d'))), AltitudeInput::None); // reserved
        assert_eq!(map_altitude_key(key(KeyCode::Char('g'))), AltitudeInput::Descend);
        assert_eq!(map_altitude_key(key(KeyCode::Esc)), AltitudeInput::Descend);
        assert_eq!(map_altitude_key(key(KeyCode::Char('q'))), AltitudeInput::Quit);
        assert_eq!(map_altitude_key(key(KeyCode::Char('/'))), AltitudeInput::EnterSearch);
        assert_eq!(map_altitude_key(key(KeyCode::Char('3'))), AltitudeInput::Switch(3));
        assert_eq!(map_altitude_key(alt(KeyCode::Char('1'))), AltitudeInput::Switch(11));
        assert_eq!(map_altitude_key(key(KeyCode::Char('n'))), AltitudeInput::New);
        assert_eq!(map_altitude_key(key(KeyCode::Char('c'))), AltitudeInput::CycleColor);
        assert_eq!(map_altitude_key(shift(KeyCode::Char('J'))), AltitudeInput::MoveDown);
        assert_eq!(map_altitude_key(shift(KeyCode::Char('K'))), AltitudeInput::MoveUp);
    }

    #[test]
    fn altitude_keymap_plain_nav_and_shift_arrows() {
        assert_eq!(map_altitude_key(key(KeyCode::Char('j'))), AltitudeInput::Down);
        assert_eq!(map_altitude_key(key(KeyCode::Char('k'))), AltitudeInput::Up);
        assert_eq!(map_altitude_key(shift(KeyCode::Down)), AltitudeInput::MoveDown);
        assert_eq!(map_altitude_key(shift(KeyCode::Up)), AltitudeInput::MoveUp);
    }

    #[test]
    fn map_key_lowercase_r_is_rename() {
        assert_eq!(map_key(key(KeyCode::Char('r'))), Input::Rename);
    }

    #[test]
    fn map_key_shift_r_is_unmapped() {
        assert_eq!(map_key(shift(KeyCode::Char('R'))), Input::None);
    }

    #[test]
    fn maps_navigation_and_commands() {
        assert_eq!(map_key(key(KeyCode::Char('j'))), Input::Down);
        assert_eq!(map_key(key(KeyCode::Down)), Input::Down);
        assert_eq!(map_key(key(KeyCode::Char('k'))), Input::Up);
        assert_eq!(map_key(key(KeyCode::Char('l'))), Input::Expand);
        assert_eq!(map_key(key(KeyCode::Right)), Input::Expand);
        assert_eq!(map_key(key(KeyCode::Left)), Input::Collapse);
        assert_eq!(map_key(key(KeyCode::Char('h'))), Input::Collapse);
        assert_eq!(map_key(key(KeyCode::Char('f'))), Input::ToggleFocusMode);
        assert_eq!(map_key(key(KeyCode::Enter)), Input::Select);
        assert_eq!(map_key(key(KeyCode::Char('g'))), Input::EnterGroups);
        assert_eq!(map_key(key(KeyCode::Char('p'))), Input::None);
        assert_eq!(map_key(key(KeyCode::Char('q'))), Input::Quit);
        assert_eq!(map_key(key(KeyCode::Esc)), Input::Quit);
        assert_eq!(map_key(shift(KeyCode::Char('K'))), Input::MoveUp);
        assert_eq!(map_key(shift(KeyCode::Char('J'))), Input::MoveDown);
        assert_eq!(map_key(shift(KeyCode::Up)), Input::MoveUp);
        assert_eq!(map_key(shift(KeyCode::Down)), Input::MoveDown);
        assert_eq!(map_key(key(KeyCode::Char('z'))), Input::ToggleAll);
        assert_eq!(map_key(key(KeyCode::Char('1'))), Input::Switch(1));
        assert_eq!(map_key(key(KeyCode::Char('9'))), Input::Switch(9));
        assert_eq!(map_key(key(KeyCode::Char('0'))), Input::Switch(10));
        assert_eq!(map_key(key(KeyCode::Char('x'))), Input::Kill);
        assert_eq!(map_key(shift(KeyCode::Char('X'))), Input::None, "X is deliberately left unmapped");
        // Option/Alt+digit reaches the second decade of sessions (11-20).
        assert_eq!(map_key(alt(KeyCode::Char('1'))), Input::Switch(11));
        assert_eq!(map_key(alt(KeyCode::Char('9'))), Input::Switch(19));
        assert_eq!(map_key(alt(KeyCode::Char('0'))), Input::Switch(20));
    }

    #[test]
    fn maps_toggle_dormant_key() {
        assert_eq!(map_key(key(KeyCode::Char('d'))), Input::ToggleDormant);
    }

    #[test]
    fn ctrl_d_undormants_the_session_under_the_cursor() {
        assert_eq!(map_key(ctrl(KeyCode::Char('d'))), Input::UndormantSession);
    }

    #[test]
    fn shift_d_undormants_everything() {
        assert_eq!(map_key(shift(KeyCode::Char('D'))), Input::UndormantAll);
    }

    #[test]
    fn shift_n_quick_creates_a_group() {
        assert_eq!(map_key(shift(KeyCode::Char('N'))), Input::QuickCreate);
    }

    #[test]
    fn plain_n_starts_a_session_create_prompt() {
        assert_eq!(map_key(key(KeyCode::Char('n'))), Input::NewSession);
    }

    #[test]
    fn shift_n_at_group_altitude_starts_a_session_create_prompt_in_the_group() {
        assert_eq!(map_altitude_key(shift(KeyCode::Char('N'))), AltitudeInput::NewSessionInGroup);
        assert_eq!(map_altitude_key(key(KeyCode::Char('n'))), AltitudeInput::New, "plain n is still the existing new-group command");
    }

    #[test]
    fn plain_d_is_still_the_toggle_not_the_ctrl_variant() {
        assert_eq!(map_key(key(KeyCode::Char('d'))), Input::ToggleDormant);
    }

    #[test]
    fn question_mark_opens_help_in_command_groups_and_settings_modes() {
        assert_eq!(map_key(key(KeyCode::Char('?'))), Input::OpenHelp);
        assert_eq!(map_altitude_key(key(KeyCode::Char('?'))), AltitudeInput::OpenHelp);
        assert_eq!(map_settings_key(key(KeyCode::Char('?'))), SettingsInput::OpenHelp);
    }

    #[test]
    fn search_keys_map_to_query_edits_and_nav() {
        assert_eq!(map_search_key(key(KeyCode::Char('a'))), SearchInput::Char('a'));
        assert_eq!(map_search_key(key(KeyCode::Char('1'))), SearchInput::Char('1'));
        assert_eq!(map_search_key(shift(KeyCode::Char('A'))), SearchInput::Char('A'));
        assert_eq!(map_search_key(key(KeyCode::Backspace)), SearchInput::Backspace);
        assert_eq!(map_search_key(key(KeyCode::Enter)), SearchInput::Select);
        assert_eq!(map_search_key(key(KeyCode::Esc)), SearchInput::Exit);
        assert_eq!(map_search_key(key(KeyCode::Up)), SearchInput::Up);
        assert_eq!(map_search_key(key(KeyCode::Down)), SearchInput::Down);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('p'))), SearchInput::Up);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('k'))), SearchInput::Up);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('n'))), SearchInput::Down);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('j'))), SearchInput::Down);
        // Bulk deletes: Ctrl-W / Alt-Backspace delete a word, Ctrl-U clears.
        assert_eq!(map_search_key(ctrl(KeyCode::Char('w'))), SearchInput::DeleteWord);
        assert_eq!(map_search_key(alt(KeyCode::Backspace)), SearchInput::DeleteWord);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('u'))), SearchInput::Clear);
        // Plain Backspace still deletes a single char.
        assert_eq!(map_search_key(key(KeyCode::Backspace)), SearchInput::Backspace);
        // Ctrl-modified letters are nav/no-op, never query text.
        assert_eq!(map_search_key(ctrl(KeyCode::Char('a'))), SearchInput::None);
    }

    #[test]
    fn ctrl_f_toggles_focus_mode_in_search_but_plain_f_stays_query_text() {
        assert_eq!(map_search_key(ctrl(KeyCode::Char('f'))), SearchInput::ToggleFocusMode);
        assert_eq!(map_search_key(key(KeyCode::Char('f'))), SearchInput::Char('f'));
    }

    #[test]
    fn settings_keys_map_to_ops() {
        assert_eq!(map_settings_key(key(KeyCode::Char('j'))), SettingsInput::Down);
        assert_eq!(map_settings_key(key(KeyCode::Down)), SettingsInput::Down);
        assert_eq!(map_settings_key(key(KeyCode::Char('k'))), SettingsInput::Up);
        assert_eq!(map_settings_key(key(KeyCode::Up)), SettingsInput::Up);
        assert_eq!(map_settings_key(key(KeyCode::Char('l'))), SettingsInput::Right);
        assert_eq!(map_settings_key(key(KeyCode::Right)), SettingsInput::Right);
        assert_eq!(map_settings_key(key(KeyCode::Char('h'))), SettingsInput::Left);
        assert_eq!(map_settings_key(key(KeyCode::Left)), SettingsInput::Left);
        assert_eq!(map_settings_key(key(KeyCode::Enter)), SettingsInput::Activate);
        assert_eq!(map_settings_key(key(KeyCode::Char(' '))), SettingsInput::Activate);
        assert_eq!(map_settings_key(key(KeyCode::Char('c'))), SettingsInput::CycleColor);
        assert_eq!(map_settings_key(key(KeyCode::Esc)), SettingsInput::Exit);
        assert_eq!(map_settings_key(key(KeyCode::Char('q'))), SettingsInput::Exit);
        assert_eq!(map_settings_key(key(KeyCode::Char(','))), SettingsInput::Exit);
        assert_eq!(map_settings_key(key(KeyCode::Char('x'))), SettingsInput::None);
    }

    #[test]
    fn settings_digits_map_to_jump_targets() {
        assert_eq!(map_settings_key(key(KeyCode::Char('1'))), SettingsInput::Jump(1));
        assert_eq!(map_settings_key(key(KeyCode::Char('9'))), SettingsInput::Jump(9));
        assert_eq!(map_settings_key(key(KeyCode::Char('0'))), SettingsInput::Jump(10));
        assert_eq!(map_settings_key(alt(KeyCode::Char('1'))), SettingsInput::Jump(11));
        assert_eq!(map_settings_key(alt(KeyCode::Char('5'))), SettingsInput::Jump(15));
        assert_eq!(map_settings_key(alt(KeyCode::Char('0'))), SettingsInput::Jump(20));
    }

    #[test]
    fn search_expand_collapse_keys_map_correctly() {
        assert_eq!(map_search_key(key(KeyCode::Right)), SearchInput::Expand);
        assert_eq!(map_search_key(key(KeyCode::Left)), SearchInput::Collapse);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('l'))), SearchInput::Expand);
        assert_eq!(map_search_key(ctrl(KeyCode::Char('h'))), SearchInput::Collapse);
        // Bare l/h stay query text -- only the arrow/Ctrl forms drive the tree.
        assert_eq!(map_search_key(key(KeyCode::Char('l'))), SearchInput::Char('l'));
        assert_eq!(map_search_key(key(KeyCode::Char('h'))), SearchInput::Char('h'));
    }

    #[test]
    fn slash_enters_search_in_command_mode() {
        assert_eq!(map_key(key(KeyCode::Char('/'))), Input::EnterSearch);
    }

    #[test]
    fn is_help_dismiss_key_matches_question_mark_esc_and_q() {
        assert!(is_help_dismiss_key(key(KeyCode::Char('?'))));
        assert!(is_help_dismiss_key(key(KeyCode::Esc)));
        assert!(is_help_dismiss_key(key(KeyCode::Char('q'))));
        assert!(!is_help_dismiss_key(key(KeyCode::Char('x'))));
        assert!(!is_help_dismiss_key(key(KeyCode::Enter)));
    }
}
