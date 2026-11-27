use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::rc::Weak;

use crate::app::ViewerController;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TabMetadata {
    title: String,
    path: Option<PathBuf>,
}

#[derive(Clone)]
pub struct DocumentTabManager {
    notebook: Option<gtk4::Notebook>,
    // Metadata is keyed by the stable GTK page identity, not by notebook index.
    // Notebook pages can be reordered without changing the identity of the page.
    tabs: Rc<RefCell<HashMap<usize, TabMetadata>>>,
    controllers: Rc<RefCell<HashMap<usize, Rc<ViewerController>>>>,
    active_controller_hooks: Rc<RefCell<Vec<Rc<dyn Fn(Rc<ViewerController>)>>>>,
    recently_closed: Rc<RefCell<Vec<TabMetadata>>>,
    #[cfg(test)]
    model_tabs: Rc<RefCell<Vec<TabMetadata>>>,
}

pub fn should_show_tabs(page_count: u32) -> bool {
    page_count > 0
}

fn update_active_header(controller: &ViewerController) {
    let path = controller.state.borrow().path.clone();
    controller.print_button.set_sensitive(path.is_some());
    if let Some(path) = path {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF document");
        controller.title.set_text(title);
    } else {
        controller.title.set_text("gpdfview");
    }
}

fn page_key(page: &gtk4::Box) -> usize {
    page.as_ptr() as usize
}

fn remove_page(
    notebook: &gtk4::Notebook,
    page: &gtk4::Widget,
    tabs: &Rc<RefCell<HashMap<usize, TabMetadata>>>,
    controllers: &Rc<RefCell<HashMap<usize, Rc<ViewerController>>>>,
    active_controller_hooks: &Rc<RefCell<Vec<Rc<dyn Fn(Rc<ViewerController>)>>>>,
    recently_closed: &Rc<RefCell<Vec<TabMetadata>>>,
) -> bool {
    let Some(page_index) = notebook.page_num(page) else {
        return false;
    };
    let was_active = notebook
        .current_page()
        .and_then(|index| notebook.nth_page(Some(index)))
        .is_some_and(|current| current.as_ptr() as usize == page.as_ptr() as usize);

    let key = page.as_ptr() as usize;
    if let Some(metadata) = tabs.borrow_mut().remove(&key) {
        recently_closed.borrow_mut().push(metadata);
    }
    let removed_controller = controllers.borrow_mut().remove(&key);
    if let Some(controller) = removed_controller.as_ref() {
        controller.clear_document_after_close();
    }

    notebook.remove_page(Some(page_index));
    notebook.set_show_tabs(should_show_tabs(notebook.n_pages()));
    let active = notebook
        .current_page()
        .and_then(|index| notebook.nth_page(Some(index)))
        .and_then(|page| controllers.borrow().get(&(page.as_ptr() as usize)).cloned());
    if was_active {
        if let Some(controller) = active.as_ref() {
            update_active_header(controller);
        } else if let Some(controller) = removed_controller.as_ref() {
            controller.reset_after_close();
        }
        if let Some(controller) = active.or(removed_controller) {
            for hook in active_controller_hooks.borrow().iter() {
                hook(controller.clone());
            }
        }
    }
    true
}

impl DocumentTabManager {
    pub fn new(notebook: gtk4::Notebook) -> Self {
        let controllers = Rc::new(RefCell::new(HashMap::new()));
        let active_controller_hooks =
            Rc::new(RefCell::new(Vec::<Rc<dyn Fn(Rc<ViewerController>)>>::new()));
        let weak_controllers: Weak<RefCell<HashMap<usize, Rc<ViewerController>>>> =
            Rc::downgrade(&controllers);
        let weak_hooks = Rc::downgrade(&active_controller_hooks);
        notebook.connect_switch_page(move |_, page, _| {
            let Some(controllers) = weak_controllers.upgrade() else {
                return;
            };
            let controller = controllers.borrow().get(&(page.as_ptr() as usize)).cloned();
            if let Some(controller) = controller {
                update_active_header(&controller);
                if let Some(hooks) = weak_hooks.upgrade() {
                    for hook in hooks.borrow().iter() {
                        hook(controller.clone());
                    }
                }
            }
        });
        Self {
            notebook: Some(notebook),
            tabs: Rc::new(RefCell::new(HashMap::new())),
            controllers,
            active_controller_hooks,
            recently_closed: Rc::new(RefCell::new(Vec::new())),
            #[cfg(test)]
            model_tabs: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[cfg(test)]
    pub fn new_model() -> Self {
        Self {
            notebook: None,
            tabs: Rc::new(RefCell::new(HashMap::new())),
            controllers: Rc::new(RefCell::new(HashMap::new())),
            active_controller_hooks: Rc::new(RefCell::new(Vec::new())),
            recently_closed: Rc::new(RefCell::new(Vec::new())),
            model_tabs: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[cfg(test)]
    pub fn record_tab(&self, title: &str) {
        self.model_tabs.borrow_mut().push(TabMetadata {
            title: title.to_string(),
            path: None,
        });
    }

    #[cfg(test)]
    pub fn record_tab_with_path(&self, title: &str, path: PathBuf) {
        self.model_tabs.borrow_mut().push(TabMetadata {
            title: title.to_string(),
            path: Some(path),
        });
    }

    #[cfg(test)]
    pub fn close_last_recorded_tab(&self) -> Option<String> {
        let metadata = self.model_tabs.borrow_mut().pop()?;
        let title = metadata.title.clone();
        self.recently_closed.borrow_mut().push(metadata);
        Some(title)
    }

    #[cfg(test)]
    pub fn recent_closed_count(&self) -> usize {
        self.recently_closed.borrow().len()
    }

    #[cfg(test)]
    pub fn restore_last_closed_tab(&self) -> Option<String> {
        self.recently_closed
            .borrow_mut()
            .pop()
            .map(|metadata| metadata.title)
    }

    pub fn restore_last_closed_path(&self) -> Option<PathBuf> {
        let mut recently_closed = self.recently_closed.borrow_mut();
        let index = recently_closed
            .iter()
            .rposition(|metadata| metadata.path.is_some())?;
        recently_closed.remove(index).path
    }

    pub fn has_restorable_closed_path(&self) -> bool {
        self.recently_closed
            .borrow()
            .iter()
            .any(|metadata| metadata.path.is_some())
    }

    #[allow(dead_code)]
    pub fn add_tab(&self, page: &gtk4::Box, title: &str) {
        self.add_tab_with_path(page, title, None);
    }

    pub fn add_tab_with_path(&self, page: &gtk4::Box, title: &str, path: Option<PathBuf>) {
        self.tabs.borrow_mut().insert(
            page_key(page),
            TabMetadata {
                title: title.to_string(),
                path,
            },
        );

        let Some(notebook) = self.notebook.as_ref() else {
            return;
        };

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

        // Use weak GTK references here. The Notebook owns the tab label and its
        // close button; capturing a strong Notebook/manager reference from the
        // button callback would create a reference cycle.
        let weak_notebook = notebook.downgrade();
        let weak_page = page.downgrade();
        let tabs = Rc::clone(&self.tabs);
        let controllers = Rc::clone(&self.controllers);
        let active_controller_hooks = Rc::clone(&self.active_controller_hooks);
        let recently_closed = Rc::clone(&self.recently_closed);
        close_button.connect_clicked(move |_| {
            let (Some(notebook), Some(page)) = (weak_notebook.upgrade(), weak_page.upgrade())
            else {
                return;
            };
            let _ = remove_page(
                &notebook,
                page.upcast_ref(),
                &tabs,
                &controllers,
                &active_controller_hooks,
                &recently_closed,
            );
        });
        label_box.append(&close_button);

        notebook.append_page(page, Some(&label_box));
        notebook.set_tab_reorderable(page, true);
        notebook.set_current_page(notebook.page_num(page));
        if let Some(controller) = self.controllers.borrow().get(&page_key(page)).cloned() {
            for hook in self.active_controller_hooks.borrow().iter() {
                hook(controller.clone());
            }
        }
        notebook.set_show_tabs(should_show_tabs(notebook.n_pages()));
    }

    pub fn has_open_documents(&self) -> bool {
        self.tabs
            .borrow()
            .values()
            .any(|metadata| metadata.path.is_some())
    }

    pub fn register_controller(&self, page: &gtk4::Box, controller: Rc<ViewerController>) {
        self.controllers
            .borrow_mut()
            .insert(page_key(page), controller.clone());
        let is_current = self.notebook.as_ref().is_some_and(|notebook| {
            notebook
                .current_page()
                .and_then(|index| notebook.nth_page(Some(index)))
                .is_some_and(|current| current.as_ptr() as usize == page.as_ptr() as usize)
        });
        if is_current {
            update_active_header(&controller);
            for hook in self.active_controller_hooks.borrow().iter() {
                hook(controller.clone());
            }
        }
    }

    pub fn subscribe_active_controller<F>(&self, hook: F)
    where
        F: Fn(Rc<ViewerController>) + 'static,
    {
        let hook: Rc<dyn Fn(Rc<ViewerController>)> = Rc::new(hook);
        self.active_controller_hooks.borrow_mut().push(hook.clone());
        if let Some(controller) = self.active_controller() {
            hook(controller);
        }
    }

    pub fn active_controller(&self) -> Option<Rc<ViewerController>> {
        let notebook = self.notebook.as_ref()?;
        let page = notebook.nth_page(Some(notebook.current_page()?))?;
        self.controllers
            .borrow()
            .get(&(page.as_ptr() as usize))
            .cloned()
    }

    #[cfg(test)]
    pub fn tab_count(&self) -> usize {
        self.model_tabs.borrow().len()
    }

    #[cfg(test)]
    pub fn current_title(&self) -> Option<String> {
        self.model_tabs
            .borrow()
            .last()
            .map(|metadata| metadata.title.clone())
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
    fn tab_metadata_keeps_title_and_path_together() {
        let manager = DocumentTabManager::new_model();
        let expected_path = PathBuf::from("/tmp/alpha.pdf");
        manager.record_tab_with_path("Alpha", expected_path.clone());

        let tabs = manager.model_tabs.borrow();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].title, "Alpha");
        assert_eq!(tabs[0].path.as_ref(), Some(&expected_path));
    }

    #[test]
    fn tab_visibility_uses_the_count_of_visible_pages() {
        assert!(!should_show_tabs(0));
        assert!(should_show_tabs(1));
        assert!(should_show_tabs(2));
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
    fn restore_last_closed_path_skips_entries_without_a_document() {
        let manager = DocumentTabManager::new_model();
        let path = PathBuf::from("/tmp/alpha.pdf");
        manager.record_tab_with_path("Alpha", path.clone());
        manager.record_tab("Empty");

        assert_eq!(manager.close_last_recorded_tab(), Some("Empty".to_string()));
        // Add Alpha to the closed stack after Empty, mirroring two independent closes.
        manager
            .recently_closed
            .borrow_mut()
            .push(manager.model_tabs.borrow_mut().remove(0));

        assert_eq!(manager.restore_last_closed_path(), Some(path));
        assert_eq!(manager.recent_closed_count(), 1);
        assert_eq!(manager.restore_last_closed_tab(), Some("Empty".to_string()));
    }

    #[test]
    fn tracks_whether_any_document_is_open() {
        let manager = DocumentTabManager::new_model();
        assert!(!manager.has_open_documents());

        manager.tabs.borrow_mut().insert(
            1,
            TabMetadata {
                title: "Empty".to_string(),
                path: None,
            },
        );
        assert!(!manager.has_open_documents());

        manager.tabs.borrow_mut().insert(
            2,
            TabMetadata {
                title: "Document".to_string(),
                path: Some(PathBuf::from("/tmp/doc.pdf")),
            },
        );
        assert!(manager.has_open_documents());
    }
}
