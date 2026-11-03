pub const CONTINUOUS_TOP_MARGIN: f64 = 24.0;
pub const CONTINUOUS_BOTTOM_MARGIN: f64 = 24.0;
pub const CONTINUOUS_SIDE_MARGIN: f64 = 36.0;
pub const CONTINUOUS_PAGE_GAP: f64 = 30.0;

pub(crate) fn cairo_search_highlight_rects(
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

pub(crate) fn compute_fit_width_scale(page_width_points: f64, viewport_width: i32) -> f64 {
    let available_width = f64::from(viewport_width.max(1));
    (available_width / page_width_points).clamp(0.1, 4.0)
}

pub(crate) fn compute_fit_page_scale(
    page_width_points: f64,
    page_height_points: f64,
    viewport_width: i32,
    viewport_height: i32,
) -> f64 {
    let available_width = f64::from(viewport_width.max(1));
    let available_height = f64::from(viewport_height.max(1));
    let width_scale = available_width / page_width_points;
    let height_scale = available_height / page_height_points;
    width_scale.min(height_scale).clamp(0.1, 4.0)
}

pub(crate) fn continuous_page_stride(page_height_points: f64, scale: f64) -> f64 {
    (page_height_points.max(1.0) * scale.max(0.1)) + CONTINUOUS_PAGE_GAP
}

pub(crate) fn continuous_page_top(page_index: i32, page_height_points: f64, scale: f64) -> f64 {
    CONTINUOUS_TOP_MARGIN + f64::from(page_index.max(0)) * continuous_page_stride(page_height_points, scale)
}

pub(crate) fn continuous_content_height(page_height_px: i32, page_count: i32) -> i32 {
    let page_count = page_count.max(1);
    let page_height = f64::from(page_height_px.max(1));
    let total = CONTINUOUS_TOP_MARGIN
        + f64::from(page_count) * page_height
        + f64::from(page_count.saturating_sub(1)) * CONTINUOUS_PAGE_GAP
        + CONTINUOUS_BOTTOM_MARGIN;
    total.ceil() as i32
}
