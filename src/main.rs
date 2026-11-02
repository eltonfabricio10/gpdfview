mod app;
mod cache;
mod dialogs;
mod document_actions;
mod document_tabs;
mod engine;
mod keyboard;
mod layout;
mod navigation;
mod poppler_lock;
mod printing;
mod rendering;
mod session;
mod sidebar;
mod ui_helpers;
mod viewer_state;
mod window;

fn main() {
    app::run();
}
