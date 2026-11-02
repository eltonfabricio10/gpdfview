use gtk4 as gtk;
use gtk4::prelude::*;

pub(crate) fn build_document_properties_dialog(
    parent: &impl IsA<gtk::Window>,
    properties: &str,
) -> gtk::Window {
    let dialog = gtk::Window::new();
    dialog.set_title(Some("Document properties"));
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_default_size(420, 220);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_bottom(12);

    let label = gtk::Label::new(Some(properties));
    label.set_xalign(0.0);
    label.set_yalign(0.0);
    label.set_wrap(true);
    label.set_selectable(true);
    content.append(&label);

    let close_button = gtk::Button::with_label("Close");
    let dialog_for_close = dialog.clone();
    close_button.connect_clicked(move |_| {
        dialog_for_close.close();
    });
    content.append(&close_button);
    dialog.set_child(Some(&content));
    dialog
}
