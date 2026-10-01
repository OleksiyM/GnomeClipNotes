//! Library-only actions on an ephemeral selection. No persisted selection state.
use crate::{
    export::{render_collection, ExportCollection, ExportOptions},
    i18n::{ntrf, tr, trf},
    library_items::{joined_content, SelectedItem},
    library_selection::Selection,
    model::MAX_CONTENT,
    ui, State,
};
use adw::prelude::*;
use std::{collections::BTreeMap, rc::Rc};

pub fn install(
    state: &Rc<State>,
    selection: &Rc<Selection>,
    window: &adw::ApplicationWindow,
    toolbar: &adw::ToolbarView,
    header: &adw::HeaderBar,
    split: &adw::OverlaySplitView,
    new_note: &gtk::Button,
) {
    let select = ui::button("Select");
    select.set_widget_name("library-select");
    header.pack_end(&select);
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bar.set_halign(gtk::Align::Center);
    bar.add_css_class("toolbar");
    ui::margins(&bar, 6);
    bar.set_visible(false);
    bar.set_widget_name("library-selection-bar");
    toolbar.add_bottom_bar(&bar);
    let summary = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    summary.set_halign(gtk::Align::Center);
    let count = gtk::Label::new(None);
    count.set_widget_name("library-selection-count");
    let all = ui::button("Select all");
    all.set_widget_name("library-select-all");
    all.set_tooltip_text(Some(&tr(
        "Select all matching items, including other pages",
    )));
    all.add_css_class("flat");
    let none = ui::button("Deselect all");
    none.set_widget_name("library-deselect-all");
    none.add_css_class("flat");
    summary.append(&count);
    summary.append(&all);
    summary.append(&none);
    bar.append(&summary);
    let operations = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    operations.set_halign(gtk::Align::Center);
    bar.append(&operations);
    let combine = ui::button("Combine");
    let copy = ui::button("Copy");
    let export = ui::button("Export");
    let move_button = ui::button("Move");
    let delete = ui::button("Delete");
    for (button, name) in [
        (&combine, "combine"),
        (&copy, "copy"),
        (&export, "export"),
        (&move_button, "move"),
        (&delete, "delete"),
    ] {
        button.set_widget_name(&format!("library-bulk-{name}"));
        operations.append(button);
        let state = Rc::downgrade(state);
        let selection = Rc::downgrade(selection);
        button.connect_clicked(move |_| {
            if let (Some(state), Some(selection)) = (state.upgrade(), selection.upgrade()) {
                activate(&state, &selection, name);
            }
        });
    }
    let more = gtk::MenuButton::builder()
        .icon_name("view-more-horizontal-symbolic")
        .tooltip_text(tr("More Actions"))
        .visible(false)
        .build();
    more.set_widget_name("library-bulk-more");
    let menu = gio::Menu::new();
    menu.append(Some(&tr("Export…")), Some("bulk.export"));
    menu.append(Some(&tr("Delete…")), Some("bulk.delete"));
    more.set_menu_model(Some(&menu));
    let actions = gio::SimpleActionGroup::new();
    for name in ["export", "delete"] {
        let action = gio::SimpleAction::new(name, None);
        let state = Rc::downgrade(state);
        let selection = Rc::downgrade(selection);
        action.connect_activate(move |_, _| {
            if let (Some(state), Some(selection)) = (state.upgrade(), selection.upgrade()) {
                activate(&state, &selection, name);
            }
        });
        actions.add_action(&action);
    }
    more.insert_action_group("bulk", Some(&actions));
    operations.append(&more);
    let compact =
        adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 960sp").unwrap());
    compact.add_setter(&export, "visible", Some(&false.to_value()));
    compact.add_setter(&delete, "visible", Some(&false.to_value()));
    compact.add_setter(&more, "visible", Some(&true.to_value()));
    compact.add_setter(
        &bar,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    window.add_breakpoint(compact);
    // Keep the existing sidebar breakpoint effective too (only the last matching
    // breakpoint is active in libadwaita).
    let narrow = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 720sp").unwrap());
    narrow.add_setter(&export, "visible", Some(&false.to_value()));
    narrow.add_setter(&delete, "visible", Some(&false.to_value()));
    narrow.add_setter(&more, "visible", Some(&true.to_value()));
    narrow.add_setter(
        &bar,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    narrow.add_setter(new_note, "visible", Some(&false.to_value()));
    narrow.add_setter(split, "collapsed", Some(&true.to_value()));
    narrow.add_setter(split, "show-sidebar", Some(&false.to_value()));
    window.add_breakpoint(narrow);
    {
        let weak = Rc::downgrade(selection);
        select.connect_clicked(move |_| {
            if let Some(s) = weak.upgrade() {
                s.set_multiple(!s.is_multiple());
            }
        });
        let weak = Rc::downgrade(selection);
        all.connect_clicked(move |_| {
            if let Some(s) = weak.upgrade() {
                s.select_all();
            }
        });
        let weak = Rc::downgrade(selection);
        none.connect_clicked(move |_| {
            if let Some(s) = weak.upgrade() {
                s.deselect_all();
            }
        });
    }
    let weak = Rc::downgrade(selection);
    selection.connect_changed(move || {
        if let Some(s) = weak.upgrade() {
            let size = s.ids().len();
            select.set_label(&if s.is_multiple() {
                tr("Cancel")
            } else {
                tr("Select")
            });
            bar.set_visible(s.is_multiple());
            count.set_text(&ntrf(
                "{count} selected",
                "{count} selected",
                size as u64,
                &[],
            ));
            let ready = s.is_ready();
            all.set_sensitive(ready);
            none.set_sensitive(ready && size > 0);
            combine.set_sensitive(ready && size >= 2);
            for widget in [&copy, &export, &move_button, &delete] {
                widget.set_sensitive(ready && size > 0);
            }
            more.set_sensitive(ready && size > 0);
        }
    });
    let weak = Rc::downgrade(selection);
    let state = Rc::downgrade(state);
    selection.connect_action(move |name| {
        if let (Some(state), Some(selection)) = (state.upgrade(), weak.upgrade()) {
            activate(&state, &selection, name);
        }
    });
}

fn protected(state: &State) -> Vec<i64> {
    state
        .editors
        .borrow()
        .iter()
        .filter(|(_, window)| window.upgrade().is_some())
        .map(|(id, _)| *id)
        .chain(state.remote_editors.borrow().keys().copied())
        .collect()
}

fn activate(state: &Rc<State>, selection: &Rc<Selection>, name: &str) {
    if !selection.is_multiple() || !selection.is_ready() {
        return;
    }
    let ids = selection.ids();
    if ids.is_empty() {
        return;
    }
    if name == "combine" {
        let result = state.store.borrow_mut().combine_items(&ids);
        match result {
            Ok((created_id, items)) => {
                let group_id = state
                    .store
                    .borrow()
                    .get(created_id)
                    .map(|item| item.group_id)
                    .unwrap_or(1);
                let destination = if group_id >= 2 {
                    state
                        .store
                        .borrow()
                        .groups()
                        .unwrap_or_default()
                        .into_iter()
                        .find(|group| group.id == group_id)
                        .map(|group| group.name)
                        .unwrap_or_else(|| tr("Notes"))
                } else {
                    tr("Notes")
                };
                selection.set_multiple(false);
                delete_dialog(state, selection, items, Some(destination));
            }
            Err(error) => ui::error(state, &error.to_string()),
        }
        return;
    }
    let result = state.store.borrow_mut().selected_items(&ids);
    let items = match result {
        Ok(items) => items,
        Err(error) => {
            ui::error(state, &error.to_string());
            return;
        }
    };
    match name {
        "copy" => {
            let size = items
                .iter()
                .try_fold((items.len() - 1) * "\n\n---\n\n".len(), |size, selected| {
                    size.checked_add(selected.item.content.len())
                });
            if size.is_none_or(|size| size > MAX_CONTENT) {
                ui::error(state, &tr("The combined text exceeds the 1 MiB clipboard limit. Export the selection instead."));
            } else if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(&joined_content(&items));
                selection.set_multiple(false);
            }
        }
        "delete" => delete_dialog(state, selection, items, None),
        "move" => {
            let summary = collection_summary(state, &items);
            let state_copy = state.clone();
            let selection = selection.clone();
            ui::choose_move(state, true, Some(&summary), move |group| {
                let result = state_copy
                    .store
                    .borrow_mut()
                    .move_selected_items(&items, group);
                match result {
                    Ok(()) => selection.set_multiple(false),
                    Err(error) => ui::error(&state_copy, &error.to_string()),
                }
            });
        }
        "export" => export_items(state, selection, items),
        _ => {}
    }
}

fn collection_summary(state: &State, items: &[SelectedItem]) -> String {
    let mut counts = BTreeMap::<i64, usize>::new();
    for selected in items {
        *counts.entry(selected.item.group_id).or_default() += 1;
    }
    let mut rows = Vec::new();
    for (id, name) in [(0, tr("History")), (1, tr("Notes"))] {
        if let Some(count) = counts.remove(&id) {
            rows.push(trf(
                "{collection}: {count}",
                &[("collection", &name), ("count", &count.to_string())],
            ));
        }
    }
    for group in state.store.borrow().groups().unwrap_or_default() {
        if group.id >= 2 {
            if let Some(count) = counts.remove(&group.id) {
                rows.push(trf(
                    "{collection}: {count}",
                    &[("collection", &group.name), ("count", &count.to_string())],
                ));
            }
        }
    }
    for (id, count) in counts {
        let name = trf("Removed folder {id}", &[("id", &id.to_string())]);
        rows.push(trf(
            "{collection}: {count}",
            &[("collection", &name), ("count", &count.to_string())],
        ));
    }
    rows.join("\n")
}

fn delete_dialog(
    state: &Rc<State>,
    selection: &Rc<Selection>,
    items: Vec<SelectedItem>,
    destination: Option<String>,
) {
    let combined = destination.is_some();
    let heading = if combined {
        ntrf(
            "Combined {count} item into a new Note. Delete original items?",
            "Combined {count} items into a new Note. Delete original items?",
            items.len() as u64,
            &[],
        )
    } else {
        ntrf(
            "Delete {count} selected item?",
            "Delete {count} selected items?",
            items.len() as u64,
            &[],
        )
    };
    let body = destination.map_or_else(
        || tr("This permanently removes the selected items from your local library."),
        |collection| {
            trf(
                "The new note is in {collection}. Deleting the originals cannot be undone.",
                &[("collection", &collection)],
            )
        },
    );
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(format!("{body}\n\n{}", collection_summary(state, &items)))
        .build();
    dialog.set_widget_name("library-bulk-delete-dialog");
    dialog.add_response("keep", &if combined { tr("Keep") } else { tr("Cancel") });
    dialog.add_response("delete", &tr("Delete"));
    dialog.set_default_response(Some("keep"));
    dialog.set_close_response("keep");
    dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    let state_copy = state.clone();
    let selection = selection.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "delete" {
            let result = state_copy
                .store
                .borrow_mut()
                .delete_selected_items(&items, &protected(&state_copy));
            match result {
                Ok(()) => selection.set_multiple(false),
                Err(error) => ui::error(&state_copy, &error.to_string()),
            }
        }
    });
    dialog.present(state.window.borrow().as_ref());
}

fn export_items(state: &Rc<State>, selection: &Rc<Selection>, items: Vec<SelectedItem>) {
    let collection = ExportCollection {
        group_id: -1,
        group_name: tr("Selected Items"),
        group_revision: Vec::new(),
        annotations: items
            .iter()
            .map(|selected| selected.annotation.clone())
            .collect(),
        items: items.into_iter().map(|selected| selected.item).collect(),
        item_revisions: Vec::new(),
    };
    let content = render_collection(
        &collection,
        ExportOptions::default(),
        &glib::TimeZone::local(),
    );
    let parent = state.window.borrow().clone();
    let state = state.clone();
    let selection = selection.clone();
    glib::MainContext::default().spawn_local(async move {
        let picker = gtk::FileDialog::builder()
            .title(tr("Export Selected Items"))
            .initial_name("Selected Notes.md")
            .modal(true)
            .build();
        let file = match picker.save_future(parent.as_ref()).await {
            Ok(file) => file,
            Err(error)
                if error.matches(gtk::DialogError::Dismissed)
                    || error.matches(gtk::DialogError::Cancelled) =>
            {
                return
            }
            Err(error) => {
                ui::error(&state, &error.to_string());
                return;
            }
        };
        match file
            .replace_contents_future(
                content.into_bytes(),
                None,
                false,
                gio::FileCreateFlags::PRIVATE,
            )
            .await
        {
            Ok(_) => selection.set_multiple(false),
            Err((_, error)) => ui::error(&state, &error.to_string()),
        }
    });
}
