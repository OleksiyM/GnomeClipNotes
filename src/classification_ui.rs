//! Library-only category assignment and optional comment editor.
use crate::{
    i18n::{tr, trf},
    model::{Category, ItemClassification},
    ui, State,
};
use adw::prelude::*;
use std::rc::Rc;

fn caption(categories: &[Category], value: &ItemClassification) -> String {
    let Some(category) = categories.iter().find(|c| Some(c.id) == value.category_id) else {
        return tr("Comment");
    };
    if let Some(child) = category
        .children
        .iter()
        .find(|c| Some(c.id) == value.subcategory_id)
    {
        trf(
            "{category} · {subcategory}",
            &[("category", &category.name), ("subcategory", &child.name)],
        )
    } else {
        category.name.clone()
    }
}

pub fn card_label(state: &Rc<State>, id: i64) -> Option<crate::card_metadata::MetadataTag> {
    let store = state.store.borrow();
    if !store.settings.classification_enabled {
        return None;
    }
    let value = store.item_classification(id).ok()?;
    value.category_id?;
    let text = caption(&store.categories().ok()?, &value);
    let row =
        crate::card_metadata::MetadataTag::new("view-list-symbolic", &text, "card-classification");
    row.set_margin_top(4);
    Some(row)
}

pub fn comment_button(state: &Rc<State>, id: i64) -> Option<gtk::Button> {
    let store = state.store.borrow();
    if !store.settings.comments_enabled || store.item_classification(id).ok()?.comment.is_empty() {
        return None;
    }
    drop(store);
    let button = gtk::Button::builder()
        .icon_name("chat-message-new-symbolic")
        .tooltip_text(tr("Edit Comment"))
        .build();
    button.set_widget_name("card-comment");
    button.add_css_class("flat");
    let weak = Rc::downgrade(state);
    button.connect_clicked(move |_| {
        if let Some(state) = weak.upgrade() {
            comment(&state, id, None);
        }
    });
    Some(button)
}

fn choice(menu: &gio::Menu, title: &str, category: i64, child: i64) {
    let item = gio::MenuItem::new(Some(title), None);
    item.set_action_and_target_value(
        Some("classification.select"),
        Some(&(category, child).to_variant()),
    );
    menu.append_item(&item);
}

pub fn add_menu(state: &Rc<State>, id: i64, button: &gtk::MenuButton, organize: &gio::Menu) {
    let store = state.store.borrow();
    let categories_enabled = store.settings.classification_enabled;
    let comments_enabled = store.settings.comments_enabled;
    if !categories_enabled && !comments_enabled {
        return;
    }
    let Ok(categories) = store.categories() else {
        return;
    };
    let Ok(value) = store.item_classification(id) else {
        return;
    };
    drop(store);
    let group = gio::SimpleActionGroup::new();
    let selected = (
        value.category_id.unwrap_or(0),
        value.subcategory_id.unwrap_or(0),
    );
    let action = gio::SimpleAction::new_stateful(
        "select",
        Some(<(i64, i64)>::static_variant_type().as_ref()),
        &selected.to_variant(),
    );
    let weak = Rc::downgrade(state);
    action.connect_activate(move |_, parameter| {
        let (Some(state), Some((category, child))) = (
            weak.upgrade(),
            parameter.and_then(|p| p.get::<(i64, i64)>()),
        ) else {
            return;
        };
        if child != 0 && state.store.borrow().settings.comments_enabled {
            comment(&state, id, Some((category, child)));
            return;
        }
        let result = if category == 0 {
            state.store.borrow().clear_item_category(id)
        } else {
            let store = state.store.borrow();
            store.item_classification(id).and_then(|mut value| {
                value.category_id = Some(category);
                value.subcategory_id = (child != 0).then_some(child);
                store.set_item_classification(id, &value)
            })
        };
        state.report(result);
    });
    group.add_action(&action);
    let edit = gio::SimpleAction::new("comment", None);
    let weak = Rc::downgrade(state);
    edit.connect_activate(move |_, _| {
        if let Some(state) = weak.upgrade() {
            comment(&state, id, None);
        }
    });
    group.add_action(&edit);
    button.insert_action_group("classification", Some(&group));
    let menu = gio::Menu::new();
    let none = gio::Menu::new();
    choice(&none, &tr("No category"), 0, 0);
    menu.append_section(None, &none);
    let choices = gio::Menu::new();
    for category in categories {
        if category.children.is_empty() {
            choice(&choices, &category.name, category.id, 0);
        } else {
            let submenu = gio::Menu::new();
            let parent = gio::Menu::new();
            choice(
                &parent,
                &trf("Select “{category}”", &[("category", &category.name)]),
                category.id,
                0,
            );
            submenu.append_section(None, &parent);
            let children = gio::Menu::new();
            for child in category.children {
                choice(&children, &child.name, category.id, child.id);
            }
            submenu.append_section(None, &children);
            choices.append_submenu(Some(&category.name), &submenu);
        }
    }
    menu.append_section(None, &choices);
    if categories_enabled {
        organize.append_submenu(Some(&tr("Category")), &menu);
    }
    if comments_enabled {
        organize.append(Some(&tr("Comment…")), Some("classification.comment"));
    }
}

pub fn comment(state: &Rc<State>, id: i64, assignment: Option<(i64, i64)>) {
    let snapshot = (|| -> crate::store::Result<_> {
        let mut store = state.store.borrow_mut();
        let selected = store.selected_items(&[id])?;
        let mut value = store.item_classification(id)?;
        if let Some((category, child)) = assignment {
            value.category_id = Some(category);
            value.subcategory_id = Some(child);
        }
        Ok((
            if store.settings.classification_enabled && value.category_id.is_some() {
                caption(&store.categories()?, &value)
            } else {
                selected[0].item.title.clone()
            },
            value,
            selected[0].revision.clone(),
        ))
    })();
    let (title, value, revision) = match snapshot {
        Ok(v) => v,
        Err(e) => {
            ui::error(state, &e.to_string());
            return;
        }
    };
    let dialog = adw::Dialog::builder()
        .title(tr("Comment"))
        .content_width(460)
        .content_height(350)
        .build();
    dialog.set_widget_name("classification-comment-dialog");
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let cancel = gtk::Button::with_label(&tr("Cancel"));
    let save = gtk::Button::with_label(&tr("Save"));
    save.set_widget_name("classification-comment-save");
    save.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&save);
    toolbar.add_top_bar(&header);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    ui::margins(&content, 18);
    let title = gtk::Label::builder()
        .label(&title)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .build();
    title.add_css_class("heading");
    content.append(&title);
    let hint = gtk::Label::builder()
        .label(tr("Optional — leave empty if you don't need a comment."))
        .wrap(true)
        .xalign(0.0)
        .build();
    hint.add_css_class("dim-label");
    content.append(&hint);
    let text = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(12)
        .bottom_margin(12)
        .left_margin(12)
        .right_margin(12)
        .build();
    text.set_widget_name("classification-comment-text");
    text.update_property(&[gtk::accessible::Property::Label(&tr("Comment"))]);
    let buffer = text.buffer();
    buffer.set_text(&value.comment);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .min_content_height(100)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&text)
        .build();
    scroll.add_css_class("frame");
    content.append(&scroll);
    let error = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .visible(false)
        .build();
    error.add_css_class("error");
    content.append(&error);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));
    let weak = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
    });
    let weak = dialog.downgrade();
    let s = state.clone();
    save.connect_clicked(move |_| {
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();
        let result = s.store.borrow_mut().save_classification_revision(
            id,
            &ItemClassification {
                comment: text,
                ..value.clone()
            },
            &revision,
        );
        match result {
            Ok(()) => {
                if let Some(dialog) = weak.upgrade() {
                    dialog.close();
                }
                s.changed();
            }
            Err(e) => {
                error.set_text(&e.to_string());
                error.set_visible(true);
            }
        }
    });
    dialog.present(state.window.borrow().as_ref());
    text.grab_focus();
}
