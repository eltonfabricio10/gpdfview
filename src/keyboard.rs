use gtk4 as gtk;

use crate::session::{default_shortcuts, AppSettings};

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

    let modifier_mask = gtk::gdk::ModifierType::CONTROL_MASK.bits()
        | gtk::gdk::ModifierType::ALT_MASK.bits()
        | gtk::gdk::ModifierType::SHIFT_MASK.bits()
        | gtk::gdk::ModifierType::META_MASK.bits();
    let mut expected_modifiers = 0;
    for fragment in &fragments[..fragments.len() - 1] {
        let modifier = match normalize_shortcut_fragment(fragment).as_str() {
            "ctrl" | "control" => gtk::gdk::ModifierType::CONTROL_MASK.bits(),
            "alt" => gtk::gdk::ModifierType::ALT_MASK.bits(),
            "shift" => gtk::gdk::ModifierType::SHIFT_MASK.bits(),
            "meta" | "super" | "cmd" => gtk::gdk::ModifierType::META_MASK.bits(),
            _ => return false,
        };
        if expected_modifiers & modifier != 0 {
            return false;
        }
        expected_modifiers |= modifier;
    }

    expected_modifiers == modifiers & modifier_mask
}

pub(crate) fn resolve_key_action(key: &str, modifiers: u32) -> Option<KeyAction> {
    let normalized = key.trim();
    let lowered = normalized.to_ascii_lowercase().replace(['_', '-'], "");
    let ctrl = gtk::gdk::ModifierType::CONTROL_MASK.bits();
    let modifier_mask = ctrl
        | gtk::gdk::ModifierType::ALT_MASK.bits()
        | gtk::gdk::ModifierType::SHIFT_MASK.bits()
        | gtk::gdk::ModifierType::META_MASK.bits();
    let has_only_ctrl = modifiers & modifier_mask == ctrl;
    let has_no_modifiers = modifiers & modifier_mask == 0;

    if has_only_ctrl && lowered == "o" {
        return Some(KeyAction::OpenFile);
    }
    if has_only_ctrl && lowered == "f" {
        return Some(KeyAction::FocusSearch);
    }
    if has_no_modifiers && lowered == "f11" {
        return Some(KeyAction::ToggleFullscreen);
    }
    if has_no_modifiers && lowered == "f5" {
        return Some(KeyAction::TogglePresentation);
    }

    if has_no_modifiers {
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
    } else {
        None
    }
}

fn action_setting_key(action: KeyAction) -> Option<&'static str> {
    match action {
        KeyAction::OpenFile => Some("open_file"),
        KeyAction::FocusSearch => Some("focus_search"),
        KeyAction::ToggleFullscreen => Some("toggle_fullscreen"),
        KeyAction::TogglePresentation => Some("toggle_presentation"),
        KeyAction::NextPage => Some("next_page"),
        KeyAction::PreviousPage => Some("previous_page"),
        KeyAction::ZoomIn => Some("zoom_in"),
        KeyAction::ZoomOut => Some("zoom_out"),
        KeyAction::FitWidth => Some("fit_width"),
        KeyAction::FitPage => Some("fit_page"),
        KeyAction::RotateLeft => Some("rotate_left"),
        KeyAction::RotateRight => Some("rotate_right"),
        KeyAction::FirstPage | KeyAction::LastPage => None,
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
        if settings
            .shortcuts
            .get(action_name)
            .is_some_and(|value| shortcut_matches(value, key, modifiers))
        {
            return Some(action);
        }
    }

    let default_action = resolve_key_action(key, modifiers)?;
    let Some(action_name) = action_setting_key(default_action) else {
        return Some(default_action);
    };
    let defaults = default_shortcuts();
    if let (Some(default), Some(configured)) = (
        defaults.get(action_name),
        settings.shortcuts.get(action_name),
    ) {
        if shortcut_matches(default, key, modifiers)
            && !shortcut_matches(configured, key, modifiers)
        {
            return None;
        }
    }
    Some(default_action)
}
