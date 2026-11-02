use cairo::{Context, PdfSurface};
use gtk::gio;
use gtk::gio::prelude::*;
use poppler::Document;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread;

#[derive(Debug)]
pub struct PrintOutcome {
    pub message: String,
}

pub struct PrintService {
    requests: SyncSender<PathBuf>,
    responses: Receiver<Result<PrintOutcome, String>>,
}

impl PrintService {
    pub fn new() -> io::Result<Self> {
        let (request_tx, request_rx) = mpsc::sync_channel::<PathBuf>(1);
        let (response_tx, responses) = mpsc::channel();
        thread::Builder::new()
            .name("gpdfview-printer".into())
            .spawn(move || {
                while let Ok(path) = request_rx.recv() {
                    if response_tx.send(print_pdf(&path)).is_err() {
                        return;
                    }
                }
            })?;

        Ok(Self {
            requests: request_tx,
            responses,
        })
    }

    pub fn submit(&self, path: PathBuf) -> Result<(), String> {
        self.requests.try_send(path).map_err(|error| match error {
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

fn print_pdf(path: &Path) -> Result<PrintOutcome, String> {
    if !path.is_file() {
        return Err(format!("{} is not a readable PDF file", path.display()));
    }

    match submit_to_cups(path) {
        Ok(output) => Ok(PrintOutcome {
            message: format!("PDF sent directly to the default printer. {output}"),
        }),
        Err(direct_error) => {
            let rendered = TemporaryPdf::create();
            render_for_printing(path, &rendered.0).map_err(|render_error| {
                format!(
                    "Direct PDF submission failed ({direct_error}); print fallback failed ({render_error})"
                )
            })?;
            let output = submit_to_cups(&rendered.0).map_err(|fallback_error| {
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

fn submit_to_cups(path: &Path) -> Result<String, String> {
    let output = Command::new("lp")
        .arg("-o")
        .arg("document-format=application/pdf")
        .arg(path)
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

fn render_for_printing(source: &Path, destination: &Path) -> Result<(), String> {
    let _guard = crate::poppler_lock::lock()?;
    let file = gio::File::for_path(source);
    let document = Document::from_file(&file.uri(), None).map_err(|error| error.to_string())?;
    let page_count = document.n_pages();
    if page_count <= 0 {
        return Err("PDF contains no printable pages".into());
    }

    let mut surface: Option<PdfSurface> = None;
    for page_index in 0..page_count {
        let page = document
            .page(page_index)
            .ok_or_else(|| format!("Could not load page {}", page_index + 1))?;
        let (width, height) = page.size();
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(format!("Page {} has invalid dimensions", page_index + 1));
        }

        if let Some(surface) = surface.as_mut() {
            surface
                .set_size(width, height)
                .map_err(|error| error.to_string())?;
        } else {
            surface = Some(
                PdfSurface::new(width, height, destination).map_err(|error| error.to_string())?,
            );
        }
        let surface = surface.as_ref().expect("print surface was created");
        let context = Context::new(surface).map_err(|error| error.to_string())?;
        page.render_for_printing(&context);
        context.show_page().map_err(|error| error.to_string())?;
    }

    let surface = surface.expect("document has at least one page");
    surface.finish();
    surface.status().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{TemporaryPdf, render_for_printing};
    use crate::poppler_lock;
    use cairo::{Context, PdfSurface};
    use gtk::gio;
    use gtk::gio::prelude::*;
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

        render_for_printing(&source.0.0, &destination.0).expect("render print fallback");

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
        let error = super::print_pdf(&missing.0).expect_err("missing input must fail");

        assert!(error.contains("not a readable PDF file"));
    }
}
