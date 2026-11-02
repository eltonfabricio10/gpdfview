use gtk4::prelude::*;

use crate::app::ViewerController;
use crate::session::toggle_favorite_document;

pub(crate) enum UiCommand {
    FocusSearch,
    ToggleSidebar,
    ToggleFavoriteDocument,
    ShowAbout,
    ToggleFullscreen,
    TogglePresentation,
    ShowDocumentProperties,
    ShowTabsHint,
}

pub(crate) fn dispatch_ui_command(
    controller: &ViewerController,
    command: UiCommand,
    sidebar: Option<&gtk4::Box>,
) {
    match command {
        UiCommand::FocusSearch => {
            controller.search_entry.grab_focus();
            controller.search_entry.select_region(0, -1);
        }
        UiCommand::ToggleSidebar => {
            if let Some(sidebar) = sidebar {
                sidebar.set_visible(!sidebar.is_visible());
            } else {
                controller
                    .status
                    .set_text("Sidebar indisponivel nesta visualizacao.");
            }
        }
        UiCommand::ToggleFavoriteDocument => {
            let path = controller.state.borrow().path.clone();
            if let Some(path) = path {
                let enabled = toggle_favorite_document(&path);
                if enabled {
                    controller.status.set_text("Documento adicionado aos favoritos.");
                } else {
                    controller.status.set_text("Documento removido dos favoritos.");
                }
            } else {
                controller.status.set_text("Abra um documento para favoritar.");
            }
        }
        UiCommand::ShowAbout => {
            controller
                .status
                .set_text("gpdfview: visualizador PDF em Rust + GTK4 inspirado no qpdfview.");
        }
        UiCommand::ToggleFullscreen => {
            if controller.window.is_fullscreen() {
                controller.window.unfullscreen();
            } else {
                controller.window.fullscreen();
            }
        }
        UiCommand::TogglePresentation => {
            controller.toggle_presentation_mode();
        }
        UiCommand::ShowDocumentProperties => {
            controller.show_document_properties();
        }
        UiCommand::ShowTabsHint => {
            controller
                .status
                .set_text("Use o botao Abrir para criar uma nova aba com outro documento.");
        }
    }
}
