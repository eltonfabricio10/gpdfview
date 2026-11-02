use crate::cache::LruCache;
use crate::engine::{PdfRenderer, RenderError, RenderResult, RenderedPage};
use crate::printing::PrintService;
use adw::prelude::*;
use cairo::{Format, ImageSurface};
use gtk::{gio, glib};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

const CACHE_LIMIT_BYTES: usize = 128 * 1024 * 1024;
const DEFAULT_ZOOM: f64 = 1.0;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct RenderKey {
    path: PathBuf,
    page_index: i32,
    scale_milli: u32,
}

impl RenderKey {
    fn new(path: PathBuf, page_index: i32, scale: f64) -> Self {
        Self {
            path,
            page_index,
            scale_milli: (scale * 1000.0).round() as u32,
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
    scale: f64,
    generation: u64,
    request_started: Option<Instant>,
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
    page_total: gtk::Label,
    zoom_label: gtk::Label,
    status: gtk::Label,
    print_button: gtk::Button,
    title: gtk::Label,
}

impl ViewerController {
    fn open_document(&self, path: PathBuf) {
        self.cache.borrow_mut().clear();
        *self.surface.borrow_mut() = None;
        self.page_spin.set_sensitive(false);
        self.page_spin.set_value(1.0);
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
            state.scale = DEFAULT_ZOOM;
        }
        self.status.set_text("Loading first page…");
        self.request_render(path, 0, DEFAULT_ZOOM);
    }

    fn navigate_to(&self, page_index: i32) {
        let (page_index, path, scale) = {
            let mut state = self.state.borrow_mut();
            if state.page_count == 0 {
                return;
            }
            state.page_index = page_index.clamp(0, state.page_count - 1);
            (state.page_index, state.path.clone(), state.scale)
        };
        if let Some(path) = path {
            self.request_render(path, page_index, scale);
        }
    }

    fn change_zoom(&self, delta: f64) {
        let (path, page_index, scale) = {
            let mut state = self.state.borrow_mut();
            if state.page_count == 0 {
                return;
            }
            state.scale = (state.scale + delta).clamp(0.1, 4.0);
            (state.path.clone(), state.page_index, state.scale)
        };
        if let Some(path) = path {
            self.request_render(path, page_index, scale);
        }
    }

    fn fit_width(&self) {
        let width = f64::from(self.scrolled.allocated_width().saturating_sub(32).max(1));
        let (path, page_index, scale) = {
            let mut state = self.state.borrow_mut();
            if state.page_width_points <= 0.0 {
                return;
            }
            state.scale = (width / state.page_width_points).clamp(0.1, 4.0);
            (state.path.clone(), state.page_index, state.scale)
        };
        if let Some(path) = path {
            self.request_render(path, page_index, scale);
        }
    }

    fn request_render(&self, path: PathBuf, page_index: i32, scale: f64) {
        let scale = if scale.is_finite() {
            scale.clamp(0.1, 4.0)
        } else {
            DEFAULT_ZOOM
        };
        {
            let mut state = self.state.borrow_mut();
            state.page_index = page_index;
            state.scale = scale;
        }

        let key = RenderKey::new(path.clone(), page_index, scale);
        let generation = self.renderer.invalidate();
        {
            let mut state = self.state.borrow_mut();
            state.generation = generation;
            state.request_started = None;
        }
        self.zoom_label
            .set_text(&format!("{:.0}%", key.scale() * 100.0));

        if let Some(cached) = self.cache.borrow_mut().get(&key).cloned() {
            *self.surface.borrow_mut() = Some(cached);
            self.page_spin.set_value(f64::from(page_index + 1));
            self.area.queue_draw();
            self.status.set_text("Page loaded from memory cache");
            let page_count = self.state.borrow().page_count;
            self.schedule_prefetch(path, page_index, page_count, scale, generation);
            return;
        }

        let generation = self.renderer.request(path, page_index, scale);
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
                || (state.scale * 1000.0).round() as u32 != (page.scale * 1000.0).round() as u32
            {
                return;
            }
        }
        let key = RenderKey::new(page.path.clone(), page.page_index, page.scale);
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
        }
        self.area.set_content_width(page.width.saturating_add(32));
        self.area.set_content_height(page.height.saturating_add(32));
        self.area.queue_draw();
        self.page_spin
            .set_range(1.0, f64::from(page.page_count.max(1)));
        self.page_spin.set_value(f64::from(page.page_index + 1));
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
        let generation = self.state.borrow().generation;
        if let Some(path) = path {
            self.schedule_prefetch(path, page.page_index, page.page_count, scale, generation);
        }
    }

    fn schedule_prefetch(
        &self,
        path: PathBuf,
        page_index: i32,
        page_count: i32,
        scale: f64,
        generation: u64,
    ) {
        let mut pages = Vec::with_capacity(2);
        if page_index > 0 {
            pages.push(page_index - 1);
        }
        if page_index + 1 < page_count {
            pages.push(page_index + 1);
        }
        self.renderer.prefetch(path, &pages, scale, generation);
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

pub fn run() {
    let app = adw::Application::builder()
        .application_id("io.github.eltonfabricio10.gpdfview")
        .build();
    app.set_flags(gio::ApplicationFlags::HANDLES_OPEN);
    let controller_slot = Rc::new(RefCell::new(None::<ViewerController>));

    let activate_slot = Rc::clone(&controller_slot);
    app.connect_activate(move |app| {
        if activate_slot.borrow().is_none()
            && let Some(controller) = build_ui(app)
        {
            *activate_slot.borrow_mut() = Some(controller);
        } else if let Some(window) = app.active_window() {
            window.present();
        }
    });

    app.connect_open(move |app, files, _hint| {
        if controller_slot.borrow().is_none()
            && let Some(controller) = build_ui(app)
        {
            *controller_slot.borrow_mut() = Some(controller);
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

fn build_ui(app: &adw::Application) -> Option<ViewerController> {
    let renderer = match PdfRenderer::new() {
        Ok(renderer) => Rc::new(renderer),
        Err(error) => {
            eprintln!("Could not start PDF renderer: {error}");
            app.quit();
            return None;
        }
    };
    let printer = match PrintService::new() {
        Ok(printer) => Rc::new(printer),
        Err(error) => {
            eprintln!("Could not start print worker: {error}");
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
    let header = gtk::HeaderBar::new();
    let open_button = gtk::Button::with_label("Open");
    open_button.set_tooltip_text(Some("Open a PDF document"));
    header.pack_start(&open_button);
    let print_button = gtk::Button::with_label("Print");
    print_button.set_sensitive(false);
    print_button.set_tooltip_text(Some("Send the current PDF to the default printer"));
    header.pack_end(&print_button);
    let title = gtk::Label::new(Some("gpdfview"));
    header.set_title_widget(Some(&title));

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&header);
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    toolbar.set_margin_top(6);
    toolbar.set_margin_bottom(6);
    toolbar.set_margin_start(12);
    toolbar.set_margin_end(12);

    let previous_button = gtk::Button::with_label("Previous");
    let page_spin = gtk::SpinButton::with_range(1.0, 1.0, 1.0);
    page_spin.set_sensitive(false);
    page_spin.set_width_chars(5);
    let page_total = gtk::Label::new(Some("/ 0"));
    let next_button = gtk::Button::with_label("Next");
    let zoom_out = gtk::Button::with_label("-");
    let zoom_label = gtk::Label::new(Some("100%"));
    let zoom_in = gtk::Button::with_label("+");
    let fit_width = gtk::Button::with_label("Fit width");

    toolbar.append(&previous_button);
    toolbar.append(&page_spin);
    toolbar.append(&page_total);
    toolbar.append(&next_button);
    toolbar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    toolbar.append(&zoom_out);
    toolbar.append(&zoom_label);
    toolbar.append(&zoom_in);
    toolbar.append(&fit_width);
    root.append(&toolbar);

    let drawing_area = gtk::DrawingArea::new();
    drawing_area.set_content_width(640);
    drawing_area.set_content_height(820);
    drawing_area.set_hexpand(true);
    drawing_area.set_vexpand(true);
    let current_surface = Rc::new(RefCell::new(None::<ImageSurface>));
    let draw_surface = Rc::clone(&current_surface);
    drawing_area.set_draw_func(move |_, context, width, height| {
        context.set_source_rgb(0.18, 0.19, 0.21);
        let _ = context.paint();

        if let Some(surface) = draw_surface.borrow().as_ref() {
            let x = f64::from((width - surface.width()) / 2).max(16.0);
            let y = f64::from((height - surface.height()) / 2).max(16.0);
            if context.set_source_surface(surface, x, y).is_ok() {
                let _ = context.paint();
            }
        } else {
            context.set_source_rgb(0.82, 0.83, 0.85);
            context.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
            context.set_font_size(16.0);
            context.move_to(24.0, 40.0);
            let _ = context.show_text("Open a PDF to start reading");
        }
    });

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
    window.set_content(Some(&root));

    let controller = ViewerController {
        renderer,
        printer,
        state: Rc::new(RefCell::new(ViewerState {
            scale: DEFAULT_ZOOM,
            ..ViewerState::default()
        })),
        cache: Rc::new(RefCell::new(LruCache::new(CACHE_LIMIT_BYTES))),
        surface: current_surface,
        area: drawing_area,
        scrolled,
        page_spin,
        page_total,
        zoom_label,
        status,
        print_button,
        title,
    };

    let open_controller = controller.clone();
    let open_window = window.clone();
    open_button.connect_clicked(move |_| {
        let dialog = gtk::FileDialog::builder().title("Open PDF").build();
        let controller = open_controller.clone();
        dialog.open(
            Some(&open_window),
            None::<&gio::Cancellable>,
            move |result| match result {
                Ok(file) => {
                    if let Some(path) = file.path() {
                        controller.open_document(path);
                    } else {
                        controller
                            .status
                            .set_text("The selected document has no local file path.");
                    }
                }
                Err(error) if error.matches(gio::IOErrorEnum::Cancelled) => {}
                Err(error) => controller
                    .status
                    .set_text(&format!("Could not open file picker: {error}")),
            },
        );
    });

    let print_controller = controller.clone();
    let print_window = window.clone();
    let print_button = controller.print_button.clone();
    print_button.connect_clicked(move |_| {
        let dialog = gtk::AlertDialog::builder()
            .message("Send this PDF to the default printer?")
            .detail("The document will be submitted through the system print queue.")
            .buttons(["Cancel", "Print"])
            .cancel_button(0)
            .default_button(1)
            .build();
        let controller = print_controller.clone();
        dialog.choose(
            Some(&print_window),
            None::<&gio::Cancellable>,
            move |response| {
                if matches!(response, Ok(1)) {
                    controller.submit_print();
                }
            },
        );
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
        if spin.is_sensitive() {
            spin_controller.navigate_to(spin.value_as_int() - 1);
        }
    });
    let zoom_out_controller = controller.clone();
    zoom_out.connect_clicked(move |_| zoom_out_controller.change_zoom(-0.1));
    let zoom_in_controller = controller.clone();
    zoom_in.connect_clicked(move |_| zoom_in_controller.change_zoom(0.1));
    let fit_controller = controller.clone();
    fit_width.connect_clicked(move |_| fit_controller.fit_width());

    let poll_controller = controller.clone();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        poll_controller.poll();
        glib::ControlFlow::Continue
    });

    window.present();
    Some(controller)
}
