use gtk4 as gtk;

use crate::session::AppSettings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyAction {
    NextPage,
    PreviousPage,
    FirstPage,
    LastPage,
    ZoomIn,
    ZoomOut,
    FitWidth,
    FitPage,
    RotateLeft,
    RotateRight,
    OpenFile,
    FocusSearch,
    ToggleFullscreen,
    TogglePresentation,
}

fn normalize_shortcut_fragment(value: &str) -> String {
    value
        .trim()
        .replace(['_', '-'], "")
        .replace(' ', "")
        .to_ascii_lowercase()
}

fn shortcut_matches(value: &str, key: &str, modifiers: u32) -> bool {
    let fragments = value
        .split('+')
        .map(str::trim)
        .filter(|fragment| !fragment.is_empty())
        .collect::<Vec<_>>();
    if fragments.is_empty() {
        return false;
    }

    let expected_key = if let Some(last) = fragments.last() {
        normalize_shortcut_fragment(last)
    } else {
        String::new()
    };
    let key_name = normalize_shortcut_fragment(key);
    if expected_key != key_name {
        return false;
    }

    let has_ctrl = modifiers & gtk::gdk::ModifierType::CONTROL_MASK.bits() != 0;
    let has_alt = modifiers & gtk::gdk::ModifierType::ALT_MASK.bits() != 0;
    let has_shift = modifiers & gtk::gdk::ModifierType::SHIFT_MASK.bits() != 0;
    let has_meta = modifiers & gtk::gdk::ModifierType::META_MASK.bits() != 0;

    let mut matched_mods = 0usize;
    for fragment in &fragments[..fragments.len() - 1] {
        match normalize_shortcut_fragment(fragment).as_str() {
            "ctrl" | "control" => matched_mods += has_ctrl as usize,
            "alt" => matched_mods += has_alt as usize,
            "shift" => matched_mods += has_shift as usize,
            "meta" | "super" | "cmd" => matched_mods += has_meta as usize,
            _ => {}
        }
    }

    matched_mods == fragments.len() - 1
}

pub(crate) fn resolve_key_action(key: &str, modifiers: u32) -> Option<KeyAction> {
    let normalized = key.trim();
    let lowered = normalized
        .to_ascii_lowercase()
        .replace(['_', '-'], "");
    let has_ctrl = modifiers & gtk::gdk::ModifierType::CONTROL_MASK.bits() != 0;

    if has_ctrl && lowered == "o" {
        return Some(KeyAction::OpenFile);
    }
    if has_ctrl && lowered == "f" {
        return Some(KeyAction::FocusSearch);
    }
    if lowered == "f11" {
        return Some(KeyAction::ToggleFullscreen);
    }
    if lowered == "f5" {
        return Some(KeyAction::TogglePresentation);
    }

    match lowered.as_str() {
        "pagedown" | "space" | "right" | "j" => Some(KeyAction::NextPage),
        "pageup" | "left" | "k" => Some(KeyAction::PreviousPage),
        "home" => Some(KeyAction::FirstPage),
        "end" => Some(KeyAction::LastPage),
        "plus" | "equal" | "kpadd" => Some(KeyAction::ZoomIn),
        "minus" | "kpsubtract" => Some(KeyAction::ZoomOut),
        "f" => Some(KeyAction::FitWidth),
        "g" => Some(KeyAction::FitPage),
        "l" => Some(KeyAction::RotateLeft),
        "r" => Some(KeyAction::RotateRight),
        _ => None,
    }
}

pub(crate) fn resolve_key_action_with_settings(
    key: &str,
    modifiers: u32,
    settings: &AppSettings,
) -> Option<KeyAction> {
    let custom_map = [
        ("open_file", KeyAction::OpenFile),
        ("focus_search", KeyAction::FocusSearch),
        ("toggle_fullscreen", KeyAction::ToggleFullscreen),
        ("toggle_presentation", KeyAction::TogglePresentation),
        ("next_page", KeyAction::NextPage),
        ("previous_page", KeyAction::PreviousPage),
        ("zoom_in", KeyAction::ZoomIn),
        ("zoom_out", KeyAction::ZoomOut),
        ("fit_width", KeyAction::FitWidth),
        ("fit_page", KeyAction::FitPage),
        ("rotate_left", KeyAction::RotateLeft),
        ("rotate_right", KeyAction::RotateRight),
    ];

    for (action_name, action) in custom_map {
        let Some(value) = settings.shortcuts.get(action_name) else {
            continue;
        };
        if shortcut_matches(value, key, modifiers) {
            return Some(action);
        }
    }

    resolve_key_action(key, modifiers)
}
