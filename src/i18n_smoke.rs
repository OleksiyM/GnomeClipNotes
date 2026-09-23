//! Explicit pseudolocale UI smoke test. The CLI wiring lives in `lib.rs`.

use crate::{
    export_ui::ExportDialog,
    i18n::tr,
    smoke::{find, snapshot_widget},
    State,
};
use adw::prelude::*;
use std::{path::PathBuf, rc::Rc, time::Duration};

async fn settle() {
    glib::timeout_future(Duration::from_millis(250)).await;
}

fn test_root() -> PathBuf {
    let root =
        PathBuf::from(std::env::var("GCN_I18N_TEST_DIR").expect("GCN_I18N_TEST_DIR is required"));
    assert_eq!(root.parent(), Some(std::path::Path::new("/tmp")));
    assert!(root
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("gnome-clip-notes-i18n.")));
    assert_eq!(
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        Some(root.join("data")),
        "i18n smoke must use its isolated data directory"
    );
    root
}

fn assert_usable_allocations(widget: &gtk::Widget) {
    if widget.is_mapped() {
        if let Some(button) = widget.downcast_ref::<gtk::Button>() {
            assert!(
                button.width() >= 20 && button.height() >= 16,
                "visible button has unusable allocation: {:?} ({}×{})",
                button.label(),
                button.width(),
                button.height()
            );
        }
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if !label.text().is_empty() {
                assert!(
                    label.width() >= 8 && label.height() >= 8,
                    "visible label has unusable allocation: {:?} ({}×{})",
                    label.text(),
                    label.width(),
                    label.height()
                );
            }
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        assert_usable_allocations(&current);
        child = current.next_sibling();
    }
}

async fn snapshot_settings_pages(dialog: &adw::Dialog, root: &std::path::Path) {
    let pages = find(dialog.upcast_ref(), &|widget| {
        widget.widget_name() == "settings-pages"
    })
    .expect("settings page stack")
    .downcast::<gtk::Stack>()
    .unwrap();
    for name in [
        "general",
        "history",
        "privacy",
        "shortcuts",
        "folders",
        "data",
    ] {
        pages.set_visible_child_name(name);
        settle().await;
        assert_eq!(pages.visible_child_name().as_deref(), Some(name));
        assert_usable_allocations(dialog.upcast_ref());
        snapshot_widget(dialog, &root.join(format!("i18n-settings-{name}.png")));
    }
    pages.set_visible_child_name("general");
    settle().await;
}

pub fn run(state: &Rc<State>) {
    let root = test_root();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        std::process::exit(1);
    }));

    assert_ne!(tr("Settings"), "Settings", "pseudolocale is not active");
    let (note, folder) = {
        let mut store = state.store.borrow_mut();
        store.create_group("Settings").unwrap();
        let folder = store
            .groups()
            .unwrap()
            .into_iter()
            .find(|group| group.id > 1 && group.name == "Settings")
            .unwrap()
            .id;
        let note = store
            .save_item(
                None,
                "Pseudolocale note",
                "A note body that must remain user data, even in the pseudolocale.",
            )
            .unwrap();
        store.move_item(note, folder).unwrap();
        (note, folder)
    };

    crate::ui::show(state);
    state.changed();
    let state = state.clone();
    glib::MainContext::default().spawn_local(async move {
        let library = state.window.borrow().as_ref().unwrap().clone();
        settle().await;
        let folder_row = find(library.upcast_ref(), &|widget| {
            widget.widget_name() == format!("collection-{folder}")
        })
        .expect("user folder row");
        assert!(find(&folder_row, &|widget| widget
            .downcast_ref::<gtk::Label>()
            .is_some_and(|label| label.text() == "Settings"))
        .is_some());
        assert!(find(&folder_row, &|widget| widget
            .downcast_ref::<gtk::Label>()
            .is_some_and(|label| label.text() == tr("Settings")))
        .is_none());
        assert_usable_allocations(library.upcast_ref());
        snapshot_widget(&library, &root.join("i18n-library.png"));

        crate::preferences::show(&state);
        settle().await;
        let settings = library
            .visible_dialog()
            .expect("settings dialog")
            .downcast::<adw::Dialog>()
            .unwrap();
        snapshot_settings_pages(&settings, &root).await;

        let language = find(settings.upcast_ref(), &|widget| {
            widget
                .downcast_ref::<adw::ComboRow>()
                .is_some_and(|row| row.title() == tr("Language"))
        })
        .expect("Language row")
        .downcast::<adw::ComboRow>()
        .unwrap();
        let model = language
            .model()
            .expect("Language model")
            .downcast::<gtk::StringList>()
            .unwrap();
        assert_eq!(model.string(0).as_deref(), Some(tr("System default").as_str()));
        assert_eq!(model.string(1).as_deref(), Some("English"));
        assert_eq!(language.selected(), 0);
        let active_translation = tr("New Note");
        language.set_selected(1);
        assert_eq!(state.store.borrow().settings.language, "en");
        assert_eq!(tr("New Note"), active_translation);
        settings.close();
        settle().await;

        let about = crate::ui::about_dialog_with_check(|| async { panic!("Opening About must not check updates") });
        about.present(Some(&library));
        settle().await;
        assert_usable_allocations(about.upcast_ref());
        snapshot_widget(&about, &root.join("i18n-about.png"));
        about.close();
        settle().await;

        crate::ui::editor_in_process(&state, Some(note));
        settle().await;
        let editor = state
            .app
            .active_window()
            .expect("native editor window")
            .downcast::<adw::ApplicationWindow>()
            .unwrap();
        assert_ne!(editor, library);
        let text = find(editor.upcast_ref(), &|widget| widget.is::<gtk::TextView>())
            .expect("editor text view")
            .downcast::<gtk::TextView>()
            .unwrap();
        assert_eq!(
            text.buffer()
                .text(
                    &text.buffer().start_iter(),
                    &text.buffer().end_iter(),
                    false,
                )
                .as_str(),
            "A note body that must remain user data, even in the pseudolocale."
        );
        assert_eq!(tr("New Note"), active_translation);
        assert_usable_allocations(editor.upcast_ref());
        snapshot_widget(&editor, &root.join("i18n-native-editor.png"));
        editor.close();
        settle().await;

        let export = ExportDialog::new(&state).unwrap();
        export.dialog.present(Some(&library));
        settle().await;
        assert!(find(export.dialog.upcast_ref(), &|widget| {
            widget.widget_name() == format!("export-group-{folder}")
        })
        .is_some());
        assert!(find(export.dialog.upcast_ref(), &|widget| widget
            .downcast_ref::<gtk::Label>()
            .is_some_and(|label| label.text() == "Settings"))
        .is_some());
        assert_usable_allocations(export.dialog.upcast_ref());
        snapshot_widget(&export.dialog, &root.join("i18n-export.png"));
        export.dialog.close();
        settle().await;

        crate::preferences::show(&state);
        settle().await;
        let settings = library
            .visible_dialog()
            .expect("reopened settings dialog")
            .downcast::<adw::Dialog>()
            .unwrap();
        let language = find(settings.upcast_ref(), &|widget| {
            widget
                .downcast_ref::<adw::ComboRow>()
                .is_some_and(|row| row.title() == tr("Language"))
        })
        .expect("reopened Language row")
        .downcast::<adw::ComboRow>()
        .unwrap();
        assert_eq!(language.selected(), 1);
        language.set_selected(0);
        assert_eq!(state.store.borrow().settings.language, "system");
        assert_eq!(tr("New Note"), active_translation);
        settings.close();
        println!("PASS pseudolocale library, settings, language persistence, native editor and export UI");
        state.app.quit();
    });
}
