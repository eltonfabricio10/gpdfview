use gtk4 as gtk;
use gtk4::prelude::*;

use crate::app::ViewerController;
use crate::session::{AppSettings, save_app_settings_for_home};

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

pub(crate) fn build_settings_dialog(
    parent: &impl IsA<gtk::Window>,
    settings: AppSettings,
    controller: ViewerController,
) -> gtk::Window {
    let dialog = gtk::Window::new();
    dialog.set_title(Some("Preferences"));
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_default_size(520, 460);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_margin_top(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_bottom(12);

    let title = gtk::Label::new(Some("Configurações do gpdfview"));
    title.set_xalign(0.0);
    title.set_markup("<b>Configurações do gpdfview</b>");
    content.append(&title);

    let layout_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let layout_label = gtk::Label::new(Some("Layout padrão:"));
    layout_label.set_width_chars(18);
    let layout_values = ["single", "continuous", "two-page"];
    let layout_model = gtk::StringList::new(&layout_values);
    let layout_combo = gtk::DropDown::new(Some(layout_model.clone()), None::<gtk::Expression>);
    layout_combo.set_selected(match settings.default_layout.as_str() {
        "continuous" => 1,
        "two-page" => 2,
        _ => 0,
    } as u32);
    layout_row.append(&layout_label);
    layout_row.append(&layout_combo);
    content.append(&layout_row);

    let sync_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let sync_label = gtk::Label::new(Some("Sincronizar split view:"));
    let sync_switch = gtk::Switch::new();
    sync_switch.set_active(settings.split_sync);
    sync_row.append(&sync_label);
    sync_row.append(&sync_switch);
    content.append(&sync_row);

    let zoom_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let zoom_label = gtk::Label::new(Some("Passo de zoom:"));
    let zoom_entry = gtk::Entry::new();
    zoom_entry.set_text(&settings.zoom_step.to_string());
    zoom_row.append(&zoom_label);
    zoom_row.append(&zoom_entry);
    content.append(&zoom_row);

    let shortcut_grid = gtk::Grid::new();
    shortcut_grid.set_row_spacing(6);
    shortcut_grid.set_column_spacing(8);
    shortcut_grid.set_hexpand(true);
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
    content.append(&shortcut_grid);

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
    let layout_values_for_save = layout_values;
    let zoom_entry_for_save = zoom_entry.clone();
    let sync_switch_for_save = sync_switch.clone();
    save_button.connect_clicked(move |_| {
        let mut next_settings = settings_for_save.clone();
        let selected_layout_index = layout_combo_for_save.selected() as usize;
        next_settings.default_layout = layout_values_for_save
            .get(selected_layout_index)
            .copied()
            .unwrap_or("single")
            .to_string();
        next_settings.split_sync = sync_switch_for_save.is_active();
        if let Ok(parsed) = zoom_entry_for_save.text().to_string().parse::<f64>() {
            next_settings.zoom_step = parsed.clamp(0.05, 1.0);
        }
        for ((action, _), entry) in shortcut_actions.iter().zip(shortcut_entries_for_save.iter()) {
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
