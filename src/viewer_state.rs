use std::path::PathBuf;
use std::time::Instant;

pub const DEFAULT_ZOOM: f64 = 1.0;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct RenderKey {
    pub path: PathBuf,
    pub page_index: i32,
    pub scale_milli: u32,
    pub rotation: i32,
}

impl RenderKey {
    pub fn new(path: PathBuf, page_index: i32, scale: f64, rotation: i32) -> Self {
        Self {
            path,
            page_index,
            scale_milli: (scale * 1000.0).round() as u32,
            rotation: normalize_rotation(rotation),
        }
    }

    pub fn scale(&self) -> f64 {
        f64::from(self.scale_milli) / 1000.0
    }
}

#[derive(Default)]
pub struct ViewerState {
    pub path: Option<PathBuf>,
    pub page_index: i32,
    pub page_count: i32,
    pub page_width_points: f64,
    pub page_height_points: f64,
    pub scale: f64,
    pub rotation: i32,
    pub generation: u64,
    pub request_started: Option<Instant>,
    pub search_highlights: Vec<(f64, f64, f64, f64)>,
}

pub fn normalize_rotation(rotation: i32) -> i32 {
    let normalized = rotation.rem_euclid(360);
    if normalized == 360 { 0 } else { normalized }
}

pub fn sanitize_scale(scale: f64) -> f64 {
    if scale.is_finite() {
        scale.clamp(0.1, 4.0)
    } else {
        DEFAULT_ZOOM
    }
}

pub fn apply_render_request(
    state: &mut ViewerState,
    path: Option<PathBuf>,
    page_index: i32,
    scale: f64,
    rotation: i32,
) -> Option<(PathBuf, i32, f64, i32)> {
    let path = path?;
    let normalized_scale = sanitize_scale(scale);
    let normalized_rotation = normalize_rotation(rotation);
    let normalized_page_index = if state.page_count == 0 {
        0
    } else {
        page_index.clamp(0, state.page_count - 1)
    };

    state.page_index = normalized_page_index;
    state.scale = normalized_scale;
    state.rotation = normalized_rotation;
    state.search_highlights.clear();

    Some((path, normalized_page_index, normalized_scale, normalized_rotation))
}
