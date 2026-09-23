pub(crate) use crate::i18n::tr;
use crate::{
    i18n::{mark, ntrf, trf},
    model::*,
    State,
};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(crate) fn button(text: &str) -> gtk::Button {
    gtk::Button::with_label(&tr(text))
}
pub(crate) fn label(text: &str) -> gtk::Label {
    gtk::Label::new(Some(&tr(text)))
}
pub(crate) fn margins(widget: &impl IsA<gtk::Widget>, n: i32) {
    widget.set_margin_top(n);
    widget.set_margin_bottom(n);
    widget.set_margin_start(n);
    widget.set_margin_end(n);
}
fn date(time: i64) -> String {
    glib::DateTime::from_unix_local(time)
        .and_then(|d| d.format("%b %e, %Y · %H:%M"))
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn content_length(content: &str) -> String {
    // Independent quantities need independent plural selection. These are two
    // complete statistics, not pieces of a grammatical sentence.
    let characters = ntrf(
        "{count} character",
        "{count} characters",
        content.chars().count() as u64,
        &[],
    );
    let words = ntrf(
        "{count} word",
        "{count} words",
        content.split_whitespace().count() as u64,
        &[],
    );
    trf(
        "{characters} · {words}",
        &[("characters", &characters), ("words", &words)],
    )
}
fn title(item: &Item) -> String {
    if item.title.is_empty() {
        if item.kind == "link" {
            tr("Link")
        } else {
            tr("Text")
        }
    } else {
        item.title.clone()
    }
}

fn group_name(group: &Group) -> String {
    match group.id {
        0 => tr("History"),
        1 => tr("Notes"),
        _ => group.name.clone(),
    }
}

fn command_label(action: &str, fallback: &str) -> String {
    match action {
        "view" => tr("View"),
        "edit" => tr("Edit"),
        "info" => tr("Info"),
        "rename" => tr("Rename…"),
        "delete" => tr("Delete…"),
        _ => fallback.to_string(),
    }
}

// Let GTK supply menu rows, focus handling and accessibility, not ordinary buttons.
fn menu_actions(widget: &impl IsA<gtk::Widget>, state: &Rc<State>, names: &[&str], id: i64) {
    let actions = gio::SimpleActionGroup::new();
    for name in names {
        let action = gio::SimpleAction::new(name, None);
        let state = Rc::downgrade(state);
        let name = name.to_string();
        action.connect_activate(move |_, _| {
            if let Some(state) = state.upgrade() {
                state.activate(&name, id);
            }
        });
        actions.add_action(&action);
    }
    widget.insert_action_group("menu", Some(&actions));
}

fn main_menu_model(settings: Option<&gio::Settings>) -> gio::Menu {
    let model = gio::Menu::new();
    let navigation = gio::Menu::new();
    for (label, action, key) in [
        (mark("New Note"), "new-note", "note-shortcut"),
        (mark("Open Clipboard"), "clipboard", "activate-shortcut"),
    ] {
        let item = gio::MenuItem::new(Some(&tr(label)), Some(&format!("menu.{action}")));
        // These are Shell-owned global shortcuts: show the actual configuration,
        // without registering a competing GTK accelerator.
        if let Some(accel) = settings.and_then(|s| s.strv(key).first().cloned()) {
            item.set_attribute_value("accel", Some(&accel.to_variant()));
        }
        navigation.append_item(&item);
    }
    model.append_section(None, &navigation);
    let application = gio::Menu::new();
    application.append(Some(&tr("Settings")), Some("menu.settings"));
    application.append(Some(&tr("About GnomeClipNotes")), Some("menu.about"));
    model.append_section(None, &application);
    model
}

pub fn init(state: &Rc<State>) {
    let bytes = glib::Bytes::from_static(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/resources.gresource"
    )));
    let resource = gio::Resource::from_data(&bytes).expect("Embedded application resources");
    gio::resources_register(&resource);
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&format!(
        "{}\n{}",
        include_str!("style.css"),
        crate::preview_native::CSS
    ));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display)
            .add_resource_path("/io/github/OleksiyM/GnomeClipNotes/icons");
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    apply_theme(&state.store.borrow().settings.theme);
}
pub(crate) fn apply_theme(theme: &str) {
    adw::StyleManager::default().set_color_scheme(match theme {
        "dark" => adw::ColorScheme::ForceDark,
        "light" => adw::ColorScheme::ForceLight,
        _ => adw::ColorScheme::Default,
    });
}

pub fn error(state: &State, message: &str) {
    let dialog = adw::AlertDialog::builder()
        .heading(tr("GnomeClipNotes"))
        .body(message)
        .build();
    dialog.add_response("ok", &tr("OK"));
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present(state.app.active_window().as_ref());
}
pub(crate) fn confirm(
    state: &Rc<State>,
    heading: &str,
    body: &str,
    action: &str,
    callback: impl Fn() + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(tr(heading))
        .body(body)
        .build();
    dialog.add_response("cancel", &tr("Cancel"));
    dialog.add_response("accept", &tr(action));
    dialog.set_response_appearance("accept", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, move |_, response| {
        if response == "accept" {
            callback();
        }
    });
    dialog.present(state.app.active_window().as_ref());
}
pub(crate) fn prompt(
    state: &Rc<State>,
    heading: &str,
    initial: &str,
    callback: impl Fn(String) + 'static,
) {
    let entry = gtk::Entry::builder()
        .text(initial)
        .activates_default(true)
        .build();
    let dialog = adw::AlertDialog::builder()
        .heading(tr(heading))
        .extra_child(&entry)
        .build();
    dialog.add_response("cancel", &tr("Cancel"));
    dialog.add_response("save", &tr("Save"));
    dialog.set_default_response(Some("save"));
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.connect_response(None, move |_, response| {
        if response == "save" {
            callback(entry.text().to_string());
        }
    });
    dialog.present(state.app.active_window().as_ref());
}

pub fn show(state: &Rc<State>) {
    if let Some(window) = state.window.borrow().as_ref() {
        window.present();
        return;
    }
    let window = adw::ApplicationWindow::builder()
        .application(&state.app)
        .title("GnomeClipNotes")
        .default_width(1040)
        .default_height(720)
        .width_request(360)
        .height_request(420)
        .build();
    let root = adw::OverlaySplitView::builder()
        .min_sidebar_width(200.0)
        .max_sidebar_width(260.0)
        .sidebar_width_fraction(0.23)
        .build();
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let window_title = adw::WindowTitle::new(&tr("History"), "");
    header.set_title_widget(Some(&window_title));
    let sidebar_toggle = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text(tr("Show Collections"))
        .active(true)
        .build();
    root.bind_property("show-sidebar", &sidebar_toggle, "active")
        .bidirectional()
        .sync_create()
        .build();
    header.pack_start(&sidebar_toggle);
    let new = gtk::Button::from_icon_name("document-new-symbolic");
    new.set_tooltip_text(Some(&tr("New Note")));
    {
        let state = state.clone();
        new.connect_clicked(move |_| editor(&state, None));
    }
    header.pack_start(&new);
    let menu = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text(tr("Main Menu"))
        .build();
    menu.set_widget_name("library-main-menu");
    menu_actions(
        &menu,
        state,
        &["new-note", "clipboard", "settings", "about"],
        0,
    );
    let shortcuts = crate::preferences::extension_settings().ok();
    menu.set_menu_model(Some(&main_menu_model(shortcuts.as_ref())));
    if let Some(settings) = shortcuts {
        let weak_menu = menu.downgrade();
        let handler = settings.connect_changed(None, move |settings, key| {
            if matches!(key, "note-shortcut" | "activate-shortcut") {
                if let Some(menu) = weak_menu.upgrade() {
                    menu.set_menu_model(Some(&main_menu_model(Some(settings))));
                }
            }
        });
        let handler = RefCell::new(Some(handler));
        window.connect_destroy(move |_| {
            if let Some(handler) = handler.borrow_mut().take() {
                settings.disconnect(handler);
            }
        });
    }
    header.pack_end(&menu);
    toolbar.add_top_bar(&header);
    let sidebar_toolbar = adw::ToolbarView::new();
    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.set_show_end_title_buttons(false);
    sidebar_header.set_title_widget(Some(&label("Collections")));
    sidebar_toolbar.add_top_bar(&sidebar_header);
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 12);
    margins(&sidebar, 12);
    let groups_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    sidebar.append(&groups_box);
    let add = gtk::Button::from_icon_name("folder-new-symbolic");
    add.set_tooltip_text(Some(&tr("New Folder")));
    {
        let state = state.clone();
        add.connect_clicked(move |_| new_group(&state));
    }
    sidebar_header.pack_end(&add);
    let sidebar_scroll = gtk::ScrolledWindow::builder()
        .child(&sidebar)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    sidebar_toolbar.set_content(Some(&sidebar_scroll));
    root.set_sidebar(Some(&sidebar_toolbar));
    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    margins(&content, 18);
    content.set_hexpand(true);
    toolbar.set_content(Some(&content));
    root.set_content(Some(&toolbar));
    let breakpoint =
        adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 720sp").unwrap());
    breakpoint.add_setter(&root, "collapsed", Some(&true.to_value()));
    breakpoint.add_setter(&root, "show-sidebar", Some(&false.to_value()));
    window.add_breakpoint(breakpoint);
    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let search = gtk::SearchEntry::builder()
        .placeholder_text(tr("Search this folder…"))
        .hexpand(true)
        .build();
    search_row.append(&search);
    let filter_button = gtk::MenuButton::builder()
        .label(tr("Filters"))
        .tooltip_text(tr("Filter Items"))
        .build();
    let filter_popover = gtk::Popover::new();
    filter_button.set_widget_name("library-filters");
    filter_popover.set_widget_name("library-filter-popover");
    // On Wayland the outer popover can lose outside-click dismissal after a
    // nested GtkDropDown closes. GTK may redirect a main-surface event to the
    // popover itself, so handle it on both widgets. Popup-surface events still
    // belong to GTK; the dismissing click must not activate content underneath.
    for owner in [
        window.clone().upcast::<gtk::Widget>(),
        filter_popover.clone().upcast::<gtk::Widget>(),
    ] {
        let outside_click = gtk::EventControllerLegacy::new();
        outside_click.set_propagation_phase(gtk::PropagationPhase::Capture);
        {
            let window = window.downgrade();
            let popover = filter_popover.downgrade();
            outside_click.connect_event(move |_, event| {
                if !matches!(
                    event.event_type(),
                    gtk::gdk::EventType::ButtonPress | gtk::gdk::EventType::TouchBegin
                ) {
                    return glib::Propagation::Proceed;
                }
                if let (Some(window), Some(popover)) = (window.upgrade(), popover.upgrade()) {
                    if popover.is_visible() && event.surface() == window.surface() {
                        popover.popdown();
                        return glib::Propagation::Stop;
                    }
                }
                glib::Propagation::Proceed
            });
        }
        owner.add_controller(outside_click);
    }
    let filters = gtk::Box::new(gtk::Orientation::Vertical, 12);
    margins(&filters, 12);
    let filter_title = label("Filter Items");
    filter_title.add_css_class("heading");
    filter_title.set_xalign(0.0);
    filters.append(&filter_title);
    let kinds = gtk::DropDown::from_strings(&[&tr("All types"), &tr("Text"), &tr("Links")]);
    kinds.set_widget_name("library-filter-kind");
    filters.append(&kinds);
    let dates = gtk::DropDown::from_strings(&[
        &tr("Any time"),
        &tr("Today"),
        &tr("Yesterday"),
        &tr("This week"),
        &tr("Last 30 days"),
        &tr("Custom range"),
    ]);
    dates.set_widget_name("library-filter-dates");
    filters.append(&dates);
    let from_picker =
        crate::date_picker::DatePicker::new("library-filter-from", &tr("From YYYY-MM-DD"));
    let until_picker =
        crate::date_picker::DatePicker::new("library-filter-until", &tr("To YYYY-MM-DD"));
    let from = from_picker.entry;
    let until = until_picker.entry;
    let from_row = from_picker.row;
    let until_row = until_picker.row;
    from_row.set_visible(false);
    until_row.set_visible(false);
    filters.append(&from_row);
    filters.append(&until_row);
    let date_hint = gtk::Label::builder()
        .wrap(true)
        .max_width_chars(28)
        .xalign(0.0)
        .visible(false)
        .build();
    date_hint.set_widget_name("library-filter-date-hint");
    date_hint.add_css_class("caption");
    date_hint.add_css_class("dim-label");
    filters.append(&date_hint);
    filter_popover.set_child(Some(&filters));
    filter_button.set_popover(Some(&filter_popover));
    search_row.append(&filter_button);
    content.append(&search_row);
    let source_names = gtk::StringList::new(&[&tr("All apps")]);
    let source_values = Rc::new(RefCell::new(Vec::<String>::new()));
    let source = gtk::DropDown::builder()
        .model(&source_names)
        .enable_search(true)
        .build();
    filters.append(&source);
    source.set_widget_name("library-filter-source");
    let clear_filters = button("Clear filters");
    clear_filters.set_widget_name("library-clear-filters");
    clear_filters.add_css_class("flat");
    filters.append(&clear_filters);
    let applied_date_range = Rc::new(Cell::new((0i64, 0i64)));
    let resetting_filters = Rc::new(Cell::new(false));
    let summary = gtk::Label::new(None);
    summary.set_xalign(0.0);
    summary.add_css_class("dim-label");
    content.append(&summary);
    let list = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .column_spacing(16)
        .row_spacing(16)
        .min_children_per_line(1)
        .max_children_per_line(3)
        .valign(gtk::Align::Start)
        .homogeneous(true)
        .build();
    list.set_widget_name("library-cards");
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    scroll.set_widget_name("library-scroll");
    let results = gtk::Stack::new();
    results.set_vexpand(true);
    results.add_named(&scroll, Some("items"));
    let empty = adw::StatusPage::builder()
        .icon_name("edit-paste-symbolic")
        .title(tr("No Items Yet"))
        .description(tr("Copy text or create a note to get started."))
        .build();
    results.add_named(&empty, Some("empty"));
    content.append(&results);
    let paging = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    paging.set_halign(gtk::Align::Center);
    let prev = button("Previous");
    let next = button("Next");
    prev.set_widget_name("library-previous");
    next.set_widget_name("library-next");
    let selection = crate::library_selection::Selection::new(&list, state, &prev, &next);
    paging.append(&prev);
    paging.append(&next);
    content.append(&paging);
    let active_group = Rc::new(Cell::new(0i64));
    let offset = Rc::new(Cell::new(0i64));
    let page_size = Rc::new(Cell::new(6i64));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let state = Rc::downgrade(state);
        let search = search.clone();
        let kinds = kinds.clone();
        let dates = dates.clone();
        let from = from.clone();
        let until = until.clone();
        let source = source.clone();
        let source_names = source_names.clone();
        let source_values = source_values.clone();
        let active_group = active_group.clone();
        let offset = offset.clone();
        let page_size = page_size.clone();
        let scroll = scroll.clone();
        let list = list.clone();
        let summary = summary.clone();
        let groups_box = groups_box.clone();
        let prev = prev.clone();
        let next = next.clone();
        let results = results.clone();
        let empty = empty.clone();
        let window_title = window_title.clone();
        let split = root.clone();
        let resetting_filters = resetting_filters.clone();
        let clear_filters = clear_filters.clone();
        move || {
            if resetting_filters.get() {
                return;
            }
            let Some(state) = state.upgrade() else {
                return;
            };
            // Incomplete date input is not an empty query. Keep the last valid
            // range while the other live filters and search continue to work.
            match date_range(dates.selected(), &from.text(), &until.text()) {
                Ok(range) => {
                    applied_date_range.set(range);
                    date_hint.set_visible(false);
                }
                Err(message) => {
                    date_hint.set_text(&trf(
                        "{message}. The previous date filter stays active.",
                        &[("message", &message)],
                    ));
                    date_hint.set_visible(true);
                }
            }
            let (since, end) = applied_date_range.get();
            let count = u8::from(kinds.selected() != 0)
                + u8::from(source.selected() != 0)
                + u8::from(since != 0 || end != 0);
            filter_button.set_label(&if count == 0 {
                tr("Filters")
            } else {
                ntrf("Filters ({count})", "Filters ({count})", count as u64, &[])
            });
            if count == 0 {
                filter_button.remove_css_class("accent");
            } else {
                filter_button.add_css_class("accent");
            }
            clear_filters.set_sensitive(
                kinds.selected() != 0
                    || dates.selected() != 0
                    || source.selected() != 0
                    || !from.text().is_empty()
                    || !until.text().is_empty(),
            );
            if let Ok(sources) = state.store.borrow().sources() {
                for name in sources {
                    if !source_values.borrow().contains(&name) {
                        source_values.borrow_mut().push(name.clone());
                        source_names.append(&name);
                    }
                }
            }
            // A refresh can originate in one of these widgets' GTK signals.
            // Keep detached children alive until event dispatch has finished.
            let had_card_focus = selection.begin_refresh();
            let mut retired = Vec::new();
            while let Some(child) = list.first_child() {
                list.remove(&child);
                retired.push(child);
            }
            while let Some(child) = groups_box.first_child() {
                groups_box.remove(&child);
                retired.push(child);
            }
            glib::idle_add_local_once(move || drop(retired));
            let groups = state.store.borrow().groups().unwrap_or_default();
            if !groups.iter().any(|g| g.id == active_group.get()) {
                active_group.set(0);
            }
            for group in &groups {
                let name = group_name(group);
                let b = gtk::Button::new();
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                row.append(&gtk::Image::from_icon_name(match group.id {
                    0 => "document-open-recent-symbolic",
                    1 => "document-edit-symbolic",
                    _ => "folder-symbolic",
                }));
                let name_label = gtk::Label::new(Some(&name));
                name_label.set_xalign(0.0);
                name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                row.append(&name_label);
                b.set_child(Some(&row));
                b.add_css_class("flat");
                b.add_css_class("collection-button");
                b.set_widget_name(&format!("collection-{}", group.id));
                b.set_halign(gtk::Align::Fill);
                if group.id == active_group.get() {
                    b.add_css_class("collection-selected");
                    window_title.set_title(&name);
                }
                let id = group.id;
                let selected = active_group.clone();
                let offset = offset.clone();
                let weak = Rc::downgrade(&state);
                let split = split.clone();
                b.connect_clicked(move |_| {
                    selected.set(id);
                    offset.set(0);
                    if split.is_collapsed() {
                        split.set_show_sidebar(false);
                    }
                    if let Some(state) = weak.upgrade() {
                        state.changed();
                    }
                });
                groups_box.append(&b);
            }
            let query = Query {
                search: search.text().to_string(),
                group_id: active_group.get(),
                kind: match kinds.selected() {
                    1 => "text",
                    2 => "link",
                    _ => "",
                }
                .into(),
                source: if source.selected() == 0 {
                    String::new()
                } else {
                    source_values
                        .borrow()
                        .get(source.selected() as usize - 1)
                        .cloned()
                        .unwrap_or_default()
                },
                since,
                until: end,
                limit: page_size.get() + 1,
                offset: offset.get(),
                ..Default::default()
            };
            let result = state.store.borrow().query(&query);
            match result {
                Ok(mut items) => {
                    prev.set_sensitive(offset.get() > 0);
                    next.set_sensitive(items.len() > page_size.get() as usize);
                    items.truncate(page_size.get() as usize);
                    scroll.vadjustment().set_value(0.0);
                    let group = groups
                        .iter()
                        .find(|g| g.id == active_group.get())
                        .map(group_name)
                        .unwrap_or_else(|| tr("History"));
                    summary.set_text(&if items.is_empty() {
                        trf("{collection} · Nothing here yet", &[("collection", &group)])
                    } else {
                        let first = (offset.get() + 1).to_string();
                        let last = (offset.get() + items.len() as i64).to_string();
                        trf(
                            "{collection} · {first}–{last}",
                            &[
                                ("collection", group.as_str()),
                                ("first", first.as_str()),
                                ("last", last.as_str()),
                            ],
                        )
                    });
                    if items.is_empty() {
                        let filtered = !query.search.is_empty()
                            || !query.kind.is_empty()
                            || !query.source.is_empty()
                            || query.since != 0
                            || query.until != 0;
                        if filtered {
                            empty.set_title(&tr("No Matching Items"));
                            empty.set_description(Some(&tr(
                                "Try a different search or clear the filters.",
                            )));
                        } else if query.group_id == 1 {
                            empty.set_title(&tr("No Notes Yet"));
                            empty.set_description(Some(&tr(
                                "Copy text or create a note to get started.",
                            )));
                        } else {
                            empty.set_title(&tr("No Items Yet"));
                            empty.set_description(Some(&tr(
                                "Copy text or create a note to get started.",
                            )));
                        }
                        results.set_visible_child_name("empty");
                    } else {
                        results.set_visible_child_name("items");
                    }
                    for item in items {
                        selection.append(item.id, &card(&state, &item));
                    }
                }
                Err(error) => summary.set_text(&error.to_string()),
            }
            selection.finish_refresh(had_card_focus);
        }
    });
    // Recalculate pages after viewport allocation, not on every rendered frame.
    // Measuring actual children respects theme padding and text scaling.
    let pending_layout = Rc::new(Cell::new(false));
    for adjustment in [scroll.hadjustment(), scroll.vadjustment()] {
        let pending = pending_layout.clone();
        let scroll = scroll.downgrade();
        let list = list.downgrade();
        let refresh = Rc::downgrade(&refresh);
        let page_size = page_size.clone();
        let offset = offset.clone();
        adjustment.connect_changed(move |_| {
            if pending.replace(true) {
                return;
            }
            let (pending, scroll, list, refresh, page_size, offset) = (
                pending.clone(),
                scroll.clone(),
                list.clone(),
                refresh.clone(),
                page_size.clone(),
                offset.clone(),
            );
            glib::idle_add_local_once(move || {
                pending.set(false);
                let (Some(scroll), Some(list), Some(refresh)) =
                    (scroll.upgrade(), list.upgrade(), refresh.upgrade())
                else {
                    return;
                };
                let Some(child) = list.first_child() else {
                    return;
                };
                let width = scroll.width();
                let height = scroll.height();
                if width <= 0 || height <= 0 {
                    return;
                }
                let min_width = child.measure(gtk::Orientation::Horizontal, -1).0.max(1);
                let columns = ((width + 16) / (min_width + 16)).clamp(1, 3) as u32;
                let column_width = (width - (columns as i32 - 1) * 16) / columns as i32;
                let row_height = child
                    .measure(gtk::Orientation::Vertical, column_width)
                    .1
                    .max(1);
                let rows = ((height + 16) / (row_height + 16)).max(1);
                let size = i64::from(columns) * i64::from(rows);
                list.set_max_children_per_line(columns);
                if page_size.replace(size) != size {
                    offset.set(offset.get() / size * size);
                    refresh();
                }
            });
        });
    }
    *state.refresh.borrow_mut() = Some(Box::new({
        let refresh = refresh.clone();
        move || refresh()
    }));
    {
        let refresh = refresh.clone();
        let kinds = kinds.clone();
        let dates = dates.clone();
        let source = source.clone();
        let from = from.clone();
        let until = until.clone();
        let offset = offset.clone();
        clear_filters.connect_clicked(move |_| {
            resetting_filters.set(true);
            kinds.set_selected(0);
            dates.set_selected(0);
            source.set_selected(0);
            from.set_text("");
            until.set_text("");
            offset.set(0);
            resetting_filters.set(false);
            refresh();
        });
    }
    {
        let refresh = refresh.clone();
        let offset = offset.clone();
        search.connect_search_changed(move |_| {
            offset.set(0);
            refresh();
        });
    }
    {
        let refresh = refresh.clone();
        let offset = offset.clone();
        kinds.connect_selected_notify(move |_| {
            offset.set(0);
            refresh();
        });
    }
    {
        let refresh = refresh.clone();
        let offset = offset.clone();
        let from_row = from_row.clone();
        let until_row = until_row.clone();
        dates.connect_selected_notify(move |d| {
            from_row.set_visible(d.selected() == 5);
            until_row.set_visible(d.selected() == 5);
            offset.set(0);
            refresh();
        });
    }
    {
        let refresh = refresh.clone();
        let offset = offset.clone();
        source.connect_selected_notify(move |_| {
            offset.set(0);
            refresh();
        });
    }
    for entry in [&from, &until] {
        let refresh = refresh.clone();
        let offset = offset.clone();
        entry.connect_changed(move |_| {
            offset.set(0);
            refresh();
        });
    }
    {
        let refresh = refresh.clone();
        let offset = offset.clone();
        let page_size = page_size.clone();
        prev.connect_clicked(move |_| {
            offset.set((offset.get() - page_size.get()).max(0));
            refresh();
        });
    }
    {
        let refresh = refresh.clone();
        let offset = offset.clone();
        let page_size = page_size.clone();
        next.connect_clicked(move |_| {
            offset.set(offset.get() + page_size.get());
            refresh();
        });
    }
    let weak = Rc::downgrade(state);
    window.connect_close_request(move |window| {
        window.set_visible(false);
        if let Some(state) = weak.upgrade() {
            *state.refresh.borrow_mut() = None;
            *state.window.borrow_mut() = None;
        }
        glib::Propagation::Proceed
    });
    window.set_content(Some(&root));
    *state.window.borrow_mut() = Some(window.clone());
    refresh();
    window.present();
}

fn date_range(index: u32, from: &str, until: &str) -> Result<(i64, i64), String> {
    let today = glib::DateTime::now_local().map_err(|e| e.to_string())?;
    let midnight = glib::DateTime::new(
        &glib::TimeZone::local(),
        today.year(),
        today.month(),
        today.day_of_month(),
        0,
        0,
        0.0,
    )
    .map_err(|e| e.to_string())?;
    Ok(match index {
        1 => (midnight.to_unix(), 0),
        2 => (
            midnight.add_days(-1).unwrap().to_unix(),
            midnight.to_unix() - 1,
        ),
        3 => (
            midnight
                .add_days(1 - today.day_of_week())
                .unwrap()
                .to_unix(),
            0,
        ),
        4 => (now() - 30 * 86400, 0),
        5 => {
            let parse = |s: &str| -> Result<glib::DateTime, String> {
                let parts: Vec<i32> = s
                    .split('-')
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| tr("Use dates in YYYY-MM-DD format"))?;
                if parts.len() != 3 {
                    return Err(tr("Use dates in YYYY-MM-DD format"));
                }
                glib::DateTime::new(
                    &glib::TimeZone::local(),
                    parts[0],
                    parts[1],
                    parts[2],
                    0,
                    0,
                    0.0,
                )
                .map_err(|_| tr("Invalid date"))
            };
            let start = parse(from)?.to_unix();
            let end = parse(until)?
                .add_days(1)
                .map_err(|e| e.to_string())?
                .to_unix()
                - 1;
            if start > end {
                return Err(tr("Start date must be before end date"));
            }
            (start, end)
        }
        _ => (0, 0),
    })
}

fn card(state: &Rc<State>, item: &Item) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("clip-card");
    card.set_size_request(200, 180);
    margins(&card, 2);
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let icon = gtk::Image::from_icon_name(if item.kind == "link" {
        "insert-link-symbolic"
    } else {
        "text-x-generic-symbolic"
    });
    head.append(&icon);
    let heading = gtk::Label::new(Some(&title(item)));
    heading.set_xalign(0.0);
    heading.set_hexpand(true);
    heading.set_ellipsize(gtk::pango::EllipsizeMode::End);
    heading.add_css_class("heading");
    head.append(&heading);
    let details_button = gtk::Button::builder()
        .icon_name("dialog-information-symbolic")
        .tooltip_text(tr("Item Details"))
        .build();
    details_button.add_css_class("flat");
    {
        let state = state.clone();
        let item = item.clone();
        details_button.connect_clicked(move |_| info(&state, &item));
    }
    head.append(&details_button);
    card.append(&head);
    let excerpt: String = item
        .content
        .split_whitespace()
        .flat_map(|word| word.chars().chain(std::iter::once(' ')))
        .take(250)
        .collect();
    let preview = gtk::Label::new(Some(excerpt.trim_end()));
    preview.set_max_width_chars(24);
    preview.set_wrap(true);
    preview.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    preview.set_lines(3);
    preview.set_ellipsize(gtk::pango::EllipsizeMode::End);
    preview.set_xalign(0.0);
    preview.set_yalign(0.0);
    preview.set_vexpand(true);
    preview.add_css_class("card-preview");
    card.append(&preview);
    let source = gtk::Label::new(Some(&if item.source.is_empty() {
        tr("Note")
    } else {
        item.source.clone()
    }));
    source.set_xalign(0.0);
    source.set_ellipsize(gtk::pango::EllipsizeMode::End);
    source.add_css_class("caption");
    source.add_css_class("dim-label");
    card.append(&source);
    let foot = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let edit = gtk::Button::from_icon_name("document-edit-symbolic");
    edit.set_tooltip_text(Some(&tr("Edit Item")));
    edit.add_css_class("flat");
    let paste = gtk::Button::from_icon_name("edit-paste-symbolic");
    paste.set_tooltip_text(Some(&tr("Paste Item")));
    paste.add_css_class("flat");
    foot.append(&edit);
    foot.append(&paste);
    let menu = gtk::MenuButton::builder()
        .icon_name("view-more-horizontal-symbolic")
        .tooltip_text(tr("More Actions"))
        .halign(gtk::Align::End)
        .hexpand(true)
        .build();
    menu.add_css_class("flat");
    menu.set_widget_name("card-actions-menu");
    let folders: Vec<_> = state
        .store
        .borrow()
        .groups()
        .unwrap_or_default()
        .into_iter()
        .filter(|group| group.id > 1)
        .collect();
    let mut action_names: Vec<String> = [
        "view",
        "edit",
        "info",
        "rename",
        "pin",
        "notes",
        "delete",
        "new-group-move",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    action_names.extend(
        folders
            .iter()
            .take(10)
            .map(|group| format!("move-group-{}", group.id)),
    );
    menu_actions(
        &menu,
        state,
        &action_names.iter().map(String::as_str).collect::<Vec<_>>(),
        item.id,
    );
    let model = gio::Menu::new();
    let details = gio::Menu::new();
    for (label, action, accelerator) in &crate::item_shortcuts::COMMANDS[..4] {
        let item = gio::MenuItem::new(
            Some(&command_label(action, label)),
            Some(&format!("menu.{action}")),
        );
        item.set_attribute_value("accel", Some(&accelerator.to_variant()));
        details.append_item(&item);
    }
    model.append_section(None, &details);
    let organize = gio::Menu::new();
    organize.append(Some(&tr("Move to Notes")), Some("menu.notes"));
    organize.append_submenu(Some(&tr("Move to folder")), &move_destinations(&folders));
    model.append_section(None, &organize);
    let destructive = gio::Menu::new();
    let delete_item = gio::MenuItem::new(Some(&tr("Delete…")), Some("menu.delete"));
    delete_item.set_attribute_value("accel", Some(&"Delete".to_variant()));
    destructive.append_item(&delete_item);
    model.append_section(None, &destructive);
    menu.set_menu_model(Some(&model));
    foot.append(&menu);
    card.append(&foot);
    {
        let state = state.clone();
        let id = item.id;
        edit.connect_clicked(move |_| editor(&state, Some(id)));
    }
    {
        let state = state.clone();
        let content = item.content.clone();
        paste.connect_clicked(move |_| state.paste(&content));
    }
    card
}

fn move_destinations(folders: &[Group]) -> gio::Menu {
    let destinations = gio::Menu::new();
    let quick = gio::Menu::new();
    for group in folders.iter().take(10) {
        quick.append(
            Some(&group.name),
            Some(&format!("menu.move-group-{}", group.id)),
        );
    }
    if !folders.is_empty() {
        destinations.append_section(None, &quick);
    }
    let other = gio::Menu::new();
    if folders.len() > 10 {
        other.append(Some(&tr("More Folders…")), Some("menu.pin"));
    }
    other.append(Some(&tr("New Folder…")), Some("menu.new-group-move"));
    destinations.append_section(None, &other);
    destinations
}

pub fn new_group(state: &Rc<State>) {
    let s = state.clone();
    prompt(state, "New Folder", "", move |name| {
        let result = s.store.borrow().create_group(&name);
        s.report(result);
    });
}

pub fn new_group_for_item(state: &Rc<State>, id: i64) {
    let s = state.clone();
    prompt(state, "New Folder", "", move |name| {
        let result = s.store.borrow_mut().create_group_for_item(id, &name);
        s.report(result);
    });
}
pub fn rename_item(state: &Rc<State>, id: i64) {
    let item = state.store.borrow().get(id);
    match item {
        Ok(item) => {
            let s = state.clone();
            prompt(state, "Rename Item", &item.title, move |name| {
                let result = s.store.borrow().rename(id, &name);
                s.report(result);
            });
        }
        Err(e) => error(state, &e.to_string()),
    }
}
pub fn delete_item(state: &Rc<State>, id: i64) {
    let s = state.clone();
    confirm(
        state,
        "Delete this item?",
        &tr("This permanently removes the item from your local library."),
        "Delete",
        move || {
            let result = s.store.borrow().delete(id);
            s.report(result);
        },
    );
}
pub fn move_item(state: &Rc<State>, id: i64) {
    let groups = match state.store.borrow().groups() {
        Ok(g) => g,
        Err(e) => {
            error(state, &e.to_string());
            return;
        }
    };
    let names: Vec<String> = groups.iter().map(group_name).collect();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let select = gtk::DropDown::from_strings(&name_refs);
    let dialog = adw::AlertDialog::builder()
        .heading(tr("Move to Folder"))
        .extra_child(&select)
        .build();
    dialog.add_response("cancel", &tr("Cancel"));
    dialog.add_response("move", &tr("Move"));
    dialog.set_close_response("cancel");
    let parent = state.app.active_window();
    let state = state.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "move" {
            if let Some(g) = groups.get(select.selected() as usize) {
                let result = state.store.borrow().move_item(id, g.id);
                state.report(result);
            }
        }
    });
    dialog.present(parent.as_ref());
}

pub fn about(state: &Rc<State>) {
    let parent = state.app.active_window();
    let dialog = about_dialog_with_check(crate::release_check::check);
    dialog.present(parent.as_ref());
}

pub(crate) fn about_dialog_with_check<F, Fut>(check_release: F) -> adw::Dialog
where
    F: Fn() -> Fut + 'static,
    Fut: std::future::Future<Output = crate::release_check::Outcome> + 'static,
{
    let dialog = adw::Dialog::builder()
        .title(tr("About"))
        .content_width(460)
        .content_height(620)
        .build();
    dialog.set_widget_name("about-dialog");
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    margins(&content, 20);
    let identity = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let icon = gtk::Image::from_icon_name(crate::APP_ID);
    icon.set_pixel_size(48);
    identity.append(&icon);
    let name = gtk::Label::new(Some("GnomeClipNotes"));
    name.add_css_class("title-1");
    name.set_wrap(true);
    name.set_justify(gtk::Justification::Center);
    identity.append(&name);
    let version = gtk::Label::new(Some(&trf(
        "Version {version}",
        &[("version", env!("CARGO_PKG_VERSION"))],
    )));
    version.add_css_class("dim-label");
    identity.append(&version);
    let status = gtk::Label::builder()
        .wrap(true)
        .justify(gtk::Justification::Center)
        .visible(false)
        .build();
    status.set_widget_name("update-status");
    identity.append(&status);
    let description =
        label("Clipboard history and Markdown notes.\nStored privately on this device.");
    description.set_wrap(true);
    description.set_justify(gtk::Justification::Center);
    description.add_css_class("dim-label");
    identity.append(&description);
    content.append(&identity);
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 8);
    actions.set_halign(gtk::Align::Center);
    let check = button("Check for Updates");
    check.add_css_class("pill");
    check.set_tooltip_text(Some(&tr("Checks GitHub for a public release only when requested. Notes and clipboard contents are never sent.")));
    let release = button("View Release");
    check.set_widget_name("check-for-updates");
    release.set_widget_name("view-release");
    release.add_css_class("suggested-action");
    release.set_visible(false);
    actions.append(&check);
    actions.append(&release);
    content.append(&actions);
    let toast_overlay = adw::ToastOverlay::new();
    let links = adw::PreferencesGroup::new();
    for (title, uri) in [
        (tr("Website"), crate::release_check::WEBSITE),
        ("GitHub".into(), crate::release_check::REPOSITORY),
        ("X".into(), "https://x.com/oleksiyML"),
    ] {
        let row = adw::ActionRow::builder()
            .title(title)
            .activatable(true)
            .build();
        row.add_suffix(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
        let weak_dialog = dialog.downgrade();
        let weak_toast = toast_overlay.downgrade();
        row.connect_activated(move |_| {
            if let (Some(dialog), Some(toast)) = (weak_dialog.upgrade(), weak_toast.upgrade()) {
                open_about_link(&dialog, &toast, uri);
            }
        });
        links.add(&row);
    }
    content.append(&links);
    let legal_group = adw::PreferencesGroup::new();
    let legal = adw::ActionRow::builder()
        .title(tr("Legal"))
        .subtitle("MIT License")
        .activatable(true)
        .build();
    legal.set_widget_name("about-legal");
    legal.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    let weak_dialog = dialog.downgrade();
    legal.connect_activated(move |_| {
        if let Some(parent) = weak_dialog.upgrade() {
            let license = adw::Dialog::builder()
                .title(tr("Legal"))
                .content_width(520)
                .content_height(480)
                .build();
            license.set_widget_name("about-license");
            let text = gtk::Label::new(Some(include_str!("../LICENSE")));
            text.set_wrap(true);
            text.set_selectable(true);
            text.set_xalign(0.0);
            margins(&text, 24);
            let toolbar = adw::ToolbarView::new();
            toolbar.add_top_bar(&adw::HeaderBar::new());
            toolbar.set_content(Some(
                &gtk::ScrolledWindow::builder()
                    .hscrollbar_policy(gtk::PolicyType::Never)
                    .child(&text)
                    .build(),
            ));
            license.set_child(Some(&toolbar));
            license.present(Some(&parent));
        }
    });
    legal_group.add(&legal);
    content.append(&legal_group);
    toolbar.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&content)
            .build(),
    ));
    toast_overlay.set_child(Some(&toolbar));
    dialog.set_child(Some(&toast_overlay));
    let destination = Rc::new(RefCell::new(None::<String>));
    let destination_open = destination.clone();
    let weak_dialog = dialog.downgrade();
    let weak_toast = toast_overlay.downgrade();
    release.connect_clicked(move |_| {
        let Some(uri) = destination_open.borrow().clone() else {
            return;
        };
        if let (Some(dialog), Some(toast)) = (weak_dialog.upgrade(), weak_toast.upgrade()) {
            open_about_link(&dialog, &toast, &uri);
        }
    });
    let job = Rc::new(RefCell::new(None::<glib::JoinHandle<()>>));
    let job_click = job.clone();
    check.connect_clicked(move |button| {
        button.set_sensitive(false);
        release.set_visible(false);
        destination.replace(None);
        status.set_visible(true);
        status.set_label(&tr("Checking for updates…"));
        let (button, release, status, destination) = (
            button.clone(),
            release.clone(),
            status.clone(),
            destination.clone(),
        );
        let request = check_release();
        job_click.replace(Some(glib::spawn_future_local(async move {
            use crate::release_check::Outcome;
            let text = match request.await {
                Outcome::Current => tr("Up to date"),
                Outcome::NewerBuild => tr("This build is newer than the latest public release."),
                Outcome::Available(version) => {
                    destination.replace(crate::release_check::release_url(&version));
                    release.set_visible(true);
                    trf("Update available: {version}", &[("version", &version)])
                }
                Outcome::NoRelease => {
                    tr("No public release found. The repository may not be available yet.")
                }
                Outcome::RateLimited => {
                    tr("GitHub refused or limited this request. Try again later.")
                }
                Outcome::Invalid => tr("GitHub returned unexpected release information."),
                Outcome::Failed => {
                    tr("Could not check for updates. Check your connection and try again.")
                }
                Outcome::Timeout => tr("The update check timed out. Try again later."),
            };
            status.set_label(&text);
            button.set_sensitive(true);
        })));
    });
    dialog.connect_closed(move |_| {
        if let Some(job) = job.borrow_mut().take() {
            job.abort();
        }
    });
    dialog
}

fn open_about_link(dialog: &adw::Dialog, toast: &adw::ToastOverlay, uri: &str) {
    let parent = dialog
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let weak_toast = toast.downgrade();
    gtk::UriLauncher::new(uri).launch(parent.as_ref(), None::<&gio::Cancellable>, move |result| {
        if result.is_err() {
            if let Some(overlay) = weak_toast.upgrade() {
                overlay.add_toast(adw::Toast::new(&tr(
                    "Could not open the link in your browser.",
                )));
            }
        }
    });
}

pub fn info(state: &Rc<State>, item: &Item) {
    let group = adw::PreferencesGroup::new();
    group.set_valign(gtk::Align::Start);
    group.set_widget_name("item-details-properties");
    for (name, value) in [
        (
            mark("Source"),
            if item.source.is_empty() {
                tr("Unknown")
            } else {
                item.source.clone()
            },
        ),
        (
            mark("Origin"),
            if item.origin == "manual" {
                tr("Note")
            } else {
                tr("Clipboard")
            },
        ),
        (mark("Created"), date(item.created_at)),
        (mark("Edited"), date(item.updated_at)),
        (mark("Last Copied"), date(item.copied_at)),
        (mark("Length"), content_length(&item.content)),
    ] {
        let row = adw::ActionRow::builder()
            .title(tr(name))
            .subtitle(value)
            .subtitle_selectable(true)
            .subtitle_lines(0)
            .build();
        row.add_css_class("property");
        row.set_use_markup(false);
        group.add(&row);
    }
    margins(&group, 24);
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &tr("Item Details"),
        &title(item),
    )));
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .child(&group)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(600)
            .build(),
    ));
    let dialog = adw::Dialog::builder()
        .title(tr("Item Details"))
        .content_width(440)
        .child(&toolbar)
        .build();
    dialog.set_widget_name("item-details");
    dialog.present(state.app.active_window().as_ref());
}

pub fn settings(state: &Rc<State>) {
    crate::preferences::show(state);
}

pub fn editor(state: &Rc<State>, id: Option<i64>) {
    open_editor(state, id, false);
}

pub fn view(state: &Rc<State>, id: i64) {
    open_editor(state, Some(id), true);
}

pub(crate) fn present_editor(window: &adw::ApplicationWindow) {
    window.present();
    if let Some(overlay) = window.content().and_downcast::<adw::ToastOverlay>() {
        let toast = adw::Toast::new(&tr("Already open"));
        toast.set_timeout(2);
        overlay.add_toast(toast);
    }
}

fn open_editor(state: &Rc<State>, id: Option<i64>, preview: bool) {
    if let Some(window) = id.and_then(|id| {
        state
            .editors
            .borrow()
            .get(&id)
            .and_then(glib::WeakRef::upgrade)
    }) {
        present_editor(&window);
        return;
    }
    if state.preview_factory.is_none() {
        if crate::editor_process::present_existing(state, id) {
            return;
        }
        if state.store.borrow().settings.preview_mode != "native" {
            crate::editor_process::open(state, id, preview);
            return;
        }
    }
    editor_in_process_mode(state, id, preview);
}

#[cfg(debug_assertions)]
pub(crate) fn editor_in_process(state: &Rc<State>, id: Option<i64>) {
    editor_in_process_mode(state, id, false);
}

pub(crate) fn editor_in_process_mode(state: &Rc<State>, id: Option<i64>, preview: bool) {
    if let Some(window) = id.and_then(|id| {
        state
            .editors
            .borrow()
            .get(&id)
            .and_then(glib::WeakRef::upgrade)
    }) {
        present_editor(&window);
        return;
    }
    let item = if let Some(id) = id {
        match state.store.borrow().get(id) {
            Ok(item) => Some(item),
            Err(e) => {
                error(state, &e.to_string());
                return;
            }
        }
    } else {
        None
    };
    let note_id = Rc::new(Cell::new(id));
    let Some(editor_lifetime) = state.update.track(crate::update::EditorKind::Native) else {
        return;
    };
    let editor_lifetime = Rc::new(RefCell::new(Some(editor_lifetime)));
    let note_name = Rc::new(RefCell::new(
        item.as_ref().map(|i| i.title.clone()).unwrap_or_default(),
    ));
    let saved_text = Rc::new(RefCell::new(
        item.as_ref().map(|i| i.content.clone()).unwrap_or_default(),
    ));
    let editor_title = if item.is_some() {
        tr("Edit Item")
    } else {
        tr("New Note")
    };
    let window = adw::ApplicationWindow::builder()
        .application(&state.app)
        .title(editor_title)
        .default_width(780)
        .default_height(720)
        .build();
    window.set_icon_name(Some(crate::APP_ID));
    if let Some(id) = id {
        state.editors.borrow_mut().insert(id, window.downgrade());
    }
    {
        let weak = Rc::downgrade(state);
        let note_id = note_id.clone();
        let editor_lifetime = editor_lifetime.clone();
        window.connect_destroy(move |window| {
            editor_lifetime.borrow_mut().take();
            if let (Some(state), Some(id)) = (weak.upgrade(), note_id.get()) {
                let matches = state
                    .editors
                    .borrow()
                    .get(&id)
                    .and_then(glib::WeakRef::upgrade)
                    .as_ref()
                    == Some(window);
                if matches {
                    state.editors.borrow_mut().remove(&id);
                }
            }
        });
    }
    let root = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let heading = adw::WindowTitle::new("", "");
    heading.set_widget_name("editor-title");
    header.set_title_widget(Some(&heading));
    let save = button("Save");
    save.set_child(Some(
        &adw::ButtonContent::builder()
            .icon_name("document-save-symbolic")
            .label(tr("Save"))
            .build(),
    ));
    save.set_tooltip_text(Some(&tr("Save Note (Ctrl+S)")));
    save.set_widget_name("save-note");
    header.pack_end(&save);
    root.add_top_bar(&header);
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add_css_class("view");
    let paper = gtk::Box::new(gtk::Orientation::Vertical, 0);
    paper.add_css_class("editor-paper");
    margins(&paper, 12);
    paper.set_overflow(gtk::Overflow::Hidden);
    let clamp = adw::Clamp::builder()
        .maximum_size(760)
        .tightening_threshold(520)
        .child(&paper)
        .build();
    page.append(&clamp);
    clamp.set_vexpand(true);
    root.set_content(Some(&page));
    let stack = gtk::Stack::new();
    stack.set_widget_name("editor-pages");
    let modes = gtk::StackSwitcher::builder()
        .stack(&stack)
        .halign(gtk::Align::Center)
        .build();
    modes.set_widget_name("editor-modes");
    margins(&modes, 6);
    root.add_top_bar(&modes);
    stack.set_vexpand(true);
    paper.append(&stack);
    let buffer = gtk::TextBuffer::new(None);
    buffer.set_enable_undo(true);
    buffer.set_max_undo_levels(200);
    buffer.set_text(item.as_ref().map(|i| i.content.as_str()).unwrap_or(""));
    buffer.set_modified(false);
    let text = gtk::TextView::builder()
        .buffer(&buffer)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(24)
        .bottom_margin(24)
        .left_margin(24)
        .right_margin(24)
        .monospace(true)
        .build();
    text.add_css_class("editor-text");
    let scroll = gtk::ScrolledWindow::builder()
        .child(&text)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    stack.add_titled(&scroll, Some("edit"), &tr("Editor"));
    let preview_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    preview_host.set_vexpand(true);
    stack.add_titled(&preview_host, Some("preview"), &tr("Preview"));
    let dirty = Rc::new(Cell::new(false));
    let sync: Rc<dyn Fn()> = {
        let dirty = dirty.clone();
        let buffer = buffer.downgrade();
        let window = window.downgrade();
        let save = save.downgrade();
        let note_id = note_id.clone();
        let note_name = note_name.clone();
        let saved_text = saved_text.clone();
        Rc::new(move || {
            let (Some(buffer), Some(window), Some(save)) =
                (buffer.upgrade(), window.upgrade(), save.upgrade())
            else {
                return;
            };
            let changed = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .as_str()
                != saved_text.borrow().as_str();
            dirty.set(changed);
            save.set_sensitive(changed);
            let name = if note_id.get().is_none() {
                tr("New Note")
            } else if note_name.borrow().trim().is_empty() {
                tr("Note")
            } else {
                note_name.borrow().clone()
            };
            let title = if changed { format!("● {name}") } else { name };
            heading.set_title(&title);
            window.set_title(Some(&title));
        })
    };
    sync();
    {
        let sync = sync.clone();
        buffer.connect_changed(move |_| sync());
    }
    {
        let buffer = buffer.clone();
        let factory = state.preview_factory.clone();
        let full = RefCell::new(None::<crate::Preview>);
        let window = window.downgrade();
        stack.connect_visible_child_name_notify(move |stack| {
            if stack.visible_child_name().as_deref() == Some("preview") {
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                if let Some(factory) = factory.as_ref() {
                    if full.borrow().is_none() {
                        let Some(window) = window.upgrade() else {
                            return;
                        };
                        let preview = factory(&window);
                        preview_host.append(&preview.widget);
                        *full.borrow_mut() = Some(preview);
                    }
                    (full.borrow().as_ref().unwrap().set_content)(&text);
                } else {
                    while let Some(child) = preview_host.first_child() {
                        preview_host.remove(&child);
                    }
                    let preview = crate::preview_native::build(&text);
                    let scroll = gtk::ScrolledWindow::builder()
                        .child(&preview)
                        .hscrollbar_policy(gtk::PolicyType::Never)
                        .vexpand(true)
                        .build();
                    preview_host.append(&scroll);
                }
            }
        });
    }
    let keys = gtk::EventControllerKey::new();
    {
        let buffer = buffer.clone();
        let save = save.clone();
        keys.connect_key_pressed(move |_, key, _, mods| {
            if mods.contains(gtk::gdk::ModifierType::CONTROL_MASK) && key == gtk::gdk::Key::s {
                if save.is_sensitive() {
                    save.emit_clicked();
                }
                return glib::Propagation::Stop;
            }
            if key == gtk::gdk::Key::Return
                && !mods.intersects(
                    gtk::gdk::ModifierType::CONTROL_MASK
                        | gtk::gdk::ModifierType::SHIFT_MASK
                        | gtk::gdk::ModifierType::ALT_MASK,
                )
                && buffer.selection_bounds().is_none()
            {
                let mut cursor = buffer.iter_at_offset(buffer.cursor_position());
                let mut start = cursor;
                start.set_line_offset(0);
                let line = buffer.text(&start, &cursor, false);
                let (prefix, remove) = continuation(&line);
                if !prefix.is_empty() || remove > 0 {
                    buffer.begin_user_action();
                    if remove > 0 {
                        buffer.delete(&mut start, &mut cursor);
                    } else {
                        buffer.insert(&mut cursor, &format!("\n{prefix}"));
                    }
                    buffer.end_user_action();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
    }
    text.add_controller(keys);
    {
        let dialog_parent = window.downgrade();
        let save_id = note_id.clone();
        let dirty = dirty.clone();
        let state = state.clone();
        let window = window.downgrade();
        let buffer = buffer.clone();
        let note_id = note_id.clone();
        let note_name = note_name.clone();
        let saved_text = saved_text.clone();
        let commit: Rc<dyn Fn(String)> = Rc::new(move |name| {
            let Some(window) = window.upgrade() else {
                return;
            };
            let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            // Renaming belongs to the library; don't overwrite a newer library name.
            let name = if let Some(id) = note_id.get() {
                match state.store.borrow().get(id) {
                    Ok(item) => item.title,
                    Err(e) => {
                        error(&state, &e.to_string());
                        return;
                    }
                }
            } else {
                name.trim().to_string()
            };
            let result = state
                .store
                .borrow_mut()
                .save_item(note_id.get(), &name, &text);
            match result {
                Ok(id) => {
                    note_id.set(Some(id));
                    *note_name.borrow_mut() = name;
                    *saved_text.borrow_mut() = text.to_string();
                    state.editors.borrow_mut().insert(id, window.downgrade());
                    sync();
                    state.changed();
                }
                Err(e) => error(&state, &e.to_string()),
            }
        });
        save.connect_clicked(move |_| {
            if !dirty.get() {
                return;
            }
            if save_id.get().is_some() {
                commit(String::new());
                return;
            }
            let Some(parent) = dialog_parent.upgrade() else {
                return;
            };
            let dialog = adw::AlertDialog::builder()
                .heading(tr("Save Note"))
                .body(tr("Name is optional."))
                .build();
            let entry = gtk::Entry::new();
            entry.set_activates_default(true);
            dialog.set_extra_child(Some(&entry));
            dialog.add_response("cancel", &tr("Cancel"));
            dialog.add_response("save", &tr("Save"));
            dialog.set_default_response(Some("save"));
            dialog.set_close_response("cancel");
            dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
            let commit = commit.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "save" {
                    commit(entry.text().to_string());
                }
            });
            dialog.present(Some(&parent));
        });
    }
    {
        let state = state.clone();
        let dirty = dirty.clone();
        let note_id = note_id.clone();
        let editor_lifetime = editor_lifetime.clone();
        window.connect_close_request(move |window| {
            if !dirty.get() {
                editor_lifetime.borrow_mut().take();
                // Closing a GTK window and finalizing its object are different:
                // callbacks/tests may still hold a reference to the closed object.
                if let Some(id) = note_id.get() {
                    let matches = state
                        .editors
                        .borrow()
                        .get(&id)
                        .and_then(glib::WeakRef::upgrade)
                        .as_ref()
                        == Some(window);
                    if matches {
                        state.editors.borrow_mut().remove(&id);
                    }
                }
                return glib::Propagation::Proceed;
            }
            let window = window.clone();
            let dirty = dirty.clone();
            confirm(
                &state,
                "Discard unsaved changes?",
                &tr("Your changes have not been saved."),
                "Discard",
                move || {
                    dirty.set(false);
                    window.close();
                },
            );
            glib::Propagation::Stop
        });
    }
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&root));
    window.set_content(Some(&overlay));
    if preview {
        stack.set_visible_child_name("preview");
    }
    window.present();
    if !preview {
        text.grab_focus();
    }
}

fn continuation(line: &str) -> (String, usize) {
    let indent: String = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let rest = &line[indent.len()..];
    for marker in ["- [ ] ", "- [x] ", "- ", "* ", "+ "] {
        if let Some(body) = rest.strip_prefix(marker) {
            return if body.trim().is_empty() {
                (String::new(), line.chars().count())
            } else {
                (
                    format!(
                        "{indent}{}",
                        if marker.starts_with("- [") {
                            "- [ ] "
                        } else {
                            marker
                        }
                    ),
                    0,
                )
            };
        }
    }
    if let Some((number, body)) = rest.split_once(". ") {
        if let Ok(n) = number.parse::<u64>() {
            return if body.trim().is_empty() {
                (String::new(), line.chars().count())
            } else {
                (format!("{indent}{}. ", n.saturating_add(1)), 0)
            };
        }
    }
    (indent, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn move_menu_preserves_order_and_limits_quick_folders() {
        for count in [0, 1, 10, 11] {
            let folders: Vec<_> = (0..count)
                .map(|i| Group {
                    id: 100 - i,
                    name: format!("Folder {i}"),
                })
                .collect();
            let menu = move_destinations(&folders);
            let footer = menu.item_link(menu.n_items() - 1, "section").unwrap();
            assert_eq!(footer.n_items(), if count > 10 { 2 } else { 1 });
            assert_eq!(
                footer
                    .item_attribute_value(footer.n_items() - 1, "action", None)
                    .unwrap()
                    .str(),
                Some("menu.new-group-move")
            );
            if count > 10 {
                assert_eq!(
                    footer
                        .item_attribute_value(0, "action", None)
                        .unwrap()
                        .str(),
                    Some("menu.pin")
                );
            }
            if count == 0 {
                assert_eq!(menu.n_items(), 1);
                continue;
            }
            let quick = menu.item_link(0, "section").unwrap();
            assert_eq!(quick.n_items(), count.min(10) as i32);
            for (i, group) in folders.iter().take(10).enumerate() {
                assert_eq!(
                    quick
                        .item_attribute_value(i as i32, "label", None)
                        .unwrap()
                        .str(),
                    Some(group.name.as_str())
                );
                assert_eq!(
                    quick
                        .item_attribute_value(i as i32, "action", None)
                        .unwrap()
                        .str(),
                    Some(format!("menu.move-group-{}", group.id).as_str())
                );
            }
        }
    }
    #[test]
    fn lists_continue_and_empty_markers_end() {
        assert_eq!(continuation("  - task"), ("  - ".into(), 0));
        assert_eq!(continuation("9. item"), ("10. ".into(), 0));
        assert_eq!(continuation("- [x] done"), ("- [ ] ".into(), 0));
        assert_eq!(continuation("  - "), (String::new(), 4));
    }
}
