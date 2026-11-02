use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4 as gtk;
use gtk4::cairo::ImageSurface;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

use crate::app::ViewerController;
use crate::cache::LruCache;
use crate::document_actions::show_open_dialog;
use crate::document_tabs::DocumentTabManager;
use crate::keyboard::{resolve_key_action, KeyAction};
use crate::layout::cairo_search_highlight_rects;
use crate::printing::PrintService;
use crate::session::{
    last_directory_for_home_path, last_window_geometry_for_home_path, load_recent_documents,
    newest_valid_recent_document, remember_window_geometry,
};
use crate::sidebar::build_sidebar;
use crate::ui_helpers::action_button;
use crate::viewer_state::{LayoutMode, ViewerState, DEFAULT_ZOOM};

pub(crate) struct DocumentControllerParams<'a> {
    pub app: adw::Application,
    pub window: &'a adw::ApplicationWindow,
    pub renderer: Rc<crate::engine::PdfRenderer>,
    pub printer: Rc<PrintService>,
    pub tab_manager: Rc<RefCell<DocumentTabManager>>,
    pub initial_path: Option<PathBuf>,
    pub title: gtk::Label,
    pub print_button: gtk::Button,
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

    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let header = gtk::HeaderBar::new();
    let title = gtk::Label::new(Some("gpdfview"));
    header.set_title_widget(Some(&title));

    let sidebar_button = action_button("sidebar-show-symbolic", "Toggle document sidebar");
    header.pack_start(&sidebar_button);
    let open_button = action_button("document-open-symbolic", "Open a PDF document");
    header.pack_start(&open_button);
    let fullscreen_button = action_button("view-fullscreen-symbolic", "Toggle fullscreen");
    header.pack_end(&fullscreen_button);
    let presentation_button = action_button("view-presentation-symbolic", "Toggle presentation mode");
    header.pack_end(&presentation_button);
    let properties_button = action_button("document-properties-symbolic", "Document properties");
    header.pack_end(&properties_button);
    let print_button = action_button("printer-symbolic", "Send the current PDF to the default printer");
    print_button.set_sensitive(false);
    header.pack_end(&print_button);

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
    });
    if let Some(path) = newest_valid_recent_document(&load_recent_documents()) {
        initial.open_document(path);
    }
    let sidebar = build_sidebar(
        app,
        &window,
        tab_manager.clone(),
        title.clone(),
        print_button.clone(),
        initial.clone(),
    );
    sidebar.set_visible(false);

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.append(&sidebar);
    content.append(&notebook);
    shell.append(&header);
    shell.append(&content);
    let open_controller = initial.clone();
    let open_window = window.clone();
    let open_tab_manager = tab_manager.clone();
    let open_app = app.clone();
    let sidebar_toggle = sidebar.clone();
    sidebar_button.connect_clicked(move |_| {
        let visible = sidebar_toggle.is_visible();
        sidebar_toggle.set_visible(!visible);
    });
    let fullscreen_window = open_window.clone();
    fullscreen_button.connect_clicked(move |_| {
        if fullscreen_window.is_fullscreen() {
            fullscreen_window.unfullscreen();
        } else {
            fullscreen_window.fullscreen();
        }
    });
    let properties_window = open_window.clone();
    let properties_controller = initial.clone();
    properties_button.connect_clicked(move |_| {
        properties_controller.show_document_properties();
        if properties_window.is_fullscreen() {
            properties_window.fullscreen();
        }
    });
    let presentation_window = open_window.clone();
    let presentation_controller = initial.clone();
    presentation_button.connect_clicked(move |_| {
        presentation_controller.toggle_presentation_mode();
        if presentation_window.is_fullscreen() {
            presentation_window.fullscreen();
        }
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
            );
        }
    });
    window.set_content(Some(&shell));
    window.present();
    Some(initial)
}

pub(crate) fn create_document_controller(params: DocumentControllerParams<'_>) -> Rc<ViewerController> {
    let DocumentControllerParams {
        app,
        window,
        renderer,
        printer,
        tab_manager,
        initial_path,
        title,
        print_button,
    } = params;
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    toolbar.set_margin_top(6);
    toolbar.set_margin_bottom(6);
    toolbar.set_margin_start(12);
    toolbar.set_margin_end(12);

    let previous_button = action_button("go-previous-symbolic", "Previous page");
    let page_spin = gtk::SpinButton::with_range(1.0, 1.0, 1.0);
    page_spin.set_sensitive(false);
    page_spin.set_width_chars(5);
    let page_total = gtk::Label::new(Some("/ 0"));
    let next_button = action_button("go-next-symbolic", "Next page");
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search in page"));
    let search_button = action_button("system-search-symbolic", "Search in this page");
    let prev_match = action_button("go-up-symbolic", "Previous match");
    let next_match = action_button("go-down-symbolic", "Next match");
    let zoom_out = action_button("zoom-out-symbolic", "Zoom out");
    let zoom_label = gtk::Label::new(Some("100%"));
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
    toolbar.append(&zoom_label);
    toolbar.append(&zoom_in);
    toolbar.append(&fit_width);
    toolbar.append(&fit_page);
    toolbar.append(&layout_toggle);
    toolbar.append(&rotate_left);
    toolbar.append(&rotate_right);
    root.append(&toolbar);

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

    let controller = Rc::new(ViewerController {
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
        page_spin,
        suppress_spin_events: Rc::new(Cell::new(false)),
        page_total,
        zoom_label,
        search_entry,
        status,
        print_button,
        title,
        toolbar: toolbar.clone(),
        window: window.clone(),
        root: root.clone(),
    });

    let draw_controller = controller.clone();
    drawing_area.set_draw_func(move |_, context, width, height| {
        context.set_source_rgb(0.18, 0.19, 0.21);
        let _ = context.paint();

        let state = draw_controller.state.borrow();
        let mut page_surfaces = Vec::new();
        if let Some(path) = state.path.clone() {
            let pages: Vec<i32> = match state.layout_mode {
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
            };
            for page_index in pages {
                let key = crate::viewer_state::RenderKey::new(path.clone(), page_index, state.scale, state.rotation);
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
            let initial_state = draw_controller.state.borrow();
            let layout_mode = initial_state.layout_mode;
            drop(initial_state);

            for (index, (page_index, surface)) in page_surfaces.into_iter().enumerate() {
                let width_for_surface = f64::from(surface.width());
                let height_for_surface = f64::from(surface.height());
                let x = match layout_mode {
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
                };
                let y = match layout_mode {
                    LayoutMode::Continuous => offset_y,
                    _ => (f64::from(height) - height_for_surface) / 2.0,
                };
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
                    let highlights = cairo_search_highlight_rects(page_height, state.scale, &state.search_highlights);
                    for (left, top, right, bottom) in highlights {
                        context.rectangle(x + left, y + top, right - left, bottom - top);
                            let _ = context.fill();
                    }
                }
            }
        } else {
            context.set_source_rgb(0.82, 0.83, 0.85);
            context.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Normal);
            context.set_font_size(16.0);
            context.move_to(24.0, 40.0);
            let _ = context.show_text("Open a PDF to start reading");
        }
    });

    let key_controller = gtk::EventControllerKey::new();
    let input_controller = key_controller.clone();
    controller.area.add_controller(key_controller);

    let scroll_controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let scroll_zoom = controller.clone();
    scroll_controller.connect_scroll(move |_, delta_x, delta_y| {
        let _ = delta_x;
        let step = crate::navigation::scroll_zoom_step(delta_y, gtk::gdk::ModifierType::CONTROL_MASK);
        if step == 0.0 {
            return gtk::glib::Propagation::Proceed;
        }
        scroll_zoom.change_zoom(step);
        gtk::glib::Propagation::Stop
    });
    controller.area.add_controller(scroll_controller);

    let keyboard_controller = controller.clone();
    let last_directory = last_directory_for_home_path();
    let open_window = window.clone();
    let open_app = app.clone();
    let open_tab_manager = tab_manager.clone();
    input_controller.connect_key_pressed(move |_, keyval, _, state| {
        let key_name = keyval.name().unwrap_or_default();
        let action = resolve_key_action(&key_name, state.bits());
        if let Some(KeyAction::OpenFile) = action {
            show_open_dialog(
                open_app.clone(),
                open_window.clone(),
                open_tab_manager.clone(),
                keyboard_controller.clone(),
                last_directory.clone(),
            );
            return gtk::glib::Propagation::Stop;
        }
        if let Some(action) = action {
            keyboard_controller.handle_key_action(action);
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });

    let print_controller = controller.clone();
    let print_button = controller.print_button.clone();
    print_button.connect_clicked(move |_| {
        print_controller.submit_print();
    });

    let previous_controller = controller.clone();
    previous_button.connect_clicked(move |_| {
        let page = previous_controller.state.borrow().page_index - 1;
        previous_controller.navigate_to(page);
    });
    let next_controller = controller.clone();
    next_button.connect_clicked(move |_| {
        let page = next_controller.state.borrow().page_index + 1;
        next_controller.navigate_to(page);
    });
    let page_spin = controller.page_spin.clone();
    let spin_controller = controller.clone();
    page_spin.connect_value_changed(move |spin| {
        if spin_controller.suppress_spin_events.get() || !spin.is_sensitive() {
            return;
        }
        let target_page = spin.value_as_int() - 1;
        if target_page != spin_controller.state.borrow().page_index {
            spin_controller.navigate_to(target_page);
        }
    });
    let zoom_out_controller = controller.clone();
    zoom_out.connect_clicked(move |_| zoom_out_controller.change_zoom(-0.1));
    let zoom_in_controller = controller.clone();
    zoom_in.connect_clicked(move |_| zoom_in_controller.change_zoom(0.1));
    let search_controller = controller.clone();
    search_button.connect_clicked(move |_| search_controller.search_current_page());
    let previous_match_controller = controller.clone();
    prev_match.connect_clicked(move |_| previous_match_controller.search_previous_match());
    let next_match_controller = controller.clone();
    next_match.connect_clicked(move |_| next_match_controller.search_next_match());
    let search_activate_controller = controller.clone();
    controller.search_entry.connect_activate(move |_| {
        search_activate_controller.search_current_page();
    });
    let fit_controller = controller.clone();
    fit_width.connect_clicked(move |_| fit_controller.fit_width());
    let fit_page_controller = controller.clone();
    fit_page.connect_clicked(move |_| fit_page_controller.fit_page());
    let layout_controller = controller.clone();
    layout_toggle.connect_clicked(move |button| {
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
    let rotate_left_controller = controller.clone();
    rotate_left.connect_clicked(move |_| rotate_left_controller.rotate_page(-90));
    let rotate_right_controller = controller.clone();
    rotate_right.connect_clicked(move |_| rotate_right_controller.rotate_page(90));

    let poll_controller = controller.clone();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        poll_controller.poll();
        glib::ControlFlow::Continue
    });

    if let Some(path) = initial_path {
        controller.open_document(path);
    }
    controller
}
