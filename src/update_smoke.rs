//! Explicit GUI driver for the updater/editor shutdown contract.
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
    find(dialog.upcast_ref(), &|widget| {
        widget
            .downcast_ref::<gtk::Button>()
            .is_some_and(|button| button.label().as_deref() == Some(label))
    })
    .expect("dialog response button")
    .downcast::<gtk::Button>()
    .unwrap()
    .emit_clicked();
}

async fn call(state: &State, method: &str) -> glib::Variant {
    state
        .app
        .dbus_connection()
        .unwrap()
        .call_future(
            Some(crate::APP_ID),
            crate::PATH,
            crate::INTERFACE,
            method,
            None,
            None,
            gio::DBusCallFlags::NONE,
            3000,
        )
        .await
        .unwrap()
}

async fn status(state: &State) -> serde_json::Value {
    let result = call(state, "GetUpdateStatus").await;
    let (json,) = result.get::<(String,)>().expect("status tuple");
    serde_json::from_str(&json).expect("status JSON")
}

pub fn run(state: &Rc<State>) {
    let dir = std::env::var("GCN_UPDATE_TEST_DIR").expect("GCN_UPDATE_TEST_DIR");
    assert!(dir.starts_with("/tmp/gnome-clip-notes-update."));
    for key in [
        "XDG_DATA_HOME",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ] {
        assert!(
            std::env::var(key).is_ok_and(|path| path.starts_with(&dir)),
            "{key} must use the private update-test profile"
        );
    }
    let failure_path = Path::new(&dir).join("failed");
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::write(&failure_path, info.to_string());
        old(info);
        std::process::exit(1);
    }));

    let state = state.clone();
    glib::MainContext::default().spawn_local(async move {
        let initial = status(&state).await;
        assert_eq!(initial["protocol"], 1);
        assert_eq!(initial["native_editors"], 0);
        assert_eq!(initial["full_editors"], 0);
        assert_eq!(initial["quitting"], false);

        crate::ui::editor_in_process(&state, None);
        glib::timeout_future(Duration::from_millis(100)).await;
        let window = state
            .app
            .active_window()
            .expect("new editor")
            .downcast::<adw::ApplicationWindow>()
            .unwrap();
        assert_eq!(status(&state).await["native_editors"], 1);
        assert!(
            !call(&state, "QuitForUpdate")
                .await
                .get::<(bool,)>()
                .unwrap()
                .0,
            "Even a clean editor must be closed by its user"
        );

        let text = find(window.upcast_ref(), &|widget| widget.is::<gtk::TextView>())
            .unwrap()
            .downcast::<gtk::TextView>()
            .unwrap();
        let draft = "Updater must never discard this draft";
        text.buffer().set_text(draft);
        assert!(
            !call(&state, "QuitForUpdate")
                .await
                .get::<(bool,)>()
                .unwrap()
                .0
        );
        assert_eq!(status(&state).await["quitting"], false);
        assert_eq!(
            text.buffer().text(
                &text.buffer().start_iter(),
                &text.buffer().end_iter(),
                false
            ),
            draft
        );

        window.close();
        let dialog = window
            .visible_dialog()
            .unwrap()
            .downcast::<adw::AlertDialog>()
            .unwrap();
        click_response(&dialog, "Cancel");
        glib::timeout_future(Duration::from_millis(100)).await;
        assert!(window.is_visible());
        assert_eq!(status(&state).await["native_editors"], 1);
        assert_eq!(
            text.buffer().text(
                &text.buffer().start_iter(),
                &text.buffer().end_iter(),
                false
            ),
            draft
        );

        window.close();
        let dialog = window
            .visible_dialog()
            .unwrap()
            .downcast::<adw::AlertDialog>()
            .unwrap();
        click_response(&dialog, "Discard");
        glib::timeout_future(Duration::from_millis(100)).await;
        // `window` deliberately remains alive: the lease follows logical close,
        // not final GObject destruction.
        assert_eq!(status(&state).await["native_editors"], 0);
        assert!(!window.is_visible());

        let id = state
            .store
            .borrow_mut()
            .save_item(None, "Saved update fixture", "Already saved")
            .unwrap();
        crate::ui::editor_in_process(&state, Some(id));
        crate::ui::editor_in_process(&state, Some(id));
        assert_eq!(status(&state).await["native_editors"], 1);
        let saved_window = state.editors.borrow()[&id].upgrade().unwrap();
        saved_window.close();
        assert_eq!(status(&state).await["native_editors"], 0);

        // The external test client verifies the final reply and process exit.
        // A client inside this process cannot await its own completed shutdown.
        std::fs::write(Path::new(&dir).join("ready"), "UPDATE_SMOKE_OK\n").unwrap();
    });
}
