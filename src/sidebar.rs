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
    is_favorite_document, load_favorite_documents, load_recent_documents, toggle_favorite_document,
};
use crate::viewer_state::RenderKey;

const THUMBNAIL_MAX_EDGE: i32 = 120;
const THUMBNAIL_CACHE_BYTES: usize = 24 * 1024 * 1024;

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

    if page_count <= 5 {
        return (0..page_count).collect();
    }

    let start = (current_page - 2).max(0);
    let end = (current_page + 2).min(page_count - 1);
    let mut pages = Vec::new();
    for page_index in start..=end {
        pages.push(page_index);
    }

    if pages.len() < 5 {
        pages.clear();
        for page_index in 0..5 {
            pages.push(page_index);
        }
    }

    pages
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
        preview.set_content_width(surface.width());
        preview.set_content_height(surface.height());
        preview.set_size_request(surface.width(), surface.height());
        let thumbnail = surface.clone();
        preview.set_draw_func(move |_, context, _, _| {
            if context.set_source_surface(&thumbnail, 0.0, 0.0).is_ok() {
                let _ = context.paint();
            }
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

    let sidebar = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    sidebar.set_size_request(220, -1);
    sidebar.set_margin_start(8);
    sidebar.set_margin_end(8);
    sidebar.set_margin_top(8);
    sidebar.set_margin_bottom(8);

    let pages_label = gtk4::Label::new(Some("Pages"));
    pages_label.set_halign(gtk4::Align::Start);
    pages_label.set_markup("<b>Pages</b>");
    sidebar.append(&pages_label);

    let pages_flow = gtk4::FlowBox::new();
    pages_flow.set_min_children_per_line(2);
    pages_flow.set_max_children_per_line(2);
    pages_flow.set_selection_mode(gtk4::SelectionMode::None);
    pages_flow.set_homogeneous(true);
    let thumbnail_cache = Rc::new(RefCell::new(LruCache::new(THUMBNAIL_CACHE_BYTES)));
    let state = controller.state.borrow();
    for page_index in page_range_for_sidebar(state.page_count, state.page_index) {
        let page_button = build_page_thumbnail_button(
            &controller,
            &thumbnail_cache,
            page_index,
            page_index == state.page_index,
        );
        pages_flow.append(&page_button);
    }
    drop(state);
    sidebar.append(&pages_flow);

    let outline_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    outline_header.set_halign(gtk4::Align::Fill);
    outline_header.set_hexpand(true);
    let outline_label = gtk4::Label::new(Some("Outline"));
    outline_label.set_halign(gtk4::Align::Start);
    outline_label.set_hexpand(true);
    outline_label.set_markup("<b>Outline</b>");
    outline_header.append(&outline_label);
    let refresh_outline = gtk4::Button::with_label("Refresh");
    outline_header.append(&refresh_outline);
    sidebar.append(&outline_header);

    let outline_list = gtk4::ListBox::new();
    outline_list.set_selection_mode(gtk4::SelectionMode::None);
    populate_outline_list(&outline_list, &controller);
    let outline_controller = controller.clone();
    let outline_list_for_refresh = outline_list.clone();
    refresh_outline.connect_clicked(move |_| {
        populate_outline_list(&outline_list_for_refresh, &outline_controller);
    });
    sidebar.append(&outline_list);

    let recent_label = gtk4::Label::new(Some("Recent"));
    recent_label.set_halign(gtk4::Align::Start);
    recent_label.set_markup("<b>Recent</b>");
    sidebar.append(&recent_label);

    let recent_list = gtk4::ListBox::new();
    recent_list.set_selection_mode(gtk4::SelectionMode::Single);
    for path in load_recent_documents() {
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Document")
            .to_string();
        append_sidebar_entry(
            &recent_list,
            path,
            &label,
            &open_context,
        );
    }
    sidebar.append(&recent_list);

    let favorites_label = gtk4::Label::new(Some("Favorites"));
    favorites_label.set_halign(gtk4::Align::Start);
    favorites_label.set_markup("<b>Favorites</b>");
    sidebar.append(&favorites_label);

    let favorites_list = gtk4::ListBox::new();
    favorites_list.set_selection_mode(gtk4::SelectionMode::Single);
    for path in load_favorite_documents() {
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Document")
            .to_string();
        append_sidebar_entry(
            &favorites_list,
            path,
            &label,
            &open_context,
        );
    }
    sidebar.append(&favorites_list);

    sidebar
}

#[cfg(test)]
mod tests {
    use super::page_range_for_sidebar;

    #[test]
    fn page_range_for_sidebar_stays_centered_around_the_current_page() {
        assert_eq!(page_range_for_sidebar(12, 5), vec![3, 4, 5, 6, 7]);
        assert_eq!(page_range_for_sidebar(5, 0), vec![0, 1, 2, 3, 4]);
        assert_eq!(page_range_for_sidebar(3, 2), vec![0, 1, 2]);
    }
}
