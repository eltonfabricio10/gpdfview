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
