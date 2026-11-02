use gtk4 as gtk;

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
