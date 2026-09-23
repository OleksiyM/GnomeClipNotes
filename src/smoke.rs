//! Explicit developer-only UI smoke run against an isolated XDG data directory.
use crate::{ui, State};
use adw::prelude::*;
use std::{path::PathBuf, rc::Rc, time::Duration};

fn snapshot(window: &gtk::Window, path: &std::path::Path) {
    snapshot_widget(window, path);
}

pub(crate) fn snapshot_widget(widget: &impl IsA<gtk::Widget>, path: &std::path::Path) {
    let window = widget.as_ref();
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        f64::from(window.width()),
        f64::from(window.height()),
    );
    if let Some(node) = snapshot.to_node() {
        let renderer = gtk::gsk::CairoRenderer::new();
        renderer
            .realize(None::<&gtk::gdk::Surface>)
            .expect("realize snapshot renderer");
        let texture = renderer.render_texture(&node, None);
        texture.save_to_png(path).expect("save UI snapshot");
        renderer.unrealize();
    } else {
        panic!("Window has no render node");
    }
}

fn verify_menus(state: &Rc<State>, dir: &std::path::Path) {
    let window = state.app.active_window().unwrap();
    let main = find(window.upcast_ref(), &|w| {
        w.widget_name() == "library-main-menu"
    })
    .unwrap()
    .downcast::<gtk::MenuButton>()
    .unwrap();
    assert!(main.popover().unwrap().is::<gtk::PopoverMenu>());
    let settings = crate::preferences::extension_settings().unwrap();
    let previous = settings.strv("activate-shortcut");
    settings
        .set_strv("activate-shortcut", ["<Super><Shift>v"])
        .unwrap();
    let navigation = main.menu_model().unwrap().item_link(0, "section").unwrap();
    assert_eq!(
        navigation
            .item_attribute_value(1, "accel", None)
            .unwrap()
            .str(),
        Some("<Super><Shift>v")
    );
    settings.set_strv("activate-shortcut", &previous).unwrap();
    main.popup();
    let dir = dir.to_path_buf();
    glib::timeout_add_local_once(Duration::from_millis(250), move || {
        snapshot_widget(&main.popover().unwrap(), &dir.join("library-main-menu.png"));
        main.popdown();
        let card = find(window.upcast_ref(), &|w| {
            w.widget_name() == "card-actions-menu"
        })
        .unwrap()
        .downcast::<gtk::MenuButton>()
        .unwrap();
        assert!(card.popover().unwrap().is::<gtk::PopoverMenu>());
        let actions = card.menu_model().unwrap().item_link(0, "section").unwrap();
        assert_eq!(
            actions
                .item_attribute_value(0, "action", None)
                .unwrap()
                .str(),
            Some("menu.view")
        );
        assert_eq!(
            card.menu_model().unwrap().n_items(),
            3,
            "Separate details, organization and deletion"
        );
        let organize = card.menu_model().unwrap().item_link(1, "section").unwrap();
        assert_eq!(
            organize
                .item_attribute_value(0, "action", None)
                .unwrap()
                .str(),
            Some("menu.notes")
        );
        let destinations = organize
            .item_link(1, "submenu")
            .expect("Native folder submenu");
        let quick = destinations.item_link(0, "section").unwrap();
        assert_eq!(quick.n_items(), 2);
        assert_eq!(
            quick.item_attribute_value(0, "label", None).unwrap().str(),
            Some("Projects")
        );
        let footer = destinations.item_link(1, "section").unwrap();
        assert_eq!(
            footer
                .item_attribute_value(0, "action", None)
                .unwrap()
                .str(),
            Some("menu.new-group-move")
        );
        card.popup();
        glib::timeout_add_local_once(Duration::from_millis(250), move || {
            let popover = card.popover().unwrap();
            snapshot_widget(&popover, &dir.join("library-card-menu.png"));
            let rename = find(popover.upcast_ref(), &|w| {
                w.type_().name() == "GtkModelButton"
                    && find(w, &|child| {
                        child
                            .downcast_ref::<gtk::Label>()
                            .is_some_and(|label| label.text() == "Rename…")
                    })
                    .is_some()
            })
            .expect("Native Rename menu row");
            assert!(rename.activate());
            glib::timeout_add_local_once(Duration::from_millis(250), move || {
                let window = window.downcast::<adw::ApplicationWindow>().unwrap();
                let dialog = window
                    .visible_dialog()
                    .expect("Rename action opens dialog")
                    .downcast::<adw::AlertDialog>()
                    .unwrap();
                assert_eq!(dialog.heading().as_deref(), Some("Rename Item"));
                assert!(!popover.is_visible(), "Action closes native menu");
                dialog.close();
                println!("PASS native menus, live shortcut hints, sections and Rename activation");
            });
        });
    });
}

pub(crate) fn find(
    widget: &gtk::Widget,
    predicate: &dyn Fn(&gtk::Widget) -> bool,
) -> Option<gtk::Widget> {
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

fn verify_details(state: Rc<State>, dir: PathBuf) {
    let window = state
        .app
        .active_window()
        .unwrap()
        .downcast::<adw::ApplicationWindow>()
        .unwrap();
    let item = state
        .store
        .borrow()
        .query(&crate::model::Query::default())
        .unwrap()[0]
        .clone();
    ui::info(&state, &item);
    let dialog = window.visible_dialog().unwrap();
    assert_eq!(dialog.widget_name(), "item-details");
    let group = find(dialog.upcast_ref(), &|w| {
        w.widget_name() == "item-details-properties"
    })
    .unwrap();
    for name in [
        "Source",
        "Origin",
        "Created",
        "Edited",
        "Last Copied",
        "Length",
    ] {
        let row = find(&group, &|w| {
            w.downcast_ref::<adw::ActionRow>()
                .is_some_and(|r| r.title() == name)
        })
        .unwrap()
        .downcast::<adw::ActionRow>()
        .unwrap();
        assert!(row.has_css_class("property"));
        assert!(row.is_subtitle_selectable());
        assert_eq!(row.subtitle_lines(), 0);
    }
    glib::timeout_add_local_once(Duration::from_millis(250), move || {
        snapshot(
            &window.clone().upcast::<gtk::Window>(),
            &dir.join("item-details.png"),
        );
        let scroll = find(dialog.upcast_ref(), &|w| w.is::<gtk::ScrolledWindow>())
            .unwrap()
            .downcast::<gtk::ScrolledWindow>()
            .unwrap();
        let adjustment = scroll.vadjustment();
        assert!(
            adjustment.upper() <= adjustment.page_size() + 1.0,
            "Ordinary details fit without scrolling"
        );
        assert!(
            scroll.height() <= group.height() + group.margin_top() + group.margin_bottom() + 2,
            "No empty space below the properties"
        );
        dialog.force_close();
        let style = adw::StyleManager::default();
        let previous = style.color_scheme();
        style.set_color_scheme(adw::ColorScheme::ForceDark);
        let mut long_item = item;
        long_item.source =
            "A source with a deliberately long name — Источник с длинным названием".repeat(3);
        window.set_default_size(420, 600);
        ui::info(&state, &long_item);
        let dialog = window.visible_dialog().unwrap();
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            snapshot(
                &window.clone().upcast::<gtk::Window>(),
                &dir.join("item-details-dark-narrow.png"),
            );
            let scroll = find(dialog.upcast_ref(), &|w| w.is::<gtk::ScrolledWindow>())
                .unwrap()
                .downcast::<gtk::ScrolledWindow>()
                .unwrap();
            assert!(scroll.hadjustment().upper() <= scroll.hadjustment().page_size() + 1.0);
            dialog.force_close();
            window.set_default_size(1040, 720);
            style.set_color_scheme(previous);
            println!("PASS readable property rows, selectable values, content-sized details and narrow layout");
        });
    });
}

pub fn run(state: &Rc<State>) {
    let dir = PathBuf::from(
        std::env::var_os("GCN_SMOKE_DIR").expect("Set GCN_SMOKE_DIR to an isolated test directory"),
    );
    assert_eq!(
        crate::store::xdg_path("XDG_DATA_HOME", ".local/share"),
        dir.join("data"),
        "Smoke data must be isolated"
    );
    {
        let mut store = state.store.borrow_mut();
        store
            .capture(
                "https://docs.gtk.org/gtk4/",
                "Firefox",
                "firefox.desktop",
                false,
            )
            .unwrap();
        store.capture("Small tools should feel like part of the desktop.\nKeep what matters. Let the rest go.","Text Editor","org.gnome.TextEditor.desktop",false).unwrap();
        store
            .capture(
                "cargo build --release --locked",
                "Terminal",
                "org.gnome.Ptyxis.desktop",
                false,
            )
            .unwrap();
        store.save_item(None,"A place for the next idea","# A quieter workspace\n\nA notebook for ideas, fragments, and things worth keeping.\n\n## This week\n- [x] Make a little space\n- [ ] Write something useful\n\n**Local first.** Everything stays on this device.\n\n```rust\nlet idea = \"something worth keeping\";\n```").unwrap();
        store.create_group("Projects").unwrap();
        store.create_group("Writing").unwrap();
    }
    ui::show(state);
    crate::preferences::verify_shortcut_consent(state);
    verify_menus(state, &dir);
    {
        let state = state.clone();
        let dir = dir.clone();
        glib::timeout_add_local_once(Duration::from_millis(1200), move || {
            verify_details(state, dir)
        });
    }
    let state = state.clone();
    glib::timeout_add_local_once(Duration::from_secs(3), move || {
        snapshot(
            &state.app.active_window().unwrap(),
            &dir.join("library.png"),
        );
        let note = state
            .store
            .borrow()
            .query(&crate::model::Query {
                group_id: 1,
                ..Default::default()
            })
            .unwrap()[0]
            .clone();
        verify_note_lifecycle(&state);
        verify_view(&state, note.id);
        ui::editor_in_process(&state, Some(note.id));
        let state = state.clone();
        glib::timeout_add_local_once(Duration::from_secs(1), move || {
            snapshot(&state.app.active_window().unwrap(), &dir.join("editor.png"));
            let window = state.app.active_window().unwrap();
            let modes = find(window.upcast_ref(), &|w| w.widget_name() == "editor-modes").unwrap();
            let toggle = find(&modes, &|w| {
                w.is::<gtk::ToggleButton>()
                    && find(w, &|child| {
                        child
                            .downcast_ref::<gtk::Label>()
                            .is_some_and(|label| label.text() == "Preview")
                    })
                    .is_some()
            })
            .unwrap()
            .downcast::<gtk::ToggleButton>()
            .unwrap();
            toggle.grab_focus();
            toggle.set_active(true);
            let pages = find(window.upcast_ref(), &|w| w.widget_name() == "editor-pages")
                .unwrap()
                .downcast::<gtk::Stack>()
                .unwrap();
            assert_eq!(pages.visible_child_name().as_deref(), Some("preview"));
            glib::timeout_add_local_once(Duration::from_secs(1), move || {
                snapshot(
                    &state.app.active_window().unwrap(),
                    &dir.join("preview.png"),
                );
                let edit_toggle = find(&modes, &|w| {
                    w.is::<gtk::ToggleButton>()
                        && find(w, &|child| {
                            child
                                .downcast_ref::<gtk::Label>()
                                .is_some_and(|label| label.text() == "Editor")
                        })
                        .is_some()
                })
                .unwrap()
                .downcast::<gtk::ToggleButton>()
                .unwrap();
                edit_toggle.set_active(true);
                assert_eq!(pages.visible_child_name().as_deref(), Some("edit"));
                assert!(!toggle.is_active());
                let text = find(window.upcast_ref(), &|w| w.is::<gtk::TextView>())
                    .unwrap()
                    .downcast::<gtk::TextView>()
                    .unwrap();
                let buffer = text.buffer();
                buffer.insert(&mut buffer.end_iter(), "\nSaved through the editor.");
                let save = find(window.upcast_ref(), &|w| {
                    w.downcast_ref::<gtk::Button>()
                        .is_some_and(|b| b.widget_name() == "save-note")
                })
                .unwrap()
                .downcast::<gtk::Button>()
                .unwrap();
                assert!(save.is_sensitive());
                assert!(window.title().unwrap().starts_with("● "));
                buffer.undo();
                assert!(!save.is_sensitive());
                assert!(!window.title().unwrap().starts_with("● "));
                buffer.redo();
                assert!(save.is_sensitive());
                save.emit_clicked();
                assert!(!save.is_sensitive());
                assert!(!window.title().unwrap().starts_with("● "));
                assert!(window.is_visible());
                assert!(state
                    .store
                    .borrow()
                    .get(note.id)
                    .unwrap()
                    .content
                    .ends_with("Saved through the editor."));
                window.close();
                ui::settings(&state);
                verify_preview_preferences(&state);
                glib::timeout_add_local_once(Duration::from_secs(1), move || {
                    snapshot(
                        &state.app.active_window().unwrap(),
                        &dir.join("settings.png"),
                    );
                    settings_sections(state.clone(), dir.clone(), 0, false, move || {
                        verify_history_retention(&state);
                        verify_ignored_app(&state);
                        ui::about(&state);
                        glib::timeout_add_local_once(Duration::from_secs(1), move || {
                            snapshot(&state.app.active_window().unwrap(), &dir.join("about.png"));
                            let parent = state.app.active_window().unwrap();
                            let about =
                                find(parent.upcast_ref(), &|w| w.widget_name() == "about-dialog")
                                    .unwrap()
                                    .downcast::<adw::Dialog>()
                                    .unwrap();
                            let legal =
                                find(about.upcast_ref(), &|w| w.widget_name() == "about-legal")
                                    .unwrap();
                            legal.emit_by_name::<()>("activated", &[]);
                            glib::timeout_add_local_once(Duration::from_secs(1), move || {
                                let license = find(parent.upcast_ref(), &|w| {
                                    w.widget_name() == "about-license"
                                })
                                .unwrap()
                                .downcast::<adw::Dialog>()
                                .unwrap();
                                assert!(find(license.upcast_ref(), &|w| w
                                    .downcast_ref::<gtk::Label>()
                                    .is_some_and(
                                        |label| label.text() == include_str!("../LICENSE")
                                    ))
                                .is_some());
                                license.force_close();
                                let status = find(parent.upcast_ref(), &|w| {
                                    w.widget_name() == "update-status"
                                })
                                .unwrap()
                                .downcast::<gtk::Label>()
                                .unwrap();
                                assert_eq!(status.text().as_str(), "");
                                assert!(!status.is_visible());
                                let check = find(parent.upcast_ref(), &|w| {
                                    w.widget_name() == "check-for-updates"
                                })
                                .unwrap();
                                assert!(check.is_sensitive());
                                let release = find(parent.upcast_ref(), &|w| {
                                    w.widget_name() == "view-release"
                                })
                                .unwrap();
                                assert!(!release.is_visible());
                                about.force_close();
                                glib::spawn_future_local(async move {
                                    verify_updates(&parent, &dir).await;
                                    visual_variants(state, dir);
                                });
                            });
                        });
                    });
                });
            });
        });
    });
}

async fn verify_updates(parent: &gtk::Window, dir: &std::path::Path) {
    use crate::release_check::Outcome;
    use std::cell::{Cell, RefCell};
    let results = Rc::new(RefCell::new(std::collections::VecDeque::from([
        Outcome::Available("1.0.0".into()),
        Outcome::Current,
        Outcome::NewerBuild,
        Outcome::NoRelease,
        Outcome::RateLimited,
        Outcome::Invalid,
        Outcome::Failed,
        Outcome::Timeout,
    ])));
    let pending = results.clone();
    let dialog = ui::about_dialog_with_check(move || {
        std::future::ready(pending.borrow_mut().pop_front().unwrap())
    });
    dialog.present(Some(parent));
    let check = find(dialog.upcast_ref(), &|w| {
        w.widget_name() == "check-for-updates"
    })
    .unwrap()
    .downcast::<gtk::Button>()
    .unwrap();
    let release = find(dialog.upcast_ref(), &|w| w.widget_name() == "view-release").unwrap();
    let status = find(dialog.upcast_ref(), &|w| w.widget_name() == "update-status")
        .unwrap()
        .downcast::<gtk::Label>()
        .unwrap();
    assert_eq!(
        results.borrow().len(),
        8,
        "Opening the dialog must not start a request"
    );
    for (index, expected) in [
        "Update available: 1.0.0",
        "Up to date",
        "This build is newer than the latest public release.",
        "No public release found. The repository may not be available yet.",
        "GitHub refused or limited this request. Try again later.",
        "GitHub returned unexpected release information.",
        "Could not check for updates. Check your connection and try again.",
        "The update check timed out. Try again later.",
    ]
    .iter()
    .enumerate()
    {
        check.emit_clicked();
        assert!(!check.is_sensitive());
        assert!(!release.is_visible());
        assert_eq!(status.text().as_str(), "Checking for updates…");
        glib::timeout_future(Duration::from_millis(120)).await;
        assert!(check.is_sensitive());
        assert_eq!(status.text().as_str(), *expected);
        assert_eq!(release.is_visible(), index == 0);
        if index == 0 {
            glib::timeout_future(Duration::from_millis(600)).await;
            snapshot(parent, &dir.join("updates-available.png"));
            adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
            glib::timeout_future(Duration::from_millis(600)).await;
            snapshot(parent, &dir.join("updates-dark.png"));
            adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
            let size = parent.default_size();
            parent.set_default_size(360, 640);
            glib::timeout_future(Duration::from_millis(600)).await;
            snapshot(parent, &dir.join("about-narrow.png"));
            assert!(dialog.width() <= parent.width());
            parent.set_default_size(size.0, size.1);
        }
    }
    dialog.force_close();

    struct CancelProbe(Rc<Cell<bool>>);
    impl Drop for CancelProbe {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let cancelled = Rc::new(Cell::new(false));
    let probe = cancelled.clone();
    let dialog = ui::about_dialog_with_check(move || {
        let guard = CancelProbe(probe.clone());
        async move {
            let _guard = guard;
            std::future::pending::<Outcome>().await
        }
    });
    dialog.present(Some(parent));
    let check = find(dialog.upcast_ref(), &|w| {
        w.widget_name() == "check-for-updates"
    })
    .unwrap()
    .downcast::<gtk::Button>()
    .unwrap();
    check.emit_clicked();
    glib::timeout_future(Duration::from_millis(60)).await;
    assert!(!cancelled.get());
    dialog.force_close();
    glib::timeout_future(Duration::from_millis(60)).await;
    assert!(
        cancelled.get(),
        "Closing About must drop the pending request"
    );
    println!("PASS manual update UI: no automatic request, all outcomes, retry and close cancellation (offline)");
}

fn verify_view(state: &Rc<State>, note_id: i64) {
    let previous = state.store.borrow().settings.preview_mode.clone();
    state.store.borrow_mut().settings.preview_mode = "native".into();
    let history_id = state
        .store
        .borrow()
        .query(&crate::model::Query::default())
        .unwrap()[0]
        .id;
    for id in [note_id, history_id] {
        let before = state.store.borrow().get(id).unwrap();
        let count = state.app.windows().len();
        state.activate("view", id);
        let window = state.editors.borrow().get(&id).unwrap().upgrade().unwrap();
        let pages = find(window.upcast_ref(), &|w| w.widget_name() == "editor-pages")
            .unwrap()
            .downcast::<gtk::Stack>()
            .unwrap();
        let text = find(window.upcast_ref(), &|w| w.is::<gtk::TextView>())
            .unwrap()
            .downcast::<gtk::TextView>()
            .unwrap();
        let save = find(window.upcast_ref(), &|w| w.widget_name() == "save-note")
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap();
        assert_eq!(pages.visible_child_name().as_deref(), Some("preview"));
        assert!(!save.is_sensitive());
        assert_eq!(state.app.windows().len(), count + 1);
        ui::editor(state, Some(id));
        assert_eq!(
            pages.visible_child_name().as_deref(),
            Some("preview"),
            "Already open keeps its tab, even for Edit"
        );
        pages.set_visible_child_name("edit");
        let buffer = text.buffer();
        buffer.insert(&mut buffer.end_iter(), "\nUnfinished thought");
        buffer.select_range(&buffer.iter_at_offset(1), &buffer.iter_at_offset(4));
        text.grab_focus();
        state.activate("view", id);
        assert_eq!(pages.visible_child_name().as_deref(), Some("edit"));
        assert_eq!(state.app.windows().len(), count + 1);
        assert_eq!(
            state.editors.borrow().get(&id).unwrap().upgrade().unwrap(),
            window
        );
        assert!(save.is_sensitive());
        assert_eq!(
            buffer
                .selection_bounds()
                .map(|(a, b)| (a.offset(), b.offset())),
            Some((1, 4))
        );
        assert_eq!(
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .as_str(),
            format!("{}\nUnfinished thought", before.content)
        );
        let after = state.store.borrow().get(id).unwrap();
        assert_eq!(after.content, before.content);
        assert_eq!(after.updated_at, before.updated_at);
        assert_eq!(after.group_id, before.group_id);
        buffer.set_text(&before.content);
        window.close();
    }
    state.store.borrow_mut().settings.preview_mode = previous;
    println!(
        "PASS View for Notes and History, clean preview, existing tab/selection/draft preserved"
    );
}

fn verify_preview_preferences(state: &Rc<State>) {
    let window = state
        .app
        .active_window()
        .unwrap()
        .downcast::<adw::ApplicationWindow>()
        .unwrap();
    let dialog = window.visible_dialog().unwrap();
    for (title, wanted) in [
        ("Lightweight · Native", "native"),
        ("Full · WebKit", "webkit"),
    ] {
        let row = find(dialog.upcast_ref(), &|w| {
            w.downcast_ref::<adw::ActionRow>()
                .is_some_and(|row| row.title() == title)
        })
        .unwrap();
        find(&row, &|w| w.is::<gtk::CheckButton>())
            .unwrap()
            .downcast::<gtk::CheckButton>()
            .unwrap()
            .set_active(true);
        assert_eq!(state.store.borrow().settings.preview_mode, wanted);
        let saved: crate::model::Settings =
            serde_json::from_slice(&std::fs::read(&state.store.borrow().config_path).unwrap())
                .unwrap();
        assert_eq!(saved.preview_mode, wanted);
    }
    println!("PASS preview radio choices and persistence");
}

fn settings_sections(
    state: Rc<State>,
    dir: PathBuf,
    index: usize,
    narrow: bool,
    done: impl FnOnce() + 'static,
) {
    let window = state.window.borrow().as_ref().unwrap().clone();
    let dialog = window.visible_dialog().unwrap();
    let pages = find(dialog.upcast_ref(), &|w| {
        w.widget_name() == "settings-pages"
    })
    .unwrap()
    .downcast::<gtk::Stack>()
    .unwrap();
    let split = find(dialog.upcast_ref(), &|w| {
        w.widget_name() == "settings-split"
    })
    .unwrap()
    .downcast::<adw::NavigationSplitView>()
    .unwrap();
    let names = [
        "general",
        "history",
        "privacy",
        "shortcuts",
        "folders",
        "data",
    ];
    if index == names.len() {
        pages.set_visible_child_name("history");
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
        if narrow {
            split.set_show_content(false);
            glib::timeout_add_local_once(Duration::from_millis(250), move || {
                snapshot(
                    window.upcast_ref(),
                    &dir.join("settings-navigation-narrow.png"),
                );
                assert!(!split.shows_content());
                done();
            });
        } else {
            done();
        }
        return;
    }
    assert_eq!(
        split.is_collapsed(),
        narrow,
        "Settings navigation adapts to its width"
    );
    if narrow {
        assert!(
            window.width() <= 440,
            "Settings must not widen the parent window"
        );
    }
    let variant = if narrow { "-narrow" } else { "" };
    let name = names[index];
    let row = find(dialog.upcast_ref(), &|w| {
        w.widget_name() == format!("settings-section-{name}")
    })
    .unwrap()
    .downcast::<gtk::ListBoxRow>()
    .unwrap();
    let sidebar = row.parent().unwrap().downcast::<gtk::ListBox>().unwrap();
    sidebar.emit_by_name::<()>("row-activated", &[&row]);
    assert_eq!(pages.visible_child_name().as_deref(), Some(name));
    assert_eq!(sidebar.selected_row(), Some(row));
    glib::timeout_add_local_once(Duration::from_millis(250), move || {
        snapshot(
            window.upcast_ref(),
            &dir.join(format!("settings-{name}{variant}.png")),
        );
        if name == "history" {
            let scale = find(dialog.upcast_ref(), &|w| {
                w.widget_name() == "history-retention-scale"
            })
            .unwrap();
            assert!(
                scale.width() <= 360,
                "Retention scale has a restrained width"
            );
        }
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        glib::timeout_add_local_once(Duration::from_millis(200), move || {
            snapshot(
                window.upcast_ref(),
                &dir.join(format!("settings-{name}{variant}-dark.png")),
            );
            adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
            settings_sections(state, dir, index + 1, narrow, done);
        });
    });
}

fn verify_note_lifecycle(state: &Rc<State>) {
    for name in ["", "A named note"] {
        ui::editor_in_process(state, None);
        let window = state
            .app
            .windows()
            .into_iter()
            .find(|w| w.title().as_deref() == Some("New Note"))
            .unwrap()
            .downcast::<adw::ApplicationWindow>()
            .unwrap();
        let save = find(window.upcast_ref(), &|w| w.widget_name() == "save-note")
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap();
        let heading = find(window.upcast_ref(), &|w| w.widget_name() == "editor-title")
            .unwrap()
            .downcast::<adw::WindowTitle>()
            .expect("Editor uses a native window title");
        assert_eq!(heading.title(), "New Note");
        let buffer = find(window.upcast_ref(), &|w| w.is::<gtk::TextView>())
            .unwrap()
            .downcast::<gtk::TextView>()
            .unwrap()
            .buffer();
        assert!(!save.is_sensitive());
        assert!(find(window.upcast_ref(), &|w| w.is::<gtk::Entry>()).is_none());
        buffer.set_text("New note lifecycle fixture");
        assert!(save.is_sensitive());
        assert_eq!(window.title().as_deref(), Some("● New Note"));
        assert_eq!(heading.title(), "● New Note");
        save.emit_clicked();
        let dialog = window
            .visible_dialog()
            .unwrap()
            .downcast::<adw::AlertDialog>()
            .unwrap();
        let entry = dialog
            .extra_child()
            .unwrap()
            .downcast::<gtk::Entry>()
            .unwrap();
        assert!(entry.text().is_empty());
        dialog.emit_by_name::<()>("response", &[&"cancel"]);
        dialog.force_close();
        assert!(save.is_sensitive());
        save.emit_clicked();
        let dialog = window
            .visible_dialog()
            .unwrap()
            .downcast::<adw::AlertDialog>()
            .unwrap();
        dialog
            .extra_child()
            .unwrap()
            .downcast::<gtk::Entry>()
            .unwrap()
            .set_text(name);
        dialog.emit_by_name::<()>("response", &[&"save"]);
        dialog.force_close();
        assert!(!save.is_sensitive());
        assert_eq!(
            window.title().as_deref(),
            Some(if name.is_empty() { "Note" } else { name })
        );
        assert_eq!(heading.title(), window.title().unwrap());
        let id = *state
            .editors
            .borrow()
            .iter()
            .find(|(_, w)| w.upgrade().as_ref() == Some(&window))
            .unwrap()
            .0;
        assert_eq!(state.store.borrow().get(id).unwrap().title, name);
        buffer.insert(&mut buffer.end_iter(), " updated");
        save.emit_clicked();
        assert!(!save.is_sensitive());
        assert!(state
            .store
            .borrow()
            .get(id)
            .unwrap()
            .content
            .ends_with(" updated"));
        window.close();
    }
    println!("PASS new note naming, unnamed save, cancellation and repeated save");
}

fn visual_variants(state: Rc<State>, dir: PathBuf) {
    let window = state.window.borrow().as_ref().unwrap().clone();
    let dialogs = window.dialogs();
    let open: Vec<adw::Dialog> = (0..dialogs.n_items())
        .filter_map(|i| dialogs.item(i)?.downcast().ok())
        .collect();
    for dialog in open {
        dialog.force_close();
    }
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    glib::timeout_add_local_once(Duration::from_millis(600), move || {
        snapshot(window.upcast_ref(), &dir.join("library-dark.png"));
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
        window.set_default_size(420, 700);
        glib::timeout_add_local_once(Duration::from_millis(600), move || {
            assert!(
                window.width() <= 440,
                "Library did not adapt to narrow width: {}",
                window.width()
            );
            snapshot(window.upcast_ref(), &dir.join("library-narrow.png"));
            ui::settings(&state);
            glib::timeout_add_local_once(Duration::from_millis(600), move || {
                snapshot(window.upcast_ref(), &dir.join("settings-narrow.png"));
                settings_sections(state.clone(), dir.clone(), 0, true, move || {
                    editor_mode_variants(state, dir)
                });
            });
        });
    });
}

fn editor_mode_variants(state: Rc<State>, dir: PathBuf) {
    let id = state.store.borrow_mut().save_item(None, "Native preview",
        "# A quieter workspace\n\n**Clear structure**, *native colours*, and [a link](https://gnome.org).\n\n> Хороший инструмент не должен отвлекать от мысли.\n\n## This week\n- [x] Keep notes local\n- [ ] Read comfortably\n\n```rust\nlet idea = \"something worth keeping\";\n```\n\nInline `code`, too.").unwrap();
    ui::editor_in_process(&state, Some(id));
    let window = state
        .app
        .windows()
        .into_iter()
        .find(|w| w.title().as_deref() == Some("Native preview"))
        .unwrap();
    window.set_default_size(420, 600);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    glib::timeout_add_local_once(Duration::from_millis(600), move || {
        assert!(
            window.width() <= 440,
            "Editor mode switcher prevents narrow layout"
        );
        snapshot(&window, &dir.join("editor-dark-narrow.png"));
        let modes = find(window.upcast_ref(), &|w| w.widget_name() == "editor-modes").unwrap();
        let preview = find(&modes, &|w| {
            w.is::<gtk::ToggleButton>()
                && find(w, &|child| {
                    child
                        .downcast_ref::<gtk::Label>()
                        .is_some_and(|label| label.text() == "Preview")
                })
                .is_some()
        })
        .unwrap()
        .downcast::<gtk::ToggleButton>()
        .unwrap();
        preview.set_active(true);
        glib::timeout_add_local_once(Duration::from_millis(600), move || {
            snapshot(&window, &dir.join("preview-dark-narrow.png"));
            window.close();
            // Let the editor's focus teardown finish before presenting another
            // window and rebuilding its collections in this synthetic test.
            glib::timeout_add_local_once(Duration::from_millis(300), move || {
                verify_library_pages(state, dir);
            });
        });
    });
}

fn verify_library_pages(state: Rc<State>, dir: PathBuf) {
    let group;
    {
        let mut store = state.store.borrow_mut();
        store.create_group("Paging regression").unwrap();
        group = store
            .groups()
            .unwrap()
            .into_iter()
            .find(|g| g.name == "Paging regression")
            .unwrap()
            .id;
        for i in 0..13 {
            let content = format!("Paging fixture {i}\n{}", "A long paragraph\n\n".repeat(60));
            store
                .capture(&content, "Paging test", "fixture.desktop", false)
                .unwrap();
            store
                .save_item(None, &format!("Note {i}"), &content)
                .unwrap();
            let id = store
                .save_item(None, &format!("Folder note {i}"), &content)
                .unwrap();
            store.move_item(id, group).unwrap();
        }
    }
    let window = state.window.borrow().as_ref().unwrap().clone();
    for i in 0..window.dialogs().n_items() {
        if let Some(dialog) = window.dialogs().item(i).and_downcast::<adw::Dialog>() {
            dialog.force_close();
        }
    }
    window.set_default_size(1040, 720);
    window.present();
    check_library_group(state, dir, vec![0, 1, group]);
}

fn check_library_group(state: Rc<State>, dir: PathBuf, mut groups: Vec<i64>) {
    let group = groups.remove(0);
    state.changed();
    let window = state.window.borrow().as_ref().unwrap().clone();
    find(window.upcast_ref(), &|w| {
        w.widget_name() == format!("collection-{group}")
    })
    .unwrap()
    .downcast::<gtk::Button>()
    .unwrap()
    .emit_clicked();
    glib::timeout_add_local_once(Duration::from_millis(700), move || {
        let prev = find(window.upcast_ref(), &|w| {
            w.widget_name() == "library-previous"
        })
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
        let next = find(window.upcast_ref(), &|w| w.widget_name() == "library-next")
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap();
        let list = find(window.upcast_ref(), &|w| w.widget_name() == "library-cards")
            .unwrap()
            .downcast::<gtk::FlowBox>()
            .unwrap();
        let scroll = find(window.upcast_ref(), &|w| {
            w.widget_name() == "library-scroll"
        })
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
        let adjustment = scroll.vadjustment();
        assert!(
            adjustment.upper() <= adjustment.page_size() + 1.0,
            "Library page requires scrolling: {} > {}",
            adjustment.upper(),
            adjustment.page_size()
        );
        assert!(!prev.is_sensitive());
        assert!(next.is_sensitive(), "No Next in collection {group}");
        let mut seen = 0;
        for _ in 0..30 {
            let count = list.observe_children().n_items();
            assert!(count > 0, "Paging opened an empty page");
            seen += count as usize;
            if !next.is_sensitive() {
                break;
            }
            next.emit_clicked();
            assert!(prev.is_sensitive());
        }
        let expected = state
            .store
            .borrow()
            .query(&crate::model::Query {
                group_id: group,
                limit: 1000,
                ..Default::default()
            })
            .unwrap()
            .len();
        assert_eq!(
            seen, expected,
            "Skipped or repeated items in collection {group}"
        );
        while prev.is_sensitive() {
            prev.emit_clicked();
        }
        println!("PASS fitting library pages and Previous/Next in collection {group}");
        if groups.is_empty() {
            glib::timeout_add_local_once(Duration::from_millis(600), move || {
                snapshot(window.upcast_ref(), &dir.join("library-paging.png"));
                window.set_default_size(420, 700);
                glib::timeout_add_local_once(Duration::from_millis(700), move || {
                    let adjustment = scroll.vadjustment();
                    assert!(
                        adjustment.upper() <= adjustment.page_size() + 1.0,
                        "Narrow library page requires scrolling"
                    );
                    snapshot(window.upcast_ref(), &dir.join("library-paging-narrow.png"));
                    verify_library_keyboard(state, dir);
                });
            });
        } else {
            check_library_group(state, dir, groups);
        }
    });
}

fn verify_library_keyboard(state: Rc<State>, dir: PathBuf) {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous_hook(info);
        std::process::exit(1);
    }));
    glib::MainContext::default().spawn_local(async move {
        use gtk::gdk::{Key, ModifierType};
        let window = state.window.borrow().as_ref().unwrap().clone();
        let list = find(window.upcast_ref(), &|w| w.widget_name() == "library-cards").unwrap().downcast::<gtk::FlowBox>().unwrap();
        let controllers = list.observe_controllers();
        let keys = (0..controllers.n_items()).filter_map(|i| controllers.item(i)?.downcast::<gtk::EventControllerKey>().ok()).find(|c| c.name().as_deref() == Some("library-item-shortcuts")).unwrap();
        let press = |key: Key, modifiers: ModifierType| keys.emit_by_name::<bool>("key-pressed", &[&key, &0u32, &modifiers]);
        let selected = || list.selected_children().first().map(|c| c.widget_name().to_string());
        let first = list.child_at_index(0).unwrap();
        first.grab_focus();
        let name = selected().expect("Keyboard focus selects its card");
        let id: i64 = name.strip_prefix("library-item-").unwrap().parse().unwrap();
        glib::timeout_future(Duration::from_secs(3)).await;
        assert_eq!(selected().as_deref(), Some(name.as_str()), "Selection does not time out");
        state.changed();
        glib::timeout_future(Duration::from_millis(300)).await;
        assert_eq!(selected().as_deref(), Some(name.as_str()), "Selection survives refresh by ID");
        snapshot(window.upcast_ref(), &dir.join("library-selected.png"));
        // Every command uses the selected ID; menus/dialogs follow the same paths.
        state.store.borrow_mut().settings.preview_mode = "native".into();
        assert!(press(Key::F3, ModifierType::empty()));
        let editor = state.editors.borrow().get(&id).unwrap().upgrade().unwrap();
        let pages = find(editor.upcast_ref(), &|w| w.widget_name() == "editor-pages").unwrap().downcast::<gtk::Stack>().unwrap();
        assert_eq!(pages.visible_child_name().as_deref(), Some("preview"));
        editor.close();
        window.present();
        list.selected_children()[0].grab_focus();
        assert!(press(Key::F4, ModifierType::empty()));
        let editor = state.editors.borrow().get(&id).unwrap().upgrade().unwrap();
        let pages = find(editor.upcast_ref(), &|w| w.widget_name() == "editor-pages").unwrap().downcast::<gtk::Stack>().unwrap();
        assert_eq!(pages.visible_child_name().as_deref(), Some("edit"));
        editor.close();
        for (key, mods, heading) in [(Key::Return, ModifierType::ALT_MASK, "Item Details"), (Key::F2, ModifierType::empty(), "Rename Item"), (Key::F8, ModifierType::empty(), "Delete this item?"), (Key::Delete, ModifierType::empty(), "Delete this item?")] {
            window.present();
            list.selected_children()[0].grab_focus();
            assert!(press(key, mods));
            let dialog = window.visible_dialog().expect("Shortcut opens its dialog");
            if let Some(alert) = dialog.downcast_ref::<adw::AlertDialog>() {
                assert_eq!(alert.heading().as_deref(), Some(heading));
            } else { assert_eq!(dialog.widget_name(), "item-details"); }
            dialog.close();
            glib::timeout_future(Duration::from_millis(300)).await;
            assert!(state.store.borrow().get(id).is_ok(), "Cancelling deletion preserves the item");
        }
        // Search must own Delete; even dispatching the grid controller cannot act.
        let search = find(window.upcast_ref(), &|w| w.is::<gtk::SearchEntry>()).unwrap().downcast::<gtk::SearchEntry>().unwrap();
        search.grab_focus();
        assert!(!press(Key::Delete, ModifierType::empty()));
        assert!(window.visible_dialog().is_none());
        list.selected_children()[0].grab_focus();
        assert!(!press(Key::F4, ModifierType::ALT_MASK));
        // Cross the page boundary and come back using only arrows.
        let next = find(window.upcast_ref(), &|w| w.widget_name() == "library-next").unwrap().downcast::<gtk::Button>().unwrap();
        if next.is_sensitive() {
            let last = list.last_child().unwrap().downcast::<gtk::FlowBoxChild>().unwrap();
            last.grab_focus();
            let last_name = selected().unwrap();
            assert!(press(Key::Right, ModifierType::empty()));
            assert_ne!(selected().as_deref(), Some(last_name.as_str()));
            assert!(press(Key::Left, ModifierType::empty()));
            assert_eq!(selected().as_deref(), Some(last_name.as_str()));
        }
        // Removing a selected item must not silently target its former neighbour.
        let removed: i64 = selected().unwrap().strip_prefix("library-item-").unwrap().parse().unwrap();
        state.store.borrow().delete(removed).unwrap();
        state.changed();
        assert!(selected().is_none());
        println!("PASS library selection, refresh identity, card shortcuts, search safety and arrow paging");
        println!("UI_SMOKE_OK {}", dir.display());
        state.app.quit();
    });
}

// Exercise the actual chooser and its refresh path, including a duplicate add.
fn verify_history_retention(state: &Rc<State>) {
    let (history, note, pinned);
    {
        let mut store = state.store.borrow_mut();
        store
            .capture("Retention history fixture", "Test", "test.desktop", false)
            .unwrap();
        history = store
            .query(&crate::model::Query {
                search: "Retention history fixture".into(),
                ..Default::default()
            })
            .unwrap()[0]
            .id;
        note = store
            .save_item(None, "Protected", "Retention note fixture")
            .unwrap();
        store
            .capture("Retention pinned fixture", "Test", "test.desktop", false)
            .unwrap();
        pinned = store
            .query(&crate::model::Query {
                search: "Retention pinned fixture".into(),
                ..Default::default()
            })
            .unwrap()[0]
            .id;
        let group = store
            .groups()
            .unwrap()
            .into_iter()
            .find(|g| g.id > 1)
            .unwrap()
            .id;
        store.move_item(pinned, group).unwrap();
        store
            .db
            .execute(
                "UPDATE items SET copied_at=?1 WHERE id IN (?2,?3,?4)",
                rusqlite::params![crate::model::now() - 2 * 86400, history, note, pinned],
            )
            .unwrap();
    }
    let window = state.window.borrow().as_ref().unwrap().clone();
    let scale = find(window.upcast_ref(), &|w| {
        w.widget_name() == "history-retention-scale"
    })
    .unwrap()
    .downcast::<gtk::Scale>()
    .unwrap();
    let apply = find(window.upcast_ref(), &|w| {
        w.widget_name() == "apply-history-retention"
    })
    .unwrap()
    .downcast::<gtk::Button>()
    .unwrap();
    assert!(!apply.is_sensitive());
    scale.set_value(0.0);
    assert!(apply.is_sensitive());
    assert_eq!(state.store.borrow().settings.retention_days, 30);
    assert_eq!(state.store.borrow().expiry_count(1).unwrap(), 1);
    assert!(state.store.borrow().get(history).is_ok());
    apply.emit_clicked();
    let alert = window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AlertDialog>()
        .unwrap();
    assert!(alert.body().contains("History"));
    assert!(alert
        .body()
        .contains("1 History entry is currently due for permanent deletion."));
    alert.emit_by_name::<()>("response", &[&"cancel"]);
    alert.force_close();
    assert_eq!(state.store.borrow().settings.retention_days, 30);
    assert!(state.store.borrow().get(history).is_ok());
    apply.emit_clicked();
    let alert = window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AlertDialog>()
        .unwrap();
    alert.emit_by_name::<()>("response", &[&"accept"]);
    alert.force_close();
    assert_eq!(state.store.borrow().settings.retention_days, 1);
    assert!(state.store.borrow().get(history).is_err());
    assert!(state.store.borrow().get(note).is_ok());
    assert!(state.store.borrow().get(pinned).is_ok());
    assert!(!apply.is_sensitive());
    scale.set_value(scale.adjustment().upper());
    apply.emit_clicked();
    assert_eq!(state.store.borrow().settings.retention_days, 0);
    assert_eq!(state.store.borrow().prune().unwrap(), 0);
    // Restore the fixture's original period; even zero removals require consent.
    scale.set_value(9.0);
    apply.emit_clicked();
    let alert = window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AlertDialog>()
        .unwrap();
    alert.emit_by_name::<()>("response", &[&"accept"]);
    alert.force_close();
    assert_eq!(state.store.borrow().settings.retention_days, 30);
    println!("PASS retention slider, cancel, confirmed History-only deletion and Forever");
}

fn verify_ignored_app(state: &Rc<State>) {
    let window = state.window.borrow().as_ref().unwrap().clone();
    let preferences = || {
        let dialogs = window.dialogs();
        (0..dialogs.n_items())
            .rev()
            .find_map(|i| {
                dialogs
                    .item(i)?
                    .downcast::<adw::Dialog>()
                    .ok()
                    .filter(|d| d.widget_name() == "settings-dialog")
            })
            .expect("Settings dialog")
    };
    for _ in 0..2 {
        let dialog = preferences();
        find(dialog.upcast_ref(), &|w| {
            w.widget_name() == "settings-pages"
        })
        .unwrap()
        .downcast::<gtk::Stack>()
        .unwrap()
        .set_visible_child_name("privacy");
        let sensitive = find(dialog.upcast_ref(), &|w| {
            w.downcast_ref::<adw::SwitchRow>()
                .is_some_and(|row| row.title() == "Ignore Passwords and Sensitive Content")
        })
        .unwrap()
        .downcast::<adw::SwitchRow>()
        .unwrap();
        let page = sensitive
            .ancestor(adw::PreferencesPage::static_type())
            .unwrap()
            .downcast::<adw::PreferencesPage>()
            .unwrap();
        assert_eq!(page.name().as_deref(), Some("privacy"));
        let original = state.store.borrow().settings.ignore_sensitive;
        assert_eq!(sensitive.is_active(), original);
        sensitive.set_active(!original);
        assert_eq!(state.store.borrow().settings.ignore_sensitive, !original);
        sensitive.set_active(original);
        assert_eq!(state.store.borrow().settings.ignore_sensitive, original);
        let choose = find(dialog.upcast_ref(), &|w| {
            w.downcast_ref::<gtk::Button>()
                .is_some_and(|b| b.label().as_deref() == Some("Choose…"))
        })
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
        choose.emit_clicked();
        let dialogs = window.dialogs();
        let alert = (0..dialogs.n_items())
            .rev()
            .find_map(|i| dialogs.item(i)?.downcast::<adw::AlertDialog>().ok())
            .expect("Application chooser");
        let select = alert
            .extra_child()
            .unwrap()
            .downcast::<gtk::DropDown>()
            .unwrap();
        let model = select.model().unwrap();
        let index = (0..model.n_items())
            .find(|&i| {
                model
                    .item(i)
                    .unwrap()
                    .downcast::<gtk::StringObject>()
                    .unwrap()
                    .string()
                    == "Firefox"
            })
            .expect("Firefox is installed on the test host");
        select.set_selected(index);
        let add = find(alert.upcast_ref(), &|w| {
            w.downcast_ref::<gtk::Button>()
                .is_some_and(|b| b.label().as_deref() == Some("Add"))
        })
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
        add.emit_clicked();
        let refreshed = preferences();
        assert_eq!(
            find(refreshed.upcast_ref(), &|w| w.widget_name()
                == "settings-pages")
            .unwrap()
            .downcast::<gtk::Stack>()
            .unwrap()
            .visible_child_name()
            .as_deref(),
            Some("privacy")
        );
        let id = "org.mozilla.firefox.desktop";
        assert!(
            find(refreshed.upcast_ref(), &|w| {
                w.downcast_ref::<adw::ActionRow>()
                    .is_some_and(|r| r.title() == id)
            })
            .is_some(),
            "Added application must be visible in the refreshed list"
        );
        let saved: crate::model::Settings = serde_json::from_slice(
            &std::fs::read(
                crate::store::xdg_path("XDG_CONFIG_HOME", ".config")
                    .join("gnome-clip-notes/settings.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            saved
                .ignored_apps
                .iter()
                .filter(|value| value.as_str() == id)
                .count(),
            1
        );
    }
}
