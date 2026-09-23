//! Explicit test-only driver; used exclusively with private XDG and D-Bus paths.
use crate::State;
use adw::prelude::*;
use std::{path::Path, rc::Rc, time::Duration};

fn find(widget: &gtk::Widget, predicate: &impl Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    if predicate(widget) {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find(&current, predicate) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn click_response(dialog: &adw::AlertDialog, label: &str) {
    find(dialog.upcast_ref(), &|w| {
        w.downcast_ref::<gtk::Button>()
            .is_some_and(|button| button.label().as_deref() == Some(label))
    })
    .expect("Dialog response button")
    .downcast::<gtk::Button>()
    .unwrap()
    .emit_clicked();
}

async fn assert_update_refused(state: &State, text: &gtk::TextView, expected: &str) {
    let connection = state.app.dbus_connection().unwrap();
    let status_reply = connection
        .call_future(
            Some(crate::APP_ID),
            crate::PATH,
            crate::INTERFACE,
            "GetUpdateStatus",
            None,
            None,
            gio::DBusCallFlags::NONE,
            3000,
        )
        .await
        .unwrap();
    let (status_json,) = status_reply.get::<(String,)>().unwrap();
    let status: serde_json::Value = serde_json::from_str(&status_json).unwrap();
    assert!(status["full_editors"].as_u64().unwrap() >= 1);

    let quit_reply = connection
        .call_future(
            Some(crate::APP_ID),
            crate::PATH,
            crate::INTERFACE,
            "QuitForUpdate",
            None,
            None,
            gio::DBusCallFlags::NONE,
            3000,
        )
        .await
        .unwrap();
    assert!(!quit_reply.get::<(bool,)>().unwrap().0);
    assert_eq!(
        text.buffer()
            .text(
                &text.buffer().start_iter(),
                &text.buffer().end_iter(),
                false
            )
            .as_str(),
        expected
    );
}

pub fn run(state: &Rc<State>, dir: &str) {
    assert!(dir.starts_with("/tmp/gnome-clip-notes-editor-test."));
    let dir = dir.to_owned();
    let state = state.clone();
    let old = std::panic::take_hook();
    let failure_path = Path::new(&dir).join("failed");
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::write(&failure_path, info.to_string());
        old(info);
        std::process::exit(1);
    }));
    glib::MainContext::default().spawn_local(async move {
        let window = state
            .app
            .active_window()
            .unwrap()
            .downcast::<adw::ApplicationWindow>()
            .unwrap();
        let text = find(window.upcast_ref(), &|w| w.is::<gtk::TextView>())
            .unwrap()
            .downcast::<gtk::TextView>()
            .unwrap();
        let source = include_str!("../tests/fixtures/markdown-preview.md");
        let pages = find(window.upcast_ref(), &|w| w.widget_name() == "editor-pages")
            .unwrap()
            .downcast::<gtk::Stack>()
            .unwrap();
        let save = find(window.upcast_ref(), &|w| w.widget_name() == "save-note")
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap();
        let view_mode = std::env::args().any(|arg| arg == "--preview");
        assert_eq!(
            pages.visible_child_name().as_deref(),
            Some(if view_mode { "preview" } else { "edit" })
        );
        assert!(!save.is_sensitive());
        text.buffer().set_text(source);
        assert_update_refused(&state, &text, source).await;
        pages.set_visible_child_name("edit");
        assert!(save.is_sensitive());
        let mut original = None;
        for cycle in 0..10 {
            pages.set_visible_child_name("preview");
            let view = find(window.upcast_ref(), &|w| {
                w.type_().name() == "WebKitWebView"
            })
            .expect("Full renderer created");
            let start = std::time::Instant::now();
            while view.property::<bool>("is-loading")
                || view.property::<Option<String>>("title").is_none()
            {
                assert!(
                    start.elapsed() < Duration::from_secs(15),
                    "Full preview did not load"
                );
                glib::timeout_future(Duration::from_millis(50)).await;
            }
            assert_eq!(
                view.property::<Option<String>>("title").as_deref(),
                Some("Markdown preview")
            );
            if let Some(original) = original.as_ref() {
                assert_eq!(&view, original, "Reuse one view per editor");
            } else {
                original = Some(view);
            }
            pages.set_visible_child_name("edit");
            assert_eq!(
                text.buffer()
                    .text(
                        &text.buffer().start_iter(),
                        &text.buffer().end_iter(),
                        false
                    )
                    .as_str(),
                source
            );
            assert!(save.is_sensitive());
            if cycle == 5 {
                adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
            }
        }
        save.emit_clicked();
        if !view_mode {
            let dialog = window
                .visible_dialog()
                .unwrap()
                .downcast::<adw::AlertDialog>()
                .unwrap();
            find(dialog.upcast_ref(), &|w| w.is::<gtk::Entry>())
                .unwrap()
                .downcast::<gtk::Entry>()
                .unwrap()
                .set_text("Isolated Full preview test");
            click_response(&dialog, "Save");
        } else {
            assert!(
                window.visible_dialog().is_none(),
                "Existing item saves without a naming prompt"
            );
        }
        glib::timeout_future(Duration::from_millis(300)).await;
        assert!(!save.is_sensitive());
        let id = *state.editors.borrow().keys().next().unwrap();
        let note = state.store.borrow().get(id).unwrap();
        assert_eq!(note.content, source);
        std::fs::write(Path::new(&dir).join("saved-id"), note.id.to_string()).unwrap();
        text.buffer()
            .insert(&mut text.buffer().end_iter(), "\nUnsaved change");
        assert_update_refused(&state, &text, &format!("{source}\nUnsaved change")).await;
        // Programmatic selection has no Wayland input serial in this driver.
        // Its failed PRIMARY ownership can collapse selection even without View.
        // Isolate buffer preservation from clipboard ownership for this test.
        let primary = text.display().primary_clipboard();
        text.buffer().remove_selection_clipboard(&primary);
        text.buffer().select_range(
            &text.buffer().iter_at_offset(1),
            &text.buffer().iter_at_offset(4),
        );
        // Exercise daemon → existing worker activation, not a local shortcut.
        let count = state.app.windows().len();
        for tab in ["edit", "preview"] {
            pages.set_visible_child_name(tab);
            glib::timeout_future(Duration::from_millis(200)).await;
            if tab == "edit" {
                text.grab_focus();
                text.buffer().select_range(
                    &text.buffer().iter_at_offset(1),
                    &text.buffer().iter_at_offset(4),
                );
                glib::timeout_future(Duration::from_millis(300)).await;
            }
            // Manual tab switching / WebKit loading precedes the operation under test.
            let selection = text
                .buffer()
                .selection_bounds()
                .map(|(a, b)| (a.offset(), b.offset()));
            if tab == "edit" {
                assert_eq!(
                    selection,
                    Some((1, 4)),
                    "Selection must be stable before sending View"
                );
            }
            state
                .app
                .dbus_connection()
                .unwrap()
                .call_future(
                    Some(crate::APP_ID),
                    crate::PATH,
                    crate::INTERFACE,
                    "Activate",
                    Some(&("view", id).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    3000,
                )
                .await
                .unwrap();
            glib::timeout_future(Duration::from_millis(200)).await;
            assert_eq!(pages.visible_child_name().as_deref(), Some(tab));
            assert_eq!(state.app.windows().len(), count);
            assert_eq!(
                text.buffer()
                    .selection_bounds()
                    .map(|(a, b)| (a.offset(), b.offset())),
                selection,
                "Reactivating an existing {tab} must preserve its selection"
            );
            assert_eq!(
                text.buffer()
                    .text(
                        &text.buffer().start_iter(),
                        &text.buffer().end_iter(),
                        false
                    )
                    .as_str(),
                format!("{source}\nUnsaved change")
            );
            assert!(save.is_sensitive());
        }
        pages.set_visible_child_name("edit");
        text.buffer().add_selection_clipboard(&primary);
        window.close();
        let dialog = window
            .visible_dialog()
            .unwrap()
            .downcast::<adw::AlertDialog>()
            .unwrap();
        click_response(&dialog, "Cancel");
        assert!(window.is_visible());
        assert!(save.is_sensitive());
        glib::timeout_future(Duration::from_millis(200)).await;
        window.close();
        let dialog = window
            .visible_dialog()
            .unwrap()
            .downcast::<adw::AlertDialog>()
            .unwrap();
        std::fs::write(Path::new(&dir).join("passed"), "EDITOR_SMOKE_OK\n").unwrap();
        // Retain no independent WebView reference across editor shutdown.
        drop(original);
        click_response(&dialog, "Discard");
    });
}
