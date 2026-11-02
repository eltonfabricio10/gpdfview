use crate::cache::LruCache;
use crate::engine::{PdfRenderer, RenderError, RenderResult, RenderedPage};
use crate::printing::PrintService;
use libadwaita as adw;
use adw::prelude::*;
use gtk4 as gtk;
use gtk4::cairo::{Format, ImageSurface};
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};


const CACHE_LIMIT_BYTES: usize = 128 * 1024 * 1024;
const DEFAULT_ZOOM: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyAction {
    NextPage,
    PreviousPage,
    FirstPage,
    LastPage,
    ZoomIn,
    ZoomOut,
    FitWidth,
    FitPage,
    RotateLeft,
    RotateRight,
    OpenFile,
    FocusSearch,
    ToggleFullscreen,
    TogglePresentation,
}

fn normalize_rotation(rotation: i32) -> i32 {
    let normalized = rotation.rem_euclid(360);
    if normalized == 360 { 0 } else { normalized }
}

#[cfg(test)]
fn rotated_page_dimensions(width_points: f64, height_points: f64, rotation: i32) -> (f64, f64) {
    match normalize_rotation(rotation) {
        90 | 270 => (height_points, width_points),
        _ => (width_points, height_points),
    }
}

fn resolve_key_action(key: &str, modifiers: u32) -> Option<KeyAction> {
    let normalized = key.trim();
    let lowered = normalized
        .to_ascii_lowercase()
        .replace('_', "")
        .replace('-', "");
    let has_ctrl = modifiers & gtk::gdk::ModifierType::CONTROL_MASK.bits() != 0;

    if has_ctrl && lowered == "o" {
        return Some(KeyAction::OpenFile);
    }
    if has_ctrl && lowered == "f" {
        return Some(KeyAction::FocusSearch);
    }
    if lowered == "f11" {
        return Some(KeyAction::ToggleFullscreen);
    }
    if lowered == "f5" {
        return Some(KeyAction::TogglePresentation);
    }

    match lowered.as_str() {
        "pagedown" | "space" | "right" | "j" => Some(KeyAction::NextPage),
        "pageup" | "left" | "k" => Some(KeyAction::PreviousPage),
        "home" => Some(KeyAction::FirstPage),
        "end" => Some(KeyAction::LastPage),
        "plus" | "equal" | "kpadd" => Some(KeyAction::ZoomIn),
        "minus" | "kpsubtract" => Some(KeyAction::ZoomOut),
        "f" => Some(KeyAction::FitWidth),
        "g" => Some(KeyAction::FitPage),
        "l" => Some(KeyAction::RotateLeft),
        "r" => Some(KeyAction::RotateRight),
        _ => None,
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct RenderKey {
    path: PathBuf,
    page_index: i32,
    scale_milli: u32,
    rotation: i32,
}

impl RenderKey {
    fn new(path: PathBuf, page_index: i32, scale: f64, rotation: i32) -> Self {
        Self {
            path,
            page_index,
            scale_milli: (scale * 1000.0).round() as u32,
            rotation: normalize_rotation(rotation),
        }
    }

    fn scale(&self) -> f64 {
        f64::from(self.scale_milli) / 1000.0
    }
}

#[derive(Default)]
struct ViewerState {
    path: Option<PathBuf>,
    page_index: i32,
    page_count: i32,
    page_width_points: f64,
    page_height_points: f64,
    scale: f64,
    rotation: i32,
    generation: u64,
    request_started: Option<Instant>,
    search_highlights: Vec<(f64, f64, f64, f64)>,
}

#[derive(Clone)]
struct ViewerController {
    renderer: Rc<PdfRenderer>,
    printer: Rc<PrintService>,
    state: Rc<RefCell<ViewerState>>,
    cache: Rc<RefCell<LruCache<RenderKey, ImageSurface>>>,
    surface: Rc<RefCell<Option<ImageSurface>>>,
    area: gtk::DrawingArea,
    scrolled: gtk::ScrolledWindow,
    page_spin: gtk::SpinButton,
    suppress_spin_events: Rc<Cell<bool>>,
    page_total: gtk::Label,
    zoom_label: gtk::Label,
    search_entry: gtk::SearchEntry,
    status: gtk::Label,
    print_button: gtk::Button,
    title: gtk::Label,
    toolbar: gtk::Box,
    window: adw::ApplicationWindow,
    root: gtk::Box,
}

impl ViewerController {
    fn set_page_spin_value(&self, value: f64) {
        self.suppress_spin_events.set(true);
        self.page_spin.set_value(value);
        self.suppress_spin_events.set(false);
    }

    fn handle_key_action(&self, action: KeyAction) {
        match action {
            KeyAction::NextPage => {
                let page = self.state.borrow().page_index + 1;
                self.navigate_to(page);
            }
            KeyAction::PreviousPage => {
                let page = self.state.borrow().page_index - 1;
                self.navigate_to(page);
            }
            KeyAction::FirstPage => self.navigate_to(0),
            KeyAction::LastPage => {
                let page_count = self.state.borrow().page_count;
                if page_count > 0 {
                    self.navigate_to(page_count - 1);
                }
            }
            KeyAction::ZoomIn => self.change_zoom(0.1),
            KeyAction::ZoomOut => self.change_zoom(-0.1),
            KeyAction::FitWidth => self.fit_width(),
            KeyAction::FitPage => self.fit_page(),
            KeyAction::RotateLeft => self.rotate_page(-90),
            KeyAction::RotateRight => self.rotate_page(90),
            KeyAction::OpenFile => {
                self.status.set_text("Open a PDF document using the toolbar button or menu action.");
            }
            KeyAction::FocusSearch => {
                self.search_entry.grab_focus();
                self.search_entry.select_region(0, -1);
            }
            KeyAction::ToggleFullscreen => self.toggle_fullscreen(),
            KeyAction::TogglePresentation => self.toggle_presentation_mode(),
        }
    }

    fn toggle_fullscreen(&self) {
        if self.window.is_fullscreen() {
            self.window.unfullscreen();
        } else {
            self.window.fullscreen();
        }
    }

    fn toggle_presentation_mode(&self) {
        let is_visible = self.toolbar.is_visible();
        self.toolbar.set_visible(!is_visible);
        self.status.set_visible(!is_visible);
        if is_visible {
            self.window.fullscreen();
        } else {
            self.window.unfullscreen();
        }
    }

    fn open_document(&self, path: PathBuf) {
        self.cache.borrow_mut().clear();
        *self.surface.borrow_mut() = None;
        self.page_spin.set_sensitive(false);
        self.set_page_spin_value(1.0);
        self.page_total.set_text("/ …");
        self.zoom_label.set_text("100%");
        self.print_button.set_sensitive(true);
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF document");
        self.title.set_text(filename);

        {
            let mut state = self.state.borrow_mut();
            state.path = Some(path.clone());
            state.page_index = 0;
            state.page_count = 0;
            state.page_width_points = 0.0;
            state.page_height_points = 0.0;
            state.scale = DEFAULT_ZOOM;
            state.rotation = 0;
            state.search_highlights.clear();
        }
        self.status.set_text("Loading first page…");
        self.request_render(path, 0, DEFAULT_ZOOM, 0);
    }

    fn navigate_to(&self, page_index: i32) {
        let (page_index, path, scale, rotation) = {
            let mut state = self.state.borrow_mut();
            if state.page_count == 0 {
                return;
            }
            state.page_index = page_index.clamp(0, state.page_count - 1);
            (state.page_index, state.path.clone(), state.scale, state.rotation)
        };
        if let Some(path) = path {
            self.request_render(path, page_index, scale, rotation);
        }
    }

    fn change_zoom(&self, delta: f64) {
        let (path, page_index, scale, rotation) = {
            let mut state = self.state.borrow_mut();
            if state.page_count == 0 {
                return;
            }
            state.scale = (state.scale + delta).clamp(0.1, 4.0);
            (state.path.clone(), state.page_index, state.scale, state.rotation)
        };
        if let Some(path) = path {
            self.request_render(path, page_index, scale, rotation);
        }
    }

    fn rotate_page(&self, delta: i32) {
        let (path, page_index, scale, rotation) = {
            let mut state = self.state.borrow_mut();
            let Some(_) = state.path.clone() else {
                return;
            };
            state.rotation = normalize_rotation(state.rotation + delta);
            (state.path.clone(), state.page_index, state.scale, state.rotation)
        };
        if let Some(path) = path {
            self.request_render(path, page_index, scale, rotation);
        }
    }

    fn fit_width(&self) {
        let width = self.scrolled.width().saturating_sub(32).max(1);
        let (path, page_index, scale, rotation) = {
            let mut state = self.state.borrow_mut();
            if state.page_width_points <= 0.0 {
                return;
            }
            state.scale = compute_fit_width_scale(state.page_width_points, width);
            (state.path.clone(), state.page_index, state.scale, state.rotation)
        };
        if let Some(path) = path {
            self.request_render(path, page_index, scale, rotation);
        }
    }

    fn fit_page(&self) {
        let viewport_width = self.scrolled.width().saturating_sub(32).max(1);
        let viewport_height = self.scrolled.height().saturating_sub(56).max(1);
        let (path, page_index, scale, rotation) = {
            let mut state = self.state.borrow_mut();
            if state.page_width_points <= 0.0 || state.page_height_points <= 0.0 {
                return;
            }
            state.scale = compute_fit_page_scale(
                state.page_width_points,
                state.page_height_points,
                viewport_width,
                viewport_height,
            );
            (state.path.clone(), state.page_index, state.scale, state.rotation)
        };
        if let Some(path) = path {
            self.request_render(path, page_index, scale, rotation);
        }
    }

    fn request_render(&self, path: PathBuf, page_index: i32, scale: f64, rotation: i32) {
        let scale = if scale.is_finite() {
            scale.clamp(0.1, 4.0)
        } else {
            DEFAULT_ZOOM
        };
        {
            let mut state = self.state.borrow_mut();
            state.page_index = page_index;
            state.scale = scale;
            state.rotation = normalize_rotation(rotation);
            state.search_highlights.clear();
        }

        let key = RenderKey::new(path.clone(), page_index, scale, rotation);
        let generation = self.renderer.invalidate();
        {
            let mut state = self.state.borrow_mut();
            state.generation = generation;
            state.request_started = None;
        }
        self.zoom_label
            .set_text(&format!("{:.0}%", key.scale() * 100.0));

        let cached = { self.cache.borrow_mut().get(&key).cloned() };
        if let Some(cached) = cached {
            *self.surface.borrow_mut() = Some(cached);
            self.set_page_spin_value(f64::from(page_index + 1));
            self.area.queue_draw();
            self.status.set_text("Page loaded from memory cache");
            let page_count = self.state.borrow().page_count;
            self.schedule_prefetch(path, page_index, page_count, scale, generation, rotation);
            return;
        }

        let generation = self.renderer.request(path.clone(), page_index, scale, rotation);
        {
            let mut state = self.state.borrow_mut();
            state.generation = generation;
            state.request_started = Some(Instant::now());
        }
        *self.surface.borrow_mut() = None;
        self.area.queue_draw();
        self.status.set_text("Rendering page…");
        let count = self.state.borrow().page_count;
        if count > 0 {
            self.page_total.set_text(&format!("/ {count}"));
        } else {
            self.page_total.set_text("/ …");
        }
    }

    fn present_page(&self, page: RenderedPage) {
        {
            let state = self.state.borrow();
            if page.generation != state.generation
                || state.path.as_ref() != Some(&page.path)
                || state.rotation != page.rotation
                || (state.scale * 1000.0).round() as u32 != (page.scale * 1000.0).round() as u32
            {
                return;
            }
        }
        let key = RenderKey::new(page.path.clone(), page.page_index, page.scale, page.rotation);
        let render_time = if page.prefetch {
            None
        } else {
            self.state
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
                self.status
                    .set_text(&format!("Could not display rendered page: {error}"));
                return;
            }
        };
        let bytes = (page.stride as usize).saturating_mul(page.height as usize);
        self.cache
            .borrow_mut()
            .insert(key, page_surface.clone(), bytes);
        if page.prefetch {
            return;
        }
        *self.surface.borrow_mut() = Some(page_surface);

        {
            let mut state = self.state.borrow_mut();
            state.page_count = page.page_count;
            state.page_index = page.page_index;
            state.page_width_points = page.page_width_points;
            state.page_height_points = page.page_height_points;
            state.rotation = page.rotation;
        }
        self.area.set_content_width(page.width.saturating_add(32));
        self.area.set_content_height(page.height.saturating_add(32));
        self.area.queue_draw();
        self.page_spin
            .set_range(1.0, f64::from(page.page_count.max(1)));
        self.set_page_spin_value(f64::from(page.page_index + 1));
        self.page_spin.set_sensitive(page.page_count > 0);
        self.page_total.set_text(&format!("/ {}", page.page_count));
        self.zoom_label
            .set_text(&format!("{:.0}%", self.state.borrow().scale * 100.0));
        self.status.set_text(&format!(
            "Page {} of {} · {} × {} pt · rendered in {:.0} ms · cache {} MiB",
            page.page_index + 1,
            page.page_count,
            page.page_width_points.round(),
            page.page_height_points.round(),
            render_time.map_or(0.0, |time| time.as_secs_f64() * 1000.0),
            self.cache.borrow().bytes_used() / (1024 * 1024)
        ));
        let path = self.state.borrow().path.clone();
        let scale = self.state.borrow().scale;
        let rotation = self.state.borrow().rotation;
        let generation = self.state.borrow().generation;
        if let Some(path) = path {
            self.schedule_prefetch(path, page.page_index, page.page_count, scale, generation, rotation);
        }
    }

    fn schedule_prefetch(
        &self,
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
        self.renderer.prefetch(path, &pages, scale, generation, rotation);
    }

    fn show_error(&self, error: RenderError) {
        if error.generation == self.state.borrow().generation {
            self.status
                .set_text(&format!("Could not render PDF: {}", error.message));
        }
    }

    fn poll(&self) {
        while let Ok(result) = self.renderer.try_recv() {
            match result {
                RenderResult::Page(page) => self.present_page(page),
                RenderResult::Error(error) => self.show_error(error),
            }
        }
        while let Ok(result) = self.printer.try_recv() {
            self.print_button.set_sensitive(true);
            match result {
                Ok(outcome) => self.status.set_text(&outcome.message),
                Err(error) => self.status.set_text(&format!("Print failed: {error}")),
            }
        }
    }

    fn search_current_page(&self) {
        let state = self.state.borrow();
        let Some(path) = state.path.clone() else {
            self.status.set_text("Open a PDF document before searching.");
            return;
        };
        let query = self.search_entry.text().trim().to_string();
        if query.is_empty() {
            self.status.set_text("Type a word or phrase to search in the PDF.");
            return;
        }
        let page_index = state.page_index;
        drop(state);

        match crate::engine::find_text_rectangles_in_page(&path, page_index, &query) {
            Ok(matches) => {
                {
                    let mut state = self.state.borrow_mut();
                    state.search_highlights = matches.clone();
                }
                if matches.is_empty() {
                    self.status.set_text(&format!(
                        "No matches found for \"{query}\" on page {}.",
                        page_index + 1
                    ));
                    self.area.queue_draw();
                    return;
                }
                self.status.set_text(&format!(
                    "Found {} match(es) for \"{query}\" on page {}.",
                    matches.len(),
                    page_index + 1
                ));
                self.area.queue_draw();
            }
            Err(error) => {
                self.state.borrow_mut().search_highlights.clear();
                self.area.queue_draw();
                self.status.set_text(&format!("Search failed: {error}"));
            }
        }
    }

    fn submit_print(&self) {
        let Some(path) = self.state.borrow().path.clone() else {
            self.status.set_text("Open a PDF document before printing.");
            return;
        };
        match self.printer.submit(path) {
            Ok(()) => {
                self.print_button.set_sensitive(false);
                self.status
                    .set_text("Submitting the PDF to the default printer…");
            }
            Err(error) => self.status.set_text(&format!("Print failed: {error}")),
        }
    }
}

fn compute_fit_width_scale(page_width_points: f64, viewport_width: i32) -> f64 {
    let available_width = f64::from(viewport_width.max(1));
    (available_width / page_width_points).clamp(0.1, 4.0)
}

fn cairo_search_highlight_rects(
    page_height_points: f64,
    scale: f64,
    rectangles: &[(f64, f64, f64, f64)],
) -> Vec<(f64, f64, f64, f64)> {
    rectangles
        .iter()
        .map(|(x1, y1, x2, y2)| {
            let x_left = (*x1 * scale).min(*x2 * scale);
            let x_right = (*x1 * scale).max(*x2 * scale);
            let y_top = (page_height_points - *y2) * scale;
            let y_bottom = (page_height_points - *y1) * scale;
            let top = y_top.min(y_bottom);
            let bottom = y_top.max(y_bottom);
            (x_left, top, x_right, bottom)
        })
        .collect()
}

fn compute_fit_page_scale(page_width_points: f64, page_height_points: f64, viewport_width: i32, viewport_height: i32) -> f64 {
    let available_width = f64::from(viewport_width.max(1));
    let available_height = f64::from(viewport_height.max(1));
    let width_scale = available_width / page_width_points;
    let height_scale = available_height / page_height_points;
    width_scale.min(height_scale).clamp(0.1, 4.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_page_scale_uses_the_tighter_dimension() {
        let scale = compute_fit_page_scale(612.0, 792.0, 800, 600);
        let expected = 600.0 / 792.0;
        assert!((scale - expected).abs() < 0.0001, "scale should fit the page height to 600px: {scale}");
    }

    #[test]
    fn fit_width_scale_uses_visible_width() {
        let scale = compute_fit_width_scale(612.0, 700);
        let expected = 700.0 / 612.0;
        assert!((scale - expected).abs() < 0.0001, "scale should fit the page width to viewport: {scale}");
    }

    #[test]
    fn recent_documents_are_kept_unique_and_recent_first() {
        let paths = vec![
            PathBuf::from("/tmp/alpha.pdf"),
            PathBuf::from("/tmp/beta.pdf"),
            PathBuf::from("/tmp/alpha.pdf"),
            PathBuf::from("/tmp/gamma.pdf"),
        ];
        let result = normalize_recent_paths(paths);
        assert_eq!(
            result,
            vec![
                PathBuf::from("/tmp/beta.pdf"),
                PathBuf::from("/tmp/alpha.pdf"),
                PathBuf::from("/tmp/gamma.pdf"),
            ]
        );
    }

    #[test]
    fn document_tab_manager_tracks_multiple_documents() {
        let manager = DocumentTabManager::new_model();
        manager.record_tab("Alpha");
        manager.record_tab("Beta");

        assert_eq!(manager.tab_count(), 2);
        assert_eq!(manager.current_title(), Some("Beta".to_string()));
    }

    #[test]
    fn tab_label_has_a_title_and_close_button_capacity() {
        let manager = DocumentTabManager::new_model();
        manager.record_tab("Alpha");
        let titles = manager.titles.borrow();
        assert_eq!(titles.len(), 1);
        assert_eq!(titles[0], "Alpha");
    }

    #[test]
    fn tab_visibility_uses_the_count_of_visible_pages() {
        assert!(should_show_tabs(1));
        assert!(should_show_tabs(2));
        assert!(!should_show_tabs(0));
    }

    #[test]
    fn key_shortcuts_map_to_navigation_and_zoom_actions() {
        assert_eq!(resolve_key_action("PageDown", 0), Some(KeyAction::NextPage));
        assert_eq!(resolve_key_action("PageUp", 0), Some(KeyAction::PreviousPage));
        assert_eq!(resolve_key_action("plus", 0), Some(KeyAction::ZoomIn));
        assert_eq!(resolve_key_action("minus", 0), Some(KeyAction::ZoomOut));
        assert_eq!(resolve_key_action("f", 0), Some(KeyAction::FitWidth));
        assert_eq!(resolve_key_action("F11", 0), Some(KeyAction::ToggleFullscreen));
        assert_eq!(resolve_key_action("F5", 0), Some(KeyAction::TogglePresentation));
        assert_eq!(
            resolve_key_action("f", gtk::gdk::ModifierType::CONTROL_MASK.bits()),
            Some(KeyAction::FocusSearch)
        );
        assert_eq!(
            resolve_key_action("o", gtk::gdk::ModifierType::CONTROL_MASK.bits()),
            Some(KeyAction::OpenFile)
        );
    }

    #[test]
    fn last_directory_persists_for_the_current_user() {
        let base = std::env::temp_dir().join(format!(
            "gpdfview-last-dir-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let home = base.join("home");
        let documents = home.join("Documents");
        let _ = fs::create_dir_all(&documents);

        remember_last_directory_for_home(&home, &documents);
        assert_eq!(last_directory_for_home(&home), Some(documents));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn newest_valid_recent_document_is_restored_first() {
        let base = std::env::temp_dir().join(format!(
            "gpdfview-resume-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::create_dir_all(&base);

        let newest = base.join("newest.pdf");
        let older = base.join("older.pdf");
        fs::write(&newest, b"pdf").expect("newest pdf file should be created");
        fs::write(&older, b"pdf").expect("older pdf file should be created");

        let result = newest_valid_recent_document(&[newest.clone(), older.clone()]);
        assert_eq!(result, Some(newest));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn selected_pdf_files_are_extracted_from_a_list_model() {
        let files = gio::ListStore::new::<gio::File>();
        files.append(&gio::File::for_path("/tmp/alpha.pdf"));
        files.append(&gio::File::for_path("/tmp/beta.txt"));
        files.append(&gio::File::for_path("/tmp/gamma.pdf"));

        let result = selected_pdf_paths_from_store(&files);
        assert_eq!(
            result,
            vec![
                PathBuf::from("/tmp/alpha.pdf"),
                PathBuf::from("/tmp/gamma.pdf"),
            ]
        );
    }

    #[test]
    fn cairo_search_highlights_invert_y_coordinates() {
        let result = cairo_search_highlight_rects(100.0, 1.0, &[(10.0, 20.0, 50.0, 30.0)]);
        assert_eq!(result, vec![(10.0, 70.0, 50.0, 80.0)]);
    }

    #[test]
    fn page_dimensions_swap_for_90_degree_rotation() {
        assert_eq!(rotated_page_dimensions(612.0, 792.0, 90), (792.0, 612.0));
        assert_eq!(rotated_page_dimensions(612.0, 792.0, 270), (792.0, 612.0));
        assert_eq!(rotated_page_dimensions(612.0, 792.0, 180), (612.0, 792.0));
    }

    #[test]
    fn window_geometry_persists_between_sessions() {
        let base = std::env::temp_dir().join(format!(
            "gpdfview-window-size-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let home = base.join("home");
        let _ = fs::create_dir_all(&home);

        remember_window_geometry_for_home(&home, 1200, 780);
        assert_eq!(last_window_geometry_for_home(&home), Some((1200, 780)));

        let _ = fs::remove_dir_all(&base);
    }
}

fn action_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon_name);
    button.set_tooltip_text(Some(tooltip));
    button.set_has_frame(false);
    button
}

fn should_show_tabs(page_count: u32) -> bool {
    page_count > 0
}

#[derive(Clone)]
struct DocumentTabManager {
    notebook: Option<gtk::Notebook>,
    titles: Rc<RefCell<Vec<String>>>,
}

impl DocumentTabManager {
    fn new(notebook: gtk::Notebook) -> Self {
        Self {
            notebook: Some(notebook),
            titles: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[cfg(test)]
    fn new_model() -> Self {
        Self {
            notebook: None,
            titles: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[cfg(test)]
    fn record_tab(&self, title: &str) {
        self.titles.borrow_mut().push(title.to_string());
    }

    fn close_tab(&self, page: &gtk::Box) -> bool {
        let Some(notebook) = self.notebook.clone() else {
            return false;
        };
        if let Some(page_index) = notebook.page_num(page) {
            notebook.remove_page(Some(page_index));
            notebook.set_show_tabs(should_show_tabs(notebook.n_pages()));
            return true;
        }
        false
    }

    fn add_tab(&self, page: &gtk::Box, title: &str) {
        let label_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        label_box.set_halign(gtk::Align::Center);
        label_box.set_valign(gtk::Align::Center);
        label_box.set_margin_start(8);
        label_box.set_margin_end(8);
        label_box.set_margin_top(2);
        label_box.set_margin_bottom(2);

        let title_label = gtk::Label::new(Some(title));
        title_label.set_margin_start(2);
        title_label.set_margin_end(2);
        label_box.append(&title_label);

        let close_button = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Close tab")
            .has_frame(false)
            .focus_on_click(false)
            .build();
        close_button.set_size_request(16, 16);
        close_button.set_margin_start(2);
        close_button.set_margin_end(2);

        if let Some(notebook) = self.notebook.clone() {
            let page_widget = page.clone();
            let closure_manager = self.clone();
            close_button.connect_clicked(move |_| {
                let _ = closure_manager.close_tab(&page_widget);
            });
            label_box.append(&close_button);

            let index = notebook.append_page(page, Some(&label_box));
            notebook.set_tab_reorderable(page, true);
            notebook.set_current_page(Some(index));
            notebook.set_show_tabs(should_show_tabs(notebook.n_pages()));
        }
        self.titles.borrow_mut().push(title.to_string());
    }

    #[cfg(test)]
    fn tab_count(&self) -> usize {
        self.titles.borrow().len()
    }

    #[cfg(test)]
    fn current_title(&self) -> Option<String> {
        self.titles.borrow().last().cloned()
    }
}

pub fn run() {
    let app = adw::Application::builder()
        .application_id("io.github.eltonfabricio10.gpdfview")
        .build();
    app.connect_startup(|_| {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);
    });
    app.set_flags(gio::ApplicationFlags::HANDLES_OPEN);
    let controller_slot = Rc::new(RefCell::new(None::<Rc<ViewerController>>));

    let activate_slot = Rc::clone(&controller_slot);
    app.connect_activate(move |app| {
        if activate_slot.borrow().is_none() {
            if let Some(controller) = build_ui(app) {
                *activate_slot.borrow_mut() = Some(controller);
            }
        } else if let Some(window) = app.active_window() {
            window.present();
        }
    });

    app.connect_open(move |app, files, _hint| {
        let current = controller_slot.borrow().clone();
        if current.is_none() {
            if let Some(controller) = build_ui(app) {
                *controller_slot.borrow_mut() = Some(controller);
            }
        }
        let Some(file) = files.first() else {
            return;
        };
        let Some(path) = file.path() else {
            if let Some(controller) = controller_slot.borrow().as_ref() {
                controller
                    .status
                    .set_text("Only local PDF files are supported in this prototype.");
            }
            return;
        };
        if let Some(controller) = controller_slot.borrow().as_ref() {
            controller.open_document(path);
        }
    });
    app.run();
}

fn new_worker_pair() -> Result<(Rc<PdfRenderer>, Rc<PrintService>), String> {
    let renderer = PdfRenderer::new().map(Rc::new).map_err(|error| format!("Could not start PDF renderer: {error}"))?;
    let printer = PrintService::new().map(Rc::new).map_err(|error| format!("Could not start print worker: {error}"))?;
    Ok((renderer, printer))
}

fn session_storage_dir_for(home: &Path) -> PathBuf {
    let storage = home.join(".local/share/gpdfview");
    let _ = fs::create_dir_all(&storage);
    storage
}

fn session_storage_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    session_storage_dir_for(Path::new(&home))
}

fn remember_window_geometry_for_home(home: &Path, width: i32, height: i32) {
    let storage = session_storage_dir_for(home);
    let file = storage.join("window_size.txt");
    let _ = fs::write(file, format!("{width} {height}"));
}

fn remember_window_geometry(width: i32, height: i32) {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    remember_window_geometry_for_home(Path::new(&home), width, height);
}

fn last_window_geometry_for_home(home: &Path) -> Option<(i32, i32)> {
    let file = session_storage_dir_for(home).join("window_size.txt");
    let content = fs::read_to_string(file).ok()?;
    let mut parts = content.split_whitespace();
    let width = parts.next()?.parse::<i32>().ok()?;
    let height = parts.next()?.parse::<i32>().ok()?;
    Some((width, height))
}

fn remember_last_directory_for_home(home: &Path, dir: &Path) {
    let storage = session_storage_dir_for(home);
    let _ = fs::create_dir_all(&storage);
    let file = storage.join("last_dir.txt");
    let trimmed = dir.to_string_lossy().trim().to_string();
    if trimmed.is_empty() {
        let _ = fs::remove_file(file);
        return;
    }
    let _ = fs::write(file, trimmed);
}

fn remember_last_directory(dir: &Path) {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    remember_last_directory_for_home(Path::new(&home), dir);
}

fn last_directory_for_home(home: &Path) -> Option<PathBuf> {
    let file = session_storage_dir_for(home).join("last_dir.txt");
    let content = fs::read_to_string(file).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

fn last_directory_for_home_path() -> Option<PathBuf> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    last_directory_for_home(Path::new(&home))
}

fn last_window_geometry_for_home_path() -> Option<(i32, i32)> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    last_window_geometry_for_home(Path::new(&home))
}

fn write_session_entries(path: &Path, entries: &[PathBuf]) {
    let _ = fs::create_dir_all(path.parent().unwrap_or(Path::new(".")));
    let joined = entries
        .iter()
        .filter_map(|entry| {
            let text = entry.to_string_lossy().to_string();
            if text.trim().is_empty() {
                None
            } else {
                Some(text)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let _ = fs::write(path, format!("{joined}\n"));
}

fn selected_pdf_paths(model: &gio::ListModel) -> Vec<PathBuf> {
    let mut selected = Vec::new();
    for index in 0..model.n_items() {
        let Some(item) = model.item(index) else {
            continue;
        };
        let Ok(file) = item.downcast::<gio::File>() else {
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

#[cfg(test)]
fn selected_pdf_paths_from_store(store: &gio::ListStore) -> Vec<PathBuf> {
    let mut selected = Vec::new();
    for index in 0..store.n_items() {
        let Some(item) = store.item(index) else {
            continue;
        };
        let Ok(file) = item.downcast::<gio::File>() else {
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

fn read_session_entries(path: &Path) -> Vec<PathBuf> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

fn normalize_recent_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for path in paths.into_iter().rev() {
        if seen.insert(path.clone()) {
            unique.push(path);
        }
    }
    unique.reverse();
    unique
}

fn remember_recent_document(path: &Path) {
    let file = session_storage_dir().join("recent.txt");
    let mut docs = read_session_entries(&file);
    docs.insert(0, path.to_path_buf());
    let normalized = normalize_recent_paths(docs);
    write_session_entries(&file, &normalized[..normalized.len().min(8)]);
}

fn newest_valid_recent_document(paths: &[PathBuf]) -> Option<PathBuf> {
    paths
        .iter()
        .find(|path| path.exists())
        .cloned()
}

fn load_recent_documents() -> Vec<PathBuf> {
    let file = session_storage_dir().join("recent.txt");
    let docs = read_session_entries(&file);
    normalize_recent_paths(docs)
}

#[allow(dead_code)]
fn toggle_favorite_document(path: &Path) -> bool {
    let file = session_storage_dir().join("favorites.txt");
    let mut favorites = read_session_entries(&file);
    if let Some(index) = favorites.iter().position(|entry| entry == path) {
        favorites.remove(index);
        write_session_entries(&file, &favorites);
        return false;
    }
    favorites.insert(0, path.to_path_buf());
    let deduped = normalize_recent_paths(favorites);
    write_session_entries(&file, &deduped[..deduped.len().min(12)]);
    true
}

fn load_favorite_documents() -> Vec<PathBuf> {
    let file = session_storage_dir().join("favorites.txt");
    normalize_recent_paths(read_session_entries(&file))
}

fn is_favorite_document(path: &Path) -> bool {
    load_favorite_documents().iter().any(|favorite| favorite == path)
}

fn open_document_in_new_tab(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    path: PathBuf,
    title_label: gtk::Label,
    print_button: gtk::Button,
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
    let new_controller = create_document_controller(
        app.clone(),
        window,
        renderer,
        printer,
        tab_manager.clone(),
        Some(path.clone()),
        title_label,
        print_button,
    );
    tab_manager.borrow().add_tab(&new_controller.root, &filename);
    remember_recent_document(&path);
    new_controller.open_document(path);
    Some(new_controller)
}

fn append_sidebar_entry(
    list: &gtk::ListBox,
    path: PathBuf,
    label_text: &str,
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    title_label: gtk::Label,
    print_button: gtk::Button,
) {
    let row = gtk::ListBoxRow::new();
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row_box.set_margin_start(8);
    row_box.set_margin_end(8);
    row_box.set_margin_top(4);
    row_box.set_margin_bottom(4);

    let label = gtk::Label::new(Some(label_text));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    row_box.append(&label);

    let star = gtk::ToggleButton::with_label("★");
    star.set_active(is_favorite_document(&path));
    let star_path = path.clone();
    star.connect_toggled(move |button| {
        let was_favorite = toggle_favorite_document(&star_path);
        button.set_active(was_favorite);
    });
    row_box.append(&star);
    row.set_child(Some(&row_box));

    let row_path = path.clone();
    let row_window = window.clone();
    let row_app = app.clone();
    let row_manager = tab_manager.clone();
    let row_title = title_label.clone();
    let row_print = print_button.clone();
    row.connect_activate(move |_| {
        let _ = open_document_in_new_tab(
            &row_app,
            &row_window,
            row_manager.clone(),
            row_path.clone(),
            row_title.clone(),
            row_print.clone(),
        );
    });

    list.append(&row);
}

fn show_open_dialog(
    app: adw::Application,
    window: adw::ApplicationWindow,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    controller: Rc<ViewerController>,
    last_directory: Option<PathBuf>,
) {
    let dialog = gtk::FileDialog::builder().title("Open PDF").build();
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("PDF documents"));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(Some(&filter));

    if let Some(directory) = last_directory {
        let folder = gio::File::for_path(&directory);
        dialog.set_initial_folder(Some(&folder));
    }

    let dialog_window = window.clone();
    let dialog_parent = dialog_window.clone();
    let dialog_app = app.clone();
    dialog.open_multiple(
        Some(&dialog_parent),
        None::<&gio::Cancellable>,
        move |result| match result {
            Ok(files) => {
                let paths = selected_pdf_paths(&files);
                if paths.is_empty() {
                    controller
                        .status
                        .set_text("No PDF files were selected from the picker.");
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
            Err(error) if error.matches(gio::IOErrorEnum::Cancelled) => {}
            Err(error) => controller
                .status
                .set_text(&format!("Could not open file picker: {error}")),
        },
    );
}

fn build_sidebar(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    title_label: gtk::Label,
    print_button: gtk::Button,
) -> gtk::Box {
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 8);
    sidebar.set_size_request(220, -1);
    sidebar.set_margin_start(8);
    sidebar.set_margin_end(8);
    sidebar.set_margin_top(8);
    sidebar.set_margin_bottom(8);

    let recent_label = gtk::Label::new(Some("Recent"));
    recent_label.set_halign(gtk::Align::Start);
    recent_label.set_markup("<b>Recent</b>");
    sidebar.append(&recent_label);

    let recent_list = gtk::ListBox::new();
    recent_list.set_selection_mode(gtk::SelectionMode::Single);
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
            app,
            window,
            tab_manager.clone(),
            title_label.clone(),
            print_button.clone(),
        );
    }
    sidebar.append(&recent_list);

    let favorites_label = gtk::Label::new(Some("Favorites"));
    favorites_label.set_halign(gtk::Align::Start);
    favorites_label.set_markup("<b>Favorites</b>");
    sidebar.append(&favorites_label);

    let favorites_list = gtk::ListBox::new();
    favorites_list.set_selection_mode(gtk::SelectionMode::Single);
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
            app,
            window,
            tab_manager.clone(),
            title_label.clone(),
            print_button.clone(),
        );
    }
    sidebar.append(&favorites_list);

    sidebar
}

fn build_ui(app: &adw::Application) -> Option<Rc<ViewerController>> {
    let (renderer, printer) = match new_worker_pair() {
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
    let sidebar = build_sidebar(app, &window, tab_manager.clone(), title.clone(), print_button.clone());
    sidebar.set_visible(false);

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.append(&sidebar);
    content.append(&notebook);
    shell.append(&header);
    shell.append(&content);

    let initial = create_document_controller(
        app.clone(),
        &window,
        renderer.clone(),
        printer.clone(),
        tab_manager.clone(),
        None,
        title.clone(),
        print_button.clone(),
    );
    if let Some(path) = newest_valid_recent_document(&load_recent_documents()) {
        initial.open_document(path);
    }
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

fn create_document_controller(
    app: adw::Application,
    window: &adw::ApplicationWindow,
    renderer: Rc<PdfRenderer>,
    printer: Rc<PrintService>,
    tab_manager: Rc<RefCell<DocumentTabManager>>,
    initial_path: Option<PathBuf>,
    title: gtk::Label,
    print_button: gtk::Button,
) -> Rc<ViewerController> {
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
    let zoom_out = action_button("zoom-out-symbolic", "Zoom out");
    let zoom_label = gtk::Label::new(Some("100%"));
    let zoom_in = action_button("zoom-in-symbolic", "Zoom in");
    let fit_width = action_button("zoom-fit-best-symbolic", "Fit width");
    let fit_page = action_button("zoom-fit-best-symbolic", "Fit page");
    let rotate_left = action_button("object-rotate-left-symbolic", "Rotate left");
    let rotate_right = action_button("object-rotate-right-symbolic", "Rotate right");

    toolbar.append(&previous_button);
    toolbar.append(&page_spin);
    toolbar.append(&page_total);
    toolbar.append(&next_button);
    toolbar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    toolbar.append(&search_entry);
    toolbar.append(&search_button);
    toolbar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    toolbar.append(&zoom_out);
    toolbar.append(&zoom_label);
    toolbar.append(&zoom_in);
    toolbar.append(&fit_width);
    toolbar.append(&fit_page);
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
            ..ViewerState::default()
        })),
        cache: Rc::new(RefCell::new(LruCache::new(CACHE_LIMIT_BYTES))),
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

        if let Some(surface) = draw_surface.borrow().as_ref() {
            let x = f64::from((width - surface.width()) / 2).max(16.0);
            let y = f64::from((height - surface.height()) / 2).max(16.0);
            if context.set_source_surface(surface, x, y).is_ok() {
                let _ = context.paint();
            }

            let state = draw_controller.state.borrow();
            if !state.search_highlights.is_empty() {
                context.set_source_rgba(1.0, 0.9, 0.1, 0.45);
                let page_height = state.page_height_points.max(1.0);
                let highlights = cairo_search_highlight_rects(page_height, state.scale, &state.search_highlights);
                for (left, top, right, bottom) in highlights {
                    context.rectangle(x + left, y + top, right - left, bottom - top);
                    context.fill().expect("search highlight should be renderable");
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
    let search_activate_controller = controller.clone();
    controller.search_entry.connect_activate(move |_| {
        search_activate_controller.search_current_page();
    });
    let fit_controller = controller.clone();
    fit_width.connect_clicked(move |_| fit_controller.fit_width());
    let fit_page_controller = controller.clone();
    fit_page.connect_clicked(move |_| fit_page_controller.fit_page());
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
