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

pub(crate) fn open_document_in_new_tab(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    path: PathBuf,
    title_label: gtk4::Label,
    print_button: gtk4::Button,
) -> Option<Rc<ViewerController>> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Document")
        .to_string();
    let (renderer, printer) = match new_worker_pair() {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("{error}");
            return None;
        }
    };
    let new_controller = create_document_controller(DocumentControllerParams {
        app: app.clone(),
        window,
        renderer,
        printer,
        tab_manager: tab_manager.clone(),
        initial_path: Some(path.clone()),
        title: title_label,
        print_button,
    });
    tab_manager.borrow().add_tab_with_path(&new_controller.root, &filename, Some(path.clone()));
    remember_recent_document(&path);
    new_controller.open_document(path);
    Some(new_controller)
}

pub(crate) fn show_open_dialog(
    app: adw::Application,
    window: adw::ApplicationWindow,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    controller: Rc<ViewerController>,
    last_directory: Option<PathBuf>,
) {
    let dialog = gtk4::FileDialog::builder().title("Open PDF").build();
    let filter = gtk4::FileFilter::new();
    filter.set_name(Some("PDF documents"));
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

    let dialog_window = window.clone();
    let dialog_parent = dialog_window.clone();
    let dialog_app = app.clone();
    dialog.open_multiple(
        Some(&dialog_parent),
        None::<&gtk4::gio::Cancellable>,
        move |result| match result {
            Ok(files) => {
                let paths = selected_pdf_paths(&files);
                if paths.is_empty() {
                    controller.status.set_text("No PDF files were selected from the picker.");
                    return;
                }
                for path in paths {
                    if let Some(parent) = path.parent() {
                        remember_last_directory(parent);
                    }
                    if let Some(new_controller) = open_document_in_new_tab(
                        &dialog_app,
                        &dialog_window,
                        tab_manager.clone(),
                        path.clone(),
                        controller.title.clone(),
                        controller.print_button.clone(),
                    ) {
                        let _ = new_controller;
                    }
                }
            }
            Err(error) if error.matches(gtk4::gio::IOErrorEnum::Cancelled) => {}
            Err(error) => controller
                .status
                .set_text(&format!("Could not open file picker: {error}")),
        },
    );
}
