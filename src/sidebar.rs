use gtk4::prelude::*;
use libadwaita as adw;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::app::ViewerController;
use crate::cache::LruCache;
use crate::document_actions::open_document_in_new_tab;
use crate::document_tabs::DocumentTabManager;
use crate::session::{
    is_favorite_document, load_document_bookmarks, load_favorite_documents, load_recent_documents,
    remove_bookmark_page, toggle_favorite_document,
};
use crate::viewer_state::RenderKey;

const THUMBNAIL_MAX_EDGE: i32 = 132;
const THUMBNAIL_CACHE_BYTES: usize = 24 * 1024 * 1024;
const THUMBNAIL_FRAME_WIDTH: i32 = 112;
const THUMBNAIL_FRAME_HEIGHT: i32 = 156;
const MAX_THUMBNAIL_WORKERS: usize = 3;
const THUMBNAIL_WINDOW_SIZE: i32 = 9;

#[derive(Default)]
struct ThumbnailWindowState {
    start: Cell<i32>,
    path: RefCell<Option<PathBuf>>,
    current_page: Cell<i32>,
}

struct ThumbnailWorkerPermit(Arc<AtomicUsize>);

impl Drop for ThumbnailWorkerPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn acquire_thumbnail_worker(
    active_workers: Arc<AtomicUsize>,
    generation: &AtomicU64,
    generation_id: u64,
) -> Option<ThumbnailWorkerPermit> {
    loop {
        if generation.load(Ordering::Relaxed) != generation_id {
            return None;
        }
        let active = active_workers.load(Ordering::Acquire);
        if active < MAX_THUMBNAIL_WORKERS
            && active_workers
                .compare_exchange(active, active + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            return Some(ThumbnailWorkerPermit(active_workers));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

struct ThumbnailPixels {
    width: i32,
    height: i32,
    stride: i32,
    pixels: Vec<u8>,
}

#[derive(Clone)]
struct SidebarOpenContext {
    app: adw::Application,
    window: adw::ApplicationWindow,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    title_label: gtk4::Label,
    status: gtk4::Label,
    print_button: gtk4::Button,
    toolbar_host: gtk4::Box,
    search_host: gtk4::Box,
}

fn page_range_for_sidebar(page_count: i32, current_page: i32) -> Vec<i32> {
    if page_count <= 0 {
        return Vec::new();
    }

    let start = current_page.clamp(0, page_count - 1);
    let end = (start + THUMBNAIL_WINDOW_SIZE).min(page_count);
    (start..end).collect()
}

fn append_sidebar_entry(
    list: &gtk4::ListBox,
    path: PathBuf,
    label_text: &str,
    context: &SidebarOpenContext,
) {
    let row = gtk4::ListBoxRow::new();
    let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    row_box.set_margin_start(8);
    row_box.set_margin_end(8);
    row_box.set_margin_top(4);
    row_box.set_margin_bottom(4);

    let label = gtk4::Label::new(Some(label_text));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    row_box.append(&label);

    let star = gtk4::ToggleButton::with_label("★");
    star.set_active(is_favorite_document(&path));
    let star_path = path.clone();
    star.connect_toggled(move |button| {
        let was_favorite = toggle_favorite_document(&star_path);
        button.set_active(was_favorite);
    });
    row_box.append(&star);
    row.set_child(Some(&row_box));

    let row_path = path.clone();
    let row_context = context.clone();
    row.connect_activate(move |_| {
        if let Err(error) = open_document_in_new_tab(
            &row_context.app,
            &row_context.window,
            row_context.tab_manager.clone(),
            row_path.clone(),
            row_context.title_label.clone(),
            row_context.print_button.clone(),
            row_context.toolbar_host.clone(),
            row_context.search_host.clone(),
        ) {
            row_context
                .status
                .set_text(&format!("Não foi possível reabrir o PDF: {error}"));
        }
    });

    list.append(&row);
}

fn thumbnail_surface_for_page(
    thumbnail_cache: &Rc<RefCell<LruCache<RenderKey, gtk4::cairo::ImageSurface>>>,
    key: &RenderKey,
) -> Option<gtk4::cairo::ImageSurface> {
    thumbnail_cache.borrow_mut().get(key).cloned()
}

fn build_page_thumbnail_button(
    controller: &ViewerController,
    thumbnail_cache: &Rc<RefCell<LruCache<RenderKey, gtk4::cairo::ImageSurface>>>,
    thumbnail_generation: Arc<AtomicU64>,
    active_thumbnail_workers: Arc<AtomicUsize>,
    generation_id: u64,
    page_index: i32,
    is_current: bool,
) -> gtk4::Button {
    let page_button = gtk4::Button::new();
    page_button.set_tooltip_text(Some(&format!("Ir para a página {}", page_index + 1)));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    content.set_margin_start(4);
    content.set_margin_end(4);
    content.set_margin_top(4);
    content.set_margin_bottom(4);

    let (path, rotation) = {
        let state = controller.state.borrow();
        (state.path.clone(), state.rotation)
    };
    let scale = f64::from(THUMBNAIL_MAX_EDGE) / 1000.0;
    let key = path
        .as_ref()
        .map(|path| RenderKey::new(path.clone(), page_index, scale, rotation));
    let thumbnail_state =
        Rc::new(RefCell::new(key.as_ref().and_then(|key| {
            thumbnail_surface_for_page(thumbnail_cache, key)
        })));
    let page_label = gtk4::Label::new(Some(&format!("{}", page_index + 1)));
    page_label.add_css_class("dim-label");
    let preview = gtk4::DrawingArea::new();
    preview.set_content_width(THUMBNAIL_FRAME_WIDTH);
    preview.set_content_height(THUMBNAIL_FRAME_HEIGHT);
    preview.set_size_request(THUMBNAIL_FRAME_WIDTH, THUMBNAIL_FRAME_HEIGHT);
    let thumbnail = thumbnail_state.clone();
    preview.set_draw_func(move |_, context, _, _| {
        let frame_width = f64::from(THUMBNAIL_FRAME_WIDTH);
        let frame_height = f64::from(THUMBNAIL_FRAME_HEIGHT);
        context.set_source_rgb(0.18, 0.19, 0.20);
        context.rectangle(0.0, 0.0, frame_width, frame_height);
        let _ = context.fill();

        let page_margin = 6.0;
        let page_width = frame_width - (page_margin * 2.0);
        let page_height = frame_height - (page_margin * 2.0);
        context.set_source_rgb(1.0, 1.0, 1.0);
        context.rectangle(page_margin, page_margin, page_width, page_height);
        let _ = context.fill_preserve();
        context.set_source_rgba(0.0, 0.0, 0.0, 0.2);
        context.set_line_width(1.0);
        let _ = context.stroke();

        let Some(thumbnail) = thumbnail.borrow().clone() else {
            return;
        };
        let surface_width = f64::from(thumbnail.width());
        let surface_height = f64::from(thumbnail.height());
        let scale = (page_width / surface_width)
            .min(page_height / surface_height)
            .min(1.0);
        let draw_width = surface_width * scale;
        let draw_height = surface_height * scale;
        let draw_x = page_margin + ((page_width - draw_width) / 2.0);
        let draw_y = page_margin + ((page_height - draw_height) / 2.0);

        context.save().ok();
        context.translate(draw_x, draw_y);
        context.scale(scale, scale);
        if context.set_source_surface(&thumbnail, 0.0, 0.0).is_ok() {
            let _ = context.paint();
        }
        context.restore().ok();
    });

    if thumbnail_state.borrow().is_none() {
        if let (Some(path), Some(key)) = (path, key) {
            let preview_weak = preview.downgrade();
            let state_for_result = thumbnail_state.clone();
            let cache_for_result = thumbnail_cache.clone();
            let generation_for_worker = thumbnail_generation.clone();
            let workers_for_worker = active_thumbnail_workers.clone();
            let label_for_result = page_label.clone();
            let (sender, receiver) = mpsc::channel();
            if thread::Builder::new()
                .name("gpdfview-thumbnail".into())
                .spawn(move || {
                    let Some(_worker_permit) = acquire_thumbnail_worker(
                        workers_for_worker,
                        &generation_for_worker,
                        generation_id,
                    ) else {
                        return;
                    };
                    let result = crate::engine::render_page_thumbnail_cancellable(
                        &path,
                        page_index,
                        rotation,
                        THUMBNAIL_MAX_EDGE,
                        || generation_for_worker.load(Ordering::Relaxed) != generation_id,
                    )
                    .and_then(|mut surface| {
                        surface.flush();
                        let width = surface.width();
                        let height = surface.height();
                        let stride = surface.stride();
                        let pixels = surface.data().map_err(|error| error.to_string())?.to_vec();
                        Ok(ThumbnailPixels {
                            width,
                            height,
                            stride,
                            pixels,
                        })
                    });
                    let _ = sender.send(result);
                })
                .is_ok()
            {
                gtk4::glib::timeout_add_local(Duration::from_millis(20), move || {
                    match receiver.try_recv() {
                        Ok(Ok(pixels)) => {
                            let bytes = pixels.pixels.len();
                            if let Ok(surface) = gtk4::cairo::ImageSurface::create_for_data(
                                pixels.pixels,
                                gtk4::cairo::Format::ARgb32,
                                pixels.width,
                                pixels.height,
                                pixels.stride,
                            ) {
                                cache_for_result.borrow_mut().insert(
                                    key.clone(),
                                    surface.clone(),
                                    bytes,
                                );
                                *state_for_result.borrow_mut() = Some(surface);
                                if let Some(preview) = preview_weak.upgrade() {
                                    preview.queue_draw();
                                }
                            } else {
                                label_for_result.set_text("Miniatura indisponível");
                            }
                            gtk4::glib::ControlFlow::Break
                        }
                        Ok(Err(error)) => {
                            label_for_result.set_text("Miniatura indisponível");
                            label_for_result.set_tooltip_text(Some(&error));
                            gtk4::glib::ControlFlow::Break
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            label_for_result.set_text("Miniatura indisponível");
                            gtk4::glib::ControlFlow::Break
                        }
                        Err(mpsc::TryRecvError::Empty) => gtk4::glib::ControlFlow::Continue,
                    }
                });
            } else {
                page_label.set_text("Miniatura indisponível");
            }
        }
    }
    content.append(&preview);
    content.append(&page_label);
    page_button.set_child(Some(&content));

    if is_current {
        page_button.add_css_class("suggested-action");
    }

    let page_to_open = page_index;
    let page_controller = controller.clone();
    page_button.connect_clicked(move |_| {
        page_controller.navigate_to(page_to_open);
    });

    page_button
}

fn populate_pages_flow(
    pages_flow: &gtk4::FlowBox,
    controller: &ViewerController,
    thumbnail_cache: &Rc<RefCell<LruCache<RenderKey, gtk4::cairo::ImageSurface>>>,
    thumbnail_generation: &Arc<AtomicU64>,
    active_thumbnail_workers: &Arc<AtomicUsize>,
    window_state: &Rc<ThumbnailWindowState>,
    previous_window_button: &gtk4::Button,
    next_window_button: &gtk4::Button,
) {
    let generation_id = thumbnail_generation.fetch_add(1, Ordering::Relaxed) + 1;
    while let Some(child) = pages_flow.first_child() {
        pages_flow.remove(&child);
    }

    let state = controller.state.borrow();
    let page_count = state.page_count;
    let current_page = state.page_index;
    let path = state.path.clone();
    drop(state);

    let start = {
        let mut window = window_state.path.borrow_mut();
        let path_changed = *window != path;
        let page_changed = window_state.current_page.replace(current_page) != current_page;
        if path_changed || page_changed {
            *window = path.clone();
            window_state.start.set(
                page_range_for_sidebar(page_count, current_page)
                    .first()
                    .copied()
                    .unwrap_or(0),
            );
        }
        let max_start = (page_count - 1).max(0);
        let start = window_state.start.get().clamp(0, max_start);
        window_state.start.set(start);
        start
    };
    previous_window_button.set_sensitive(start > 0);
    next_window_button.set_sensitive(start + THUMBNAIL_WINDOW_SIZE < page_count);
    let visible_pages =
        (start..(start + THUMBNAIL_WINDOW_SIZE).min(page_count)).collect::<Vec<_>>();
    if visible_pages.is_empty() {
        let message = if path.is_some() {
            "Este PDF não contém páginas disponíveis."
        } else {
            "Abra um PDF para ver as miniaturas."
        };
        let placeholder = gtk4::Label::new(Some(message));
        placeholder.set_xalign(0.0);
        placeholder.add_css_class("dim-label");
        placeholder.set_margin_start(8);
        placeholder.set_margin_end(8);
        placeholder.set_margin_top(4);
        placeholder.set_margin_bottom(4);
        pages_flow.insert(&placeholder, -1);
        return;
    }

    for page_index in visible_pages {
        let page_button = build_page_thumbnail_button(
            controller,
            thumbnail_cache,
            thumbnail_generation.clone(),
            active_thumbnail_workers.clone(),
            generation_id,
            page_index,
            page_index == current_page,
        );
        page_button.set_size_request(THUMBNAIL_FRAME_WIDTH + 28, -1);
        pages_flow.insert(&page_button, -1);
    }
}

fn shift_thumbnail_window(
    direction: i32,
    pages_flow: &gtk4::FlowBox,
    controller: &ViewerController,
    thumbnail_cache: &Rc<RefCell<LruCache<RenderKey, gtk4::cairo::ImageSurface>>>,
    thumbnail_generation: &Arc<AtomicU64>,
    active_thumbnail_workers: &Arc<AtomicUsize>,
    window_state: &Rc<ThumbnailWindowState>,
    previous_window_button: &gtk4::Button,
    next_window_button: &gtk4::Button,
) {
    let page_count = controller.state.borrow().page_count;
    let max_start = (page_count - 1).max(0);
    let start = (window_state.start.get() + direction * THUMBNAIL_WINDOW_SIZE).clamp(0, max_start);
    window_state.start.set(start);
    populate_pages_flow(
        pages_flow,
        controller,
        thumbnail_cache,
        thumbnail_generation,
        active_thumbnail_workers,
        window_state,
        previous_window_button,
        next_window_button,
    );
}

fn populate_outline_list(
    list: &gtk4::ListBox,
    controller: Rc<ViewerController>,
    generation: Arc<AtomicU64>,
) {
    let request_id = generation.fetch_add(1, Ordering::Relaxed) + 1;
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let Some(path) = controller.state.borrow().path.clone() else {
        append_outline_message(list, "Abra um documento para carregar o sumário.");
        return;
    };

    append_outline_message(list, "Carregando a estrutura do documento…");
    let (sender, receiver) = mpsc::channel();
    let path_for_worker = path.clone();
    let generation_for_worker = generation.clone();
    if let Err(error) = thread::Builder::new()
        .name("gpdfview-outline".into())
        .spawn(move || {
            let result = crate::engine::document_outline_cancellable(&path_for_worker, || {
                generation_for_worker.load(Ordering::Relaxed) != request_id
            });
            let _ = sender.send(result);
        })
    {
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        append_outline_message(
            list,
            &format!("Não foi possível iniciar a leitura do sumário: {error}"),
        );
        return;
    }

    let weak_list = list.downgrade();
    gtk4::glib::timeout_add_local(Duration::from_millis(20), move || {
        if generation.load(Ordering::Relaxed) != request_id {
            return gtk4::glib::ControlFlow::Break;
        }
        match receiver.try_recv() {
            Ok(result) => {
                if let Some(list) = weak_list.upgrade() {
                    while let Some(child) = list.first_child() {
                        list.remove(&child);
                    }
                    populate_outline_result(&list, &controller, result);
                }
                gtk4::glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some(list) = weak_list.upgrade() {
                    while let Some(child) = list.first_child() {
                        list.remove(&child);
                    }
                    append_outline_message(
                        &list,
                        "A leitura do sumário foi interrompida antes de terminar.",
                    );
                }
                gtk4::glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => gtk4::glib::ControlFlow::Continue,
        }
    });
}

fn append_outline_message(list: &gtk4::ListBox, message: &str) {
    let row = gtk4::ListBoxRow::new();
    let label = gtk4::Label::new(Some(message));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_margin_start(8);
    label.set_margin_end(8);
    label.set_margin_top(4);
    label.set_margin_bottom(4);
    row.set_child(Some(&label));
    list.append(&row);
}

fn populate_outline_result(
    list: &gtk4::ListBox,
    controller: &ViewerController,
    result: Result<Vec<crate::engine::OutlineEntry>, String>,
) {
    match result {
        Ok(entries) if entries.is_empty() => {
            append_outline_message(list, "Este PDF não tem um sumário navegável.");
        }
        Ok(entries) => {
            let current_page = controller.state.borrow().page_index;
            let manager = controller.tab_manager.clone();
            for entry in entries {
                let row = gtk4::ListBoxRow::new();
                let button = gtk4::Button::new();
                button.set_halign(gtk4::Align::Fill);
                button.set_hexpand(true);
                let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                row_box.set_margin_start(8);
                row_box.set_margin_end(8);
                row_box.set_margin_top(4);
                row_box.set_margin_bottom(4);
                let title = gtk4::Label::new(Some(&entry.title));
                title.set_xalign(0.0);
                title.set_hexpand(true);
                title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                title.set_margin_start((entry.depth.min(8) * 12) as i32);
                row_box.append(&title);
                let page = gtk4::Label::new(Some(&format!("{}", entry.page_index + 1)));
                page.set_xalign(1.0);
                row_box.append(&page);
                button.set_child(Some(&row_box));
                button
                    .set_tooltip_text(Some(&format!("Ir para a página {}", entry.page_index + 1)));
                let page_index = entry.page_index;
                let outline_manager = manager.clone();
                button.connect_clicked(move |_| {
                    if let Some(controller) = outline_manager
                        .upgrade()
                        .and_then(|manager| manager.borrow().active_controller())
                    {
                        controller.navigate_to(page_index);
                    }
                });
                row.set_child(Some(&button));
                list.append(&row);
                if entry.page_index == current_page {
                    list.select_row(Some(&row));
                }
            }
        }
        Err(error) => {
            append_outline_message(
                list,
                &format!("Não foi possível carregar o sumário: {error}"),
            );
        }
    }
}

fn populate_app_bookmarks_list(list: &gtk4::ListBox, controller: &ViewerController) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let Some(path) = controller.state.borrow().path.clone() else {
        let row = gtk4::ListBoxRow::new();
        let label = gtk4::Label::new(Some("Abra um documento para criar marcadores de página."));
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_margin_start(8);
        label.set_margin_end(8);
        label.set_margin_top(4);
        label.set_margin_bottom(4);
        row.set_child(Some(&label));
        list.append(&row);
        return;
    };

    let bookmarks = load_document_bookmarks(&path);
    if bookmarks.is_empty() {
        let row = gtk4::ListBoxRow::new();
        let label = gtk4::Label::new(Some("Nenhum marcador salvo para este documento."));
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_margin_start(8);
        label.set_margin_end(8);
        label.set_margin_top(4);
        label.set_margin_bottom(4);
        row.set_child(Some(&label));
        list.append(&row);
        return;
    }

    let current_page = controller.state.borrow().page_index;
    for bookmark in bookmarks {
        let row = gtk4::ListBoxRow::new();
        let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        let label = gtk4::Label::new(Some(&bookmark.label));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        row_box.append(&label);

        let page_label = gtk4::Label::new(Some(&format!("{}", bookmark.page_index + 1)));
        page_label.set_xalign(1.0);
        row_box.append(&page_label);

        let bookmark_page = bookmark.page_index;
        let bookmark_path = bookmark.path.clone();
        let bookmark_manager = controller.tab_manager.clone();
        row.connect_activate(move |_| {
            if let Some(controller) = bookmark_manager
                .upgrade()
                .and_then(|manager| manager.borrow().active_controller())
            {
                controller.navigate_to(bookmark_page);
            }
        });

        let remove_button = gtk4::Button::from_icon_name("user-trash-symbolic");
        remove_button.set_has_frame(false);
        remove_button.set_tooltip_text(Some("Remover marcador"));
        let remove_path = bookmark_path.clone();
        let remove_page = bookmark_page;
        let remove_manager = controller.tab_manager.clone();
        let weak_list = list.downgrade();
        remove_button.connect_clicked(move |_| {
            if remove_bookmark_page(&remove_path, remove_page) {
                if let (Some(list), Some(controller)) = (
                    weak_list.upgrade(),
                    remove_manager
                        .upgrade()
                        .and_then(|manager| manager.borrow().active_controller()),
                ) {
                    populate_app_bookmarks_list(&list, &controller);
                }
            }
        });
        row_box.append(&remove_button);
        row.set_child(Some(&row_box));
        list.append(&row);
        if bookmark_page == current_page {
            list.select_row(Some(&row));
        }
    }
}

fn populate_search_results_list(list: &gtk4::ListBox, controller: &ViewerController) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let state = controller.state.borrow();
    if state.search_results.is_empty() {
        let row = gtk4::ListBoxRow::new();
        let label = gtk4::Label::new(Some("Sem resultados de busca no momento."));
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_margin_start(8);
        label.set_margin_end(8);
        label.set_margin_top(4);
        label.set_margin_bottom(4);
        row.set_child(Some(&label));
        list.append(&row);
        return;
    }

    let current = state.search_global_index;
    let entries = state.search_results.clone();
    drop(state);

    for (index, entry) in entries.iter().enumerate() {
        let row = gtk4::ListBoxRow::new();
        let button = gtk4::Button::new();
        button.set_halign(gtk4::Align::Fill);
        button.set_hexpand(true);
        let marker = if index == current { "▶ " } else { "" };
        button.set_label(&format!("{}Pag {}", marker, entry.page_index + 1));
        let result_controller = controller.clone();
        button.connect_clicked(move |_| {
            result_controller.focus_global_search_result(index);
        });
        row.set_child(Some(&button));
        list.append(&row);
    }
}

fn populate_properties_result(list: &gtk4::ListBox, result: Result<Vec<(String, String)>, String>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    match result {
        Ok(properties) if properties.is_empty() => {
            append_sidebar_message(list, "Nenhuma propriedade disponível.");
        }
        Ok(properties) => {
            for (name, value) in properties {
                let row = gtk4::ListBoxRow::new();
                let content = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                content.set_margin_start(8);
                content.set_margin_end(8);
                content.set_margin_top(6);
                content.set_margin_bottom(6);
                let name_label = gtk4::Label::new(Some(&name));
                name_label.set_xalign(0.0);
                name_label.add_css_class("dim-label");
                let value_label = gtk4::Label::new(Some(&value));
                value_label.set_xalign(0.0);
                value_label.set_wrap(true);
                value_label.set_selectable(true);
                content.append(&name_label);
                content.append(&value_label);
                row.set_child(Some(&content));
                list.append(&row);
            }
        }
        Err(error) => append_sidebar_message(
            list,
            &format!("Não foi possível ler as propriedades: {error}"),
        ),
    }
}

fn populate_properties_list(
    list: &gtk4::ListBox,
    controller: &ViewerController,
    generation: Arc<AtomicU64>,
) {
    let generation_id = generation.fetch_add(1, Ordering::AcqRel) + 1;
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let path = controller.state.borrow().path.clone();
    let Some(path) = path else {
        append_sidebar_message(list, "Abra um PDF para ver suas propriedades.");
        return;
    };
    append_sidebar_message(list, "Carregando propriedades…");

    let weak_list = list.downgrade();
    let generation_for_worker = generation.clone();
    let (sender, receiver) = mpsc::channel();
    if thread::Builder::new()
        .name("gpdfview-properties".into())
        .spawn(move || {
            if generation_for_worker.load(Ordering::Acquire) != generation_id {
                return;
            }
            let result = crate::engine::document_properties(&path);
            let _ = sender.send(result);
        })
        .is_err()
    {
        populate_properties_result(list, Err("Não foi possível iniciar a leitura.".to_string()));
        return;
    }

    gtk4::glib::timeout_add_local(Duration::from_millis(20), move || {
        if generation.load(Ordering::Acquire) != generation_id || weak_list.upgrade().is_none() {
            return gtk4::glib::ControlFlow::Break;
        }
        match receiver.try_recv() {
            Ok(result) => {
                if let Some(list) = weak_list.upgrade() {
                    populate_properties_result(&list, result);
                }
                gtk4::glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some(list) = weak_list.upgrade() {
                    populate_properties_result(
                        &list,
                        Err("A leitura foi interrompida antes de terminar.".to_string()),
                    );
                }
                gtk4::glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => gtk4::glib::ControlFlow::Continue,
        }
    });
}

fn append_sidebar_message(list: &gtk4::ListBox, message: &str) {
    let row = gtk4::ListBoxRow::new();
    let label = gtk4::Label::new(Some(message));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_margin_start(8);
    label.set_margin_end(8);
    label.set_margin_top(6);
    label.set_margin_bottom(6);
    row.set_child(Some(&label));
    list.append(&row);
}

pub fn build_sidebar(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    title_label: gtk4::Label,
    print_button: gtk4::Button,
    controller: Rc<ViewerController>,
    toolbar_host: gtk4::Box,
    search_host: gtk4::Box,
) -> (gtk4::Box, gtk4::Box) {
    let open_context = SidebarOpenContext {
        app: app.clone(),
        window: window.clone(),
        tab_manager: tab_manager.clone(),
        title_label: title_label.clone(),
        status: controller.status.clone(),
        print_button: print_button.clone(),
        toolbar_host,
        search_host,
    };

    let sidebar_root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    sidebar_root.set_size_request(160, -1);
    sidebar_root.add_css_class("sidebar");

    let panel_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    panel_header.set_margin_start(8);
    panel_header.set_margin_end(8);
    panel_header.set_margin_top(6);
    panel_header.set_margin_bottom(6);
    let panel_title = gtk4::Label::new(Some("Miniaturas"));
    panel_title.set_xalign(0.0);
    panel_title.set_hexpand(true);
    panel_header.append(&panel_title);
    let previous_window_button = gtk4::Button::from_icon_name("go-up-symbolic");
    previous_window_button.set_has_frame(false);
    previous_window_button.set_tooltip_text(Some("Mostrar o bloco anterior de páginas"));
    panel_header.append(&previous_window_button);
    let next_window_button = gtk4::Button::from_icon_name("go-down-symbolic");
    next_window_button.set_has_frame(false);
    next_window_button.set_tooltip_text(Some("Mostrar o próximo bloco de páginas"));
    panel_header.append(&next_window_button);
    let close_button = gtk4::Button::from_icon_name("window-close-symbolic");
    close_button.set_has_frame(false);
    close_button.set_tooltip_text(Some("Ocultar painel"));
    panel_header.append(&close_button);
    sidebar_root.append(&panel_header);
    sidebar_root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    let hide_sidebar = sidebar_root.clone();
    close_button.connect_clicked(move |_| {
        hide_sidebar.set_visible(false);
    });

    let sidebar_scrolled = gtk4::ScrolledWindow::new();
    sidebar_scrolled.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    sidebar_scrolled.set_vexpand(true);
    sidebar_scrolled.set_hexpand(false);

    let sidebar = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    sidebar.set_margin_start(6);
    sidebar.set_margin_end(6);
    sidebar.set_margin_top(6);
    sidebar.set_margin_bottom(6);
    sidebar_scrolled.set_child(Some(&sidebar));
    sidebar_root.append(&sidebar_scrolled);

    let auxiliary_sidebar = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    auxiliary_sidebar.set_size_request(180, -1);
    auxiliary_sidebar.add_css_class("sidebar");
    let auxiliary_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    auxiliary_header.set_margin_start(8);
    auxiliary_header.set_margin_end(8);
    auxiliary_header.set_margin_top(6);
    auxiliary_header.set_margin_bottom(6);
    let auxiliary_title = gtk4::Label::new(Some("Estrutura"));
    auxiliary_title.set_xalign(0.0);
    auxiliary_title.set_hexpand(true);
    auxiliary_header.append(&auxiliary_title);
    let auxiliary_close = gtk4::Button::from_icon_name("window-close-symbolic");
    auxiliary_close.set_has_frame(false);
    auxiliary_close.set_tooltip_text(Some("Ocultar painel auxiliar"));
    auxiliary_header.append(&auxiliary_close);
    auxiliary_sidebar.append(&auxiliary_header);
    auxiliary_sidebar.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    let hide_auxiliary_sidebar = auxiliary_sidebar.clone();
    auxiliary_close.connect_clicked(move |_| hide_auxiliary_sidebar.set_visible(false));
    let auxiliary_scrolled = gtk4::ScrolledWindow::new();
    auxiliary_scrolled.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    auxiliary_scrolled.set_vexpand(true);
    let auxiliary_stack = gtk4::Stack::new();
    auxiliary_stack.set_hexpand(true);
    auxiliary_stack.set_vexpand(true);
    let auxiliary_title_for_stack = auxiliary_title.clone();
    auxiliary_stack.connect_visible_child_notify(move |stack| {
        let title = match stack.visible_child_name().as_deref() {
            Some("outline") => "Estrutura",
            Some("search") => "Busca",
            Some("recent") => "Recentes",
            Some("favorites") => "Favoritos",
            Some("bookmarks") => "Marcadores",
            Some("properties") => "Propriedades",
            _ => "Painel",
        };
        auxiliary_title_for_stack.set_text(title);
    });
    auxiliary_scrolled.set_child(Some(&auxiliary_stack));
    auxiliary_sidebar.append(&auxiliary_scrolled);

    let pages_flow = gtk4::FlowBox::new();
    pages_flow.set_min_children_per_line(1);
    pages_flow.set_max_children_per_line(1);
    pages_flow.set_selection_mode(gtk4::SelectionMode::None);
    pages_flow.set_homogeneous(false);
    pages_flow.set_row_spacing(8);
    pages_flow.set_column_spacing(0);
    let thumbnail_cache = Rc::new(RefCell::new(LruCache::new(THUMBNAIL_CACHE_BYTES)));
    let thumbnail_generation = Arc::new(AtomicU64::new(0));
    let active_thumbnail_workers = Arc::new(AtomicUsize::new(0));
    let thumbnail_window_state = Rc::new(ThumbnailWindowState::default());
    populate_pages_flow(
        &pages_flow,
        &controller,
        &thumbnail_cache,
        &thumbnail_generation,
        &active_thumbnail_workers,
        &thumbnail_window_state,
        &previous_window_button,
        &next_window_button,
    );
    for (button, direction) in [(&previous_window_button, -1), (&next_window_button, 1)] {
        let pages_flow = pages_flow.clone();
        let thumbnail_cache = thumbnail_cache.clone();
        let thumbnail_generation = thumbnail_generation.clone();
        let active_thumbnail_workers = active_thumbnail_workers.clone();
        let thumbnail_window_state = thumbnail_window_state.clone();
        let previous_window_button = previous_window_button.clone();
        let next_window_button = next_window_button.clone();
        let manager = Rc::downgrade(&tab_manager);
        button.connect_clicked(move |_| {
            let Some(controller) = manager
                .upgrade()
                .and_then(|manager| manager.borrow().active_controller())
            else {
                return;
            };
            shift_thumbnail_window(
                direction,
                &pages_flow,
                &controller,
                &thumbnail_cache,
                &thumbnail_generation,
                &active_thumbnail_workers,
                &thumbnail_window_state,
                &previous_window_button,
                &next_window_button,
            );
        });
    }
    let thumbnails_section = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    thumbnails_section.append(&pages_flow);
    sidebar.append(&thumbnails_section);

    let outline_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    outline_header.set_halign(gtk4::Align::Fill);
    outline_header.set_hexpand(true);
    let outline_label = gtk4::Label::new(Some("Sumário do PDF"));
    outline_label.set_halign(gtk4::Align::Start);
    outline_label.set_hexpand(true);
    outline_label.set_markup("<b>Sumário do PDF</b>");
    outline_header.append(&outline_label);
    let refresh_outline = gtk4::Button::with_label("Atualizar");
    outline_header.append(&refresh_outline);

    let outline_list = gtk4::ListBox::new();
    outline_list.set_selection_mode(gtk4::SelectionMode::Single);
    outline_list.set_css_classes(&["boxed-list"]);
    let outline_generation = Arc::new(AtomicU64::new(0));
    populate_outline_list(
        &outline_list,
        controller.clone(),
        outline_generation.clone(),
    );
    let outline_manager = Rc::downgrade(&tab_manager);
    let outline_list_for_refresh = outline_list.clone();
    let outline_generation_for_refresh = outline_generation.clone();
    refresh_outline.connect_clicked(move |_| {
        if let Some(manager) = outline_manager.upgrade() {
            if let Some(controller) = manager.borrow().active_controller() {
                populate_outline_list(
                    &outline_list_for_refresh,
                    controller,
                    outline_generation_for_refresh.clone(),
                );
            }
        }
    });
    let outline_section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    outline_section.append(&outline_header);
    outline_section.append(&outline_list);
    auxiliary_stack.add_titled(&outline_section, Some("outline"), "Estrutura");

    let search_list = gtk4::ListBox::new();
    search_list.set_selection_mode(gtk4::SelectionMode::None);
    search_list.set_css_classes(&["boxed-list"]);
    populate_search_results_list(&search_list, &controller);
    let search_section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let search_title = gtk4::Label::new(Some("<b>Busca</b>"));
    search_title.set_use_markup(true);
    search_title.set_xalign(0.0);
    search_section.append(&search_title);
    search_section.append(&search_list);
    auxiliary_stack.add_titled(&search_section, Some("search"), "Busca");

    let recent_list = gtk4::ListBox::new();
    recent_list.set_selection_mode(gtk4::SelectionMode::Single);
    recent_list.set_css_classes(&["boxed-list"]);
    for path in load_recent_documents() {
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Documento")
            .to_string();
        append_sidebar_entry(&recent_list, path, &label, &open_context);
    }
    let recent_section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let recent_title = gtk4::Label::new(Some("<b>Recentes</b>"));
    recent_title.set_use_markup(true);
    recent_title.set_xalign(0.0);
    recent_section.append(&recent_title);
    recent_section.append(&recent_list);
    auxiliary_stack.add_titled(&recent_section, Some("recent"), "Recentes");

    let favorites_list = gtk4::ListBox::new();
    favorites_list.set_selection_mode(gtk4::SelectionMode::Single);
    favorites_list.set_css_classes(&["boxed-list"]);
    for path in load_favorite_documents() {
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Documento")
            .to_string();
        append_sidebar_entry(&favorites_list, path, &label, &open_context);
    }
    let favorites_section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let favorites_title = gtk4::Label::new(Some("<b>Favoritos</b>"));
    favorites_title.set_use_markup(true);
    favorites_title.set_xalign(0.0);
    favorites_section.append(&favorites_title);
    favorites_section.append(&favorites_list);
    auxiliary_stack.add_titled(&favorites_section, Some("favorites"), "Favoritos");

    let app_bookmarks_list = gtk4::ListBox::new();
    app_bookmarks_list.set_selection_mode(gtk4::SelectionMode::Single);
    app_bookmarks_list.set_css_classes(&["boxed-list"]);
    populate_app_bookmarks_list(&app_bookmarks_list, &controller);
    let app_bookmarks_section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let app_bookmarks_title = gtk4::Label::new(Some("<b>Marcadores</b>"));
    app_bookmarks_title.set_use_markup(true);
    app_bookmarks_title.set_xalign(0.0);
    app_bookmarks_section.append(&app_bookmarks_title);
    app_bookmarks_section.append(&app_bookmarks_list);
    auxiliary_stack.add_titled(&app_bookmarks_section, Some("bookmarks"), "Marcadores");

    let properties_list = gtk4::ListBox::new();
    properties_list.set_selection_mode(gtk4::SelectionMode::None);
    properties_list.set_css_classes(&["boxed-list"]);
    let properties_generation = Arc::new(AtomicU64::new(0));
    populate_properties_list(&properties_list, &controller, properties_generation.clone());
    let properties_section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let properties_title = gtk4::Label::new(Some("<b>Propriedades</b>"));
    properties_title.set_use_markup(true);
    properties_title.set_xalign(0.0);
    properties_section.append(&properties_title);
    properties_section.append(&properties_list);
    auxiliary_stack.add_titled(&properties_section, Some("properties"), "Propriedades");
    let auxiliary_switcher = gtk4::StackSwitcher::new();
    auxiliary_switcher.set_stack(Some(&auxiliary_stack));
    auxiliary_switcher.set_halign(gtk4::Align::Center);
    auxiliary_sidebar.append(&auxiliary_switcher);

    let active_pages = pages_flow.clone();
    let active_outline = outline_list.clone();
    let active_search = search_list.clone();
    let active_bookmarks = app_bookmarks_list.clone();
    let active_properties = properties_list.clone();
    let active_properties_generation = properties_generation.clone();
    let active_thumbnail_cache = thumbnail_cache.clone();
    let active_thumbnail_generation = thumbnail_generation.clone();
    let active_thumbnail_workers = active_thumbnail_workers.clone();
    let active_thumbnail_window_state = thumbnail_window_state.clone();
    let active_previous_window_button = previous_window_button.clone();
    let active_next_window_button = next_window_button.clone();
    let observed_controllers = Rc::new(RefCell::new(std::collections::HashSet::new()));
    let weak_tab_manager = Rc::downgrade(&tab_manager);
    tab_manager
        .borrow()
        .subscribe_active_controller(move |active_controller| {
            populate_pages_flow(
                &active_pages,
                &active_controller,
                &active_thumbnail_cache,
                &active_thumbnail_generation,
                &active_thumbnail_workers,
                &active_thumbnail_window_state,
                &active_previous_window_button,
                &active_next_window_button,
            );
            populate_outline_list(
                &active_outline,
                active_controller.clone(),
                outline_generation.clone(),
            );
            populate_search_results_list(&active_search, &active_controller);
            populate_app_bookmarks_list(&active_bookmarks, &active_controller);
            populate_properties_list(
                &active_properties,
                &active_controller,
                active_properties_generation.clone(),
            );

            let controller_id = Rc::as_ptr(&active_controller.state) as usize;
            if !observed_controllers.borrow_mut().insert(controller_id) {
                return;
            }
            let watched_state = active_controller.state.clone();
            let pages = active_pages.clone();
            let outline = active_outline.clone();
            let search = active_search.clone();
            let bookmarks = active_bookmarks.clone();
            let properties = active_properties.clone();
            let properties_generation = active_properties_generation.clone();
            let outline_generation = outline_generation.clone();
            let cache = active_thumbnail_cache.clone();
            let thumbnail_generation = active_thumbnail_generation.clone();
            let thumbnail_workers = active_thumbnail_workers.clone();
            let thumbnail_window_state = active_thumbnail_window_state.clone();
            let previous_window_button = active_previous_window_button.clone();
            let next_window_button = active_next_window_button.clone();
            let manager = weak_tab_manager.clone();
            let properties_path =
                Rc::new(RefCell::new(active_controller.state.borrow().path.clone()));
            let observed_properties_path = properties_path.clone();
            active_controller.subscribe_state_changes(move || {
                let Some(manager) = manager.upgrade() else {
                    return;
                };
                let is_active = manager
                    .borrow()
                    .active_controller()
                    .is_some_and(|current| Rc::ptr_eq(&current.state, &watched_state));
                if !is_active {
                    return;
                }
                let Some(controller) = manager.borrow().active_controller() else {
                    return;
                };
                populate_pages_flow(
                    &pages,
                    &controller,
                    &cache,
                    &thumbnail_generation,
                    &thumbnail_workers,
                    &thumbnail_window_state,
                    &previous_window_button,
                    &next_window_button,
                );
                populate_outline_list(&outline, controller.clone(), outline_generation.clone());
                populate_search_results_list(&search, &controller);
                populate_app_bookmarks_list(&bookmarks, &controller);
                let current_path = controller.state.borrow().path.clone();
                let mut previous_path = observed_properties_path.borrow_mut();
                if *previous_path != current_path {
                    *previous_path = current_path;
                    populate_properties_list(
                        &properties,
                        &controller,
                        properties_generation.clone(),
                    );
                }
            });
        });

    (sidebar_root, auxiliary_sidebar)
}

#[cfg(test)]
mod tests {
    use super::page_range_for_sidebar;

    #[test]
    fn page_range_for_sidebar_starts_with_the_requested_page() {
        assert_eq!(page_range_for_sidebar(12, 5), vec![5, 6, 7, 8, 9, 10, 11]);
        assert_eq!(page_range_for_sidebar(5, 0), vec![0, 1, 2, 3, 4]);
        assert_eq!(page_range_for_sidebar(3, 2), vec![2]);
    }
}
