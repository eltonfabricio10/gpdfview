use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::app::{create_document_controller, new_worker_pair, ViewerController};
use crate::document_tabs::DocumentTabManager;
use crate::session::{remember_last_directory, remember_recent_document};
use crate::window::DocumentControllerParams;

pub(crate) fn selected_pdf_paths(model: &gtk4::gio::ListModel) -> Vec<PathBuf> {
    let mut selected = Vec::new();
    for index in 0..model.n_items() {
        let Some(item) = model.item(index) else {
            continue;
        };
        let Ok(file) = item.downcast::<gtk4::gio::File>() else {
            continue;
        };
        let Some(path) = file.path() else {
            continue;
        };
        let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if ext.eq_ignore_ascii_case("pdf") {
            selected.push(path);
        }
    }
    selected
}

#[derive(Clone)]
pub(crate) struct OpenDocumentContext {
    pub app: adw::Application,
    pub window: adw::ApplicationWindow,
    pub tab_manager: Rc<RefCell<DocumentTabManager>>,
    pub title_label: gtk4::Label,
    pub print_button: gtk4::Button,
    pub toolbar_host: gtk4::Box,
    pub search_host: gtk4::Box,
}

pub(crate) fn open_document_in_new_tab(
    context: &OpenDocumentContext,
    path: PathBuf,
) -> Result<Rc<ViewerController>, String> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Document")
        .to_string();
    let (renderer, printer) = new_worker_pair()?;
    let new_controller = create_document_controller(DocumentControllerParams {
        app: context.app.clone(),
        window: &context.window,
        renderer,
        printer,
        tab_manager: context.tab_manager.clone(),
        initial_path: Some(path.clone()),
        title: context.title_label.clone(),
        print_button: context.print_button.clone(),
        toolbar_host: context.toolbar_host.clone(),
        search_host: context.search_host.clone(),
    });
    context.tab_manager.borrow().add_tab_with_path(
        &new_controller.root,
        &filename,
        Some(path.clone()),
    );
    context
        .tab_manager
        .borrow()
        .register_controller(&new_controller.root, new_controller.clone());
    remember_recent_document(&path);
    new_controller.open_document(path);
    Ok(new_controller)
}

pub(crate) fn show_open_dialog(
    context: OpenDocumentContext,
    controller: Rc<ViewerController>,
    last_directory: Option<PathBuf>,
) {
    let dialog = gtk4::FileDialog::builder().title("Abrir PDF").build();
    let filter = gtk4::FileFilter::new();
    filter.set_name(Some("Documentos PDF"));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");
    let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(Some(&filter));

    if let Some(directory) = last_directory {
        let folder = gtk4::gio::File::for_path(&directory);
        dialog.set_initial_folder(Some(&folder));
    }

    let dialog_parent = context.window.clone();
    dialog.open_multiple(
        Some(&dialog_parent),
        None::<&gtk4::gio::Cancellable>,
        move |result| match result {
            Ok(files) => {
                let paths = selected_pdf_paths(&files);
                if paths.is_empty() {
                    controller
                        .status
                        .set_text("Nenhum arquivo PDF foi selecionado.");
                    return;
                }
                for path in paths {
                    if let Some(parent) = path.parent() {
                        remember_last_directory(parent);
                    }
                    if let Err(error) = open_document_in_new_tab(&context, path.clone()) {
                        controller
                            .status
                            .set_text(&format!("Não foi possível abrir o PDF: {error}"));
                    }
                }
            }
            Err(error) if error.matches(gtk4::gio::IOErrorEnum::Cancelled) => {}
            Err(error) => controller.status.set_text(&format!(
                "Não foi possível abrir o seletor de arquivos: {error}"
            )),
        },
    );
}
