use gtk4 as gtk;
use gtk4::prelude::*;

use crate::app::ViewerController;
use crate::session::{save_app_settings_for_home, AppSettings};

pub(crate) fn build_document_properties_dialog(
    parent: &impl IsA<gtk::Window>,
    properties: &[(String, String)],
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

    let rows = gtk::Grid::new();
    rows.set_row_spacing(8);
    rows.set_column_spacing(16);
    rows.set_column_homogeneous(false);
    for (index, (name, value)) in properties.iter().enumerate() {
        let name_label = gtk::Label::new(Some(name));
        name_label.set_xalign(0.0);
        name_label.add_css_class("dim-label");
        let value_label = gtk::Label::new(Some(value));
        value_label.set_xalign(0.0);
        value_label.set_selectable(true);
        value_label.set_wrap(true);
        value_label.set_hexpand(true);
        rows.attach(&name_label, 0, index as i32, 1, 1);
        rows.attach(&value_label, 1, index as i32, 1, 1);
    }
    content.append(&rows);

    let close_button = gtk::Button::with_label("Close");
    let dialog_for_close = dialog.clone();
    close_button.connect_clicked(move |_| {
        dialog_for_close.close();
    });
    content.append(&close_button);
    dialog.set_child(Some(&content));
    dialog
}

pub(crate) fn build_settings_dialog(
    parent: &impl IsA<gtk::Window>,
    settings: AppSettings,
    controller: ViewerController,
) -> gtk::Window {
    let dialog = gtk::Window::new();
    dialog.set_title(Some("Preferences"));
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_default_size(560, 580);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_margin_top(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_bottom(12);

    let title = gtk::Label::new(Some("Configurações do gpdfview"));
    title.set_xalign(0.0);
    title.set_markup("<b>Configurações do gpdfview</b>");
    content.append(&title);

    let settings_sections = gtk::Box::new(gtk::Orientation::Vertical, 12);

    let view_section = gtk::Frame::new(Some("Visualização do documento"));
    let view_options = gtk::Box::new(gtk::Orientation::Vertical, 10);
    view_options.set_margin_top(10);
    view_options.set_margin_bottom(10);
    view_options.set_margin_start(10);
    view_options.set_margin_end(10);

    let layout_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let layout_label = gtk::Label::new(Some("Layout padrão:"));
    layout_label.set_width_chars(18);
    layout_label.set_xalign(0.0);
    let layout_ids = ["single", "continuous", "two-page"];
    let layout_labels = ["Página única", "Contínuo", "Duas páginas"];
    let layout_model = gtk::StringList::new(&layout_labels);
    let layout_combo = gtk::DropDown::new(Some(layout_model.clone()), None::<gtk::Expression>);
    layout_combo.set_selected(
        layout_ids
            .iter()
            .position(|layout| *layout == settings.default_layout)
            .unwrap_or(0) as u32,
    );
    layout_row.append(&layout_label);
    layout_row.append(&layout_combo);
    view_options.append(&layout_row);

    let sync_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let sync_label = gtk::Label::new(Some("Sincronizar painéis divididos"));
    sync_label.set_xalign(0.0);
    sync_label.set_hexpand(true);
    let sync_switch = gtk::Switch::new();
    sync_switch.set_active(settings.split_sync);
    sync_row.append(&sync_label);
    sync_row.append(&sync_switch);
    view_options.append(&sync_row);

    let zoom_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let zoom_label = gtk::Label::new(Some("Incremento do zoom:"));
    zoom_label.set_xalign(0.0);
    let zoom_entry = gtk::Entry::new();
    zoom_entry.set_text(&settings.zoom_step.to_string());
    zoom_row.append(&zoom_label);
    zoom_row.append(&zoom_entry);
    view_options.append(&zoom_row);
    view_section.set_child(Some(&view_options));
    settings_sections.append(&view_section);

    let shortcuts_section = gtk::Frame::new(Some("Atalhos de teclado"));
    let shortcut_grid = gtk::Grid::new();
    shortcut_grid.set_row_spacing(6);
    shortcut_grid.set_column_spacing(8);
    shortcut_grid.set_hexpand(true);
    shortcut_grid.set_margin_top(10);
    shortcut_grid.set_margin_bottom(10);
    shortcut_grid.set_margin_start(10);
    shortcut_grid.set_margin_end(10);
    let shortcut_actions = [
        ("open_file", "Abrir arquivo"),
        ("focus_search", "Procurar"),
        ("next_page", "Próxima página"),
        ("previous_page", "Página anterior"),
        ("toggle_fullscreen", "Tela cheia"),
        ("toggle_presentation", "Apresentação"),
        ("zoom_in", "Zoom +"),
        ("zoom_out", "Zoom -"),
    ];
    let shortcut_entries = shortcut_actions
        .iter()
        .enumerate()
        .map(|(index, (action, label))| {
            let action_label = gtk::Label::new(Some(label));
            action_label.set_xalign(0.0);
            let entry = gtk::Entry::new();
            if let Some(value) = settings.shortcuts.get(*action) {
                entry.set_text(value);
            }
            shortcut_grid.attach(&action_label, 0, index as i32, 1, 1);
            shortcut_grid.attach(&entry, 1, index as i32, 1, 1);
            entry
        })
        .collect::<Vec<_>>();
    shortcuts_section.set_child(Some(&shortcut_grid));
    settings_sections.append(&shortcuts_section);
    content.append(&settings_sections);

    let actions_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions_row.set_halign(gtk::Align::End);
    let cancel_button = gtk::Button::with_label("Cancelar");
    let save_button = gtk::Button::with_label("Salvar");
    save_button.add_css_class("suggested-action");
    actions_row.append(&cancel_button);
    actions_row.append(&save_button);
    content.append(&actions_row);

    let dialog_for_cancel = dialog.clone();
    let dialog_for_save = dialog.clone();
    let controller_for_save = controller.clone();
    let settings_for_save = settings.clone();
    let shortcut_entries_for_save = shortcut_entries.clone();
    let layout_combo_for_save = layout_combo.clone();
    let layout_ids_for_save = layout_ids;
    let zoom_entry_for_save = zoom_entry.clone();
    let sync_switch_for_save = sync_switch.clone();
    save_button.connect_clicked(move |_| {
        let mut next_settings = settings_for_save.clone();
        let selected_layout_index = layout_combo_for_save.selected() as usize;
        next_settings.default_layout = layout_ids_for_save
            .get(selected_layout_index)
            .copied()
            .unwrap_or("single")
            .to_string();
        next_settings.split_sync = sync_switch_for_save.is_active();
        if let Ok(parsed) = zoom_entry_for_save.text().to_string().parse::<f64>() {
            next_settings.zoom_step = parsed.clamp(0.05, 1.0);
        }
        for ((action, _), entry) in shortcut_actions
            .iter()
            .zip(shortcut_entries_for_save.iter())
        {
            let value = entry.text().to_string();
            if !value.trim().is_empty() {
                next_settings.shortcuts.insert((*action).to_string(), value);
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        save_app_settings_for_home(std::path::Path::new(&home), &next_settings);
        controller_for_save.show_status("Configurações salvas.");
        dialog_for_save.close();
    });
    cancel_button.connect_clicked(move |_| {
        dialog_for_cancel.close();
    });

    dialog.set_child(Some(&content));
    dialog
}
