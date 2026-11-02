use cairo::{Context, PdfSurface};
use gtk4::gio;
use gtk4::gio::prelude::*;
use poppler::Document;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread;

#[derive(Clone, Copy, Debug)]
pub enum PrintOrientation {
    Portrait,
    Landscape,
}

#[derive(Clone, Debug)]
pub struct PrintOptions {
    pub page_range: Option<(u32, u32)>,
    pub copies: u32,
    pub orientation: PrintOrientation,
    pub scale_percent: u32,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            page_range: None,
            copies: 1,
            orientation: PrintOrientation::Portrait,
            scale_percent: 100,
        }
    }
}

#[derive(Clone, Debug)]
struct PrintRequest {
    path: PathBuf,
    options: PrintOptions,
}

#[derive(Debug)]
pub struct PrintOutcome {
    pub message: String,
}

pub struct PrintService {
    requests: SyncSender<PrintRequest>,
    responses: Receiver<Result<PrintOutcome, String>>,
}

impl PrintService {
    pub fn new() -> io::Result<Self> {
        let (request_tx, request_rx) = mpsc::sync_channel::<PrintRequest>(1);
        let (response_tx, responses) = mpsc::channel();
        thread::Builder::new()
            .name("gpdfview-printer".into())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    if response_tx.send(print_pdf(&request.path, &request.options)).is_err() {
                        return;
                    }
                }
            })?;

        Ok(Self {
            requests: request_tx,
            responses,
        })
    }

    pub fn submit_with_options(&self, path: PathBuf, options: PrintOptions) -> Result<(), String> {
        self.requests
            .try_send(PrintRequest { path, options })
            .map_err(|error| match error {
            TrySendError::Full(_) => "A print job is already running".to_string(),
            TrySendError::Disconnected(_) => "The print worker is unavailable".to_string(),
        })
    }

    pub fn try_recv(&self) -> Result<Result<PrintOutcome, String>, mpsc::TryRecvError> {
        self.responses.try_recv()
    }
}

struct TemporaryPdf(PathBuf);

impl TemporaryPdf {
    fn create() -> Self {
        static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!("gpdfview-print-{}-{id}.pdf", std::process::id())))
    }
}

impl Drop for TemporaryPdf {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn print_pdf(path: &Path, options: &PrintOptions) -> Result<PrintOutcome, String> {
    if !path.is_file() {
        return Err(format!("{} is not a readable PDF file", path.display()));
    }

    match submit_to_cups(path, options) {
        Ok(output) => Ok(PrintOutcome {
            message: format!("PDF sent directly to the default printer. {output}"),
        }),
        Err(direct_error) => {
            let rendered = TemporaryPdf::create();
            render_for_printing(path, &rendered.0, options).map_err(|render_error| {
                format!(
                    "Direct PDF submission failed ({direct_error}); print fallback failed ({render_error})"
                )
            })?;
            let mut fallback_options = options.clone();
            fallback_options.page_range = None;
            let output = submit_to_cups(&rendered.0, &fallback_options).map_err(|fallback_error| {
                format!(
                    "Direct PDF submission failed ({direct_error}); rendered fallback failed ({fallback_error})"
                )
            })?;
            Ok(PrintOutcome {
                message: format!(
                    "Rendered pages and sent the fallback PDF to the default printer. {output}"
                ),
            })
        }
    }
}

fn submit_to_cups(path: &Path, options: &PrintOptions) -> Result<String, String> {
    let mut command = Command::new("lp");
    command.arg("-o").arg("document-format=application/pdf");
    if let Some((start, end)) = options.page_range {
        let from = start.max(1);
        let to = end.max(from);
        command.arg("-P").arg(format!("{from}-{to}"));
    }
    let copies = options.copies.max(1);
    command.arg("-n").arg(copies.to_string());
    let orientation_value = match options.orientation {
        PrintOrientation::Portrait => 3,
        PrintOrientation::Landscape => 4,
    };
    command
        .arg("-o")
        .arg(format!("orientation-requested={orientation_value}"));
    let scale = options.scale_percent.clamp(10, 400);
    command.arg("-o").arg(format!("scaling={scale}"));
    command.arg(path);

    let output = command
        .output()
        .map_err(|error| format!("could not start CUPS lp: {error}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if detail.is_empty() {
            format!("lp exited with {}", output.status)
        } else {
            detail
        })
    }
}

fn render_for_printing(source: &Path, destination: &Path, options: &PrintOptions) -> Result<(), String> {
    let _guard = crate::poppler_lock::lock()?;
    let file = gio::File::for_path(source);
    let document = Document::from_file(&file.uri(), None).map_err(|error| error.to_string())?;
    let page_count = document.n_pages();
    if page_count <= 0 {
        return Err("PDF contains no printable pages".into());
    }

    let (start_page, end_page) = if let Some((start, end)) = options.page_range {
        let start = start.max(1) as i32 - 1;
        let end = end.max(start as u32 + 1) as i32 - 1;
        (start.clamp(0, page_count - 1), end.clamp(0, page_count - 1))
    } else {
        (0, page_count - 1)
    };
    if start_page > end_page {
        return Err("Invalid page range for printing".to_string());
    }

    let scale = f64::from(options.scale_percent.clamp(10, 400)) / 100.0;

    let mut surface: Option<PdfSurface> = None;
    for page_index in start_page..=end_page {
        let page = document
            .page(page_index)
            .ok_or_else(|| format!("Could not load page {}", page_index + 1))?;
        let (width, height) = page.size();
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(format!("Page {} has invalid dimensions", page_index + 1));
        }
        let target_width = width * scale;
        let target_height = height * scale;

        if let Some(surface) = surface.as_mut() {
            surface
                .set_size(target_width, target_height)
                .map_err(|error| error.to_string())?;
        } else {
            surface = Some(
                PdfSurface::new(target_width, target_height, destination)
                    .map_err(|error| error.to_string())?,
            );
        }
        let surface = surface
            .as_ref()
            .ok_or_else(|| "Could not initialize print surface".to_string())?;
        let context = Context::new(surface).map_err(|error| error.to_string())?;
        context.scale(scale, scale);
        page.render_for_printing(&context);
        context.show_page().map_err(|error| error.to_string())?;
    }

    let surface = surface.ok_or_else(|| "PDF contains no printable pages".to_string())?;
    surface.finish();
    surface.status().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{PrintOptions, TemporaryPdf, render_for_printing};
    use crate::poppler_lock;
    use cairo::{Context, PdfSurface};
    use gtk4::gio;
    use gtk4::gio::prelude::*;
    use poppler::Document;

    struct PdfFixture(TemporaryPdf);

    impl PdfFixture {
        fn create() -> Self {
            let path = TemporaryPdf::create();
            let surface = PdfSurface::new(612.0, 792.0, &path.0).expect("create source PDF");
            for (page, (width, height)) in [(612.0, 792.0), (420.0, 600.0), (300.0, 500.0)]
                .into_iter()
                .enumerate()
            {
                if page > 0 {
                    surface
                        .set_size(width, height)
                        .expect("set source page size");
                }
                let context = Context::new(&surface).expect("create source PDF context");
                context.move_to(40.0, 60.0);
                context
                    .show_text(&format!("Printable page {page}"))
                    .expect("draw source page");
                context.show_page().expect("finish source page");
            }
            surface.finish();
            Self(path)
        }
    }

    #[test]
    fn fallback_preserves_all_pages_in_a_bounded_page_by_page_pass() {
        let source = PdfFixture::create();
        let destination = TemporaryPdf::create();

        render_for_printing(&source.0.0, &destination.0, &PrintOptions::default())
            .expect("render print fallback");

        let file = gio::File::for_path(&destination.0);
        let _guard = poppler_lock::lock().expect("lock Poppler in test");
        let document = Document::from_file(&file.uri(), None).expect("open rendered PDF");
        assert_eq!(document.n_pages(), 3);
        assert_eq!(
            document.page(1).expect("second page").size(),
            (420.0, 600.0)
        );
        assert_eq!(document.page(2).expect("third page").size(), (300.0, 500.0));
    }

    #[test]
    fn printing_a_missing_file_returns_a_clear_error() {
        let missing = TemporaryPdf::create();
        let error = super::print_pdf(&missing.0, &PrintOptions::default())
            .expect_err("missing input must fail");

        assert!(error.contains("not a readable PDF file"));
    }
}
