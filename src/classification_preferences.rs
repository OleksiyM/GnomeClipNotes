use crate::{
    i18n::{mark, ntrf, tr},
    preferences, store, ui, State,
};
use adw::prelude::*;
use std::{cell::Cell, rc::Rc};

pub fn page(state: &Rc<State>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(tr("Comments & Categories"))
        .name("categories")
        .icon_name("view-list-symbolic")
        .build();
    let enable_group = adw::PreferencesGroup::new();
    let comments = adw::SwitchRow::builder()
        .title(tr("Use comments in Library"))
        .subtitle(tr(
            "Add a personal comment to any item, with or without a category.",
        ))
        .active(state.store.borrow().settings.comments_enabled)
        .build();
    comments.set_widget_name("comments-enabled");
    enable_group.add(&comments);
    {
        let state = Rc::downgrade(state);
        let guard = Cell::new(false);
        comments.connect_active_notify(move |row| {
            if guard.get() {
                return;
            }
            if let Some(state) = state.upgrade() {
                let value = row.is_active();
                if !preferences::save_change(&state, |settings| settings.comments_enabled = value) {
                    guard.set(true);
                    row.set_active(!value);
                    guard.set(false);
                }
            }
        });
    }
    let enabled = state.store.borrow().settings.classification_enabled;
    let toggle = adw::SwitchRow::builder()
        .title(tr("Use categories in Library"))
        .subtitle(tr(
            "Organize items with categories and subcategories, independently of folders.",
        ))
        .active(enabled)
        .build();
    toggle.set_widget_name("classification-enabled");
    enable_group.add(&toggle);
    page.add(&enable_group);
    let group = adw::PreferencesGroup::builder().title(tr("Your categories"))
        .description(tr("For example: a project as a category, with Research, Ideas and Decisions as subcategories."))
        .visible(enabled).build();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    list.set_widget_name("classification-categories");
    list.add_css_class("boxed-list");
    group.add(&list);
    let add = gtk::Button::builder()
        .label(tr("Add Category…"))
        .valign(gtk::Align::Center)
        .build();
    add.set_widget_name("classification-add-category");
    group.set_header_suffix(Some(&add));
    page.add(&group);
    {
        let state = Rc::downgrade(state);
        let guard = Cell::new(false);
        toggle.connect_active_notify(move |row| {
            if guard.get() {
                return;
            }
            if let Some(state) = state.upgrade() {
                let value = row.is_active();
                if preferences::save_change(&state, |settings| {
                    settings.classification_enabled = value
                }) {
                    group.set_visible(value);
                } else {
                    guard.set(true);
                    row.set_active(!value);
                    guard.set(false);
                }
            }
        });
    }
    {
        let state = Rc::downgrade(state);
        let list = list.downgrade();
        add.connect_clicked(move |_| {
            if let Some(state) = state.upgrade() {
                name_dialog(&state, &list, None, None, "");
            }
        });
    }
    populate(state, &list);
    page
}

fn finish(state: &Rc<State>, list: &glib::WeakRef<gtk::ListBox>, result: store::Result<()>) {
    match result {
        Ok(()) => {
            state.changed();
            if let Some(list) = list.upgrade() {
                populate(state, &list);
            }
        }
        Err(error) => ui::error(state, &error.to_string()),
    }
}

fn name_dialog(
    state: &Rc<State>,
    list: &glib::WeakRef<gtk::ListBox>,
    parent: Option<i64>,
    id: Option<i64>,
    name: &str,
) {
    let title = match (parent, id) {
        (None, None) => mark("New Category"),
        (None, Some(_)) => mark("Rename Category"),
        (Some(_), None) => mark("New Subcategory"),
        (Some(_), Some(_)) => mark("Rename Subcategory"),
    };
    let s = state.clone();
    let list = list.clone();
    ui::prompt(state, title, name, move |name| {
        let result = {
            let store = s.store.borrow();
            match (parent, id) {
                (None, None) => store.create_category(&name).map(|_| ()),
                (None, Some(id)) => store.rename_category(id, &name),
                (Some(parent), None) => store.create_subcategory(parent, &name).map(|_| ()),
                (Some(_), Some(id)) => store.rename_subcategory(id, &name),
            }
        };
        finish(&s, &list, result);
    });
}

fn populate(state: &Rc<State>, list: &gtk::ListBox) {
    let categories = match state.store.borrow().categories() {
        Ok(value) => value,
        Err(error) => {
            ui::error(state, &error.to_string());
            return;
        }
    };
    let mut expanded = Vec::new();
    let mut retired = Vec::new();
    while let Some(widget) = list.first_child() {
        if let Some(row) = widget.downcast_ref::<adw::ExpanderRow>() {
            if row.is_expanded() {
                expanded.push(row.widget_name().to_string());
            }
        }
        list.remove(&widget);
        retired.push(widget);
    }
    glib::idle_add_local_once(move || drop(retired));
    if categories.is_empty() {
        list.append(
            &adw::ActionRow::builder()
                .title(tr("No categories yet"))
                .subtitle(tr(
                    "Add your own categories. Nothing is assigned automatically.",
                ))
                .build(),
        );
    }
    let root_ids: Vec<_> = categories.iter().map(|c| c.id).collect();
    for category in categories {
        let row = adw::ExpanderRow::builder()
            .title(&category.name)
            .use_markup(false)
            .title_lines(1)
            .build();
        let widget_name = format!("classification-category-{}", category.id);
        row.set_widget_name(&widget_name);
        row.set_tooltip_text(Some(&category.name));
        row.set_expanded(expanded.contains(&widget_name));
        row.add_suffix(&actions(
            state,
            list,
            None,
            category.id,
            &category.name,
            &root_ids,
        ));
        let child_ids: Vec<_> = category.children.iter().map(|c| c.id).collect();
        for child in category.children {
            let child_row = adw::ActionRow::builder()
                .title(&child.name)
                .use_markup(false)
                .title_lines(2)
                .build();
            child_row.set_widget_name(&format!("classification-child-{}", child.id));
            child_row.set_tooltip_text(Some(&child.name));
            child_row.add_suffix(&actions(
                state,
                list,
                Some(category.id),
                child.id,
                &child.name,
                &child_ids,
            ));
            row.add_row(&child_row);
        }
        let add = adw::ActionRow::builder()
            .title(tr("Add Subcategory…"))
            .activatable(true)
            .build();
        add.add_prefix(&gtk::Image::from_icon_name("list-add-symbolic"));
        add.set_widget_name(&format!("classification-add-child-{}", category.id));
        let weak = Rc::downgrade(state);
        let list_weak = list.downgrade();
        add.connect_activated(move |_| {
            if let Some(state) = weak.upgrade() {
                name_dialog(&state, &list_weak, Some(category.id), None, "");
            }
        });
        row.add_row(&add);
        // All parents are expandable, including those awaiting their first child.
        list.append(&row);
    }
}

fn actions(
    state: &Rc<State>,
    list: &gtk::ListBox,
    parent: Option<i64>,
    id: i64,
    name: &str,
    ids: &[i64],
) -> gtk::MenuButton {
    let menu = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text(tr("Category Actions"))
        .valign(gtk::Align::Center)
        .build();
    menu.add_css_class("flat");
    let model = gio::Menu::new();
    let group = gio::SimpleActionGroup::new();
    let position = ids.iter().position(|value| *value == id).unwrap();
    for (action_name, label) in [
        ("rename", mark("Rename…")),
        ("up", mark("Move up")),
        ("down", mark("Move down")),
        ("delete", mark("Delete…")),
    ] {
        let action = gio::SimpleAction::new(action_name, None);
        action.set_enabled(match action_name {
            "up" => position > 0,
            "down" => position + 1 < ids.len(),
            _ => true,
        });
        let weak = Rc::downgrade(state);
        let list = list.downgrade();
        let name = name.to_owned();
        let ids = ids.to_vec();
        action.connect_activate(move |_, _| {
            let Some(state) = weak.upgrade() else { return; };
            match action_name {
                "rename" => name_dialog(&state, &list, parent, Some(id), &name),
                "delete" => {
                    let count = if parent.is_some() { state.store.borrow().subcategory_item_count(id) }
                        else { state.store.borrow().category_item_count(id) };
                    let count = match count { Ok(n) => n, Err(e) => { ui::error(&state, &e.to_string()); return; } };
                    let body = if parent.is_some() {
                        ntrf("This subcategory will be removed from {count} item. Categories, comments and items will be kept.",
                             "This subcategory will be removed from {count} items. Categories, comments and items will be kept.", count as u64, &[])
                    } else {
                        ntrf("This category and its subcategories will be removed from {count} item. Comments and items will be kept.",
                             "This category and its subcategories will be removed from {count} items. Comments and items will be kept.", count as u64, &[])
                    };
                    let s = state.clone(); let list = list.clone();
                    ui::confirm(&state, if parent.is_some() { mark("Delete this subcategory?") } else { mark("Delete this category?") }, &body, "Delete", move || {
                        let result = if parent.is_some() { s.store.borrow().delete_subcategory(id) } else { s.store.borrow().delete_category(id) };
                        finish(&s, &list, result);
                    });
                }
                _ => {
                    let mut order = ids.clone();
                    let other = if action_name == "up" { position - 1 } else { position + 1 };
                    order.swap(position, other);
                    let result = if let Some(parent) = parent { state.store.borrow_mut().reorder_subcategories(parent, &order) }
                        else { state.store.borrow_mut().reorder_categories(&order) };
                    finish(&state, &list, result);
                }
            }
        });
        group.add_action(&action);
        model.append(Some(&tr(label)), Some(&format!("category.{action_name}")));
    }
    menu.insert_action_group("category", Some(&group));
    menu.set_menu_model(Some(&model));
    menu
}
