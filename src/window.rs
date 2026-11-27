use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::cairo::ImageSurface;
use gtk4::prelude::*;
use libadwaita as adw;

use crate::app::ViewerController;
use crate::cache::LruCache;
use crate::document_actions::{open_document_in_new_tab, show_open_dialog};
use crate::document_tabs::DocumentTabManager;
use crate::keyboard::KeyAction;
use crate::layout::{
    cairo_search_highlight_rects, continuous_page_stride, CONTINUOUS_PAGE_GAP,
    CONTINUOUS_SIDE_MARGIN, CONTINUOUS_TOP_MARGIN,
};
use crate::printing::PrintService;
use crate::session::{
    last_directory_for_home_path, last_window_geometry_for_home_path,
    last_window_state_for_home_path, load_recent_documents, newest_valid_recent_document,
    remember_resume_last_document, remember_window_geometry, remember_window_state,
    should_resume_last_document,
};
use crate::sidebar::build_sidebar;
use crate::ui_commands::{dispatch_ui_command, UiCommand};
use crate::ui_helpers::action_button;
use crate::viewer_state::{LayoutMode, SplitViewMode, ViewerState, DEFAULT_ZOOM};

pub(crate) struct DocumentControllerParams<'a> {
    pub app: adw::Application,
    pub window: &'a adw::ApplicationWindow,
    pub renderer: Rc<crate::engine::PdfRenderer>,
    pub printer: Rc<PrintService>,
    pub tab_manager: Rc<RefCell<DocumentTabManager>>,
    pub initial_path: Option<PathBuf>,
    pub title: gtk::Label,
    pub print_button: gtk::Button,
    pub toolbar_host: gtk::Box,
}

fn create_menu_button(label: &str) -> (gtk::MenuButton, gtk::Box) {
    let button = gtk::MenuButton::new();
    button.set_label(label);
    button.set_has_frame(false);

    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    menu_box.set_margin_start(6);
    menu_box.set_margin_end(6);
    menu_box.set_margin_top(6);
    menu_box.set_margin_bottom(6);

    let popover = gtk::Popover::new();
    popover.set_child(Some(&menu_box));
    button.set_popover(Some(&popover));

    (button, menu_box)
}

fn append_menu_item<F>(menu_box: &gtk::Box, label: &str, on_click: F) -> gtk::Button
where
    F: Fn() + 'static,
{
    let item = gtk::Button::with_label(label);
    item.set_halign(gtk::Align::Fill);
    item.set_hexpand(true);
    item.set_has_frame(false);
    item.add_css_class("flat");
    item.connect_clicked(move |_| on_click());
    menu_box.append(&item);
    item
}

fn append_menu_separator(menu_box: &gtk::Box) {
    menu_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
}

fn active_controller(
    tab_manager: &Rc<RefCell<DocumentTabManager>>,
    fallback: &Rc<ViewerController>,
) -> Rc<ViewerController> {
    tab_manager
        .borrow()
        .active_controller()
        .unwrap_or_else(|| fallback.clone())
}

pub(crate) fn build_ui(app: &adw::Application) -> Option<Rc<ViewerController>> {
    let (renderer, printer) = match crate::app::new_worker_pair() {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("{error}");
            app.quit();
            return None;
        }
    };

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("gpdfview")
        .default_width(1080)
        .default_height(760)
        .build();
    if let Some((width, height)) = last_window_geometry_for_home_path() {
        window.set_default_size(width, height);
        remember_window_geometry(width, height);
    } else {
        remember_window_geometry(1080, 760);
    }
    if let Some((was_maximized, was_fullscreen)) = last_window_state_for_home_path() {
        if was_maximized {
            window.maximize();
        }
        if was_fullscreen {
            window.fullscreen();
        }
    }

    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let menu_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    menu_bar.set_hexpand(true);
    menu_bar.set_margin_start(4);
    menu_bar.set_margin_end(4);
    menu_bar.set_margin_top(2);
    menu_bar.set_margin_bottom(2);
    let (menu_file, file_menu_box) = create_menu_button("Arquivo");
    let (menu_edit, edit_menu_box) = create_menu_button("Editar");
    let (menu_view, view_menu_box) = create_menu_button("Exibir");
    let (menu_tabs, tabs_menu_box) = create_menu_button("Abas");
    let (menu_bookmarks, bookmarks_menu_box) = create_menu_button("Marcadores");
    let (menu_help, help_menu_box) = create_menu_button("Ajuda");
    menu_bar.append(&menu_file);
    menu_bar.append(&menu_edit);
    menu_bar.append(&menu_view);
    menu_bar.append(&menu_tabs);
    menu_bar.append(&menu_bookmarks);
    menu_bar.append(&menu_help);
    let header = gtk::HeaderBar::new();
    let title = gtk::Label::new(Some("gpdfview"));
    header.set_title_widget(Some(&title));

    let sidebar_button = action_button("sidebar-show-symbolic", "Toggle document sidebar");
    header.pack_start(&sidebar_button);
    let open_button = action_button("document-open-symbolic", "Open a PDF document");
    header.pack_start(&open_button);
    let fullscreen_button = action_button("view-fullscreen-symbolic", "Toggle fullscreen");
    header.pack_end(&fullscreen_button);
    let presentation_button =
        action_button("view-presentation-symbolic", "Toggle presentation mode");
    header.pack_end(&presentation_button);
    let properties_button = action_button("document-properties-symbolic", "Document properties");
    header.pack_end(&properties_button);
    let print_button = action_button(
        "printer-symbolic",
        "Send the current PDF to the default printer",
    );
    print_button.set_sensitive(false);
    header.pack_end(&print_button);

    let toolbar_host = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    let last_directory = last_directory_for_home_path();

    let notebook = gtk::Notebook::new();
    notebook.set_scrollable(true);
    notebook.set_show_border(false);
    notebook.set_show_tabs(false);
    notebook.set_tab_pos(gtk::PositionType::Top);
    notebook.set_hexpand(true);
    notebook.set_vexpand(true);

    let tab_manager = Rc::new(RefCell::new(DocumentTabManager::new(notebook.clone())));

    let initial = create_document_controller(DocumentControllerParams {
        app: app.clone(),
        window: &window,
        renderer: renderer.clone(),
        printer: printer.clone(),
        tab_manager: tab_manager.clone(),
        initial_path: None,
        title: title.clone(),
        print_button: print_button.clone(),
        toolbar_host: toolbar_host.clone(),
    });
    toolbar_host.append(&initial.toolbar);
    {
        let print_manager = tab_manager.clone();
        let fallback = initial.clone();
        print_button.connect_clicked(move |_| {
            active_controller(&print_manager, &fallback).submit_print();
        });
    }

    if should_resume_last_document() {
        if let Some(path) = newest_valid_recent_document(&load_recent_documents()) {
            let title_text = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Documento")
                .to_string();
            tab_manager
                .borrow()
                .add_tab_with_path(&initial.root, &title_text, Some(path.clone()));
            tab_manager
                .borrow()
                .register_controller(&initial.root, initial.clone());
            initial.open_document(path);
        }
    }

    let sidebar = build_sidebar(
        app,
        &window,
        tab_manager.clone(),
        title.clone(),
        print_button.clone(),
        initial.clone(),
        toolbar_host.clone(),
    );
    sidebar.set_visible(false);

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.append(&sidebar);
    content.append(&notebook);
    shell.append(&header);
    shell.append(&menu_bar);
    shell.append(&toolbar_host);
    shell.append(&content);
    let open_controller = initial.clone();
    let open_window = window.clone();
    let open_tab_manager = tab_manager.clone();
    let open_app = app.clone();
    let sidebar_toggle = sidebar.clone();
    let sidebar_command_controller = initial.clone();
    sidebar_button.connect_clicked(move |_| {
        dispatch_ui_command(
            &sidebar_command_controller,
            UiCommand::ToggleSidebar,
            Some(&sidebar_toggle),
        );
    });

    let menu_open_controller = open_controller.clone();
    let menu_open_tab_manager = open_tab_manager.clone();
    let menu_open_app = open_app.clone();
    let menu_open_window = open_window.clone();
    let menu_last_directory = last_directory.clone();
    let file_open_item = append_menu_item(&file_menu_box, "Abrir PDF...", move || {
        show_open_dialog(
            menu_open_app.clone(),
            menu_open_window.clone(),
            menu_open_tab_manager.clone(),
            menu_open_controller.clone(),
            menu_last_directory.clone(),
            menu_open_controller.toolbar_host.clone(),
        );
    });
    let menu_print_manager = tab_manager.clone();
    let menu_print_fallback = initial.clone();
    let file_print_item = append_menu_item(&file_menu_box, "Imprimir", move || {
        active_controller(&menu_print_manager, &menu_print_fallback).submit_print();
    });
    let menu_props_manager = tab_manager.clone();
    let menu_props_fallback = initial.clone();
    let file_properties_item = append_menu_item(&file_menu_box, "Propriedades", move || {
        dispatch_ui_command(
            &active_controller(&menu_props_manager, &menu_props_fallback),
            UiCommand::ShowDocumentProperties,
            None,
        );
    });
    append_menu_separator(&file_menu_box);
    let menu_about_from_file = initial.clone();
    let _file_about_item = append_menu_item(&file_menu_box, "Sobre", move || {
        dispatch_ui_command(&menu_about_from_file, UiCommand::ShowAbout, None);
    });

    let menu_search_manager = tab_manager.clone();
    let menu_search_fallback = initial.clone();
    let edit_focus_item = append_menu_item(&edit_menu_box, "Focar busca", move || {
        dispatch_ui_command(
            &active_controller(&menu_search_manager, &menu_search_fallback),
            UiCommand::FocusSearch,
            None,
        );
    });
    let edit_next_manager = tab_manager.clone();
    let edit_next_fallback = initial.clone();
    let edit_next_match_item = append_menu_item(&edit_menu_box, "Proximo resultado", move || {
        active_controller(&edit_next_manager, &edit_next_fallback).search_next_match();
    });
    let edit_prev_manager = tab_manager.clone();
    let edit_prev_fallback = initial.clone();
    let edit_prev_match_item = append_menu_item(&edit_menu_box, "Resultado anterior", move || {
        active_controller(&edit_prev_manager, &edit_prev_fallback).search_previous_match();
    });
    let first_page_manager = tab_manager.clone();
    let first_page_fallback = initial.clone();
    let first_page_item = append_menu_item(&edit_menu_box, "Primeira página", move || {
        active_controller(&first_page_manager, &first_page_fallback)
            .handle_key_action(KeyAction::FirstPage);
    });
    let last_page_manager = tab_manager.clone();
    let last_page_fallback = initial.clone();
    let last_page_item = append_menu_item(&edit_menu_box, "Última página", move || {
        active_controller(&last_page_manager, &last_page_fallback)
            .handle_key_action(KeyAction::LastPage);
    });
    let history_back_manager = tab_manager.clone();
    let history_back_fallback = initial.clone();
    let history_back_item = append_menu_item(&edit_menu_box, "Voltar para página", move || {
        active_controller(&history_back_manager, &history_back_fallback).go_page_back();
    });
    let history_forward_manager = tab_manager.clone();
    let history_forward_fallback = initial.clone();
    let history_forward_item =
        append_menu_item(&edit_menu_box, "Avançar para página", move || {
            active_controller(&history_forward_manager, &history_forward_fallback)
                .go_page_forward();
        });
    history_back_item.set_sensitive(false);
    history_forward_item.set_sensitive(false);

    let menu_sidebar = sidebar.clone();
    let menu_sidebar_controller = initial.clone();
    let view_sidebar_item =
        append_menu_item(&view_menu_box, "Alternar painel lateral", move || {
            dispatch_ui_command(
                &menu_sidebar_controller,
                UiCommand::ToggleSidebar,
                Some(&menu_sidebar),
            );
        });
    let view_fullscreen_manager = tab_manager.clone();
    let view_fullscreen_fallback = initial.clone();
    let view_fullscreen_item = append_menu_item(&view_menu_box, "Tela cheia", move || {
        dispatch_ui_command(
            &active_controller(&view_fullscreen_manager, &view_fullscreen_fallback),
            UiCommand::ToggleFullscreen,
            None,
        );
    });
    let view_presentation_manager = tab_manager.clone();
    let view_presentation_fallback = initial.clone();
    let view_presentation_item = append_menu_item(&view_menu_box, "Modo apresentacao", move || {
        dispatch_ui_command(
            &active_controller(&view_presentation_manager, &view_presentation_fallback),
            UiCommand::TogglePresentation,
            None,
        );
    });
    let view_split_horizontal_manager = tab_manager.clone();
    let view_split_horizontal_fallback = initial.clone();
    let view_split_horizontal_item =
        append_menu_item(&view_menu_box, "Dividir horizontalmente", move || {
            dispatch_ui_command(
                &active_controller(
                    &view_split_horizontal_manager,
                    &view_split_horizontal_fallback,
                ),
                UiCommand::ToggleSplitViewHorizontal,
                None,
            );
        });
    let view_split_vertical_manager = tab_manager.clone();
    let view_split_vertical_fallback = initial.clone();
    let view_split_vertical_item =
        append_menu_item(&view_menu_box, "Dividir verticalmente", move || {
            dispatch_ui_command(
                &active_controller(&view_split_vertical_manager, &view_split_vertical_fallback),
                UiCommand::ToggleSplitViewVertical,
                None,
            );
        });
    let view_close_split_manager = tab_manager.clone();
    let view_close_split_fallback = initial.clone();
    let view_close_split_item = append_menu_item(&view_menu_box, "Fechar divisao", move || {
        dispatch_ui_command(
            &active_controller(&view_close_split_manager, &view_close_split_fallback),
            UiCommand::CloseSplitView,
            None,
        );
    });
    let view_sync_split_manager = tab_manager.clone();
    let view_sync_split_fallback = initial.clone();
    let view_sync_split_item = append_menu_item(&view_menu_box, "Sincronizar panes", move || {
        dispatch_ui_command(
            &active_controller(&view_sync_split_manager, &view_sync_split_fallback),
            UiCommand::ToggleSplitSync,
            None,
        );
    });

    let menu_tabs_controller = initial.clone();
    let tabs_hint_item = append_menu_item(&tabs_menu_box, "Gerenciar abas", move || {
        dispatch_ui_command(&menu_tabs_controller, UiCommand::ShowTabsHint, None);
    });
    let restore_tab_controller = initial.clone();
    let restore_tab_manager = tab_manager.clone();
    let restore_tab_app = app.clone();
    let restore_tab_window = window.clone();
    let restore_tab_title = title.clone();
    let restore_tab_button = print_button.clone();
    let _restore_tab_item = append_menu_item(&tabs_menu_box, "Restaurar última aba", move || {
        if let Some(path) = restore_tab_manager.borrow().restore_last_closed_path() {
            if open_document_in_new_tab(
                &restore_tab_app,
                &restore_tab_window,
                restore_tab_manager.clone(),
                path,
                restore_tab_title.clone(),
                restore_tab_button.clone(),
                restore_tab_controller.toolbar_host.clone(),
            )
            .is_some()
            {
                restore_tab_controller
                    .status
                    .set_text("Última aba fechada restaurada.");
            }
        } else {
            restore_tab_controller
                .status
                .set_text("Não há aba fechada para restaurar.");
        }
    });

    let menu_favorite_manager = tab_manager.clone();
    let menu_favorite_fallback = initial.clone();
    let bookmarks_toggle_item =
        append_menu_item(&bookmarks_menu_box, "Alternar favorito", move || {
            dispatch_ui_command(
                &active_controller(&menu_favorite_manager, &menu_favorite_fallback),
                UiCommand::ToggleFavoriteDocument,
                None,
            );
        });
    let menu_page_bookmark_manager = tab_manager.clone();
    let menu_page_bookmark_fallback = initial.clone();
    let page_bookmark_toggle_item = append_menu_item(
        &bookmarks_menu_box,
        "Alternar marcador da página",
        move || {
            dispatch_ui_command(
                &active_controller(&menu_page_bookmark_manager, &menu_page_bookmark_fallback),
                UiCommand::ToggleCurrentPageBookmark,
                None,
            );
        },
    );

    let menu_settings_manager = tab_manager.clone();
    let menu_settings_fallback = initial.clone();
    let help_settings_item = append_menu_item(&help_menu_box, "Configurações", move || {
        dispatch_ui_command(
            &active_controller(&menu_settings_manager, &menu_settings_fallback),
            UiCommand::OpenSettingsDialog,
            None,
        );
    });
    let menu_about_manager = tab_manager.clone();
    let menu_about_fallback = initial.clone();
    let help_about_item = append_menu_item(&help_menu_box, "Sobre", move || {
        dispatch_ui_command(
            &active_controller(&menu_about_manager, &menu_about_fallback),
            UiCommand::ShowAbout,
            None,
        );
    });

    // Keep references alive and update sensitivity based on document availability.
    let _always_enabled_items = [
        file_open_item,
        edit_focus_item,
        view_sidebar_item,
        view_fullscreen_item,
        tabs_hint_item,
        help_settings_item,
        help_about_item,
    ];
    let doc_sensitive_items = Rc::new(vec![
        file_print_item,
        file_properties_item,
        edit_next_match_item,
        edit_prev_match_item,
        history_back_item.clone(),
        history_forward_item.clone(),
        view_presentation_item,
        view_split_horizontal_item,
        view_split_vertical_item,
        view_close_split_item,
        view_sync_split_item,
        bookmarks_toggle_item,
        page_bookmark_toggle_item,
    ]);
    let active_toolbar_host = toolbar_host.clone();
    let observed_history_controllers =
        Rc::new(RefCell::new(std::collections::HashSet::<usize>::new()));
    let weak_history_back = history_back_item.downgrade();
    let weak_history_forward = history_forward_item.downgrade();
    let weak_first_page = first_page_item.downgrade();
    let weak_last_page = last_page_item.downgrade();
    tab_manager
        .borrow()
        .subscribe_active_controller(move |controller| {
            if let Some(current_toolbar) = active_toolbar_host.first_child() {
                active_toolbar_host.remove(&current_toolbar);
            }
            active_toolbar_host.append(&controller.toolbar);
            let has_document = controller.state.borrow().path.is_some();
            for item in doc_sensitive_items.iter() {
                item.set_sensitive(has_document);
            }
            let controller_id = Rc::as_ptr(&controller.state) as usize;
            if observed_history_controllers
                .borrow_mut()
                .insert(controller_id)
            {
                let state = Rc::downgrade(&controller.state);
                let back_item = weak_history_back.clone();
                let forward_item = weak_history_forward.clone();
                let first_item = weak_first_page.clone();
                let last_item = weak_last_page.clone();
                controller.subscribe_state_changes(move || {
                    let Some(state) = state.upgrade() else {
                        return;
                    };
                    let state = state.borrow();
                    if let Some(item) = back_item.upgrade() {
                        item.set_sensitive(!state.page_history.is_empty());
                    }
                    if let Some(item) = forward_item.upgrade() {
                        item.set_sensitive(!state.forward_history.is_empty());
                    }
                    if let Some(item) = first_item.upgrade() {
                        item.set_sensitive(state.path.is_some() && state.page_count > 0);
                    }
                    if let Some(item) = last_item.upgrade() {
                        item.set_sensitive(state.path.is_some() && state.page_count > 0);
                    }
                });
            }
            let state = controller.state.borrow();
            if let Some(item) = weak_history_back.upgrade() {
                item.set_sensitive(has_document && !state.page_history.is_empty());
            }
            if let Some(item) = weak_history_forward.upgrade() {
                item.set_sensitive(has_document && !state.forward_history.is_empty());
            }
            let has_pages = has_document && state.page_count > 0;
            if let Some(item) = weak_first_page.upgrade() {
                item.set_sensitive(has_pages);
            }
            if let Some(item) = weak_last_page.upgrade() {
                item.set_sensitive(has_pages);
            }
        });

    let fullscreen_manager = tab_manager.clone();
    let fullscreen_fallback = initial.clone();
    fullscreen_button.connect_clicked(move |_| {
        dispatch_ui_command(
            &active_controller(&fullscreen_manager, &fullscreen_fallback),
            UiCommand::ToggleFullscreen,
            None,
        );
    });
    let properties_manager = tab_manager.clone();
    let properties_fallback = initial.clone();
    properties_button.connect_clicked(move |_| {
        dispatch_ui_command(
            &active_controller(&properties_manager, &properties_fallback),
            UiCommand::ShowDocumentProperties,
            None,
        );
    });
    let presentation_manager = tab_manager.clone();
    let presentation_fallback = initial.clone();
    presentation_button.connect_clicked(move |_| {
        dispatch_ui_command(
            &active_controller(&presentation_manager, &presentation_fallback),
            UiCommand::TogglePresentation,
            None,
        );
    });
    open_button.connect_clicked({
        let open_controller = open_controller.clone();
        let open_tab_manager = open_tab_manager.clone();
        let open_app = open_app.clone();
        let open_window = open_window.clone();
        let last_directory = last_directory.clone();
        move |_| {
            show_open_dialog(
                open_app.clone(),
                open_window.clone(),
                open_tab_manager.clone(),
                open_controller.clone(),
                last_directory.clone(),
                open_controller.toolbar_host.clone(),
            );
        }
    });
    let close_tab_manager = tab_manager.clone();
    let close_controller = initial.clone();
    window.connect_close_request(move |win| {
        let has_open_documents = close_tab_manager.borrow().has_open_documents()
            || close_controller.state.borrow().path.is_some();
        remember_resume_last_document(has_open_documents);
        let is_maximized = win.is_maximized();
        let is_fullscreen = win.is_fullscreen();
        remember_window_state(is_maximized, is_fullscreen);
        if !is_maximized && !is_fullscreen {
            let width = win.width();
            let height = win.height();
            if width > 0 && height > 0 {
                remember_window_geometry(width, height);
            }
        }
        gtk::glib::Propagation::Proceed
    });
    window.set_content(Some(&shell));
    window.present();
    Some(initial)
}

pub(crate) fn create_document_controller(
    params: DocumentControllerParams<'_>,
) -> Rc<ViewerController> {
    let DocumentControllerParams {
        app,
        window,
        renderer,
        printer,
        tab_manager,
        initial_path,
        title,
        print_button,
        toolbar_host,
    } = params;
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    toolbar.set_hexpand(true);
    toolbar.set_margin_top(2);
    toolbar.set_margin_bottom(2);
    toolbar.set_margin_start(8);
    toolbar.set_margin_end(8);

    let previous_button = action_button("go-previous-symbolic", "Previous page");
    previous_button.set_sensitive(false);
    let page_spin = gtk::SpinButton::with_range(1.0, 1.0, 1.0);
    page_spin.set_sensitive(false);
    page_spin.set_width_chars(5);
    let page_total = gtk::Label::new(Some("/ 0"));
    let next_button = action_button("go-next-symbolic", "Next page");
    next_button.set_sensitive(false);
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search in document"));
    search_entry.set_hexpand(true);
    let search_button = action_button("system-search-symbolic", "Search in this document");
    let prev_match = action_button("go-up-symbolic", "Previous match");
    let next_match = action_button("go-down-symbolic", "Next match");
    let zoom_out = action_button("zoom-out-symbolic", "Zoom out");
    let (zoom_button, zoom_menu_box) = create_menu_button("100%");
    zoom_button.set_has_frame(true);
    zoom_button.set_tooltip_text(Some("Choose zoom level"));
    zoom_button.set_size_request(88, -1);
    zoom_button.set_sensitive(false);
    let zoom_in = action_button("zoom-in-symbolic", "Zoom in");
    let fit_width = action_button("zoom-fit-best-symbolic", "Fit width");
    let fit_page = action_button("zoom-fit-best-symbolic", "Fit page");
    let layout_toggle = gtk::Button::with_label("Single");
    layout_toggle.set_tooltip_text(Some("Cycle single-page, continuous, and two-page layout"));
    let rotate_left = action_button("object-rotate-left-symbolic", "Rotate left");
    let rotate_right = action_button("object-rotate-right-symbolic", "Rotate right");

    toolbar.append(&previous_button);
    toolbar.append(&page_spin);
    toolbar.append(&page_total);
    toolbar.append(&next_button);
    toolbar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    toolbar.append(&search_entry);
    toolbar.append(&search_button);
    toolbar.append(&prev_match);
    toolbar.append(&next_match);
    toolbar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    toolbar.append(&zoom_out);
    toolbar.append(&zoom_button);
    toolbar.append(&zoom_in);
    toolbar.append(&fit_width);
    toolbar.append(&fit_page);
    toolbar.append(&layout_toggle);
    toolbar.append(&rotate_left);
    toolbar.append(&rotate_right);
    let drawing_area = gtk::DrawingArea::new();
    drawing_area.set_content_width(640);
    drawing_area.set_content_height(820);
    drawing_area.set_hexpand(true);
    drawing_area.set_vexpand(true);
    let current_surface = Rc::new(RefCell::new(None::<ImageSurface>));
    let draw_surface = Rc::clone(&current_surface);
    let scrolled = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&drawing_area)
        .build();
    root.append(&scrolled);

    let status = gtk::Label::new(Some("Open a PDF document to begin"));
    status.set_xalign(0.0);
    status.set_margin_start(12);
    status.set_margin_end(12);
    status.set_margin_top(6);
    status.set_margin_bottom(6);
    root.append(&status);

    let (search_sender, search_receiver) = std::sync::mpsc::channel();

    let controller = Rc::new(ViewerController {
        app: app.clone(),
        tab_manager: Rc::downgrade(&tab_manager),
        renderer: renderer.clone(),
        printer: printer.clone(),
        state: Rc::new(RefCell::new(ViewerState {
            scale: DEFAULT_ZOOM,
            layout_mode: LayoutMode::SinglePage,
            ..ViewerState::default()
        })),
        cache: Rc::new(RefCell::new(LruCache::new(crate::app::CACHE_LIMIT_BYTES))),
        surface: current_surface,
        area: drawing_area.clone(),
        scrolled,
        previous_button: previous_button.clone(),
        page_spin,
        suppress_spin_events: Rc::new(Cell::new(false)),
        page_total,
        next_button: next_button.clone(),
        zoom_button: zoom_button.clone(),
        search_entry,
        status,
        print_button,
        title,
        toolbar: toolbar.clone(),
        toolbar_host: toolbar_host.clone(),
        window: window.clone(),
        root: root.clone(),
        state_change_hooks: Rc::new(RefCell::new(Vec::new())),
        search_sender,
        search_receiver: Rc::new(RefCell::new(search_receiver)),
        search_generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    });

    for percentage in [25, 50, 75, 100, 125, 150, 200, 300, 400] {
        let controller = Rc::downgrade(&controller);
        append_menu_item(&zoom_menu_box, &format!("{percentage}%"), move || {
            if let Some(controller) = controller.upgrade() {
                controller.set_zoom(f64::from(percentage) / 100.0);
            }
        });
    }

    let draw_controller = Rc::downgrade(&controller);
    drawing_area.set_draw_func(move |_, context, width, height| {
        let Some(draw_controller) = draw_controller.upgrade() else {
            return;
        };
        let has_document = draw_controller.state.borrow().path.is_some();
        if has_document {
            context.set_source_rgb(0.50, 0.50, 0.50);
        } else {
            context.set_source_rgb(0.16, 0.17, 0.18);
        }
        let _ = context.paint();

        let state = draw_controller.state.borrow();
        let split_mode = state.split_mode;
        let layout_mode = state.layout_mode;

        if split_mode == SplitViewMode::Off && layout_mode == LayoutMode::Continuous {
            let Some(path) = state.path.clone() else {
                drop(state);
                context.set_source_rgb(0.78, 0.79, 0.80);
                context.select_font_face(
                    "Sans",
                    gtk4::cairo::FontSlant::Normal,
                    gtk4::cairo::FontWeight::Normal,
                );
                context.set_font_size(16.0);
                context.move_to(24.0, 40.0);
                let _ = context.show_text("Open a PDF to start reading");
                return;
            };
            let page_count = state.page_count.max(0);
            let scale = state.scale;
            let rotation = state.rotation;
            let current_page = state.page_index;
            let highlights = state.search_highlights.clone();
            let page_height_points = state.page_height_points.max(1.0);
            let fallback_surface = draw_surface.borrow().as_ref().cloned();
            let estimated_width = if state.page_width_points > 0.0 {
                (state.page_width_points * scale).max(1.0)
            } else {
                fallback_surface
                    .as_ref()
                    .map(|surface| f64::from(surface.width()))
                    .unwrap_or(612.0)
            };
            let estimated_height = if state.page_height_points > 0.0 {
                (state.page_height_points * scale).max(1.0)
            } else {
                fallback_surface
                    .as_ref()
                    .map(|surface| f64::from(surface.height()))
                    .unwrap_or(792.0)
            };
            let adjustment = draw_controller.scrolled.vadjustment();
            let view_top = adjustment.value();
            let view_bottom = view_top + adjustment.page_size();
            drop(state);

            if page_count > 0 {
                let page_gap = CONTINUOUS_PAGE_GAP;
                let top_margin = CONTINUOUS_TOP_MARGIN;
                let side_margin = CONTINUOUS_SIDE_MARGIN;
                let page_width = estimated_width;
                let page_height = estimated_height;
                let page_stride = page_height + page_gap;
                let x = ((f64::from(width) - page_width) / 2.0).max(side_margin);

                let first_page = ((view_top - top_margin) / page_stride).floor() as i32 - 2;
                let last_page = ((view_bottom - top_margin) / page_stride).ceil() as i32 + 2;
                let start_page = first_page.clamp(0, page_count - 1);
                let end_page = last_page.clamp(0, page_count - 1);

                for page_index in start_page..=end_page {
                    let y = top_margin + f64::from(page_index) * (page_height + page_gap);
                    context.set_source_rgba(0.0, 0.0, 0.0, 0.12);
                    context.rectangle(x + 4.0, y + 6.0, page_width, page_height);
                    let _ = context.fill();

                    context.set_source_rgb(1.0, 1.0, 1.0);
                    context.rectangle(x, y, page_width, page_height);
                    let _ = context.fill_preserve();
                    context.set_source_rgba(0.0, 0.0, 0.0, 0.20);
                    context.set_line_width(1.0);
                    let _ = context.stroke();

                    let key = crate::viewer_state::RenderKey::new(
                        path.clone(),
                        page_index,
                        scale,
                        rotation,
                    );
                    let page_surface = draw_controller
                        .cache
                        .borrow_mut()
                        .get(&key)
                        .cloned()
                        .or_else(|| {
                            if page_index == current_page {
                                fallback_surface.clone()
                            } else {
                                None
                            }
                        });
                    if let Some(surface) = page_surface {
                        let surface_width = f64::from(surface.width());
                        let surface_height = f64::from(surface.height());
                        let draw_x = x + ((page_width - surface_width) / 2.0).max(0.0);
                        let draw_y = y + ((page_height - surface_height) / 2.0).max(0.0);
                        if context.set_source_surface(&surface, draw_x, draw_y).is_ok() {
                            let _ = context.paint();
                        }
                    }

                    if !highlights.is_empty() && page_index == current_page {
                        context.set_source_rgba(1.0, 0.9, 0.1, 0.45);
                        let highlight_rects =
                            cairo_search_highlight_rects(page_height_points, scale, &highlights);
                        for (left, top, right, bottom) in highlight_rects {
                            context.rectangle(x + left, y + top, right - left, bottom - top);
                            let _ = context.fill();
                        }
                    }
                }
            }

            return;
        }

        let mut page_surfaces = Vec::new();
        if let Some(path) = state.path.clone() {
            let pages: Vec<i32> = if split_mode != SplitViewMode::Off {
                state.split_pages()
            } else {
                match layout_mode {
                    LayoutMode::SinglePage => vec![state.page_index],
                    LayoutMode::Continuous => {
                        let start = (state.page_index - 2).max(0);
                        let end = (state.page_index + 2).min(state.page_count.max(1) - 1);
                        (start..=end).collect()
                    }
                    LayoutMode::TwoPage => {
                        let mut pages = vec![state.page_index];
                        if state.page_count > 0 && state.page_index + 1 < state.page_count {
                            pages.push(state.page_index + 1);
                        }
                        if pages.len() == 1 && state.page_index > 0 {
                            pages.insert(0, state.page_index - 1);
                        }
                        pages
                    }
                }
            };
            for page_index in pages {
                let key = crate::viewer_state::RenderKey::new(
                    path.clone(),
                    page_index,
                    state.scale,
                    state.rotation,
                );
                let cached = draw_controller
                    .cache
                    .borrow()
                    .snapshot()
                    .into_iter()
                    .find(|(cached_key, _)| *cached_key == key)
                    .map(|(_, surface)| surface);
                if let Some(surface) = cached {
                    page_surfaces.push((page_index, surface));
                }
            }
        }
        drop(state);

        if page_surfaces.is_empty() {
            if let Some(surface) = draw_surface.borrow().as_ref() {
                page_surfaces.push((0, surface.clone()));
            }
        }

        if !page_surfaces.is_empty() {
            let total_pages = page_surfaces.len();
            let mut offset_y = 16.0;

            for (index, (page_index, surface)) in page_surfaces.into_iter().enumerate() {
                let width_for_surface = f64::from(surface.width());
                let height_for_surface = f64::from(surface.height());
                let x = if split_mode != SplitViewMode::Off && total_pages > 1 {
                    match split_mode {
                        SplitViewMode::Horizontal => {
                            let gutter = 24.0;
                            let total_width = width_for_surface * 2.0 + gutter;
                            let left_margin = (f64::from(width) - total_width) / 2.0;
                            if index == 0 {
                                left_margin
                            } else {
                                left_margin + width_for_surface + gutter
                            }
                        }
                        SplitViewMode::Vertical | SplitViewMode::Off => {
                            (f64::from(width) - width_for_surface) / 2.0
                        }
                    }
                } else {
                    match layout_mode {
                        LayoutMode::TwoPage if total_pages > 1 => {
                            let gutter = 24.0;
                            let total_width = width_for_surface * 2.0 + gutter;
                            let left_margin = (f64::from(width) - total_width) / 2.0;
                            if index == 0 {
                                left_margin
                            } else {
                                left_margin + width_for_surface + gutter
                            }
                        }
                        _ => (f64::from(width) - width_for_surface) / 2.0,
                    }
                };
                let y = if split_mode == SplitViewMode::Vertical && total_pages > 1 {
                    let gutter = 24.0;
                    let total_height = height_for_surface * 2.0 + gutter;
                    let top_margin = (f64::from(height) - total_height) / 2.0;
                    if index == 0 {
                        top_margin
                    } else {
                        top_margin + height_for_surface + gutter
                    }
                } else {
                    match layout_mode {
                        LayoutMode::Continuous => offset_y,
                        _ => (f64::from(height) - height_for_surface) / 2.0,
                    }
                };
                context.set_source_rgb(1.0, 1.0, 1.0);
                context.rectangle(x, y, width_for_surface, height_for_surface);
                let _ = context.fill_preserve();
                context.set_source_rgba(0.0, 0.0, 0.0, 0.20);
                context.set_line_width(1.0);
                let _ = context.stroke();
                if context.set_source_surface(&surface, x, y).is_ok() {
                    let _ = context.paint();
                }
                if layout_mode == LayoutMode::Continuous {
                    offset_y += height_for_surface + 24.0;
                }

                let state = draw_controller.state.borrow();
                if !state.search_highlights.is_empty() && page_index == state.page_index {
                    context.set_source_rgba(1.0, 0.9, 0.1, 0.45);
                    let page_height = state.page_height_points.max(1.0);
                    let highlights = cairo_search_highlight_rects(
                        page_height,
                        state.scale,
                        &state.search_highlights,
                    );
                    for (left, top, right, bottom) in highlights {
                        context.rectangle(x + left, y + top, right - left, bottom - top);
                        let _ = context.fill();
                    }
                }
            }
        } else {
            let has_document = draw_controller.state.borrow().path.is_some();
            context.set_source_rgb(0.78, 0.79, 0.80);
            context.select_font_face(
                "Sans",
                gtk4::cairo::FontSlant::Normal,
                gtk4::cairo::FontWeight::Normal,
            );
            context.set_font_size(16.0);
            context.move_to(24.0, 40.0);
            if has_document {
                let _ = context.show_text("Loading document...");
            } else {
                let _ = context.show_text("Open a PDF to start reading");
            }
        }
    });

    let key_controller = gtk::EventControllerKey::new();
    let input_controller = key_controller.clone();
    controller.area.add_controller(key_controller);

    let scroll_controller =
        gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let scroll_zoom = Rc::downgrade(&controller);
    scroll_controller.connect_scroll(move |scroll, delta_x, delta_y| {
        let _ = delta_x;
        let modifiers = scroll.current_event_state();
        let step = crate::navigation::scroll_zoom_step(delta_y, modifiers);
        if step != 0.0 {
            let Some(scroll_zoom) = scroll_zoom.upgrade() else {
                return gtk::glib::Propagation::Proceed;
            };
            scroll_zoom.change_zoom(step);
            return gtk::glib::Propagation::Stop;
        }

        gtk::glib::Propagation::Proceed
    });
    controller.area.add_controller(scroll_controller);

    let continuous_sync_controller = Rc::downgrade(&controller);
    controller
        .scrolled
        .vadjustment()
        .connect_value_changed(move |adjustment| {
            let Some(continuous_sync_controller) = continuous_sync_controller.upgrade() else {
                return;
            };
            let (layout_mode, page_count, page_height_points, scale, current_page) = {
                let state = continuous_sync_controller.state.borrow();
                (
                    state.layout_mode,
                    state.page_count,
                    state.page_height_points,
                    state.scale,
                    state.page_index,
                )
            };
            if layout_mode != LayoutMode::Continuous || page_count <= 0 || page_height_points <= 0.0
            {
                return;
            }

            let fallback_height_points = continuous_sync_controller
                .surface
                .borrow()
                .as_ref()
                .map(|surface| f64::from(surface.height()) / scale.max(0.1))
                .unwrap_or(page_height_points);
            let stride = continuous_page_stride(fallback_height_points, scale);
            let offset = (adjustment.value() - CONTINUOUS_TOP_MARGIN).max(0.0);
            let target_page = ((offset / stride).floor() as i32).clamp(0, page_count - 1);
            if target_page != current_page {
                continuous_sync_controller.navigate_to(target_page);
            }
        });

    let keyboard_controller = Rc::downgrade(&controller);
    let last_directory = last_directory_for_home_path();
    let open_window = window.clone();
    let open_app = app.clone();
    let open_tab_manager = tab_manager.clone();
    input_controller.connect_key_pressed(move |_, keyval, _, state| {
        let Some(keyboard_controller) = keyboard_controller.upgrade() else {
            return gtk::glib::Propagation::Proceed;
        };
        let key_name = keyval.name().unwrap_or_default();
        let settings = crate::session::load_app_settings();
        let action =
            crate::keyboard::resolve_key_action_with_settings(&key_name, state.bits(), &settings);
        if let Some(KeyAction::OpenFile) = action {
            show_open_dialog(
                open_app.clone(),
                open_window.clone(),
                open_tab_manager.clone(),
                keyboard_controller.clone(),
                last_directory.clone(),
                keyboard_controller.toolbar_host.clone(),
            );
            return gtk::glib::Propagation::Stop;
        }
        if let Some(action) = action {
            keyboard_controller.handle_key_action(action);
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });

    let previous_controller = Rc::downgrade(&controller);
    previous_button.connect_clicked(move |_| {
        let Some(previous_controller) = previous_controller.upgrade() else {
            return;
        };
        let page = previous_controller.state.borrow().page_index - 1;
        previous_controller.navigate_to(page);
    });
    let next_controller = Rc::downgrade(&controller);
    next_button.connect_clicked(move |_| {
        let Some(next_controller) = next_controller.upgrade() else {
            return;
        };
        let page = next_controller.state.borrow().page_index + 1;
        next_controller.navigate_to(page);
    });
    let page_spin = controller.page_spin.clone();
    let spin_controller = Rc::downgrade(&controller);
    page_spin.connect_value_changed(move |spin| {
        let Some(spin_controller) = spin_controller.upgrade() else {
            return;
        };
        if spin_controller.suppress_spin_events.get() || !spin.is_sensitive() {
            return;
        }
        let target_page = spin.value_as_int() - 1;
        if target_page != spin_controller.state.borrow().page_index {
            spin_controller.navigate_to(target_page);
        }
    });
    let zoom_out_controller = Rc::downgrade(&controller);
    zoom_out.connect_clicked(move |_| {
        let Some(zoom_out_controller) = zoom_out_controller.upgrade() else {
            return;
        };
        let step = crate::session::load_app_settings()
            .zoom_step
            .clamp(0.05, 1.0);
        zoom_out_controller.change_zoom(-step);
    });
    let zoom_in_controller = Rc::downgrade(&controller);
    zoom_in.connect_clicked(move |_| {
        let Some(zoom_in_controller) = zoom_in_controller.upgrade() else {
            return;
        };
        let step = crate::session::load_app_settings()
            .zoom_step
            .clamp(0.05, 1.0);
        zoom_in_controller.change_zoom(step);
    });
    let search_controller = Rc::downgrade(&controller);
    search_button.connect_clicked(move |_| {
        if let Some(search_controller) = search_controller.upgrade() {
            search_controller.search_current_page();
        }
    });
    let previous_match_controller = Rc::downgrade(&controller);
    prev_match.connect_clicked(move |_| {
        if let Some(previous_match_controller) = previous_match_controller.upgrade() {
            previous_match_controller.search_previous_match();
        }
    });
    let next_match_controller = Rc::downgrade(&controller);
    next_match.connect_clicked(move |_| {
        if let Some(next_match_controller) = next_match_controller.upgrade() {
            next_match_controller.search_next_match();
        }
    });
    let search_activate_controller = Rc::downgrade(&controller);
    controller.search_entry.connect_activate(move |_| {
        if let Some(search_activate_controller) = search_activate_controller.upgrade() {
            search_activate_controller.search_current_page();
        }
    });
    let fit_controller = Rc::downgrade(&controller);
    fit_width.connect_clicked(move |_| {
        if let Some(fit_controller) = fit_controller.upgrade() {
            fit_controller.fit_width();
        }
    });
    let fit_page_controller = Rc::downgrade(&controller);
    fit_page.connect_clicked(move |_| {
        if let Some(fit_page_controller) = fit_page_controller.upgrade() {
            fit_page_controller.fit_page();
        }
    });
    let layout_controller = Rc::downgrade(&controller);
    layout_toggle.connect_clicked(move |button| {
        let Some(layout_controller) = layout_controller.upgrade() else {
            return;
        };
        let mut state = layout_controller.state.borrow_mut();
        state.layout_mode = state.layout_mode.next();
        button.set_label(match state.layout_mode {
            LayoutMode::SinglePage => "Single",
            LayoutMode::Continuous => "Cont",
            LayoutMode::TwoPage => "Dual",
        });
        drop(state);
        layout_controller.area.queue_draw();
    });
    let rotate_left_controller = Rc::downgrade(&controller);
    rotate_left.connect_clicked(move |_| {
        if let Some(rotate_left_controller) = rotate_left_controller.upgrade() {
            rotate_left_controller.rotate_page(-90);
        }
    });
    let rotate_right_controller = Rc::downgrade(&controller);
    rotate_right.connect_clicked(move |_| {
        if let Some(rotate_right_controller) = rotate_right_controller.upgrade() {
            rotate_right_controller.rotate_page(90);
        }
    });

    let poll_controller = Rc::downgrade(&controller);
    glib::timeout_add_local(Duration::from_millis(16), move || {
        if let Some(poll_controller) = poll_controller.upgrade() {
            poll_controller.poll();
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });

    if let Some(path) = initial_path {
        controller.open_document(path);
    }
    controller
}
