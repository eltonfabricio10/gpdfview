use gtk4::prelude::*;
use libadwaita as adw;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

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
const THUMBNAIL_FRAME_WIDTH: i32 = 96;
const THUMBNAIL_FRAME_HEIGHT: i32 = 136;

#[derive(Clone)]
struct SidebarOpenContext {
    app: adw::Application,
    window: adw::ApplicationWindow,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    title_label: gtk4::Label,
    print_button: gtk4::Button,
}

fn page_range_for_sidebar(page_count: i32, current_page: i32) -> Vec<i32> {
    if page_count <= 0 {
        return Vec::new();
    }

    if page_count <= 9 {
        return (0..page_count).collect();
    }

    let start = (current_page - 4).max(0);
    let end = (current_page + 4).min(page_count - 1);
    (start..=end).collect()
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
        let _ = open_document_in_new_tab(
            &row_context.app,
            &row_context.window,
            row_context.tab_manager.clone(),
            row_path.clone(),
            row_context.title_label.clone(),
            row_context.print_button.clone(),
        );
    });

    list.append(&row);
}

fn thumbnail_surface_for_page(
    controller: &ViewerController,
    thumbnail_cache: &Rc<RefCell<LruCache<RenderKey, gtk4::cairo::ImageSurface>>>,
    page_index: i32,
) -> Option<gtk4::cairo::ImageSurface> {
    let state = controller.state.borrow();
    let path = state.path.clone()?;
    let scale = f64::from(THUMBNAIL_MAX_EDGE) / 1000.0;
    let rotation = state.rotation;
    let key = RenderKey::new(path.clone(), page_index, scale, rotation);
    drop(state);

    if let Some(cached) = thumbnail_cache.borrow_mut().get(&key).cloned() {
        return Some(cached);
    }

    let rendered = crate::engine::render_page_thumbnail(&path, page_index, rotation, THUMBNAIL_MAX_EDGE).ok()?;
    let bytes = rendered
        .stride()
        .max(0)
        .saturating_mul(rendered.height().max(0)) as usize;
    thumbnail_cache
        .borrow_mut()
        .insert(key, rendered.clone(), bytes);
    Some(rendered)
}

fn build_page_thumbnail_button(
    controller: &ViewerController,
    thumbnail_cache: &Rc<RefCell<LruCache<RenderKey, gtk4::cairo::ImageSurface>>>,
    page_index: i32,
    is_current: bool,
) -> gtk4::Button {
    let page_button = gtk4::Button::new();
    page_button.set_tooltip_text(Some(&format!("Go to page {}", page_index + 1)));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    content.set_margin_start(4);
    content.set_margin_end(4);
    content.set_margin_top(4);
    content.set_margin_bottom(4);

    if let Some(surface) = thumbnail_surface_for_page(controller, thumbnail_cache, page_index) {
        let preview = gtk4::DrawingArea::new();
        preview.set_content_width(THUMBNAIL_FRAME_WIDTH);
        preview.set_content_height(THUMBNAIL_FRAME_HEIGHT);
        preview.set_size_request(THUMBNAIL_FRAME_WIDTH, THUMBNAIL_FRAME_HEIGHT);
        let thumbnail = surface.clone();
        preview.set_draw_func(move |_, context, _, _| {
            let frame_width = f64::from(THUMBNAIL_FRAME_WIDTH);
            let frame_height = f64::from(THUMBNAIL_FRAME_HEIGHT);
            context.set_source_rgb(0.95, 0.95, 0.96);
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
        content.append(&preview);
    }

    let label = gtk4::Label::new(Some(&format!("{}", page_index + 1)));
    content.append(&label);
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
) {
    while let Some(child) = pages_flow.first_child() {
        pages_flow.remove(&child);
    }

    let state = controller.state.borrow();
    let visible_pages = page_range_for_sidebar(state.page_count, state.page_index);
    if visible_pages.is_empty() {
        let placeholder = gtk4::Label::new(Some("Loading page thumbnails…"));
        placeholder.set_xalign(0.0);
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
            page_index,
            page_index == state.page_index,
        );
        page_button.set_size_request(THUMBNAIL_FRAME_WIDTH + 28, -1);
        pages_flow.insert(&page_button, -1);
    }
}

fn section_toggle(label: &str, active: bool) -> gtk4::ToggleButton {
    let button = gtk4::ToggleButton::with_label(label);
    button.set_active(active);
    button.set_has_frame(false);
    button
}

fn populate_outline_list(list: &gtk4::ListBox, controller: &ViewerController) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let Some(path) = controller.state.borrow().path.clone() else {
        let row = gtk4::ListBoxRow::new();
        let label = gtk4::Label::new(Some("Open a document to load bookmarks."));
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

    match crate::engine::document_outline(&path) {
        Ok(entries) if entries.is_empty() => {
            let row = gtk4::ListBoxRow::new();
            let label = gtk4::Label::new(Some("This document has no outline/bookmarks."));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.set_margin_start(8);
            label.set_margin_end(8);
            label.set_margin_top(4);
            label.set_margin_bottom(4);
            row.set_child(Some(&label));
            list.append(&row);
        }
        Ok(entries) => {
            for entry in entries {
                let row = gtk4::ListBoxRow::new();
                let button = gtk4::Button::new();
                button.set_halign(gtk4::Align::Fill);
                button.set_hexpand(true);
                let indent = "  ".repeat(entry.depth as usize);
                button.set_label(&format!("{}{}", indent, entry.title));
                button.set_tooltip_text(Some(&format!("Go to page {}", entry.page_index + 1)));
                let page_index = entry.page_index;
                let outline_controller = controller.clone();
                button.connect_clicked(move |_| {
                    outline_controller.navigate_to(page_index);
                });
                row.set_child(Some(&button));
                list.append(&row);
            }
        }
        Err(error) => {
            let row = gtk4::ListBoxRow::new();
            let label = gtk4::Label::new(Some(&format!("Could not load outline: {error}")));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.set_margin_start(8);
            label.set_margin_end(8);
            label.set_margin_top(4);
            label.set_margin_bottom(4);
            row.set_child(Some(&label));
            list.append(&row);
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
        let label = gtk4::Label::new(Some("Nenhum marcador desta página."));
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
        row_box.append(&label);

        let page_label = gtk4::Label::new(Some(&format!("P{}", bookmark.page_index + 1)));
        page_label.set_xalign(1.0);
        row_box.append(&page_label);

        let bookmark_page = bookmark.page_index;
        let bookmark_path = bookmark.path.clone();
        let bookmark_controller = controller.clone();
        row.connect_activate(move |_| {
            bookmark_controller.navigate_to(bookmark_page);
        });

        let remove_button = gtk4::Button::with_label("×");
        remove_button.set_has_frame(false);
        remove_button.set_tooltip_text(Some("Remover marcador"));
        let remove_path = bookmark_path.clone();
        let remove_page = bookmark_page;
        remove_button.connect_clicked(move |_| {
            let _ = remove_bookmark_page(&remove_path, remove_page);
        });
        row_box.append(&remove_button);
        row.set_child(Some(&row_box));
        list.append(&row);
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

pub fn build_sidebar(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    title_label: gtk4::Label,
    print_button: gtk4::Button,
    controller: Rc<ViewerController>,
) -> gtk4::Box {
    let open_context = SidebarOpenContext {
        app: app.clone(),
        window: window.clone(),
        tab_manager: tab_manager.clone(),
        title_label: title_label.clone(),
        print_button: print_button.clone(),
    };

    let sidebar_root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    sidebar_root.set_size_request(178, -1);

    let panel_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    panel_header.set_margin_start(8);
    panel_header.set_margin_end(8);
    panel_header.set_margin_top(6);
    panel_header.set_margin_bottom(6);
    let panel_title = gtk4::Label::new(Some("Miniaturas"));
    panel_title.set_xalign(0.0);
    panel_title.set_hexpand(true);
    panel_header.append(&panel_title);
    let close_button = gtk4::Button::from_icon_name("window-close-symbolic");
    close_button.set_has_frame(false);
    close_button.set_tooltip_text(Some("Ocultar painel"));
    panel_header.append(&close_button);
    sidebar_root.append(&panel_header);
    sidebar_root.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    let section_toggle_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    section_toggle_row.set_margin_start(8);
    section_toggle_row.set_margin_end(8);
    section_toggle_row.set_margin_top(6);
    section_toggle_row.set_margin_bottom(6);
    let mini_toggle = section_toggle("Mini", true);
    let outline_toggle = section_toggle("Marc", true);
    let search_toggle = section_toggle("Busca", true);
    let recent_toggle = section_toggle("Rec", false);
    let favorites_toggle = section_toggle("Fav", false);
    section_toggle_row.append(&mini_toggle);
    section_toggle_row.append(&outline_toggle);
    section_toggle_row.append(&search_toggle);
    section_toggle_row.append(&recent_toggle);
    section_toggle_row.append(&favorites_toggle);
    sidebar_root.append(&section_toggle_row);
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

    let pages_flow = gtk4::FlowBox::new();
    pages_flow.set_min_children_per_line(1);
    pages_flow.set_max_children_per_line(1);
    pages_flow.set_selection_mode(gtk4::SelectionMode::None);
    pages_flow.set_homogeneous(false);
    pages_flow.set_row_spacing(8);
    pages_flow.set_column_spacing(0);
    let thumbnail_cache = Rc::new(RefCell::new(LruCache::new(THUMBNAIL_CACHE_BYTES)));
    populate_pages_flow(&pages_flow, &controller, &thumbnail_cache);
    let thumbnails_section = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    thumbnails_section.append(&pages_flow);
    sidebar.append(&thumbnails_section);

    let outline_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    outline_header.set_halign(gtk4::Align::Fill);
    outline_header.set_hexpand(true);
    let outline_label = gtk4::Label::new(Some("Marcadores"));
    outline_label.set_halign(gtk4::Align::Start);
    outline_label.set_hexpand(true);
    outline_label.set_markup("<b>Marcadores</b>");
    outline_header.append(&outline_label);
    let refresh_outline = gtk4::Button::with_label("Atualizar");
    outline_header.append(&refresh_outline);

    let outline_list = gtk4::ListBox::new();
    outline_list.set_selection_mode(gtk4::SelectionMode::None);
    outline_list.set_css_classes(&["boxed-list"]);
    populate_outline_list(&outline_list, &controller);
    let outline_controller = controller.clone();
    let outline_list_for_refresh = outline_list.clone();
    refresh_outline.connect_clicked(move |_| {
        populate_outline_list(&outline_list_for_refresh, &outline_controller);
    });
    let outline_section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    outline_section.append(&outline_header);
    outline_section.append(&outline_list);
    let outline_expander = gtk4::Expander::new(Some("Marcadores"));
    outline_expander.set_expanded(false);
    outline_expander.set_child(Some(&outline_section));
    sidebar.append(&outline_expander);

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
    let search_expander = gtk4::Expander::new(Some("Busca"));
    search_expander.set_expanded(true);
    search_expander.set_child(Some(&search_section));
    sidebar.append(&search_expander);

    let recent_list = gtk4::ListBox::new();
    recent_list.set_selection_mode(gtk4::SelectionMode::Single);
    recent_list.set_css_classes(&["boxed-list"]);
    for path in load_recent_documents() {
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Document")
            .to_string();
        append_sidebar_entry(&recent_list, path, &label, &open_context);
    }
    let recent_section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let recent_title = gtk4::Label::new(Some("<b>Recentes</b>"));
    recent_title.set_use_markup(true);
    recent_title.set_xalign(0.0);
    recent_section.append(&recent_title);
    recent_section.append(&recent_list);
    let recent_expander = gtk4::Expander::new(Some("Recentes"));
    recent_expander.set_expanded(false);
    recent_expander.set_child(Some(&recent_section));
    recent_expander.set_visible(false);
    sidebar.append(&recent_expander);

    let favorites_list = gtk4::ListBox::new();
    favorites_list.set_selection_mode(gtk4::SelectionMode::Single);
    favorites_list.set_css_classes(&["boxed-list"]);
    for path in load_favorite_documents() {
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Document")
            .to_string();
        append_sidebar_entry(&favorites_list, path, &label, &open_context);
    }
    let favorites_section = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let favorites_title = gtk4::Label::new(Some("<b>Favoritos</b>"));
    favorites_title.set_use_markup(true);
    favorites_title.set_xalign(0.0);
    favorites_section.append(&favorites_title);
    favorites_section.append(&favorites_list);
    let favorites_expander = gtk4::Expander::new(Some("Favoritos"));
    favorites_expander.set_expanded(false);
    favorites_expander.set_child(Some(&favorites_section));
    favorites_expander.set_visible(false);
    sidebar.append(&favorites_expander);

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
    let app_bookmarks_expander = gtk4::Expander::new(Some("Marcadores do app"));
    app_bookmarks_expander.set_expanded(false);
    app_bookmarks_expander.set_child(Some(&app_bookmarks_section));
    sidebar.append(&app_bookmarks_expander);

    {
        let thumbnails_section = thumbnails_section.clone();
        mini_toggle.connect_toggled(move |button| {
            thumbnails_section.set_visible(button.is_active());
        });
    }
    {
        let outline_expander = outline_expander.clone();
        outline_toggle.connect_toggled(move |button| {
            outline_expander.set_visible(button.is_active());
        });
    }
    {
        let search_expander = search_expander.clone();
        search_toggle.connect_toggled(move |button| {
            search_expander.set_visible(button.is_active());
        });
    }
    {
        let recent_expander = recent_expander.clone();
        recent_toggle.connect_toggled(move |button| {
            recent_expander.set_visible(button.is_active());
        });
    }
    {
        let favorites_expander = favorites_expander.clone();
        favorites_toggle.connect_toggled(move |button| {
            favorites_expander.set_visible(button.is_active());
        });
    }

    let state_signature = Rc::new(RefCell::new((
        None::<PathBuf>,
        -1,
        -1,
        0,
        String::new(),
        0_usize,
        0_usize,
    )));
    let watch_controller = controller.clone();
    let watch_flow = pages_flow.clone();
    let watch_outline = outline_list.clone();
    let watch_cache = thumbnail_cache.clone();
    let watch_search = search_list.clone();
    let watch_bookmarks = app_bookmarks_list.clone();
    let watch_signature = state_signature.clone();
    controller.subscribe_state_changes(move || {
        let snapshot = {
            let state = watch_controller.state.borrow();
            (
                state.path.clone(),
                state.page_index,
                state.page_count,
                state.rotation,
                state.search_query.clone(),
                state.search_results.len(),
                state.search_global_index,
            )
        };
        let previous = watch_signature.borrow().clone();
        if snapshot == previous {
            return;
        }

        populate_pages_flow(&watch_flow, &watch_controller, &watch_cache);
        if snapshot.0 != previous.0 {
            populate_outline_list(&watch_outline, &watch_controller);
            populate_app_bookmarks_list(&watch_bookmarks, &watch_controller);
        }
        if snapshot.1 != previous.1 || snapshot.0 != previous.0 {
            populate_app_bookmarks_list(&watch_bookmarks, &watch_controller);
        }
        if snapshot.4 != previous.4 || snapshot.5 != previous.5 || snapshot.6 != previous.6 {
            populate_search_results_list(&watch_search, &watch_controller);
        }
        *watch_signature.borrow_mut() = snapshot;
    });

    sidebar_root
}

#[cfg(test)]
mod tests {
    use super::page_range_for_sidebar;

    #[test]
    fn page_range_for_sidebar_tracks_a_centered_window() {
        assert_eq!(page_range_for_sidebar(12, 5), vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(page_range_for_sidebar(5, 0), vec![0, 1, 2, 3, 4]);
        assert_eq!(page_range_for_sidebar(3, 2), vec![0, 1, 2]);
    }
}
