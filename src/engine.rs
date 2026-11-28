use gtk4::cairo::{Context, Format, ImageSurface};
use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib::translate::ToGlibPtr;
use poppler::{Document, Page};
use std::collections::VecDeque;
use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread;

const RENDER_SCALE_LIMIT: f64 = 4.0;
const PAGE_MEMORY_LIMIT_BYTES: usize = 128 * 1024 * 1024;
const MAX_PENDING_PREFETCHES: usize = 4;
const MAX_PDF_FILE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct RenderRequest {
    pub path: PathBuf,
    pub page_index: i32,
    pub scale: f64,
    pub rotation: i32,
    pub generation: u64,
    pub prefetch: bool,
}

#[derive(Debug)]
pub struct RenderedPage {
    pub generation: u64,
    pub path: PathBuf,
    pub scale: f64,
    pub rotation: i32,
    pub prefetch: bool,
    pub page_index: i32,
    pub page_count: i32,
    pub page_width_points: f64,
    pub page_height_points: f64,
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub pixels: Vec<u8>,
}

#[derive(Debug)]
pub struct RenderError {
    pub generation: u64,
    pub message: String,
}

#[derive(Debug)]
pub enum RenderResult {
    Page(RenderedPage),
    Error(RenderError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineEntry {
    pub title: String,
    pub page_index: i32,
    pub depth: u32,
}

#[cfg(test)]
pub fn document_outline(path: &PathBuf) -> Result<Vec<OutlineEntry>, String> {
    document_outline_cancellable(path, || false)
}

pub fn document_outline_cancellable<F>(
    path: &PathBuf,
    is_cancelled: F,
) -> Result<Vec<OutlineEntry>, String>
where
    F: Fn() -> bool,
{
    let document = load_document(path)?;
    let _guard = crate::poppler_lock::lock()?;
    let mut entries = Vec::new();

    unsafe {
        let root = poppler::ffi::poppler_index_iter_new(document.to_glib_none().0);
        if root.is_null() {
            return Ok(entries);
        }
        collect_outline_entries(root, 0, &mut entries, &is_cancelled);
        poppler::ffi::poppler_index_iter_free(root);
    }

    Ok(entries)
}

#[cfg(test)]
pub fn render_page_thumbnail(
    path: &PathBuf,
    page_index: i32,
    rotation: i32,
    max_edge_px: i32,
) -> Result<ImageSurface, String> {
    render_page_thumbnail_cancellable(path, page_index, rotation, max_edge_px, || false)
}

pub fn render_page_thumbnail_cancellable<F>(
    path: &PathBuf,
    page_index: i32,
    rotation: i32,
    max_edge_px: i32,
    is_cancelled: F,
) -> Result<ImageSurface, String>
where
    F: Fn() -> bool,
{
    let document = load_document(path)?;
    let _guard = crate::poppler_lock::lock()?;
    if is_cancelled() {
        return Err("Thumbnail request was cancelled".into());
    }
    let page_count = document.n_pages();
    if page_index < 0 || page_index >= page_count {
        return Err(format!("Page {} does not exist", page_index + 1));
    }
    let page = document
        .page(page_index)
        .ok_or_else(|| format!("Page {} does not exist", page_index + 1))?;
    let (page_width_points, page_height_points) = page.size();
    let rotation = normalize_rotation(rotation);
    let (rotated_width, rotated_height) =
        rotated_page_dimensions(page_width_points, page_height_points, rotation);

    let scale = thumbnail_scale(rotated_width, rotated_height, max_edge_px)?;
    let width = (rotated_width * scale).ceil() as i32;
    let height = (rotated_height * scale).ceil() as i32;
    if width <= 0 || height <= 0 {
        return Err("PDF page has invalid dimensions".into());
    }

    let surface =
        ImageSurface::create(Format::ARgb32, width, height).map_err(|error| error.to_string())?;
    let context = Context::new(&surface).map_err(|error| error.to_string())?;
    let angle = rotation as f64 * std::f64::consts::PI / 180.0;
    context.translate(width as f64 / 2.0, height as f64 / 2.0);
    context.rotate(angle);
    context.translate(
        -page_width_points * scale / 2.0,
        -page_height_points * scale / 2.0,
    );
    context.scale(scale, scale);
    page.render(&context);
    surface.flush();

    Ok(surface)
}

unsafe fn collect_outline_entries(
    iter: *mut poppler::ffi::PopplerIndexIter,
    depth: u32,
    entries: &mut Vec<OutlineEntry>,
    is_cancelled: &impl Fn() -> bool,
) {
    loop {
        if is_cancelled() {
            break;
        }
        if let Some(entry) = outline_entry_from_iter(iter, depth) {
            entries.push(entry);
        }

        let child = poppler::ffi::poppler_index_iter_get_child(iter);
        if !child.is_null() {
            collect_outline_entries(child, depth + 1, entries, is_cancelled);
            poppler::ffi::poppler_index_iter_free(child);
        }

        if poppler::ffi::poppler_index_iter_next(iter) == 0 {
            break;
        }
    }
}

unsafe fn outline_entry_from_iter(
    iter: *mut poppler::ffi::PopplerIndexIter,
    depth: u32,
) -> Option<OutlineEntry> {
    let action = poppler::ffi::poppler_index_iter_get_action(iter);
    if action.is_null() {
        return None;
    }

    let any = (*action).any;
    let title = if any.title.is_null() {
        "Section".to_string()
    } else {
        let value = CStr::from_ptr(any.title)
            .to_string_lossy()
            .trim()
            .to_string();
        if value.is_empty() {
            "Section".to_string()
        } else {
            value
        }
    };

    let page_index = if any.type_ == poppler::ffi::POPPLER_ACTION_GOTO_DEST {
        let goto_dest = (*action).goto_dest;
        if goto_dest.dest.is_null() {
            None
        } else {
            Some(((*goto_dest.dest).page_num - 1).max(0))
        }
    } else {
        None
    };

    poppler::ffi::poppler_action_free(action);
    page_index.map(|page_index| OutlineEntry {
        title,
        page_index,
        depth,
    })
}

#[allow(dead_code)]
pub fn find_text_in_page(path: &PathBuf, page_index: i32, query: &str) -> Result<usize, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }

    let document = load_document(path)?;
    let page = document
        .page(page_index)
        .ok_or_else(|| format!("Page {} does not exist", page_index + 1))?;
    Ok(page.find_text(trimmed).len())
}

#[allow(dead_code)]
pub fn find_text_rectangles_in_page(
    path: &PathBuf,
    page_index: i32,
    query: &str,
) -> Result<Vec<(f64, f64, f64, f64)>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let document = load_document(path)?;
    let page = document
        .page(page_index)
        .ok_or_else(|| format!("Page {} does not exist", page_index + 1))?;
    Ok(page
        .find_text(trimmed)
        .into_iter()
        .map(|rect| (rect.x1(), rect.y1(), rect.x2(), rect.y2()))
        .collect())
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentTextMatch {
    pub page_index: i32,
    pub rect: (f64, f64, f64, f64),
}

pub struct DocumentSearchResult {
    pub matches: Vec<DocumentTextMatch>,
    pub truncated: bool,
}

const MAX_DOCUMENT_SEARCH_RESULTS: usize = 1_000;

pub fn find_text_rectangles_in_document_cancellable<F, P>(
    path: &PathBuf,
    query: &str,
    options: poppler::FindFlags,
    is_cancelled: F,
    mut on_progress: P,
) -> Result<DocumentSearchResult, String>
where
    F: Fn() -> bool,
    P: FnMut(i32, i32),
{
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(DocumentSearchResult {
            matches: Vec::new(),
            truncated: false,
        });
    }

    let document = load_document(path)?;
    let result = (|| {
        let page_count = {
            let _guard = crate::poppler_lock::lock()?;
            document.n_pages()
        };
        let mut matches = Vec::new();
        let mut truncated = false;
        'pages: for page_index in 0..page_count {
            if is_cancelled() {
                return Ok(DocumentSearchResult {
                    matches,
                    truncated: false,
                });
            }
            let guard = crate::poppler_lock::lock()?;
            if let Some(page) = document.page(page_index) {
                for rect in page.find_text_with_options(trimmed, options) {
                    if matches.len() == MAX_DOCUMENT_SEARCH_RESULTS {
                        truncated = true;
                        break 'pages;
                    }
                    matches.push(DocumentTextMatch {
                        page_index,
                        rect: (rect.x1(), rect.y1(), rect.x2(), rect.y2()),
                    });
                }
                drop(page);
            }
            drop(guard);
            on_progress(page_index + 1, page_count);
        }
        Ok(DocumentSearchResult { matches, truncated })
    })();
    let guard = crate::poppler_lock::lock()?;
    drop(document);
    drop(guard);
    result
}

#[derive(Default)]
struct PendingState {
    request: Option<RenderRequest>,
    prefetches: VecDeque<RenderRequest>,
    stopped: bool,
}

pub struct PdfRenderer {
    pending: Arc<(Mutex<PendingState>, Condvar)>,
    responses: mpsc::Receiver<RenderResult>,
    next_generation: AtomicU64,
}

impl PdfRenderer {
    pub fn new() -> Result<Self, std::io::Error> {
        let pending = Arc::new((Mutex::new(PendingState::default()), Condvar::new()));
        let (response_tx, responses) = mpsc::channel();
        let worker_pending = Arc::clone(&pending);

        thread::Builder::new()
            .name("gpdfview-renderer".into())
            .spawn(move || render_worker(worker_pending, response_tx))
            .map(|_| Self {
                pending,
                responses,
                next_generation: AtomicU64::new(1),
            })
    }

    pub fn invalidate(&self) -> u64 {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let (lock, wake) = &*self.pending;
        if let Ok(mut state) = lock.lock() {
            state.request = None;
            state.prefetches.clear();
            wake.notify_one();
        }
        generation
    }

    pub fn shutdown(&self) {
        let (lock, wake) = &*self.pending;
        if let Ok(mut state) = lock.lock() {
            state.stopped = true;
            state.request = None;
            state.prefetches.clear();
            wake.notify_one();
        }
    }

    pub fn request(&self, path: PathBuf, page_index: i32, scale: f64, rotation: i32) -> u64 {
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let request = RenderRequest {
            path,
            page_index,
            scale: if scale.is_finite() {
                scale.clamp(0.1, RENDER_SCALE_LIMIT)
            } else {
                1.0
            },
            rotation: normalize_rotation(rotation),
            generation,
            prefetch: false,
        };
        let (lock, wake) = &*self.pending;
        let Ok(mut state) = lock.lock() else {
            return generation;
        };
        if state.stopped {
            return generation;
        }
        state.prefetches.clear();
        state.request = Some(request);
        wake.notify_one();
        generation
    }

    pub fn prefetch(
        &self,
        path: PathBuf,
        page_indices: &[i32],
        scale: f64,
        generation: u64,
        rotation: i32,
    ) {
        let scale = if scale.is_finite() {
            scale.clamp(0.1, RENDER_SCALE_LIMIT)
        } else {
            1.0
        };
        let rotation = normalize_rotation(rotation);
        let (lock, wake) = &*self.pending;
        let Ok(mut state) = lock.lock() else {
            return;
        };
        if state.stopped {
            return;
        }
        state
            .prefetches
            .retain(|request| request.generation == generation);
        for &page_index in page_indices {
            if state.prefetches.iter().any(|request| {
                request.path == path
                    && request.page_index == page_index
                    && request.scale == scale
                    && request.rotation == rotation
            }) {
                continue;
            }
            state.prefetches.push_back(RenderRequest {
                path: path.clone(),
                page_index,
                scale,
                rotation,
                generation,
                prefetch: true,
            });
            while state.prefetches.len() > MAX_PENDING_PREFETCHES {
                state.prefetches.pop_front();
            }
        }
        wake.notify_one();
    }

    pub fn try_recv(&self) -> Result<RenderResult, mpsc::TryRecvError> {
        self.responses.try_recv()
    }
}

impl Drop for PdfRenderer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn render_worker(
    pending: Arc<(Mutex<PendingState>, Condvar)>,
    responses: mpsc::Sender<RenderResult>,
) {
    let mut current_path = None;
    let mut current_document: Option<Document> = None;

    loop {
        let request = {
            let (lock, wake) = &*pending;
            let mut state = match lock.lock() {
                Ok(state) => state,
                Err(_) => {
                    drop_document(&mut current_document);
                    return;
                }
            };

            while state.request.is_none() && state.prefetches.is_empty() && !state.stopped {
                state = match wake.wait(state) {
                    Ok(state) => state,
                    Err(_) => {
                        drop_document(&mut current_document);
                        return;
                    }
                };
            }

            if state.stopped {
                drop_document(&mut current_document);
                return;
            }
            state
                .request
                .take()
                .or_else(|| state.prefetches.pop_front())
                .expect("render work checked above")
        };

        let result = if current_path.as_ref() != Some(&request.path) {
            match load_document(&request.path) {
                Ok(document) => {
                    current_path = Some(request.path.clone());
                    drop_document(&mut current_document);
                    current_document = Some(document);
                    render_page(
                        current_document.as_ref().expect("document was just loaded"),
                        &request,
                    )
                }
                Err(message) => Err(message),
            }
        } else if let Some(document) = current_document.as_ref() {
            render_page(document, &request)
        } else {
            Err("PDF document is not open".to_string())
        };

        let response = match result {
            Ok(page) => RenderResult::Page(page),
            Err(message) => RenderResult::Error(RenderError {
                generation: request.generation,
                message,
            }),
        };

        if responses.send(response).is_err() {
            drop_document(&mut current_document);
            return;
        }
    }
}

fn drop_document(document: &mut Option<Document>) {
    if let Some(document) = document.take() {
        let _guard = crate::poppler_lock::lock();
        drop(document);
    }
}

fn load_document(path: &PathBuf) -> Result<Document, String> {
    validate_pdf_path(path)?;
    let _guard = crate::poppler_lock::lock()?;
    let file = gio::File::for_path(path);
    Document::from_file(&file.uri(), None).map_err(|error| error.to_string())
}

pub(crate) fn validate_pdf_path(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!(
            "{} não é um arquivo PDF local legível.",
            path.display()
        ));
    }
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        return Err("O arquivo selecionado não tem extensão PDF.".to_string());
    }
    let metadata = path
        .metadata()
        .map_err(|error| format!("Não foi possível consultar o PDF: {error}"))?;
    if metadata.len() == 0 {
        return Err("O arquivo PDF está vazio.".to_string());
    }
    if metadata.len() > MAX_PDF_FILE_BYTES {
        return Err(format!(
            "O PDF excede o limite operacional de {} MiB.",
            MAX_PDF_FILE_BYTES / (1024 * 1024)
        ));
    }
    Ok(())
}

pub fn document_properties(path: &PathBuf) -> Result<Vec<(String, String)>, String> {
    let document = load_document(path)?;
    let guard = crate::poppler_lock::lock()?;
    let fallback_title = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled PDF")
        .to_string();
    let title = document
        .title()
        .map(|value| value.to_string())
        .unwrap_or_else(|| fallback_title.clone());
    let author = document
        .author()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    let subject = document
        .subject()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    let creator = document
        .creator()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    let producer = document
        .producer()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    let page_count = document.n_pages().to_string();
    let pdf_version = document
        .pdf_version_string()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let mut properties = [
        ("Title", title),
        ("Author", author),
        ("Subject", subject),
        ("Creator", creator),
        ("Producer", producer),
        ("Pages", page_count),
        ("PDF version", pdf_version),
    ]
    .into_iter()
    .filter_map(|(label, value)| {
        let normalized = value.trim();
        if normalized.is_empty() {
            None
        } else {
            Some((label.to_string(), normalized.to_string()))
        }
    })
    .collect::<Vec<_>>();

    if let Ok(metadata) = std::fs::metadata(path) {
        properties.push(("File size".to_string(), format!("{} bytes", metadata.len())));
    }
    properties.push(("File path".to_string(), path.display().to_string()));
    drop(document);
    drop(guard);
    Ok(properties)
}

fn normalize_rotation(rotation: i32) -> i32 {
    let normalized = rotation.rem_euclid(360);
    if normalized == 360 {
        0
    } else {
        normalized
    }
}

fn rotated_page_dimensions(width_points: f64, height_points: f64, rotation: i32) -> (f64, f64) {
    match normalize_rotation(rotation) {
        90 | 270 => (height_points, width_points),
        _ => (width_points, height_points),
    }
}

fn render_page(document: &Document, request: &RenderRequest) -> Result<RenderedPage, String> {
    let _guard = crate::poppler_lock::lock()?;
    let page_count = document.n_pages();
    // A saved page can exceed the new length if the document was replaced.
    let mut request = request.clone();
    if !request.prefetch && page_count > 0 {
        request.page_index = request.page_index.clamp(0, page_count - 1);
    }
    if request.page_index < 0 || request.page_index >= page_count {
        return Err(format!("Page {} does not exist", request.page_index + 1));
    }
    let page = document
        .page(request.page_index)
        .ok_or_else(|| format!("Page {} does not exist", request.page_index + 1))?;
    let (page_width_points, page_height_points) = page.size();
    let (rotated_width, rotated_height) =
        rotated_page_dimensions(page_width_points, page_height_points, request.rotation);
    render_to_pixels(
        &page,
        &request,
        page_count,
        page_width_points,
        page_height_points,
        rotated_width,
        rotated_height,
    )
}

fn render_to_pixels(
    page: &Page,
    request: &RenderRequest,
    page_count: i32,
    page_width_points: f64,
    page_height_points: f64,
    rotated_width: f64,
    rotated_height: f64,
) -> Result<RenderedPage, String> {
    let scale = choose_render_scale(rotated_width, rotated_height, request.scale)?;
    let width = (rotated_width * scale).ceil() as i32;
    let height = (rotated_height * scale).ceil() as i32;
    if width <= 0 || height <= 0 {
        return Err("PDF page has invalid dimensions".into());
    }
    let pixel_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "PDF page dimensions exceed supported limits".to_string())?;
    if pixel_bytes > PAGE_MEMORY_LIMIT_BYTES {
        return Err(format!(
            "PDF page is too large to render safely ({} MiB limit)",
            PAGE_MEMORY_LIMIT_BYTES / (1024 * 1024)
        ));
    }
    let mut surface =
        ImageSurface::create(Format::ARgb32, width, height).map_err(|error| error.to_string())?;
    let context = Context::new(&surface).map_err(|error| error.to_string())?;
    let angle = request.rotation as f64 * std::f64::consts::PI / 180.0;
    context.translate(width as f64 / 2.0, height as f64 / 2.0);
    context.rotate(angle);
    context.translate(
        -page_width_points * scale / 2.0,
        -page_height_points * scale / 2.0,
    );
    context.scale(scale, scale);
    page.render(&context);
    surface.flush();
    drop(context);

    let stride = surface.stride();
    let pixels = surface.data().map_err(|error| error.to_string())?.to_vec();

    Ok(RenderedPage {
        generation: request.generation,
        path: request.path.clone(),
        scale,
        rotation: request.rotation,
        prefetch: request.prefetch,
        page_index: request.page_index,
        page_count,
        page_width_points: rotated_width,
        page_height_points: rotated_height,
        width,
        height,
        stride,
        pixels,
    })
}

fn thumbnail_scale(width: f64, height: f64, max_edge_px: i32) -> Result<f64, String> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("PDF page has invalid dimensions".into());
    }
    // Thumbnails must stay small even for gigantic PDF media boxes.
    let max_edge = f64::from(max_edge_px.clamp(16, 1024));
    let scale = (max_edge / width.max(height)).min(1.0);
    if !scale.is_finite() || scale <= 0.0 {
        return Err("PDF page dimensions exceed supported limits".into());
    }
    Ok(scale)
}

fn choose_render_scale(
    rotated_width_points: f64,
    rotated_height_points: f64,
    requested_scale: f64,
) -> Result<f64, String> {
    if !rotated_width_points.is_finite()
        || !rotated_height_points.is_finite()
        || rotated_width_points <= 0.0
        || rotated_height_points <= 0.0
    {
        return Err("PDF page has invalid dimensions".to_string());
    }

    let area_points = rotated_width_points * rotated_height_points;
    let max_pixels = PAGE_MEMORY_LIMIT_BYTES as f64 / 4.0;
    let max_safe_scale = (max_pixels / area_points).sqrt();
    if !max_safe_scale.is_finite() || max_safe_scale <= 0.0 {
        return Err("PDF page dimensions exceed supported limits".to_string());
    }

    // Keep a small safety margin to avoid ceil/stride edge cases crossing the memory cap.
    let adaptive_scale = (max_safe_scale * 0.98).max(0.01);
    let final_scale = requested_scale.min(adaptive_scale);
    if final_scale <= 0.0 || !final_scale.is_finite() {
        return Err("PDF page is too large to render safely".to_string());
    }

    Ok(final_scale)
}

#[cfg(test)]
mod tests {
    use super::{PdfRenderer, RenderResult};
    use cairo::{Context, PdfSurface};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn thumbnail_dimensions_remain_bounded_for_giant_pages() {
        for (width, height) in [(100_000.0_f64, 100_000.0_f64), (1.0, 1_000_000.0)] {
            let scale = super::thumbnail_scale(width, height, 156).unwrap();
            assert!((width * scale).ceil() <= 156.0);
            assert!((height * scale).ceil() <= 156.0);
        }
        assert!(super::thumbnail_scale(f64::NAN, 100.0, 156).is_err());
        assert!(super::thumbnail_scale(100.0, 0.0, 156).is_err());
    }

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct PdfFixture(PathBuf);

    impl PdfFixture {
        fn two_page() -> Self {
            let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "gpdfview-engine-test-{}-{id}.pdf",
                std::process::id()
            ));
            let surface = PdfSurface::new(612.0, 792.0, &path).expect("create test PDF");
            let context = Context::new(&surface).expect("create PDF context");
            context.move_to(40.0, 60.0);
            context
                .show_text("First fixture page")
                .expect("draw first page");
            context.show_page().expect("finish first fixture page");
            context.move_to(40.0, 60.0);
            context
                .show_text("Second fixture page")
                .expect("draw second page");
            surface.finish();
            Self(path)
        }

        fn large_page() -> Self {
            let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "gpdfview-engine-test-{}-{id}.pdf",
                std::process::id()
            ));
            let surface = PdfSurface::new(10_000.0, 10_000.0, &path).expect("create large PDF");
            let context = Context::new(&surface).expect("create large PDF context");
            context.move_to(40.0, 60.0);
            context.show_text("Large page").expect("draw large page");
            surface.finish();
            Self(path)
        }

        fn invalid() -> Self {
            let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "gpdfview-engine-test-{}-{id}.pdf",
                std::process::id()
            ));
            fs::write(&path, b"not a PDF").expect("create invalid PDF fixture");
            Self(path)
        }
    }

    impl Drop for PdfFixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn finds_text_matches_on_a_pdf_page() {
        let fixture = PdfFixture::two_page();
        let matches = super::find_text_in_page(&fixture.0, 0, "First").expect("search should work");
        assert!(matches > 0, "expected at least one match for 'First'");
    }

    #[test]
    fn thumbnail_render_returns_a_small_surface() {
        let fixture = PdfFixture::two_page();
        let surface = super::render_page_thumbnail(&fixture.0, 0, 0, 120)
            .expect("thumbnail render should succeed");
        assert!(surface.width() > 0);
        assert!(surface.height() > 0);
        assert!(surface.width().max(surface.height()) <= 120);
    }

    #[test]
    fn outline_lookup_is_safe_for_valid_documents() {
        let fixture = PdfFixture::two_page();
        let outline = super::document_outline(&fixture.0).expect("outline lookup should succeed");
        assert!(outline.len() <= 10_000);
    }

    #[test]
    fn document_properties_include_page_count_even_without_metadata() {
        let fixture = PdfFixture::two_page();
        let properties =
            super::document_properties(&fixture.0).expect("document metadata should be readable");
        assert!(
            properties
                .iter()
                .any(|(name, value)| name == "Pages" && value == "2"),
            "expected page count in metadata output: {properties:?}"
        );
    }

    #[test]
    fn loads_and_renders_a_pdf_page_on_the_worker() {
        let fixture = PdfFixture::two_page();
        let renderer = PdfRenderer::new().expect("start worker");
        let generation = renderer.request(fixture.0.clone(), 0, 0.5, 0);
        let deadline = Instant::now() + Duration::from_secs(5);

        loop {
            match renderer.try_recv() {
                Ok(RenderResult::Page(page)) => {
                    assert_eq!(page.generation, generation);
                    assert_eq!(page.page_index, 0);
                    assert_eq!(page.page_count, 2);
                    assert_eq!(page.width, 306);
                    assert_eq!(page.height, 396);
                    assert_eq!(page.pixels.len(), (page.stride * page.height) as usize);
                    break;
                }
                Ok(RenderResult::Error(error)) => panic!("render failed: {}", error.message),
                Err(std::sync::mpsc::TryRecvError::Empty) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("renderer did not return a page: {error}"),
            }
        }
    }

    #[test]
    fn clamps_saved_page_to_replaced_document_length() {
        let fixture = PdfFixture::two_page();
        let renderer = PdfRenderer::new().expect("start worker");
        let generation = renderer.request(fixture.0.clone(), 99, 1.0, 0);
        let deadline = Instant::now() + Duration::from_secs(5);

        loop {
            match renderer.try_recv() {
                Ok(RenderResult::Page(page)) => {
                    assert_eq!(page.generation, generation);
                    assert_eq!(page.page_index, 1);
                    break;
                }
                Ok(RenderResult::Error(error)) => panic!("saved page did not clamp: {error:?}"),
                Err(std::sync::mpsc::TryRecvError::Empty) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("renderer did not return an error: {error}"),
            }
        }
    }

    #[test]
    fn prefetches_an_adjacent_page_without_advancing_the_visible_generation() {
        let fixture = PdfFixture::two_page();
        let renderer = PdfRenderer::new().expect("start worker");
        let generation = renderer.request(fixture.0.clone(), 0, 0.5, 0);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut visible_page_received = false;

        while Instant::now() < deadline {
            match renderer.try_recv() {
                Ok(RenderResult::Page(page)) if !page.prefetch => {
                    visible_page_received = true;
                    break;
                }
                Ok(RenderResult::Error(error)) => panic!("render failed: {}", error.message),
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }
        assert!(visible_page_received, "visible page did not render");

        renderer.prefetch(fixture.0.clone(), &[1], 0.5, generation, 0);
        let mut prefetched_page_received = false;
        while Instant::now() < deadline {
            match renderer.try_recv() {
                Ok(RenderResult::Page(page)) if page.prefetch => {
                    assert_eq!(page.generation, generation);
                    assert_eq!(page.page_index, 1);
                    prefetched_page_received = true;
                    break;
                }
                Ok(RenderResult::Error(error)) => panic!("prefetch failed: {}", error.message),
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }
        assert!(prefetched_page_received, "adjacent page did not prefetch");
    }

    #[test]
    fn coalesces_rapid_navigation_to_the_latest_page_request() {
        let fixture = PdfFixture::two_page();
        let renderer = PdfRenderer::new().expect("start worker");
        renderer.request(fixture.0.clone(), 0, 0.5, 0);
        renderer.request(fixture.0.clone(), 1, 0.5, 0);
        let latest_generation = renderer.request(fixture.0.clone(), 0, 0.5, 0);
        let deadline = Instant::now() + Duration::from_secs(5);

        while Instant::now() < deadline {
            match renderer.try_recv() {
                Ok(RenderResult::Page(page)) if page.generation == latest_generation => {
                    assert_eq!(page.page_index, 0);
                    return;
                }
                Ok(RenderResult::Error(error)) if error.generation == latest_generation => {
                    panic!("latest render failed: {}", error.message);
                }
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }
        panic!("latest page request was not rendered");
    }

    #[test]
    fn auto_scales_a_page_that_exceeds_the_render_memory_limit() {
        let fixture = PdfFixture::large_page();
        let renderer = PdfRenderer::new().expect("start worker");
        let generation = renderer.request(fixture.0.clone(), 0, 4.0, 0);
        let deadline = Instant::now() + Duration::from_secs(5);

        while Instant::now() < deadline {
            match renderer.try_recv() {
                Ok(RenderResult::Page(page)) if page.generation == generation => {
                    assert!(page.scale < 4.0);
                    assert!(page.width > 0);
                    assert!(page.height > 0);
                    let bytes = (page.stride as usize) * (page.height as usize);
                    assert!(bytes <= super::PAGE_MEMORY_LIMIT_BYTES);
                    return;
                }
                Ok(RenderResult::Error(error)) if error.generation == generation => {
                    panic!(
                        "oversized render should have been downscaled: {}",
                        error.message
                    );
                }
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }
        panic!("oversized page request did not return a page");
    }

    #[test]
    fn shutdown_rejects_new_render_work() {
        let fixture = PdfFixture::two_page();
        let renderer = PdfRenderer::new().expect("start worker");
        renderer.shutdown();

        let generation = renderer.request(fixture.0.clone(), 0, 1.0, 0);
        let (_, wake) = &*renderer.pending;
        wake.notify_all();
        thread::sleep(Duration::from_millis(25));

        assert!(renderer.try_recv().is_err());
        let state = renderer.pending.0.lock().expect("renderer state");
        assert!(state.stopped);
        assert!(state.request.is_none());
        assert!(state.prefetches.is_empty());
        assert!(generation > 0);
    }

    #[test]
    fn prefetch_queue_is_bounded() {
        let renderer = PdfRenderer::new().expect("start worker");
        let path = PathBuf::from("/tmp/bounded-prefetch.pdf");
        let generation = renderer.next_generation.fetch_add(1, Ordering::Relaxed);
        renderer.prefetch(path, &[0, 1, 2, 3, 4, 5, 6, 7], 1.0, generation, 0);

        let (lock, wake) = &*renderer.pending;
        let mut state = lock.lock().expect("renderer state");
        state.request = None;
        let queued = state.prefetches.len();
        state.stopped = true;
        state.prefetches.clear();
        wake.notify_all();
        drop(state);

        assert!(queued <= super::MAX_PENDING_PREFETCHES);
    }

    #[test]
    fn rejects_empty_missing_and_non_pdf_paths_before_poppler() {
        let base = std::env::temp_dir().join(format!(
            "gpdfview-path-validation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch available")
                .as_nanos()
        ));
        fs::create_dir_all(&base).expect("create validation directory");
        let missing = base.join("missing.pdf");
        let empty = base.join("empty.pdf");
        let text = base.join("notes.txt");
        fs::write(&empty, b"").expect("create empty PDF");
        fs::write(&text, b"not a PDF").expect("create text file");

        assert!(super::validate_pdf_path(&missing).is_err());
        assert!(super::validate_pdf_path(&empty).is_err());
        assert!(super::validate_pdf_path(&text).is_err());
        assert!(super::validate_pdf_path(&base).is_err());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reports_an_invalid_pdf_as_a_render_error() {
        let fixture = PdfFixture::invalid();
        let renderer = PdfRenderer::new().expect("start worker");
        let generation = renderer.request(fixture.0.clone(), 0, 1.0, 0);
        let deadline = Instant::now() + Duration::from_secs(5);

        while Instant::now() < deadline {
            match renderer.try_recv() {
                Ok(RenderResult::Error(error)) if error.generation == generation => {
                    assert!(!error.message.is_empty());
                    return;
                }
                Ok(RenderResult::Page(page)) if page.generation == generation => {
                    panic!("invalid PDF unexpectedly rendered");
                }
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }
        panic!("invalid PDF did not return an error");
    }
}
