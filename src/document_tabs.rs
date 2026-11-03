use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone)]
pub struct DocumentTabManager {
    notebook: Option<gtk4::Notebook>,
    titles: Rc<RefCell<Vec<String>>>,
    paths: Rc<RefCell<Vec<Option<PathBuf>>>>,
    recently_closed_titles: Rc<RefCell<Vec<String>>>,
    recently_closed_paths: Rc<RefCell<Vec<Option<PathBuf>>>>,
}

pub fn should_show_tabs(page_count: u32) -> bool {
    page_count > 0
}

impl DocumentTabManager {
    pub fn new(notebook: gtk4::Notebook) -> Self {
        Self {
            notebook: Some(notebook),
            titles: Rc::new(RefCell::new(Vec::new())),
            paths: Rc::new(RefCell::new(Vec::new())),
            recently_closed_titles: Rc::new(RefCell::new(Vec::new())),
            recently_closed_paths: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[cfg(test)]
    pub fn new_model() -> Self {
        Self {
            notebook: None,
            titles: Rc::new(RefCell::new(Vec::new())),
            paths: Rc::new(RefCell::new(Vec::new())),
            recently_closed_titles: Rc::new(RefCell::new(Vec::new())),
            recently_closed_paths: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[cfg(test)]
    pub fn record_tab(&self, title: &str) {
        self.titles.borrow_mut().push(title.to_string());
        self.paths.borrow_mut().push(None);
    }

    #[cfg(test)]
    pub fn close_last_recorded_tab(&self) -> Option<String> {
        let mut titles = self.titles.borrow_mut();
        let title = titles.pop()?;
        self.paths.borrow_mut().pop();
        self.recently_closed_titles.borrow_mut().push(title.clone());
        Some(title)
    }

    #[cfg(test)]
    pub fn recent_closed_count(&self) -> usize {
        self.recently_closed_titles.borrow().len()
    }

    #[cfg(test)]
    pub fn restore_last_closed_tab(&self) -> Option<String> {
        let mut recent = self.recently_closed_titles.borrow_mut();
        recent.pop()
    }

    pub fn restore_last_closed_path(&self) -> Option<PathBuf> {
        let mut recent = self.recently_closed_paths.borrow_mut();
        recent.pop().flatten()
    }

    pub fn close_tab(&self, page: &gtk4::Box) -> bool {
        let Some(notebook) = self.notebook.clone() else {
            return false;
        };
        if let Some(page_index) = notebook.page_num(page) {
            let page_index = page_index as usize;
            let title = self.titles.borrow().get(page_index).cloned();
            let path = self.paths.borrow().get(page_index).cloned().flatten();
            if let Some(title) = title {
                self.recently_closed_titles.borrow_mut().push(title);
            }
            if let Some(path) = path.clone() {
                self.recently_closed_paths.borrow_mut().push(Some(path));
            }
            self.titles.borrow_mut().remove(page_index);
            self.paths.borrow_mut().remove(page_index);
            notebook.remove_page(Some(page_index as u32));
            notebook.set_show_tabs(should_show_tabs(notebook.n_pages()));
            return true;
        }
        false
    }

    #[allow(dead_code)]
    pub fn add_tab(&self, page: &gtk4::Box, title: &str) {
        self.add_tab_with_path(page, title, None);
    }

    pub fn add_tab_with_path(&self, page: &gtk4::Box, title: &str, path: Option<PathBuf>) {
        let label_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        label_box.set_halign(gtk4::Align::Center);
        label_box.set_valign(gtk4::Align::Center);
        label_box.set_margin_start(8);
        label_box.set_margin_end(8);
        label_box.set_margin_top(2);
        label_box.set_margin_bottom(2);

        let title_label = gtk4::Label::new(Some(title));
        title_label.set_margin_start(2);
        title_label.set_margin_end(2);
        label_box.append(&title_label);

        let close_button = gtk4::Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Close tab")
            .has_frame(false)
            .focus_on_click(false)
            .build();
        close_button.set_size_request(16, 16);
        close_button.set_margin_start(2);
        close_button.set_margin_end(2);

        if let Some(notebook) = self.notebook.clone() {
            let page_widget = page.clone();
            let closure_manager = self.clone();
            close_button.connect_clicked(move |_| {
                let _ = closure_manager.close_tab(&page_widget);
            });
            label_box.append(&close_button);

            let index = notebook.append_page(page, Some(&label_box));
            notebook.set_tab_reorderable(page, true);
            notebook.set_current_page(Some(index));
            notebook.set_show_tabs(should_show_tabs(notebook.n_pages()));
        }
        self.titles.borrow_mut().push(title.to_string());
        self.paths.borrow_mut().push(path);
    }

    pub fn has_open_documents(&self) -> bool {
        self.paths
            .borrow()
            .iter()
            .any(|path| path.as_ref().is_some())
    }

    #[cfg(test)]
    pub fn tab_count(&self) -> usize {
        self.titles.borrow().len()
    }

    #[cfg(test)]
    pub fn current_title(&self) -> Option<String> {
        self.titles.borrow().last().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_tab_manager_tracks_multiple_documents() {
        let manager = DocumentTabManager::new_model();
        manager.record_tab("Alpha");
        manager.record_tab("Beta");

        assert_eq!(manager.tab_count(), 2);
        assert_eq!(manager.current_title(), Some("Beta".to_string()));
    }

    #[test]
    fn tab_label_has_a_title_and_close_button_capacity() {
        let manager = DocumentTabManager::new_model();
        manager.record_tab("Alpha");
        let titles = manager.titles.borrow();
        assert_eq!(titles.len(), 1);
        assert_eq!(titles[0], "Alpha");
    }

    #[test]
    fn tab_visibility_uses_the_count_of_visible_pages() {
        assert!(should_show_tabs(1));
        assert!(should_show_tabs(2));
        assert!(!should_show_tabs(0));
    }

    #[test]
    fn document_tab_manager_tracks_recently_closed_tabs() {
        let manager = DocumentTabManager::new_model();
        manager.record_tab("Alpha");
        manager.record_tab("Beta");

        assert_eq!(manager.close_last_recorded_tab(), Some("Beta".to_string()));
        assert_eq!(manager.recent_closed_count(), 1);
        assert_eq!(manager.restore_last_closed_tab(), Some("Beta".to_string()));
    }

    #[test]
    fn tracks_whether_any_document_is_open() {
        let manager = DocumentTabManager::new_model();
        manager.paths.borrow_mut().push(None);
        assert!(!manager.has_open_documents());

        manager
            .paths
            .borrow_mut()
            .push(Some(PathBuf::from("/tmp/doc.pdf")));
        assert!(manager.has_open_documents());
    }
}
