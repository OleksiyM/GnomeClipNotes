//! Export UI regression tests; never use the real session or user's database.
use crate::{
    export_ui::ExportDialog,
    smoke::{find, snapshot_widget},
    State,
};
use adw::prelude::*;
use std::{path::PathBuf, rc::Rc, time::Duration};

async fn settle() {
    glib::timeout_future(Duration::from_millis(250)).await;
}
fn check(dialog: &ExportDialog, name: &str) -> gtk::CheckButton {
    find(dialog.dialog.upcast_ref(), &|w| w.widget_name() == name)
        .unwrap()
        .downcast()
        .unwrap()
}
fn result(dialog: &ExportDialog) -> String {
    find(dialog.dialog.upcast_ref(), &|w| {
        w.widget_name() == "export-result"
    })
    .unwrap()
    .downcast::<gtk::Label>()
    .unwrap()
    .text()
    .to_string()
}
async fn confirmation(window: &adw::ApplicationWindow) -> adw::AlertDialog {
    for _ in 0..100 {
        if let Some(alert) = window.visible_dialog().and_downcast::<adw::AlertDialog>() {
            if alert.widget_name() == "export-confirm-cleanup" {
                return alert;
            }
        }
        glib::timeout_future(Duration::from_millis(50)).await;
    }
    panic!("Cleanup confirmation did not appear");
}

pub fn run(state: &Rc<State>) {
    let dir = PathBuf::from(std::env::var("GCN_SMOKE_DIR").expect("Isolated test required"));
    assert!(dir
        .to_string_lossy()
        .starts_with("/tmp/gnome-clip-notes-test."));
    assert_eq!(
        crate::store::xdg_path("XDG_DATA_HOME", ".local/share"),
        dir.join("data")
    );
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        std::process::exit(1);
    }));
    crate::ui::show(state);
    let state = state.clone();
    glib::MainContext::default().spawn_local(async move {
        let window = state.window.borrow().as_ref().unwrap().clone();
        let empty = ExportDialog::new(&state).unwrap();
        empty.dialog.present(Some(&window)); settle().await;
        assert!(!check(&empty, "export-group-1").is_sensitive());
        assert!(!check(&empty, "export-group-1").is_active());
        assert!(!find(empty.dialog.upcast_ref(), &|w| w.widget_name()=="export-start").unwrap().is_sensitive());
        empty.dialog.close(); settle().await;
        let (note, folder, custom) = {
            let mut store = state.store.borrow_mut();
            store.capture("History is private", "Test", "", false).unwrap();
            store.create_group("Материалы / Project").unwrap();
            let folder = store.groups().unwrap().into_iter().find(|g|g.id>1).unwrap().id;
            let note = store.save_item(None, "Planning", "# A document\n\nA **readable** export.\n\n- one\n- two").unwrap();
            let custom = store.save_item(None, "Research", "A saved fragment with a [link](https://example.org).").unwrap();
            store.move_item(custom, folder).unwrap();
            (note, folder, custom)
        };
        state.changed();
        settle().await;
        crate::ui::settings(&state);
        settle().await;
        let settings = window.visible_dialog().unwrap();
        let export_button = find(settings.upcast_ref(), &|w|w.widget_name()=="settings-export").unwrap().downcast::<gtk::Button>().unwrap();
        export_button.emit_clicked();
        settle().await;
        let dialog = window.visible_dialog().unwrap();
        assert_eq!(dialog.widget_name(), "export-dialog");
        // Exercise the real response button, not a synthetic response + force_close.
        crate::ui::error(&state, "Application launch failed (test)");
        settle().await;
        let error = window.visible_dialog().unwrap().downcast::<adw::AlertDialog>().unwrap();
        let ok = find(error.upcast_ref(), &|w|w.downcast_ref::<gtk::Button>().is_some_and(|b|b.label().as_deref()==Some("OK"))).unwrap().downcast::<gtk::Button>().unwrap();
        ok.emit_clicked();
        settle().await;
        assert_ne!(window.visible_dialog(), Some(error.upcast()), "OK closes the nested error dialog");
        snapshot_widget(&window, &dir.join("export-from-settings.png"));
        dialog.close(); settle().await;
        settings.close(); settle().await;

        let export = ExportDialog::new(&state).unwrap();
        let weak = Rc::downgrade(&export);
        export.dialog.present(Some(&window)); settle().await;
        assert!(find(export.dialog.upcast_ref(), &|w|w.widget_name()=="export-group-0").is_none());
        for name in ["export-metadata", "export-delete-items", "export-delete-folders"] { assert!(!check(&export,name).is_active()); }
        check(&export, &format!("export-group-{folder}")).set_active(true);
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
        settle().await;
        snapshot_widget(&window, &dir.join("export-light.png"));
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        settle().await;
        snapshot_widget(&window, &dir.join("export-dark.png"));
        let destination = dir.join("exports");
        std::fs::create_dir(&destination).unwrap();
        export.clone().run_to(destination.clone()).await;
        assert!(result(&export).contains("All items and folders were kept"));
        assert!(destination.join("Notes.md").is_file());
        assert!(destination.join("Материалы Project.md").is_file());
        assert!(!destination.join("History.md").exists());
        assert_eq!(state.store.borrow().settings.last_export_folder.as_deref(),Some(gio::File::for_path(&destination).uri().as_str()));
        settle().await;
        snapshot_widget(&window, &dir.join("export-result.png"));
        // Actual desktop launch API with a private file-manager substitute.
        let helper = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/test-export-open.sh");
        let handler = gio::AppInfo::create_from_commandline(
            format!("bash {} %u", glib::shell_quote(helper).to_str().unwrap()), Some("Export test folder handler"), gio::AppInfoCreateFlags::SUPPORTS_URIS,
        ).unwrap();
        handler.set_as_default_for_type("inode/directory").unwrap();
        let open = find(export.dialog.upcast_ref(), &|w|w.widget_name()=="export-open-folder").unwrap().downcast::<gtk::Button>().unwrap();
        open.emit_clicked();
        let marker = dir.join("folder-opened-uri");
        for _ in 0..100 {
            if marker.exists() { break; }
            glib::timeout_future(Duration::from_millis(50)).await;
        }
        let launched = std::fs::read_to_string(&marker).expect("Folder handler launched");
        assert_eq!(gio::File::for_commandline_arg(launched.trim()).uri(), gio::File::for_path(&destination).uri());
        settle().await;
        // Inject a failed completion into the same handler used by the real launcher.
        // System app fallback makes a deliberately broken desktop association unreliable.
        let error = find(export.dialog.upcast_ref(), &|w|w.widget_name()=="export-open-folder-error").unwrap().downcast::<gtk::Label>().unwrap();
        open.set_sensitive(false);
        crate::export_ui::folder_launch_finished(&open, &error, Err(glib::Error::new(gtk::DialogError::Failed, "Application launch failed")));
        assert!(error.is_visible(), "Failed folder launch is reported inline");
        assert!(error.text().contains("Exported files are unchanged"));
        assert!(open.is_sensitive(), "Retry remains available");
        assert_eq!(window.visible_dialog(), Some(export.dialog.clone()));
        assert!(export.dialog.can_close());
        open.emit_clicked();
        assert!(!error.is_visible(), "Retry clears the old message");
        settle().await;
        assert!(handler.delete());
        crate::export_ui::folder_launch_finished(&open, &error, Err(glib::Error::new(gtk::DialogError::Dismissed, "Cancelled")));
        assert!(!error.is_visible(), "Cancellation is not an error");
        println!("PASS actual folder launch, injected failed completion without modal trap, retry, cancellation and normal OK response");
        export.dialog.close(); settle().await; drop(export);
        assert!(weak.upgrade().is_none(), "Closed dialog releases its controller");
        println!("PASS Settings → Data entry, safe defaults, light/dark UI, collection files, remembered destination, dialog lifecycle");

        // Cancel cleanup after durable files exist. Reopening resets all cleanup options.
        let export = ExportDialog::new(&state).unwrap();
        export.dialog.present(Some(&window)); settle().await;
        assert!(!check(&export,"export-delete-items").is_active());
        check(&export,"export-delete-items").set_active(true);
        let job = glib::MainContext::default().spawn_local(export.clone().run_to(destination.clone()));
        let alert = confirmation(&window).await;
        assert_eq!(alert.default_response().as_deref(),Some("keep"));
        alert.emit_by_name::<()>("response", &[&"keep"]); alert.force_close();
        job.await.unwrap();
        assert!(state.store.borrow().get(note).is_ok());
        assert!(destination.join("Notes (2).md").is_file());
        assert!(result(&export).contains("Cleanup was cancelled"));
        export.dialog.close(); settle().await;
        println!("PASS explicit Keep in App, no overwrites, cleanup options not remembered");

        // Edit while the confirmation is open: only the unchanged custom item is removed.
        let export = ExportDialog::new(&state).unwrap();
        export.dialog.present(Some(&window)); settle().await;
        check(&export,&format!("export-group-{folder}")).set_active(true);
        check(&export,"export-delete-items").set_active(true);
        check(&export,"export-delete-folders").set_active(true);
        let job = glib::MainContext::default().spawn_local(export.clone().run_to(destination.clone()));
        let alert = confirmation(&window).await;
        assert!(alert.body().contains("Delete 2 exported items from the application."));
        assert!(alert.body().contains("Delete 1 empty custom folder from the application."));
        settle().await; snapshot_widget(&window,&dir.join("export-confirmation.png"));
        state.store.borrow_mut().save_item(Some(note),"Changed during export","Keep this newer text").unwrap();
        alert.emit_by_name::<()>("response", &[&"delete"]); alert.force_close();
        job.await.unwrap();
        assert!(state.store.borrow().get(note).is_ok());
        assert!(state.store.borrow().get(custom).is_err());
        assert!(!state.store.borrow().groups().unwrap().iter().any(|g|g.id==folder));
        assert!(result(&export).contains("item was modified"));
        export.dialog.close(); settle().await;
        println!("PASS exact confirmation counts, changed item retained with reason, unchanged item and empty custom folder removed");

        let export = ExportDialog::new(&state).unwrap();
        export.dialog.present(Some(&window));
        settle().await;
        check(&export, "export-delete-items").set_active(true);
        let job = glib::MainContext::default().spawn_local(export.clone().run_to(destination.clone()));
        let alert = confirmation(&window).await;
        std::fs::write(destination.join("Notes (4).md"), "Changed outside the app").unwrap();
        alert.emit_by_name::<()>("response", &[&"delete"]);
        alert.force_close();
        job.await.unwrap();
        assert!(state.store.borrow().get(note).is_ok());
        assert!(result(&export).contains("export file is missing or changed"));
        export.dialog.close();
        settle().await;
        println!("PASS file modified during confirmation prevents cleanup and explains why");

        let export = ExportDialog::new(&state).unwrap();
        window.set_default_size(420, 700);
        export.dialog.present(Some(&window));
        settle().await;
        check(&export, "export-delete-items").set_active(true);
        assert!(!check(&export,"export-delete-folders").is_sensitive());
        snapshot_widget(&window, &dir.join("export-narrow.png"));
        state.remote_editors.borrow_mut().insert(note, "test-only-protected-editor".into());
        export.clone().run_to(destination.clone()).await;
        assert!(state.store.borrow().get(note).is_ok());
        assert!(result(&export).contains("item is open in an editor"));
        state.remote_editors.borrow_mut().remove(&note);
        export.dialog.close();
        settle().await;
        println!("PASS narrow layout and open-editor protection, with explanation instead of a misleading deletion prompt");

        let export = ExportDialog::new(&state).unwrap();
        export.dialog.present(Some(&window)); settle().await;
        export.clone().run_to(dir.join("missing-parent").join("export")).await;
        assert!(result(&export).contains("Nothing was deleted"));
        assert!(state.store.borrow().get(note).is_ok());
        export.dialog.close(); settle().await;
        println!("PASS failed export retains data and reports failure");
        state.store.borrow_mut().create_group("Empty folder").unwrap();
        let empty_id = state.store.borrow().groups().unwrap().into_iter().find(|g|g.name=="Empty folder").unwrap().id;
        let export = ExportDialog::new(&state).unwrap();
        export.dialog.present(Some(&window)); settle().await;
        assert!(!check(&export,&format!("export-group-{empty_id}")).is_sensitive());
        assert!(!check(&export,&format!("export-group-{empty_id}")).is_active());
        // Selection was valid when shown, then the saved item moved elsewhere.
        state.store.borrow().move_item(note, 0).unwrap();
        let count_before = std::fs::read_dir(&destination).unwrap().count();
        let remembered = state.store.borrow().settings.last_export_folder.clone();
        export.clone().run_to(destination.clone()).await;
        assert!(result(&export).contains("now empty"));
        assert!(!result(&export).contains("removed safely"));
        assert_eq!(std::fs::read_dir(&destination).unwrap().count(), count_before);
        assert_eq!(state.store.borrow().settings.last_export_folder, remembered);
        assert!(state.store.borrow().get(note).is_ok());
        settle().await;
        snapshot_widget(&window, &dir.join("export-became-empty.png"));
        export.dialog.close(); settle().await;
        println!("PASS disabled empty collections and selection emptied before export: no files or cleanup");
        println!("EXPORT SMOKE PASS: {}",dir.display());
        state.app.quit();
    });
}
