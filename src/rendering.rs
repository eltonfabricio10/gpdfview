use std::path::PathBuf;
use std::time::Instant;

use gtk4::cairo::{Format, ImageSurface};
use gtk4::prelude::*;

use crate::app::ViewerController;
use crate::engine::{RenderResult, RenderedPage};
use crate::viewer_state::apply_render_request;
use crate::session::remember_document_view;
use crate::viewer_state::LayoutMode;

pub(crate) fn request_render(
    controller: &ViewerController,
    path: PathBuf,
    page_index: i32,
    scale: f64,
    rotation: i32,
) {
    let (page_index, scale, rotation) = {
        let mut state = controller.state.borrow_mut();
        let request_path = path.clone();
        let requested_page = page_index;
        let requested_scale = scale;
        let requested_rotation = rotation;
        let render_context = apply_render_request(
            &mut state,
            Some(request_path),
            requested_page,
            requested_scale,
            requested_rotation,
        );
        let Some((_, page_index, scale, rotation)) = render_context else {
            return;
        };
        (page_index, scale, rotation)
    };

    let key = crate::viewer_state::RenderKey::new(path.clone(), page_index, scale, rotation);
    let generation = controller.renderer.invalidate();
    {
        let mut state = controller.state.borrow_mut();
        state.generation = generation;
        state.request_started = None;
    }
    controller
        .zoom_label
        .set_text(&format!("{:.0}%", key.scale() * 100.0));

    let cached = { controller.cache.borrow_mut().get(&key).cloned() };
    if let Some(cached) = cached {
        *controller.surface.borrow_mut() = Some(cached);
        controller.set_page_spin_value(f64::from(page_index + 1));
        controller.area.queue_draw();
        controller.status.set_text("Page loaded from memory cache");
        let page_count = controller.state.borrow().page_count;
        controller.schedule_prefetch(path, page_index, page_count, scale, generation, rotation);
        return;
    }

    let generation = controller.renderer.request(path.clone(), page_index, scale, rotation);
    {
        let mut state = controller.state.borrow_mut();
        state.generation = generation;
        state.request_started = Some(Instant::now());
    }
    controller.status.set_text("Rendering page…");
    let count = controller.state.borrow().page_count;
    if count > 0 {
        controller.page_total.set_text(&format!("/ {count}"));
    } else {
        controller.page_total.set_text("/ …");
    }
}

pub(crate) fn present_page(controller: &ViewerController, page: RenderedPage) {
    {
        let state = controller.state.borrow();
        if page.generation != state.generation
            || state.path.as_ref() != Some(&page.path)
            || state.rotation != page.rotation
            || (state.scale * 1000.0).round() as u32 != (page.scale * 1000.0).round() as u32
        {
            return;
        }
    }
    let key = crate::viewer_state::RenderKey::new(page.path.clone(), page.page_index, page.scale, page.rotation);
    let render_time = if page.prefetch {
        None
    } else {
        controller
            .state
            .borrow_mut()
            .request_started
            .take()
            .map(|started| started.elapsed())
    };
    let page_surface = match ImageSurface::create_for_data(
        page.pixels,
        Format::ARgb32,
        page.width,
        page.height,
        page.stride,
    ) {
        Ok(surface) => surface,
        Err(error) => {
            controller
                .status
                .set_text(&format!("Could not display rendered page: {error}"));
            return;
        }
    };
    let bytes = (page.stride as usize).saturating_mul(page.height as usize);
    controller
        .cache
        .borrow_mut()
        .insert(key, page_surface.clone(), bytes);
    if page.prefetch {
        return;
    }
    *controller.surface.borrow_mut() = Some(page_surface);

    let pending_scroll = {
        let mut state = controller.state.borrow_mut();
        state.page_count = page.page_count;
        state.page_index = page.page_index;
        state.page_width_points = page.page_width_points;
        state.page_height_points = page.page_height_points;
        state.rotation = page.rotation;
        if state.page_index >= 0 {
            state.pending_search_rect.take()
        } else {
            None
        }
    };
    {
        let state = controller.state.borrow();
        if state.layout_mode == LayoutMode::Continuous {
            let width = page.width.saturating_add(96);
            let height = page
                .height
                .saturating_mul(page.page_count.max(1))
                .saturating_add(page.page_count.saturating_mul(30))
                .saturating_add(48);
            controller.area.set_content_width(width);
            controller.area.set_content_height(height);
        } else {
            controller.area.set_content_width(page.width.saturating_add(32));
            controller.area.set_content_height(page.height.saturating_add(32));
        }
    }
    controller.area.queue_draw();
    controller
        .page_spin
        .set_range(1.0, f64::from(page.page_count.max(1)));
    controller.set_page_spin_value(f64::from(page.page_index + 1));
    controller.page_spin.set_sensitive(page.page_count > 0);
    controller.page_total.set_text(&format!("/ {}", page.page_count));
    controller
        .zoom_label
        .set_text(&format!("{:.0}%", controller.state.borrow().scale * 100.0));
    controller.status.set_text(&format!(
        "Page {} of {} · {} × {} pt · rendered in {:.0} ms · cache {} MiB",
        page.page_index + 1,
        page.page_count,
        page.page_width_points.round(),
        page.page_height_points.round(),
        render_time.map_or(0.0, |time| time.as_secs_f64() * 1000.0),
        controller.cache.borrow().bytes_used() / (1024 * 1024)
    ));
    let path = controller.state.borrow().path.clone();
    let scale = controller.state.borrow().scale;
    let rotation = controller.state.borrow().rotation;
    let generation = controller.state.borrow().generation;
    if let Some(path) = path {
        let state = controller.state.borrow();
        remember_document_view(
            &path,
            state.page_index,
            state.scale,
            state.rotation,
            state.layout_mode.as_str(),
        );
        drop(state);
        controller.schedule_prefetch(path, page.page_index, page.page_count, scale, generation, rotation);
    }

    if let Some(rect) = pending_scroll {
        let state = controller.state.borrow();
        controller.scroll_to_search_match(rect, state.page_height_points, state.scale);
    }
}

pub(crate) fn schedule_prefetch(
    controller: &ViewerController,
    path: PathBuf,
    page_index: i32,
    page_count: i32,
    scale: f64,
    generation: u64,
    rotation: i32,
) {
    let mut pages = Vec::with_capacity(2);
    if page_index > 0 {
        pages.push(page_index - 1);
    }
    if page_index + 1 < page_count {
        pages.push(page_index + 1);
    }
    controller.renderer.prefetch(path, &pages, scale, generation, rotation);
}

pub(crate) fn poll(controller: &ViewerController) {
    while let Ok(result) = controller.renderer.try_recv() {
        match result {
            RenderResult::Page(page) => controller.present_page(page),
            RenderResult::Error(error) => controller.show_error(error),
        }
    }
    while let Ok(result) = controller.printer.try_recv() {
        controller.print_button.set_sensitive(true);
        match result {
            Ok(outcome) => controller.status.set_text(&outcome.message),
            Err(error) => controller.status.set_text(&format!("Print failed: {error}")),
        }
    }
}

pub(crate) fn search_current_page(controller: &ViewerController) {
    let state = controller.state.borrow();
    let Some(path) = state.path.clone() else {
        controller.status.set_text("Open a PDF document before searching.");
        return;
    };
    let query = controller.search_entry.text().trim().to_string();
    if query.is_empty() {
        controller.status.set_text("Type a word or phrase to search in the PDF.");
        return;
    }
    let page_index = state.page_index;
    drop(state);

    match crate::engine::find_text_rectangles_in_document(&path, &query) {
        Ok(matches) => {
            let mut ordered_matches = matches;
            if let Some(pivot) = ordered_matches
                .iter()
                .position(|entry| entry.page_index >= page_index)
            {
                ordered_matches.rotate_left(pivot);
            }
            {
                let mut state = controller.state.borrow_mut();
                state.search_query = query.clone();
                state.search_results = ordered_matches
                    .into_iter()
                    .map(|entry| crate::viewer_state::SearchOccurrence {
                        page_index: entry.page_index,
                        rect: entry.rect,
                    })
                    .collect();
                state.search_global_index = 0;
                state.search_highlights.clear();
                state.search_index = 0;
                state.pending_search_rect = None;
            }
            let total = controller.state.borrow().search_results.len();
            if total == 0 {
                controller.status.set_text(&format!(
                    "No matches found for \"{query}\" in this document.",
                ));
                controller.area.queue_draw();
                return;
            }
            controller.focus_global_search_result(0);
        }
        Err(error) => {
            let mut state = controller.state.borrow_mut();
            state.search_results.clear();
            state.search_highlights.clear();
            state.search_query.clear();
            state.pending_search_rect = None;
            controller.area.queue_draw();
            controller.status.set_text(&format!("Search failed: {error}"));
        }
    }
}
