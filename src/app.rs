use crate::cache::LruCache;
use crate::dialogs::build_document_properties_dialog;
use crate::engine::{PdfRenderer, RenderError, RenderedPage};
use crate::keyboard::KeyAction;
use crate::navigation::{
    change_zoom, fit_page, fit_width, handle_key_action, navigate_to, rotate_page,
    toggle_presentation_mode,
};
use crate::printing::{PrintOptions, PrintOrientation, PrintService};
use crate::rendering::{poll, present_page, request_render, schedule_prefetch, search_current_page};
use crate::session::{load_app_settings, load_document_view};
#[cfg(test)]
use crate::session::{
    last_directory_for_home, last_window_geometry_for_home, newest_valid_recent_document,
    normalize_recent_paths, remember_last_directory_for_home,
    remember_window_geometry_for_home,
};
use crate::viewer_state::{RenderKey, ViewerState, DEFAULT_ZOOM};
use crate::window::DocumentControllerParams;
use libadwaita as adw;
use adw::prelude::*;
use gtk4 as gtk;
use gtk4::cairo::ImageSurface;
use gtk::gio;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

#[cfg(test)]
use std::fs;

#[cfg(test)]
use crate::keyboard::resolve_key_action;
#[cfg(test)]
use crate::layout::{
    cairo_search_highlight_rects, compute_fit_page_scale, compute_fit_width_scale,
};
#[cfg(test)]
use crate::viewer_state::{apply_render_request, normalize_rotation};


pub(crate) const CACHE_LIMIT_BYTES: usize = 128 * 1024 * 1024;

type StateChangeHook = Box<dyn Fn() + 'static>;

#[cfg(test)]
fn rotated_page_dimensions(width_points: f64, height_points: f64, rotation: i32) -> (f64, f64) {
    match normalize_rotation(rotation) {
        90 | 270 => (height_points, width_points),
        _ => (width_points, height_points),
    }
}

fn normalize_print_page_range(mode: &str, page_count: u32, current_page: u32) -> Option<(u32, u32)> {
    let total_pages = page_count.max(1);
    let current = current_page.clamp(1, total_pages);
    match mode {
        "all" => Some((1, total_pages)),
        "current" => Some((current, current)),
        "range" => Some((1, total_pages)),
        _ => Some((1, total_pages)),
    }
}

#[derive(Clone)]
pub(crate) struct ViewerController {
    pub(crate) renderer: Rc<PdfRenderer>,
    pub(crate) printer: Rc<PrintService>,
    pub(crate) state: Rc<RefCell<ViewerState>>,
    pub(crate) cache: Rc<RefCell<LruCache<RenderKey, ImageSurface>>>,
    pub(crate) surface: Rc<RefCell<Option<ImageSurface>>>,
    pub(crate) area: gtk::DrawingArea,
    pub(crate) scrolled: gtk::ScrolledWindow,
    pub(crate) page_spin: gtk::SpinButton,
    pub(crate) suppress_spin_events: Rc<Cell<bool>>,
    pub(crate) page_total: gtk::Label,
    pub(crate) zoom_label: gtk::Label,
    pub(crate) search_entry: gtk::SearchEntry,
    pub(crate) status: gtk::Label,
    pub(crate) print_button: gtk::Button,
    pub(crate) title: gtk::Label,
    pub(crate) toolbar: gtk::Box,
    pub(crate) window: adw::ApplicationWindow,
    pub(crate) root: gtk::Box,
    pub(crate) state_change_hooks: Rc<RefCell<Vec<StateChangeHook>>>,
}

impl ViewerController {
    pub(crate) fn set_page_spin_value(&self, value: f64) {
        self.suppress_spin_events.set(true);
        self.page_spin.set_value(value);
        self.suppress_spin_events.set(false);
    }

    pub(crate) fn handle_key_action(&self, action: KeyAction) {
        handle_key_action(self, action);
    }

    pub(crate) fn toggle_presentation_mode(&self) {
        toggle_presentation_mode(self);
    }

    pub(crate) fn subscribe_state_changes<F>(&self, hook: F)
    where
        F: Fn() + 'static,
    {
        self.state_change_hooks.borrow_mut().push(Box::new(hook));
    }

    pub(crate) fn notify_state_changed(&self) {
        for hook in self.state_change_hooks.borrow().iter() {
            hook();
        }
    }

    pub(crate) fn open_document(&self, path: PathBuf) {
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
            state.search_query.clear();
            state.search_results.clear();
            state.search_global_index = 0;
            state.search_highlights.clear();
            state.search_index = 0;
            state.pending_search_rect = None;
            if let Some(saved) = load_document_view(&path) {
                state.page_index = saved.page_index.max(0);
                state.scale = crate::viewer_state::sanitize_scale(saved.scale);
                state.rotation = crate::viewer_state::normalize_rotation(saved.rotation);
                state.layout_mode = crate::viewer_state::LayoutMode::from_str(&saved.layout_mode);
            }
        }
        self.status.set_text("Loading first page…");
        self.notify_state_changed();
        let (page_index, scale, rotation) = {
            let state = self.state.borrow();
            (state.page_index, state.scale, state.rotation)
        };
        self.request_render(path, page_index, scale, rotation);
    }

    pub(crate) fn navigate_to(&self, page_index: i32) {
        navigate_to(self, page_index);
        self.notify_state_changed();
    }

    pub(crate) fn change_zoom(&self, delta: f64) {
        change_zoom(self, delta);
        self.notify_state_changed();
    }

    pub(crate) fn rotate_page(&self, delta: i32) {
        rotate_page(self, delta);
        self.notify_state_changed();
    }

    pub(crate) fn fit_width(&self) {
        fit_width(self);
        self.notify_state_changed();
    }

    pub(crate) fn fit_page(&self) {
        fit_page(self);
        self.notify_state_changed();
    }

    pub(crate) fn request_render(&self, path: PathBuf, page_index: i32, scale: f64, rotation: i32) {
        request_render(self, path, page_index, scale, rotation);
    }

    pub(crate) fn present_page(&self, page: RenderedPage) {
        present_page(self, page);
        self.notify_state_changed();
    }

    pub(crate) fn schedule_prefetch(
        &self,
        path: PathBuf,
        page_index: i32,
        page_count: i32,
        scale: f64,
        generation: u64,
        rotation: i32,
    ) {
        schedule_prefetch(self, path, page_index, page_count, scale, generation, rotation);
    }

    pub(crate) fn show_status(&self, message: &str) {
        self.status.set_text(message);
    }

    pub(crate) fn show_document_properties(&self) {
        let Some(path) = self.state.borrow().path.clone() else {
            self.show_status("Open a PDF document before viewing its properties.");
            return;
        };

        match crate::engine::document_properties(&path) {
            Ok(properties) => {
                let dialog = build_document_properties_dialog(&self.window, &properties);
                dialog.present();
            }
            Err(error) => {
                self.show_status(&format!("Could not read PDF properties: {error}"));
            }
        }
    }

    pub(crate) fn show_settings_dialog(&self) {
        let settings = load_app_settings();
        let dialog = crate::dialogs::build_settings_dialog(&self.window, settings, self.clone());
        dialog.present();
    }

    pub(crate) fn show_error(&self, error: RenderError) {
        if error.generation == self.state.borrow().generation {
            self.status
                .set_text(&format!("Could not render PDF: {}", error.message));
        }
    }

    pub(crate) fn poll(&self) {
        poll(self);
    }

    pub(crate) fn search_current_page(&self) {
        search_current_page(self);
        self.notify_state_changed();
    }

    pub(crate) fn focus_global_search_result(&self, index: usize) {
        let (target_page, target_rect, page_highlights, page_match_index, total, current_page, page_height_points, scale) = {
            let state = self.state.borrow();
            let total = state.search_results.len();
            if total == 0 {
                self.status.set_text("No search matches in this document.");
                return;
            }
            let normalized_index = index % total;
            let selected = state.search_results[normalized_index].clone();
            let page_highlights = state
                .search_results
                .iter()
                .filter(|entry| entry.page_index == selected.page_index)
                .map(|entry| entry.rect)
                .collect::<Vec<_>>();
            let page_match_index = page_highlights
                .iter()
                .position(|rect| *rect == selected.rect)
                .unwrap_or(0);
            (
                selected.page_index,
                selected.rect,
                page_highlights,
                page_match_index,
                total,
                state.page_index,
                state.page_height_points,
                state.scale,
            )
        };

        if current_page != target_page {
            self.navigate_to(target_page);
        }

        {
            let mut state = self.state.borrow_mut();
            state.search_global_index = index % total;
            state.search_highlights = page_highlights;
            state.search_index = page_match_index;
            state.pending_search_rect = if current_page == target_page {
                None
            } else {
                Some(target_rect)
            };
        }

        if current_page == target_page {
            self.scroll_to_search_match(target_rect, page_height_points, scale);
        }

        let state = self.state.borrow();
        self.status.set_text(&format!(
            "Match {} of {} in document (page {}).",
            state.search_global_index + 1,
            total,
            target_page + 1,
        ));
        self.area.queue_draw();
        self.notify_state_changed();
    }

    pub(crate) fn scroll_to_search_match(&self, rect: (f64, f64, f64, f64), page_height_points: f64, scale: f64) {
        let target = search_scroll_target(rect, page_height_points, scale);
        let adjustment = self.scrolled.vadjustment();
        let lower = adjustment.lower();
        let upper = adjustment.upper();
        let page_size = adjustment.page_size();
        let max_value = (upper - page_size).max(lower);
        adjustment.set_value((target - 64.0).clamp(lower, max_value));
    }

    pub(crate) fn search_next_match(&self) {
        let total = self.state.borrow().search_results.len();
        if total == 0 {
            self.status.set_text("No search matches in this document.");
            return;
        }
        let next_index = {
            let state = self.state.borrow();
            crate::viewer_state::next_search_index(state.search_global_index, total, 1)
        };
        self.focus_global_search_result(next_index);
    }

    pub(crate) fn search_previous_match(&self) {
        let total = self.state.borrow().search_results.len();
        if total == 0 {
            self.status.set_text("No search matches in this document.");
            return;
        }
        let previous_index = {
            let state = self.state.borrow();
            crate::viewer_state::next_search_index(state.search_global_index, total, -1)
        };
        self.focus_global_search_result(previous_index);
    }

    pub(crate) fn submit_print(&self) {
        self.show_print_options_dialog();
    }

    pub(crate) fn submit_print_with_options(&self, options: PrintOptions) {
        let Some(path) = self.state.borrow().path.clone() else {
            self.status.set_text("Open a PDF document before printing.");
            return;
        };
        match self.printer.submit_with_options(path, options) {
            Ok(()) => {
                self.print_button.set_sensitive(false);
                self.status
                    .set_text("Submitting the PDF to the default printer…");
            }
            Err(error) => self.status.set_text(&format!("Print failed: {error}")),
        }
    }

    #[allow(deprecated)]
    pub(crate) fn show_print_options_dialog(&self) {
        let state = self.state.borrow();
        let page_count = state.page_count.max(1);
        let current_page = state.page_index.clamp(0, page_count - 1) + 1;
        drop(state);

        let dialog = gtk::Dialog::builder()
            .transient_for(&self.window)
            .modal(true)
            .title("Print options")
            .build();
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Print", gtk::ResponseType::Accept);

        let content = dialog.content_area();
        let grid = gtk::Grid::new();
        grid.set_margin_top(12);
        grid.set_margin_bottom(12);
        grid.set_margin_start(12);
        grid.set_margin_end(12);
        grid.set_row_spacing(8);
        grid.set_column_spacing(10);

        let range_label = gtk::Label::new(Some("Print range"));
        range_label.set_halign(gtk::Align::Start);
        let range_mode = gtk::ComboBoxText::new();
        range_mode.append(Some("all"), "All pages");
        range_mode.append(Some("current"), "Current page");
        range_mode.append(Some("range"), "Custom range");
        range_mode.set_active_id(Some("all"));
        grid.attach(&range_label, 0, 0, 1, 1);
        grid.attach(&range_mode, 1, 0, 1, 1);

        let from_label = gtk::Label::new(Some("From page"));
        from_label.set_halign(gtk::Align::Start);
        let from_spin = gtk::SpinButton::with_range(1.0, f64::from(page_count), 1.0);
        from_spin.set_value(1.0);
        grid.attach(&from_label, 0, 1, 1, 1);
        grid.attach(&from_spin, 1, 1, 1, 1);

        let to_label = gtk::Label::new(Some("To page"));
        to_label.set_halign(gtk::Align::Start);
        let to_spin = gtk::SpinButton::with_range(1.0, f64::from(page_count), 1.0);
        to_spin.set_value(f64::from(page_count));
        grid.attach(&to_label, 0, 2, 1, 1);
        grid.attach(&to_spin, 1, 2, 1, 1);

        let copies_label = gtk::Label::new(Some("Copies"));
        copies_label.set_halign(gtk::Align::Start);
        let copies_spin = gtk::SpinButton::with_range(1.0, 99.0, 1.0);
        copies_spin.set_value(1.0);
        grid.attach(&copies_label, 0, 3, 1, 1);
        grid.attach(&copies_spin, 1, 3, 1, 1);

        let orientation_label = gtk::Label::new(Some("Orientation"));
        orientation_label.set_halign(gtk::Align::Start);
        let orientation = gtk::ComboBoxText::new();
        orientation.append(Some("portrait"), "Portrait");
        orientation.append(Some("landscape"), "Landscape");
        orientation.set_active_id(Some("portrait"));
        grid.attach(&orientation_label, 0, 4, 1, 1);
        grid.attach(&orientation, 1, 4, 1, 1);

        let scale_label = gtk::Label::new(Some("Scale (%)"));
        scale_label.set_halign(gtk::Align::Start);
        let scale_spin = gtk::SpinButton::with_range(10.0, 400.0, 5.0);
        scale_spin.set_value(100.0);
        grid.attach(&scale_label, 0, 5, 1, 1);
        grid.attach(&scale_spin, 1, 5, 1, 1);

        let range_mode_for_updates = range_mode.clone();
        let from_spin_for_updates = from_spin.clone();
        let to_spin_for_updates = to_spin.clone();
        range_mode.connect_changed(move |_| {
            let mode = range_mode_for_updates
                .active_id()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "all".to_string());
            let custom_range = mode == "range";
            from_spin_for_updates.set_sensitive(custom_range);
            to_spin_for_updates.set_sensitive(custom_range);
            let (from, to) = normalize_print_page_range(mode.as_str(), page_count as u32, current_page as u32)
                .unwrap_or((1, page_count as u32));
            from_spin_for_updates.set_value(from as f64);
            to_spin_for_updates.set_value(to as f64);
        });
        {
            let mode = range_mode
                .active_id()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "all".to_string());
            let custom_range = mode == "range";
            from_spin.set_sensitive(custom_range);
            to_spin.set_sensitive(custom_range);
            let (from, to) = normalize_print_page_range(mode.as_str(), page_count as u32, current_page as u32)
                .unwrap_or((1, page_count as u32));
            from_spin.set_value(from as f64);
            to_spin.set_value(to as f64);
        }

        content.append(&grid);

        let controller = self.clone();
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                let mode = range_mode
                    .active_id()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "all".to_string());
                let (mut from, mut to) = match mode.as_str() {
                    "all" => (1_u32, page_count as u32),
                    "current" => (current_page as u32, current_page as u32),
                    "range" => (
                        from_spin.value_as_int().max(1) as u32,
                        to_spin.value_as_int().max(1) as u32,
                    ),
                    _ => (1_u32, page_count as u32),
                };
                if mode == "range" && to < from {
                    std::mem::swap(&mut from, &mut to);
                }
                let options = PrintOptions {
                    page_range: Some((from, to)),
                    copies: copies_spin.value_as_int().max(1) as u32,
                    orientation: match orientation.active_id().as_deref() {
                        Some("landscape") => PrintOrientation::Landscape,
                        _ => PrintOrientation::Portrait,
                    },
                    scale_percent: scale_spin.value_as_int().clamp(10, 400) as u32,
                };
                controller.submit_print_with_options(options);
            }
            dialog.close();
        });

        dialog.present();
    }
}

fn search_scroll_target(rect: (f64, f64, f64, f64), page_height_points: f64, scale: f64) -> f64 {
    let y_top_points = page_height_points - rect.1.max(rect.3);
    (y_top_points * scale).max(0.0)
}


#[cfg(test)]
#[allow(clippy::items_after_test_module)]
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
    fn render_request_state_clamps_scale_and_normalizes_rotation() {
        let mut state = ViewerState {
            page_count: 10,
            page_index: 1,
            scale: 0.05,
            rotation: 270,
            ..ViewerState::default()
        };

        let result = apply_render_request(
            &mut state,
            Some(PathBuf::from("/tmp/example.pdf")),
            8,
            5.2,
            450,
        );

        assert_eq!(
            result,
            Some((PathBuf::from("/tmp/example.pdf"), 8, 4.0, 90))
        );
        assert_eq!(state.page_index, 8);
        assert_eq!(state.scale, 4.0);
        assert_eq!(state.rotation, 90);
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
    fn search_scroll_target_converts_from_pdf_to_cairo_space() {
        let rect = (10.0, 20.0, 50.0, 30.0);
        let target = super::search_scroll_target(rect, 100.0, 1.0);
        assert_eq!(target, 70.0);
    }

    #[test]
    fn custom_print_ranges_are_normalized_for_all_current_and_page_selections() {
        assert_eq!(normalize_print_page_range("all", 10, 3), Some((1, 10)));
        assert_eq!(normalize_print_page_range("current", 10, 3), Some((3, 3)));
        assert_eq!(normalize_print_page_range("range", 10, 3), Some((1, 10)));
        assert_eq!(normalize_print_page_range("range", 10, 11), Some((1, 10)));
    }

    #[test]
    fn ctrl_scroll_zoom_steps_in_the_expected_direction() {
        use gtk4::gdk::ModifierType;

        assert_eq!(crate::navigation::scroll_zoom_step(-1.0, ModifierType::CONTROL_MASK), 0.1);
        assert_eq!(crate::navigation::scroll_zoom_step(1.0, ModifierType::CONTROL_MASK), -0.1);
        assert_eq!(crate::navigation::scroll_zoom_step(-1.0, ModifierType::SHIFT_MASK), 0.0);
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

pub(crate) fn build_ui(app: &adw::Application) -> Option<Rc<ViewerController>> {
    crate::window::build_ui(app)
}

pub(crate) fn create_document_controller(
    params: DocumentControllerParams<'_>,
) -> Rc<ViewerController> {
    crate::window::create_document_controller(params)
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

pub(crate) fn new_worker_pair() -> Result<(Rc<PdfRenderer>, Rc<PrintService>), String> {
    let renderer = PdfRenderer::new().map(Rc::new).map_err(|error| format!("Could not start PDF renderer: {error}"))?;
    let printer = PrintService::new().map(Rc::new).map_err(|error| format!("Could not start print worker: {error}"))?;
    Ok((renderer, printer))
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

