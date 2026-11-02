use std::path::PathBuf;
use std::time::Instant;

pub const DEFAULT_ZOOM: f64 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutMode {
    #[default]
    SinglePage,
    Continuous,
    TwoPage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SplitViewMode {
    #[default]
    Off,
    Horizontal,
    Vertical,
}

impl LayoutMode {
    pub fn toggle(self) -> Self {
        match self {
            Self::SinglePage => Self::Continuous,
            Self::Continuous => Self::TwoPage,
            Self::TwoPage => Self::SinglePage,
        }
    }

    pub fn next(self) -> Self {
        self.toggle()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SinglePage => "single",
            Self::Continuous => "continuous",
            Self::TwoPage => "two-page",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "continuous" => Self::Continuous,
            "two-page" => Self::TwoPage,
            _ => Self::SinglePage,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct RenderKey {
    pub path: PathBuf,
    pub page_index: i32,
    pub scale_milli: u32,
    pub rotation: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchOccurrence {
    pub page_index: i32,
    pub rect: (f64, f64, f64, f64),
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
    pub layout_mode: LayoutMode,
    pub split_mode: SplitViewMode,
    pub split_secondary_page_index: i32,
    pub split_sync: bool,
    pub search_query: String,
    pub search_results: Vec<SearchOccurrence>,
    pub search_global_index: usize,
    pub search_highlights: Vec<(f64, f64, f64, f64)>,
    pub search_index: usize,
    pub pending_search_rect: Option<(f64, f64, f64, f64)>,
}

impl ViewerState {
    pub fn sync_split_view_to_primary(&mut self) {
        let page_count = self.page_count.max(1);
        self.split_secondary_page_index = self.page_index.clamp(0, page_count - 1);
    }

    pub fn split_pages(&self) -> Vec<i32> {
        if self.split_mode == SplitViewMode::Off || self.page_count <= 0 {
            return vec![self.page_index];
        }
        let primary = self.page_index.clamp(0, self.page_count - 1);
        let secondary = self
            .split_secondary_page_index
            .clamp(0, self.page_count - 1);
        if primary == secondary {
            vec![primary]
        } else {
            vec![primary, secondary]
        }
    }
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

pub fn next_search_index(current: usize, total: usize, direction: i32) -> usize {
    if total == 0 {
        return 0;
    }
    let current = current as i32;
    let total = total as i32;
    ((current + direction).rem_euclid(total)) as usize
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

#[cfg(test)]
mod tests {
    use super::{LayoutMode, ViewerState};

    #[test]
    fn layout_mode_cycles_across_single_continuous_and_two_page() {
        assert_eq!(LayoutMode::SinglePage.toggle(), LayoutMode::Continuous);
        assert_eq!(LayoutMode::Continuous.toggle(), LayoutMode::TwoPage);
        assert_eq!(LayoutMode::TwoPage.toggle(), LayoutMode::SinglePage);
    }

    #[test]
    fn viewer_defaults_to_single_page_layout() {
        let state = ViewerState::default();
        assert_eq!(state.layout_mode, LayoutMode::SinglePage);
    }

    #[test]
    fn search_index_wraps_around_for_next_and_previous_results() {
        assert_eq!(super::next_search_index(0, 3, 1), 1);
        assert_eq!(super::next_search_index(2, 3, 1), 0);
        assert_eq!(super::next_search_index(0, 3, -1), 2);
    }

    #[test]
    fn split_view_keeps_secondary_page_in_sync_when_enabled() {
        let mut state = ViewerState {
            page_count: 4,
            page_index: 1,
            split_mode: super::SplitViewMode::Horizontal,
            split_secondary_page_index: 2,
            ..ViewerState::default()
        };

        let pages = state.split_pages();
        assert_eq!(pages, vec![1, 2]);

        state.sync_split_view_to_primary();
        assert_eq!(state.split_secondary_page_index, 1);
    }
}
