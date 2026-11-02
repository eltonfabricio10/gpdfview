use gtk4::prelude::*;

use crate::app::ViewerController;
use crate::session::{toggle_bookmark_page, toggle_favorite_document};
use crate::viewer_state::SplitViewMode;

pub(crate) enum UiCommand {
    FocusSearch,
    ToggleSidebar,
    ToggleFavoriteDocument,
    ShowAbout,
    ToggleFullscreen,
    TogglePresentation,
    ToggleCurrentPageBookmark,
    ToggleSplitViewHorizontal,
    ToggleSplitViewVertical,
    CloseSplitView,
    ToggleSplitSync,
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
        UiCommand::ToggleCurrentPageBookmark => {
            let Some(path) = controller.state.borrow().path.clone() else {
                controller.status.set_text("Abra um documento antes de criar um marcador.");
                return;
            };
            let page_index = controller.state.borrow().page_index;
            let label = format!("Página {}", page_index + 1);
            let added = toggle_bookmark_page(&path, page_index, Some(&label));
            if added {
                controller.status.set_text("Marcador da página atual adicionado.");
            } else {
                controller.status.set_text("Marcador da página atual removido.");
            }
        }
        UiCommand::ToggleSplitViewHorizontal => {
            let mut state = controller.state.borrow_mut();
            state.split_mode = SplitViewMode::Horizontal;
            let max_index = state.page_count.max(1) - 1;
            state.split_secondary_page_index = if state.page_count > 0 {
                (state.page_index + 1).clamp(0, max_index)
            } else {
                0
            };
            drop(state);
            controller.area.queue_draw();
        }
        UiCommand::ToggleSplitViewVertical => {
            let mut state = controller.state.borrow_mut();
            state.split_mode = SplitViewMode::Vertical;
            let max_index = state.page_count.max(1) - 1;
            state.split_secondary_page_index = if state.page_count > 0 {
                (state.page_index + 1).clamp(0, max_index)
            } else {
                0
            };
            drop(state);
            controller.area.queue_draw();
        }
        UiCommand::CloseSplitView => {
            let mut state = controller.state.borrow_mut();
            state.split_mode = SplitViewMode::Off;
            state.split_secondary_page_index = state.page_index;
            drop(state);
            controller.area.queue_draw();
        }
        UiCommand::ToggleSplitSync => {
            let mut state = controller.state.borrow_mut();
            state.split_sync = !state.split_sync;
            if state.split_sync {
                state.sync_split_view_to_primary();
            }
            let mode_message = if state.split_sync { "sincronizado" } else { "desacoplado" };
            controller.status.set_text(&format!("Divisao de visualizacao {mode_message}."));
            drop(state);
            controller.area.queue_draw();
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
