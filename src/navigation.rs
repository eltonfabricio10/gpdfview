use gtk4::gdk::ModifierType;
use gtk4::prelude::*;

use crate::app::ViewerController;
use crate::keyboard::KeyAction;
use crate::layout::{
    compute_fit_page_scale, compute_fit_width_scale, continuous_page_top,
    CONTINUOUS_BOTTOM_MARGIN, CONTINUOUS_SIDE_MARGIN, CONTINUOUS_TOP_MARGIN,
};
use crate::viewer_state::{apply_render_request, sanitize_scale};

fn zoom_step_from_settings() -> f64 {
    crate::session::load_app_settings().zoom_step.clamp(0.05, 1.0)
}

pub(crate) fn scroll_zoom_step(delta_y: f64, state: ModifierType) -> f64 {
    if !state.contains(ModifierType::CONTROL_MASK) {
        return 0.0;
    }
    let zoom_step = zoom_step_from_settings();
    if delta_y < 0.0 {
        zoom_step
    } else if delta_y > 0.0 {
        -zoom_step
    } else {
        0.0
    }
}

pub(crate) fn handle_key_action(controller: &ViewerController, action: KeyAction) {
    let zoom_step = zoom_step_from_settings();
    match action {
        KeyAction::NextPage => {
            let page = controller.state.borrow().page_index + 1;
            navigate_to(controller, page);
        }
        KeyAction::PreviousPage => {
            let page = controller.state.borrow().page_index - 1;
            navigate_to(controller, page);
        }
        KeyAction::FirstPage => navigate_to(controller, 0),
        KeyAction::LastPage => {
            let page_count = controller.state.borrow().page_count;
            if page_count > 0 {
                navigate_to(controller, page_count - 1);
            }
        }
        KeyAction::ZoomIn => change_zoom(controller, zoom_step),
        KeyAction::ZoomOut => change_zoom(controller, -zoom_step),
        KeyAction::FitWidth => fit_width(controller),
        KeyAction::FitPage => fit_page(controller),
        KeyAction::RotateLeft => rotate_page(controller, -90),
        KeyAction::RotateRight => rotate_page(controller, 90),
        KeyAction::OpenFile => {
            controller.status.set_text("Open a PDF document using the toolbar button or menu action.");
        }
        KeyAction::FocusSearch => {
            controller.search_entry.grab_focus();
            controller.search_entry.select_region(0, -1);
        }
        KeyAction::ToggleFullscreen => toggle_fullscreen(controller),
        KeyAction::TogglePresentation => toggle_presentation_mode(controller),
    }
}

pub(crate) fn toggle_fullscreen(controller: &ViewerController) {
    if controller.window.is_fullscreen() {
        controller.window.unfullscreen();
    } else {
        controller.window.fullscreen();
    }
}

pub(crate) fn toggle_presentation_mode(controller: &ViewerController) {
    let is_visible = controller.toolbar.is_visible();
    controller.toolbar.set_visible(!is_visible);
    controller.status.set_visible(!is_visible);
    if is_visible {
        controller.window.fullscreen();
    } else {
        controller.window.unfullscreen();
    }
}

pub(crate) fn navigate_to(controller: &ViewerController, page_index: i32) {
    let (path, page_index, scale, rotation) = {
        let mut state = controller.state.borrow_mut();
        if state.page_count == 0 {
            return;
        }
        let path = state.path.clone();
        let current_scale = state.scale;
        let current_rotation = state.rotation;
        let target = page_index.clamp(0, state.page_count - 1);
        let render_context = apply_render_request(
            &mut state,
            path,
            target,
            current_scale,
            current_rotation,
        );
        match render_context {
            Some((path, page_index, scale, rotation)) => (path, page_index, scale, rotation),
            None => return,
        }
    };
    controller.request_render(path, page_index, scale, rotation);
}

pub(crate) fn change_zoom(controller: &ViewerController, delta: f64) {
    let (path, page_index, scale, rotation) = {
        let mut state = controller.state.borrow_mut();
        if state.page_count == 0 {
            return;
        }
        let path = state.path.clone();
        let current_page = state.page_index;
        let current_rotation = state.rotation;
        let next_scale = sanitize_scale(state.scale + delta);
        let render_context = apply_render_request(
            &mut state,
            path,
            current_page,
            next_scale,
            current_rotation,
        );
        match render_context {
            Some((path, page_index, scale, rotation)) => (path, page_index, scale, rotation),
            None => return,
        }
    };
    controller.request_render(path, page_index, scale, rotation);
}

pub(crate) fn rotate_page(controller: &ViewerController, delta: i32) {
    let (path, page_index, scale, rotation) = {
        let mut state = controller.state.borrow_mut();
        let path = match state.path.clone() {
            Some(path) => path,
            None => return,
        };
        let current_page = state.page_index;
        let current_scale = state.scale;
        let current_rotation = state.rotation;
        let render_context = apply_render_request(
            &mut state,
            Some(path),
            current_page,
            current_scale,
            current_rotation + delta,
        );
        match render_context {
            Some((path, page_index, scale, rotation)) => (path, page_index, scale, rotation),
            None => return,
        }
    };
    controller.request_render(path, page_index, scale, rotation);
}

pub(crate) fn fit_width(controller: &ViewerController) {
    let base_width = controller.scrolled.width().max(1);
    let (path, page_index, scale, rotation, layout_mode, page_height_points) = {
        let mut state = controller.state.borrow_mut();
        if state.page_width_points <= 0.0 {
            return;
        }
        let path = state.path.clone();
        let current_page = state.page_index;
        let current_rotation = state.rotation;
        let viewport_width = if state.layout_mode == crate::viewer_state::LayoutMode::Continuous {
            base_width
                .saturating_sub((CONTINUOUS_SIDE_MARGIN as i32) * 2)
                .saturating_sub(8)
                .max(1)
        } else {
            base_width.saturating_sub(32).max(1)
        };
        let next_scale = compute_fit_width_scale(state.page_width_points, viewport_width);
        let layout_mode = state.layout_mode;
        let page_height_points = state.page_height_points;
        let render_context = apply_render_request(
            &mut state,
            path,
            current_page,
            next_scale,
            current_rotation,
        );
        match render_context {
            Some((path, page_index, scale, rotation)) => {
                (path, page_index, scale, rotation, layout_mode, page_height_points)
            }
            None => return,
        }
    };
    controller.request_render(path, page_index, scale, rotation);
    if layout_mode == crate::viewer_state::LayoutMode::Continuous && page_height_points > 0.0 {
        let adjustment = controller.scrolled.vadjustment();
        let target = continuous_page_top(page_index, page_height_points, scale);
        let lower = adjustment.lower();
        let upper = adjustment.upper();
        let page_size = adjustment.page_size();
        let max_value = (upper - page_size).max(lower);
        adjustment.set_value(target.clamp(lower, max_value));
    }
}

pub(crate) fn fit_page(controller: &ViewerController) {
    let base_width = controller.scrolled.width().max(1);
    let base_height = controller.scrolled.height().max(1);
    let (path, page_index, scale, rotation, layout_mode, page_height_points) = {
        let mut state = controller.state.borrow_mut();
        if state.page_width_points <= 0.0 || state.page_height_points <= 0.0 {
            return;
        }
        let path = state.path.clone();
        let current_page = state.page_index;
        let current_rotation = state.rotation;
        let (viewport_width, viewport_height) = if state.layout_mode
            == crate::viewer_state::LayoutMode::Continuous
        {
            (
                base_width
                    .saturating_sub((CONTINUOUS_SIDE_MARGIN as i32) * 2)
                    .saturating_sub(8)
                    .max(1),
                base_height
                    .saturating_sub((CONTINUOUS_TOP_MARGIN + CONTINUOUS_BOTTOM_MARGIN) as i32)
                    .saturating_sub(8)
                    .max(1),
            )
        } else {
            (
                base_width.saturating_sub(32).max(1),
                base_height.saturating_sub(56).max(1),
            )
        };
        let next_scale = compute_fit_page_scale(
            state.page_width_points,
            state.page_height_points,
            viewport_width,
            viewport_height,
        );
        let layout_mode = state.layout_mode;
        let page_height_points = state.page_height_points;
        let render_context = apply_render_request(
            &mut state,
            path,
            current_page,
            next_scale,
            current_rotation,
        );
        match render_context {
            Some((path, page_index, scale, rotation)) => {
                (path, page_index, scale, rotation, layout_mode, page_height_points)
            }
            None => return,
        }
    };
    controller.request_render(path, page_index, scale, rotation);
    if layout_mode == crate::viewer_state::LayoutMode::Continuous && page_height_points > 0.0 {
        let adjustment = controller.scrolled.vadjustment();
        let target = continuous_page_top(page_index, page_height_points, scale);
        let lower = adjustment.lower();
        let upper = adjustment.upper();
        let page_size = adjustment.page_size();
        let max_value = (upper - page_size).max(lower);
        adjustment.set_value(target.clamp(lower, max_value));
    }
}
