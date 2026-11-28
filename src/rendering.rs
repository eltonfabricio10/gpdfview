use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Instant;

use gtk4::cairo::{Format, ImageSurface};
use gtk4::prelude::*;

use crate::app::ViewerController;
use crate::engine::{RenderResult, RenderedPage};
use crate::layout::continuous_content_height;
use crate::session::remember_document_view;
use crate::viewer_state::apply_render_request;
use crate::viewer_state::LayoutMode;

pub(crate) fn update_canvas_size(controller: &ViewerController) {
    let (layout_mode, split_mode, page_count) = {
        let state = controller.state.borrow();
        (state.layout_mode, state.split_mode, state.page_count)
    };
    let Some(surface) = controller.surface.borrow().as_ref().cloned() else {
        return;
    };

    if layout_mode == LayoutMode::Continuous
        && split_mode == crate::viewer_state::SplitViewMode::Off
    {
        controller
            .area
            .set_content_width(surface.width().saturating_add(96));
        controller
            .area
            .set_content_height(continuous_content_height(surface.height(), page_count));
    } else if split_mode == crate::viewer_state::SplitViewMode::Horizontal
        || (split_mode == crate::viewer_state::SplitViewMode::Off
            && layout_mode == LayoutMode::TwoPage)
    {
        controller
            .area
            .set_content_width(surface.width().saturating_mul(2).saturating_add(64));
        controller
            .area
            .set_content_height(surface.height().saturating_add(32));
    } else if split_mode == crate::viewer_state::SplitViewMode::Vertical {
        controller
            .area
            .set_content_width(surface.width().saturating_add(32));
        controller
            .area
            .set_content_height(surface.height().saturating_mul(2).saturating_add(64));
    } else {
        controller
            .area
            .set_content_width(surface.width().saturating_add(32));
        controller
            .area
            .set_content_height(surface.height().saturating_add(32));
    }
}

pub(crate) fn request_companion_render(
    controller: &ViewerController,
    path: PathBuf,
    page_index: i32,
    scale: f64,
    rotation: i32,
) {
    let key = crate::viewer_state::RenderKey::new(path.clone(), page_index, scale, rotation);
    if controller.cache.borrow_mut().get(&key).is_some() {
        return;
    }
    let generation = controller.state.borrow().generation;
    controller
        .renderer
        .prefetch(path, &[page_index], scale, generation, rotation);
}

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
    let page_count = controller.state.borrow().page_count;
    controller
        .previous_button
        .set_sensitive(page_count > 0 && page_index > 0);
    controller
        .next_button
        .set_sensitive(page_count > 0 && page_index + 1 < page_count);

    let key = crate::viewer_state::RenderKey::new(path.clone(), page_index, scale, rotation);
    let generation = controller.renderer.invalidate();
    {
        let mut state = controller.state.borrow_mut();
        state.generation = generation;
        state.request_started = None;
    }
    controller
        .zoom_button
        .set_label(&format!("{:.0}%", key.scale() * 100.0));

    let cached = { controller.cache.borrow_mut().get(&key).cloned() };
    if let Some(cached) = cached {
        *controller.surface.borrow_mut() = Some(cached);
        update_canvas_size(controller);
        controller.set_page_spin_value(f64::from(page_index + 1));
        controller.area.queue_draw();
        controller.status.set_text("Página carregada do cache.");
        let page_count = controller.state.borrow().page_count;
        controller.schedule_prefetch(path, page_index, page_count, scale, generation, rotation);
        return;
    }

    let generation = controller
        .renderer
        .request(path.clone(), page_index, scale, rotation);
    {
        let mut state = controller.state.borrow_mut();
        state.generation = generation;
        state.request_started = Some(Instant::now());
    }
    controller.status.set_text("Renderizando página…");
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
        {
            return;
        }
    }
    let key = crate::viewer_state::RenderKey::new(
        page.path.clone(),
        page.page_index,
        page.scale,
        page.rotation,
    );
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
                .set_text(&format!("Não foi possível exibir a página: {error}"));
            return;
        }
    };
    let bytes = (page.stride as usize).saturating_mul(page.height as usize);
    controller
        .cache
        .borrow_mut()
        .insert(key, page_surface.clone(), bytes);
    if page.prefetch {
        let is_visible_companion = controller
            .state
            .borrow()
            .companion_pages()
            .contains(&page.page_index);
        if is_visible_companion {
            controller.area.queue_draw();
        }
        return;
    }
    *controller.surface.borrow_mut() = Some(page_surface);

    let pending_scroll = {
        let mut state = controller.state.borrow_mut();
        state.page_count = page.page_count;
        state.page_index = page.page_index;
        state.page_width_points = page.page_width_points;
        state.page_height_points = page.page_height_points;
        // Present the effective zoom when the engine reduces a raster to its memory budget.
        state.scale = page.scale;
        state.rotation = page.rotation;
        if state.page_index >= 0 {
            state.pending_search_rect.take()
        } else {
            None
        }
    };
    update_canvas_size(controller);
    controller.area.queue_draw();
    controller
        .page_spin
        .set_range(1.0, f64::from(page.page_count.max(1)));
    controller.page_indicator.set_visible_child_name("page");
    controller.set_page_spin_value(f64::from(page.page_index + 1));
    controller.page_spin.set_sensitive(page.page_count > 0);
    controller.zoom_button.set_sensitive(page.page_count > 0);
    controller
        .previous_button
        .set_sensitive(page.page_index > 0 && page.page_count > 0);
    controller
        .next_button
        .set_sensitive(page.page_index + 1 < page.page_count && page.page_count > 0);
    controller
        .page_total
        .set_text(&format!("/ {}", page.page_count));
    controller.sync_layout_toggle_label();
    controller
        .zoom_button
        .set_label(&format!("{:.0}%", controller.state.borrow().scale * 100.0));
    controller.status.set_text(&format!(
        "Página {} de {} · {} × {} pt · renderizada em {:.0} ms · cache {} MiB · hits {} · evictions {}",
        page.page_index + 1,
        page.page_count,
        page.page_width_points.round(),
        page.page_height_points.round(),
        render_time.map_or(0.0, |time| time.as_secs_f64() * 1000.0),
        controller.cache.borrow().bytes_used() / (1024 * 1024),
        controller.cache.borrow().stats().hits,
        controller.cache.borrow().stats().evictions
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
        let companion_pages = state
            .companion_pages()
            .into_iter()
            .filter(|page_index| *page_index != state.page_index)
            .collect::<Vec<_>>();
        drop(state);
        for companion_page in companion_pages {
            request_companion_render(controller, path.clone(), companion_page, scale, rotation);
        }
        controller.schedule_prefetch(
            path,
            page.page_index,
            page.page_count,
            scale,
            generation,
            rotation,
        );
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
    controller
        .renderer
        .prefetch(path, &pages, scale, generation, rotation);
}

pub(crate) fn poll(controller: &ViewerController) {
    while let Ok(result) = controller.renderer.try_recv() {
        match result {
            RenderResult::Page(page) => controller.present_page(page),
            RenderResult::Error(error) => controller.show_error(error),
        }
    }
    loop {
        match controller.printer.try_recv() {
            Ok(result) => {
                controller.print_button.set_sensitive(true);
                match result {
                    Ok(outcome) => controller.status.set_text(&outcome.message),
                    Err(error) => controller
                        .status
                        .set_text(&format!("Falha na impressão: {error}")),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                controller.print_button.set_sensitive(true);
                controller
                    .status
                    .set_text("O serviço de impressão foi interrompido. Reinicie o aplicativo para tentar novamente.");
                break;
            }
        }
    }
    let search_progress = controller
        .search_progress_receiver
        .borrow()
        .try_iter()
        .collect::<Vec<_>>();
    for progress in search_progress {
        let current_generation = controller.search_generation.load(Ordering::Relaxed);
        let current_path = controller.state.borrow().path.clone();
        if progress.generation == current_generation
            && current_path.as_ref() == Some(&progress.path)
        {
            controller.status.set_text(&format!(
                "Buscando página {} de {}…",
                progress.current_page, progress.total_pages
            ));
        }
    }
    let search_responses = controller
        .search_receiver
        .borrow()
        .try_iter()
        .collect::<Vec<_>>();
    for response in search_responses {
        let current_generation = controller.search_generation.load(Ordering::Relaxed);
        let current_path = controller.state.borrow().path.clone();
        let current_query = controller.state.borrow().search_query.clone();
        if !is_search_response_current(
            current_generation,
            response.generation,
            current_path.as_ref(),
            &response.path,
            &current_query,
            &response.query,
        ) {
            continue;
        }
        complete_document_search(controller, response);
    }
}

pub(crate) fn search_current_page(controller: &ViewerController) {
    let generation = controller.search_generation.fetch_add(1, Ordering::Relaxed) + 1;
    let state = controller.state.borrow();
    let Some(path) = state.path.clone() else {
        controller
            .status
            .set_text("Abra um PDF antes de iniciar a busca.");
        return;
    };
    let query = controller.search_entry.text().trim().to_string();
    if query.is_empty() {
        drop(state);
        let mut state = controller.state.borrow_mut();
        state.search_query.clear();
        state.search_results.clear();
        state.search_global_index = 0;
        state.search_highlights.clear();
        state.pending_search_rect = None;
        drop(state);
        controller.search_generation.fetch_add(1, Ordering::Relaxed);
        controller.search_previous_button.set_sensitive(false);
        controller.search_next_button.set_sensitive(false);
        controller.area.queue_draw();
        controller
            .status
            .set_text("Digite uma palavra ou frase para buscar no PDF.");
        controller.notify_state_changed();
        return;
    }
    let page_index = state.page_index;
    drop(state);
    let mut options = poppler::FindFlags::empty();
    if controller.search_case_sensitive.is_active() {
        options |= poppler::FindFlags::CASE_SENSITIVE;
    }
    if controller.search_whole_words.is_active() {
        options |= poppler::FindFlags::WHOLE_WORDS_ONLY;
    }

    {
        let mut state = controller.state.borrow_mut();
        state.search_query = query.clone();
        state.search_highlight_all = controller.search_highlight_all.is_active();
        state.search_results.clear();
        state.search_global_index = 0;
        state.search_highlights.clear();
        state.search_index = 0;
        state.pending_search_rect = None;
    }
    controller.search_previous_button.set_sensitive(false);
    controller.search_next_button.set_sensitive(false);
    controller.status.set_text("Buscando no documento…");
    controller.area.queue_draw();

    let sender = controller.search_sender.clone();
    let progress_sender = controller.search_progress_sender.clone();
    let progress_path = path.clone();
    let cancellation_generation = controller.search_generation.clone();
    if let Err(error) = thread::Builder::new()
        .name("gpdfview-search".into())
        .spawn(move || {
            let result = crate::engine::find_text_rectangles_in_document_cancellable(
                &path,
                &query,
                options,
                || cancellation_generation.load(Ordering::Relaxed) != generation,
                |current_page, total_pages| {
                    let _ = progress_sender.try_send(crate::app::SearchProgress {
                        generation,
                        path: progress_path.clone(),
                        current_page,
                        total_pages,
                    });
                },
            );
            let _ = sender.send(crate::app::SearchResponse {
                generation,
                path,
                query,
                pivot_page: page_index,
                result,
            });
        })
    {
        controller
            .status
            .set_text(&format!("Não foi possível iniciar a busca: {error}"));
    }
}

fn complete_document_search(controller: &ViewerController, response: crate::app::SearchResponse) {
    match response.result {
        Ok(search_result) => {
            let was_truncated = search_result.truncated;
            let mut ordered_matches = search_result.matches;
            if let Some(pivot) = ordered_matches
                .iter()
                .position(|entry| entry.page_index >= response.pivot_page)
            {
                ordered_matches.rotate_left(pivot);
            }
            {
                let mut state = controller.state.borrow_mut();
                state.search_query = response.query.clone();
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
            controller.search_previous_button.set_sensitive(total > 0);
            controller.search_next_button.set_sensitive(total > 0);
            if total == 0 {
                controller.status.set_text(&format!(
                    "Nenhuma ocorrência encontrada para \"{}\" neste documento.",
                    response.query,
                ));
                controller.area.queue_draw();
                controller.notify_state_changed();
                return;
            }
            controller.focus_global_search_result(0);
            if was_truncated {
                controller.status.set_text(&format!(
                    "Exibindo os primeiros {total} resultados. Refine a busca para reduzir ocorrências."
                ));
            }
        }
        Err(error) => {
            let mut state = controller.state.borrow_mut();
            state.search_results.clear();
            state.search_highlights.clear();
            state.search_query.clear();
            state.pending_search_rect = None;
            drop(state);
            controller.search_previous_button.set_sensitive(false);
            controller.search_next_button.set_sensitive(false);
            controller.area.queue_draw();
            controller
                .status
                .set_text(&format!("A busca falhou: {error}"));
            controller.notify_state_changed();
        }
    }
}

fn is_search_response_current(
    current_generation: u64,
    response_generation: u64,
    current_path: Option<&PathBuf>,
    response_path: &PathBuf,
    current_query: &str,
    response_query: &str,
) -> bool {
    response_generation == current_generation
        && current_path == Some(response_path)
        && current_query == response_query
}

#[cfg(test)]
mod tests {
    use super::is_search_response_current;
    use std::path::PathBuf;

    #[test]
    fn accepts_only_search_responses_matching_generation_path_and_query() {
        let current = PathBuf::from("/tmp/current.pdf");
        let other = PathBuf::from("/tmp/other.pdf");

        assert!(is_search_response_current(
            4,
            4,
            Some(&current),
            &current,
            "Aparecida",
            "Aparecida"
        ));
        assert!(!is_search_response_current(
            5,
            4,
            Some(&current),
            &current,
            "Aparecida",
            "Aparecida"
        ));
        assert!(!is_search_response_current(
            4,
            4,
            Some(&current),
            &other,
            "Aparecida",
            "Aparecida"
        ));
        assert!(!is_search_response_current(
            4,
            4,
            Some(&current),
            &current,
            "Maria",
            "Aparecida"
        ));
    }
}
