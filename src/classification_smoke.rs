//! Classification controls exercised only by the existing isolated UI harness.
use crate::{model::ItemClassification, preferences, smoke, ui, State};
use adw::prelude::*;
use std::{path::Path, rc::Rc, time::Duration};

fn widget(root: &impl IsA<gtk::Widget>, name: &str) -> gtk::Widget {
    smoke::find(root.as_ref(), &|w| w.widget_name() == name)
        .unwrap_or_else(|| panic!("Missing classification smoke widget: {name}"))
}

fn click(root: &impl IsA<gtk::Widget>, name: &str) {
    widget(root, name)
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
}

async fn settle() {
    glib::timeout_future(Duration::from_millis(400)).await;
}

fn check_metadata_width() {
    let long = crate::card_metadata::MetadataTag::new(
        "folder-symbolic",
        &"A long folder or category name ".repeat(8),
        "metadata-width-check",
    );
    let short =
        crate::card_metadata::MetadataTag::new("folder-symbolic", "Notes", "metadata-width-check");
    for tag in [&long, &short] {
        let row = tag.first_child().unwrap();
        let natural = row.measure(gtk::Orientation::Horizontal, -1).1;
        for width in [240, 800] {
            let height = tag.measure(gtk::Orientation::Vertical, width).1;
            tag.allocate(width, height, -1, None);
            assert_eq!(
                row.compute_bounds(tag).unwrap().width() as i32,
                natural.min(width)
            );
        }
        let (minimum, preferred, _, _) = tag.measure(gtk::Orientation::Horizontal, -1);
        assert_eq!(minimum, preferred, "Metadata must not widen the grid");
    }
}

fn settings(window: &adw::ApplicationWindow) -> adw::Dialog {
    let dialog = window.visible_dialog().expect("Settings dialog");
    widget(&dialog, "settings-pages")
        .downcast::<gtk::Stack>()
        .unwrap()
        .set_visible_child_name("categories");
    dialog
}

fn dropdown(root: &impl IsA<gtk::Widget>, name: &str, label: &str) {
    let dropdown = widget(root, name).downcast::<gtk::DropDown>().unwrap();
    let model = dropdown.model().unwrap();
    let index = (0..model.n_items())
        .find(|&i| {
            model
                .item(i)
                .unwrap()
                .downcast::<gtk::StringObject>()
                .unwrap()
                .string()
                == label
        })
        .unwrap_or_else(|| panic!("Missing filter choice: {label}"));
    dropdown.set_selected(index);
}

async fn search_dropdown(control: &gtk::DropDown, term: &str, expected: &str) {
    control.emit_activate();
    settle().await;
    let popup = smoke::find(control.upcast_ref(), &|w| w.is::<gtk::Popover>())
        .unwrap()
        .downcast::<gtk::Popover>()
        .unwrap();
    let entry = smoke::find(popup.upcast_ref(), &|w| w.is::<gtk::SearchEntry>())
        .unwrap()
        .downcast::<gtk::SearchEntry>()
        .unwrap();
    let list = smoke::find(popup.upcast_ref(), &|w| w.is::<gtk::ListView>())
        .unwrap()
        .downcast::<gtk::ListView>()
        .unwrap();
    let model = list.model().unwrap();
    entry.set_text("no-such-category-7fb8");
    settle().await;
    assert_eq!(model.n_items(), 0, "Unknown name must yield no choices");
    entry.set_text(term);
    settle().await;
    assert_eq!(
        model.n_items(),
        1,
        "Partial name must narrow dropdown choices"
    );
    assert_eq!(
        model
            .item(0)
            .unwrap()
            .downcast::<gtk::StringObject>()
            .unwrap()
            .string(),
        expected
    );
    list.emit_by_name::<()>("activate", &[&0u32]);
    settle().await;
    assert_eq!(
        control
            .selected_item()
            .unwrap()
            .downcast::<gtk::StringObject>()
            .unwrap()
            .string(),
        expected
    );
}

pub(crate) async fn run(state: Rc<State>, dir: &Path) {
    check_metadata_width();
    println!("Classification UI: Settings and assignment");
    // Reopen to start with ordinary selection and empty search/collection filters.
    let previous = state.window.borrow().as_ref().unwrap().clone();
    previous.close();
    settle().await;
    let (category, child, id) = {
        let mut store = state.store.borrow_mut();
        let category = store
            .create_category("Smoke category — a deliberately long label")
            .unwrap();
        let child = store
            .create_subcategory(category, "What did I learn? — a long question")
            .unwrap();
        let id = store
            .save_item(None, "Classification smoke target", "Unchanged smoke body")
            .unwrap();
        for n in 0..105 {
            let body = if n == 104 {
                "A full preview needs its own boundary from the category. This deliberately longer note fills several lines so that the label below cannot rely on empty space to remain distinct from the content. More material follows in the complete note."
            } else {
                "Fixture body"
            };
            let item = store
                .save_item(None, &format!("Classification fixture {n}"), body)
                .unwrap();
            store
                .set_item_classification(
                    item,
                    &ItemClassification {
                        category_id: Some(category),
                        subcategory_id: Some(child),
                        comment: "matching answer".into(),
                    },
                )
                .unwrap();
        }
        (category, child, id)
    };
    ui::show(&state);
    let window = state.window.borrow().as_ref().unwrap().clone();
    assert!(!state.store.borrow().settings.classification_enabled);
    preferences::show(&state);
    let dialog = settings(&window);
    let toggle = widget(&dialog, "classification-enabled")
        .downcast::<adw::SwitchRow>()
        .unwrap();
    toggle.set_active(true);
    assert!(state.store.borrow().settings.classification_enabled);
    widget(&dialog, "comments-enabled")
        .downcast::<adw::SwitchRow>()
        .unwrap()
        .set_active(true);
    let list = widget(&dialog, "classification-categories");
    let row = widget(&list, &format!("classification-category-{category}"))
        .downcast::<adw::ExpanderRow>()
        .unwrap();
    row.set_expanded(true);
    settle().await;
    assert!(smoke::find(&list, &|w| w
        .downcast_ref::<adw::ActionRow>()
        .is_some_and(|r| r.title().contains("What did I learn?")))
    .is_some());
    smoke::snapshot_widget(&dialog, &dir.join("classification-settings-light.png"));
    dialog.close();
    settle().await;

    click(&window, "collection-1");
    let search = smoke::find(window.upcast_ref(), &|w| w.is::<gtk::SearchEntry>())
        .unwrap()
        .downcast::<gtk::SearchEntry>()
        .unwrap();
    search.set_text("Classification smoke target");
    settle().await;
    let menu = || {
        let card = widget(&window, &format!("library-item-{id}"));
        widget(&card, "card-actions-menu")
            .downcast::<gtk::MenuButton>()
            .unwrap()
    };
    let card_menu = menu();
    card_menu.popup();
    settle().await;
    let popup = card_menu.popover().unwrap();
    assert!(
        popup.width() < 360,
        "Long questions must not widen the root menu"
    );
    smoke::snapshot_widget(&popup, &dir.join("classification-card-menu.png"));
    card_menu.popdown();
    settle().await;
    let filters = widget(&window, "library-filters")
        .downcast::<gtk::MenuButton>()
        .unwrap();
    filters.popup();
    settle().await;
    let category_filter = widget(&window, "library-filter-category");
    smoke::snapshot_widget(
        &filters.popover().unwrap(),
        &dir.join("classification-filter-menu.png"),
    );
    assert!(
        category_filter.width() >= 240,
        "Filters must leave room for labels: actual width {}",
        category_filter.width()
    );
    search_dropdown(
        &category_filter.downcast::<gtk::DropDown>().unwrap(),
        "DELIBERATELY",
        "Smoke category — a deliberately long label",
    )
    .await;
    search_dropdown(
        &widget(&window, "library-filter-subcategory")
            .downcast::<gtk::DropDown>()
            .unwrap(),
        "LEARN",
        "What did I learn? — a long question",
    )
    .await;
    dropdown(&window, "library-filter-category", "All categories");
    filters.popdown();
    settle().await;
    let assign = |category: i64, child: i64| {
        menu()
            .activate_action(
                "classification.select",
                Some(&(category, child).to_variant()),
            )
            .unwrap();
    };
    let annotation = || state.store.borrow().item_classification(id).unwrap();
    assign(category, 0);
    assert_eq!(annotation().category_id, Some(category));
    assign(category, child);
    let comment = window
        .visible_dialog()
        .expect("Child opens optional comment");
    let text = widget(&comment, "classification-comment-text")
        .downcast::<gtk::TextView>()
        .unwrap();
    text.buffer().set_text("Cancelled answer");
    let cancel = smoke::find(comment.upcast_ref(), &|w| {
        w.downcast_ref::<gtk::Button>()
            .is_some_and(|b| b.label().as_deref() == Some("Cancel"))
    })
    .unwrap()
    .downcast::<gtk::Button>()
    .unwrap();
    cancel.emit_clicked();
    settle().await;
    assert_eq!(annotation().subcategory_id, None);
    assert!(annotation().comment.is_empty());
    assign(category, child);
    let comment = window.visible_dialog().unwrap();
    widget(&comment, "classification-comment-text")
        .downcast::<gtk::TextView>()
        .unwrap()
        .buffer()
        .set_text("matching answer\nSecond line");
    settle().await;
    smoke::snapshot_widget(&comment, &dir.join("classification-comment-light.png"));
    click(&comment, "classification-comment-save");
    settle().await;
    assert_eq!(annotation().subcategory_id, Some(child));
    let card = widget(&window, &format!("library-item-{id}"));
    let tag = widget(&card, "card-classification");
    assert!(tag.is::<crate::card_metadata::MetadataTag>());
    assert!(tag.first_child().unwrap().has_css_class("metadata-tag"));
    assert!(!tag.is_focusable());
    click(&card, "card-comment");
    window
        .visible_dialog()
        .expect("Comment icon opens saved comment")
        .close();
    settle().await;
    smoke::snapshot_widget(&card, &dir.join("classification-annotated-card.png"));
    assign(0, 0);
    let value = annotation();
    assert_eq!(value.category_id, None);
    assert_eq!(value.comment, "matching answer\nSecond line");
    let card = widget(&window, &format!("library-item-{id}"));
    assert!(smoke::find(&card, &|w| w.widget_name() == "card-classification").is_none());
    assert!(widget(&card, "card-comment").get_visible());
    assert_eq!(
        state.store.borrow().get(id).unwrap().content,
        "Unchanged smoke body"
    );

    println!("Classification UI: assignment passed; filtering and selection");
    search.set_text("");
    settle().await;
    dropdown(
        &window,
        "library-filter-category",
        "Smoke category — a deliberately long label",
    );
    settle().await;
    dropdown(
        &window,
        "library-filter-subcategory",
        "What did I learn? — a long question",
    );
    let comment_filter = widget(&window, "library-filter-comment")
        .downcast::<gtk::Entry>()
        .unwrap();
    comment_filter.set_text("matching answer");
    settle().await;
    println!("Classification UI: filters applied");
    click(&window, "library-select");
    click(&window, "library-select-all");
    let count = widget(&window, "library-selection-count")
        .downcast::<gtk::Label>()
        .unwrap();
    assert_eq!(count.text().as_str(), "105 selected");
    let cards = widget(&window, "library-cards")
        .downcast::<gtk::FlowBox>()
        .unwrap();
    assert!(
        cards.observe_children().n_items() < 105,
        "Select all must span pages"
    );
    window.set_default_size(560, 720);
    settle().await;
    smoke::snapshot_widget(
        &window,
        &dir.join("classification-library-light-narrow.png"),
    );
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    settle().await;
    smoke::snapshot_widget(&window, &dir.join("classification-library-dark-narrow.png"));
    // Changing a criterion invalidates the entire checked set immediately.
    comment_filter.set_text("no matching answer");
    assert_eq!(count.text().as_str(), "0 selected");
    settle().await;
    comment_filter.set_text("matching answer");
    settle().await;
    click(&window, "library-select-all");
    assert_eq!(count.text().as_str(), "105 selected");
    preferences::show(&state);
    let dialog = settings(&window);
    settle().await;
    smoke::snapshot_widget(&dialog, &dir.join("classification-settings-dark.png"));
    widget(&dialog, "classification-enabled")
        .downcast::<adw::SwitchRow>()
        .unwrap()
        .set_active(false);
    assert_eq!(count.text().as_str(), "0 selected");
    assert!(!widget(&window, "library-filter-category").get_visible());
    assert!(comment_filter.get_visible());
    assert_eq!(comment_filter.text().as_str(), "matching answer");
    widget(&dialog, "comments-enabled")
        .downcast::<adw::SwitchRow>()
        .unwrap()
        .set_active(false);
    dialog.close();
    settle().await;
    assert!(!widget(&window, "library-classification-filters").get_visible());
    assert_eq!(
        state.store.borrow().category_item_count(category).unwrap(),
        105
    );
    assert_eq!(annotation().comment, value.comment);
    assert_eq!(comment_filter.text().as_str(), "");
    click(&window, "library-select-all");
    let total = state
        .store
        .borrow()
        .matching_item_ids(&crate::model::Query {
            group_id: 1,
            ..Default::default()
        })
        .unwrap()
        .len();
    assert_eq!(count.text().as_str(), format!("{total} selected"));
    click(&window, "library-select");
    preferences::show(&state);
    let dialog = settings(&window);
    widget(&dialog, "classification-enabled")
        .downcast::<adw::SwitchRow>()
        .unwrap()
        .set_active(true);
    dialog.close();
    search.set_text("Classification smoke target");
    settle().await;
    assign(category, child);
    assert!(
        window.visible_dialog().is_none(),
        "Categories alone do not prompt for a comment"
    );
    assert_eq!(annotation().subcategory_id, Some(child));
    assert_eq!(annotation().comment, value.comment);
    click(&window, "collection--1");
    search.set_text("Classification smoke target");
    settle().await;
    let card = widget(&window, &format!("library-item-{id}"));
    for name in ["card-collection", "card-classification"] {
        let tag = widget(&card, name);
        assert!(tag.is::<crate::card_metadata::MetadataTag>());
        assert!(tag.first_child().unwrap().has_css_class("metadata-tag"));
    }
    smoke::snapshot_widget(&card, &dir.join("metadata-tags-dark.png"));
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    settle().await;
    smoke::snapshot_widget(&card, &dir.join("metadata-tags-light.png"));
    println!("PASS classification Settings, assignment/cancel/comment, filters, all-page selection and disable preservation");
}
