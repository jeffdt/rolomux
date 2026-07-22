//! Keyboard input decoding: maps crossterm key events to the mode-specific
//! Input / SearchInput / GroupInput / SettingsInput command enums the event
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
    ToggleFocusMode,
    ToggleShortcuts,
    Rename,
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
pub enum GroupInput { Up, Down, MoveUp, MoveDown, New, Rename, CycleColor, Delete, ToggleShortcuts, Exit, None }

/// Key mapping for group-management mode while NOT editing a name. During an
/// inline rename the loop routes keys through `map_search_key` instead.
pub fn map_group_key(key: KeyEvent) -> GroupInput {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Char('J') | KeyCode::Down if shift => GroupInput::MoveDown,
        KeyCode::Char('K') | KeyCode::Up if shift => GroupInput::MoveUp,
        KeyCode::Char('j') | KeyCode::Down => GroupInput::Down,
        KeyCode::Char('k') | KeyCode::Up => GroupInput::Up,
        KeyCode::Char('n') => GroupInput::New,
        KeyCode::Enter | KeyCode::Char('r') => GroupInput::Rename,
        KeyCode::Char('c') => GroupInput::CycleColor,
        KeyCode::Char('d') => GroupInput::Delete,
        KeyCode::Char('?') => GroupInput::ToggleShortcuts,
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('g') => GroupInput::Exit,
        _ => GroupInput::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsInput {
    Up,
    Down,
    Left,
    Right,
    Activate,
    CycleColor,
    ToggleShortcuts,
    Exit,
    None,
}

/// Key mapping for settings mode. `,` exits (mirroring how it also enters,
/// same as `g` for Groups mode), alongside the usual `q`/`Esc`. The palette
/// checklist has a fixed display order (`ALL_NAMED_COLORS` canonical order),
/// so there is no reorder key here (unlike Groups mode's `⇧JK`).
pub fn map_settings_key(key: KeyEvent) -> SettingsInput {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => SettingsInput::Down,
        KeyCode::Char('k') | KeyCode::Up => SettingsInput::Up,
        KeyCode::Char('l') | KeyCode::Right => SettingsInput::Right,
        KeyCode::Char('h') | KeyCode::Left => SettingsInput::Left,
        KeyCode::Enter | KeyCode::Char(' ') => SettingsInput::Activate,
        KeyCode::Char('c') => SettingsInput::CycleColor,
        KeyCode::Char('?') => SettingsInput::ToggleShortcuts,
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(',') => SettingsInput::Exit,
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
    match key.code {
        KeyCode::Char('K') | KeyCode::Up if shift => Input::MoveUp,
        KeyCode::Char('J') | KeyCode::Down if shift => Input::MoveDown,
        KeyCode::Char('R') if shift => Input::Rename,
        KeyCode::Char('x') => Input::Kill,
        KeyCode::Char('j') | KeyCode::Down => Input::Down,
        KeyCode::Char('k') | KeyCode::Up => Input::Up,
        KeyCode::Char('l') | KeyCode::Right => Input::Expand,
        KeyCode::Left => Input::Collapse,
        KeyCode::Char('f') => Input::ToggleFocusMode,
        KeyCode::Char('z') => Input::ToggleAll,
        KeyCode::Enter => Input::Select,
        KeyCode::Char('g') => Input::EnterGroups,
        KeyCode::Char(',') => Input::EnterSettings,
        KeyCode::Char('/') => Input::EnterSearch,
        KeyCode::Char('d') => Input::ToggleDormant,
        KeyCode::Char('?') => Input::ToggleShortcuts,
        KeyCode::Char(c @ '1'..='9') if alt => Input::Switch(10 + (c as usize - '0' as usize)),
        KeyCode::Char('0') if alt => Input::Switch(20),
        KeyCode::Char(c @ '1'..='9') => Input::Switch(c as usize - '0' as usize),
        KeyCode::Char('0') => Input::Switch(10),
        KeyCode::Char('q') | KeyCode::Esc => Input::Quit,
        _ => Input::None,
    }
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
    fn group_keys_map_to_ops() {
        assert_eq!(map_group_key(key(KeyCode::Char('j'))), GroupInput::Down);
        assert_eq!(map_group_key(key(KeyCode::Char('k'))), GroupInput::Up);
        assert_eq!(map_group_key(shift(KeyCode::Char('J'))), GroupInput::MoveDown);
        assert_eq!(map_group_key(shift(KeyCode::Char('K'))), GroupInput::MoveUp);
        assert_eq!(map_group_key(shift(KeyCode::Down)), GroupInput::MoveDown);
        assert_eq!(map_group_key(shift(KeyCode::Up)), GroupInput::MoveUp);
        assert_eq!(map_group_key(key(KeyCode::Char('n'))), GroupInput::New);
        assert_eq!(map_group_key(key(KeyCode::Enter)), GroupInput::Rename);
        assert_eq!(map_group_key(key(KeyCode::Char('r'))), GroupInput::Rename);
        assert_eq!(map_group_key(key(KeyCode::Char('c'))), GroupInput::CycleColor);
        assert_eq!(map_group_key(key(KeyCode::Char('d'))), GroupInput::Delete);
        assert_eq!(map_group_key(key(KeyCode::Esc)), GroupInput::Exit);
        assert_eq!(map_group_key(key(KeyCode::Char('q'))), GroupInput::Exit);
        assert_eq!(map_group_key(key(KeyCode::Char('g'))), GroupInput::Exit);
        assert_eq!(map_group_key(key(KeyCode::Char('x'))), GroupInput::None);
    }

    #[test]
    fn map_key_lowercase_r_is_unmapped() {
        assert_eq!(map_key(key(KeyCode::Char('r'))), Input::None);
    }

    #[test]
    fn map_key_shift_r_is_rename() {
        assert_eq!(map_key(shift(KeyCode::Char('R'))), Input::Rename);
    }

    #[test]
    fn maps_navigation_and_commands() {
        assert_eq!(map_key(key(KeyCode::Char('j'))), Input::Down);
        assert_eq!(map_key(key(KeyCode::Down)), Input::Down);
        assert_eq!(map_key(key(KeyCode::Char('k'))), Input::Up);
        assert_eq!(map_key(key(KeyCode::Char('l'))), Input::Expand);
        assert_eq!(map_key(key(KeyCode::Right)), Input::Expand);
        assert_eq!(map_key(key(KeyCode::Left)), Input::Collapse);
        assert_eq!(map_key(key(KeyCode::Char('h'))), Input::None, "h is retired; f replaces it");
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
    fn question_mark_toggles_shortcuts_in_command_groups_and_settings_modes() {
        assert_eq!(map_key(key(KeyCode::Char('?'))), Input::ToggleShortcuts);
        assert_eq!(map_group_key(key(KeyCode::Char('?'))), GroupInput::ToggleShortcuts);
        assert_eq!(map_settings_key(key(KeyCode::Char('?'))), SettingsInput::ToggleShortcuts);
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
}
