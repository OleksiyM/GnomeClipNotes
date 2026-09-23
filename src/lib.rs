mod date_picker;
mod editor_process;
#[cfg(debug_assertions)]
mod editor_smoke;
mod export;
#[cfg(debug_assertions)]
mod export_smoke;
mod export_ui;
#[cfg(debug_assertions)]
mod filter_smoke;
pub mod i18n;
#[cfg(debug_assertions)]
mod i18n_smoke;
mod item_shortcuts;
mod library_selection;
mod model;
mod preferences;
mod preview_native;
mod release_check;
#[cfg(debug_assertions)]
mod smoke;
mod store;
mod ui;
mod update;
mod update_lock;
#[cfg(debug_assertions)]
mod update_smoke;

use adw::prelude::*;
use glib::variant::ToVariant;
use std::{cell::RefCell, rc::Rc};

pub const APP_ID: &str = "io.github.OleksiyM.GnomeClipNotes";
const PATH: &str = "/io/github/OleksiyM/GnomeClipNotes";
const INTERFACE: &str = "io.github.OleksiyM.GnomeClipNotes.Service";
thread_local! { static SERVICE_STATE: RefCell<Option<Rc<State>>> = const { RefCell::new(None) }; }
const XML: &str = r#"<node><interface name="io.github.OleksiyM.GnomeClipNotes.Service">
<method name="Capture"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="b" direction="in"/><arg type="b" direction="out"/></method>
<method name="Query"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
<method name="GetItem"><arg type="x" direction="in"/><arg type="s" direction="out"/></method>
<method name="Activate"><arg type="s" direction="in"/><arg type="x" direction="in"/></method>
<method name="Pause"><arg type="u" direction="in"/></method>
<method name="EditorSaved"><arg type="x" direction="in"/><arg type="s" direction="in"/></method>
<method name="GetUpdateStatus"><arg type="s" direction="out"/></method>
<method name="QuitForUpdate"><arg type="b" direction="out"/></method>
<signal name="Changed"/><signal name="PasteRequested"><arg type="s"/><arg type="b"/></signal>
</interface></node>"#;

pub struct State {
    pub app: adw::Application,
    pub store: RefCell<store::Store>,
    pub window: RefCell<Option<adw::ApplicationWindow>>,
    pub refresh: RefCell<Option<Box<dyn Fn()>>>,
    pub editors: RefCell<std::collections::HashMap<i64, glib::WeakRef<adw::ApplicationWindow>>>,
    pub remote_editors: RefCell<std::collections::HashMap<i64, String>>,
    pub preview_factory: Option<PreviewFactory>,
    pub update: Rc<update::UpdateState>,
}

/// Only the short-lived editor executable supplies a WebKit-backed renderer.
/// The core and persistent daemon do not refer to any WebKit type.
pub struct Preview {
    pub widget: gtk::Widget,
    pub set_content: Box<dyn Fn(&str)>,
}
pub type PreviewFactory = Rc<dyn Fn(&adw::ApplicationWindow) -> Preview>;

pub fn run_editor(factory: PreviewFactory) -> glib::ExitCode {
    editor_process::run(factory)
}
impl State {
    pub fn open_clipboard(self: &Rc<Self>) {
        if self.update.stopping() {
            return;
        }
        let state = self.clone();
        if preferences::maybe_offer_super_v(self, move || state.open_clipboard()) {
            return;
        }
        let Some(connection) = self.app.dbus_connection() else {
            ui::show(self);
            return;
        };
        let state = self.clone();
        connection.call(Some("org.gnome.Shell"), "/io/github/OleksiyM/GnomeClipNotes/Bridge",
            "io.github.OleksiyM.GnomeClipNotes.Bridge", "Toggle", None, None,
            gio::DBusCallFlags::NONE, 3000, gio::Cancellable::NONE, move |result| {
                if result.is_err() {
                    ui::show(&state);
                    ui::error(&state, &ui::tr("Enable the GnomeClipNotes GNOME extension to use the clipboard panel. Your saved items are available here."));
                }
            });
    }
    pub fn changed(&self) {
        if self.preview_factory.is_some() {
            if let Some(connection) = self.app.dbus_connection() {
                for id in self.editors.borrow().keys() {
                    connection.call(
                        Some(APP_ID),
                        PATH,
                        INTERFACE,
                        "EditorSaved",
                        Some(&(*id, self.app.application_id().unwrap().as_str()).to_variant()),
                        None,
                        gio::DBusCallFlags::NONE,
                        3000,
                        gio::Cancellable::NONE,
                        |_| {},
                    );
                }
            }
            return;
        }
        if let Some(connection) = self.app.dbus_connection() {
            let _ = connection.emit_signal(None, PATH, INTERFACE, "Changed", None);
        }
        if let Some(refresh) = self.refresh.borrow().as_ref() {
            refresh();
        }
    }
    pub fn report(&self, result: store::Result<()>) {
        match result {
            Ok(()) => self.changed(),
            Err(error) => ui::error(self, &error.to_string()),
        }
    }
    pub fn paste(self: &Rc<Self>, text: &str) {
        let active = self.store.borrow().settings.paste_mode == "active";
        if let Some(connection) = self.app.dbus_connection() {
            let state = self.clone();
            let content = text.to_owned();
            connection.call(Some("org.gnome.Shell"),"/io/github/OleksiyM/GnomeClipNotes/Bridge","io.github.OleksiyM.GnomeClipNotes.Bridge","Paste",Some(&(text,active).to_variant()),None,gio::DBusCallFlags::NONE,3000,gio::Cancellable::NONE,move |result| {
                if result.is_err() {
                    if let Some(display)=gtk::gdk::Display::default() {display.clipboard().set_text(&content);}
                    ui::error(&state,"Copied to clipboard. Enable the GnomeClipNotes GNOME extension to paste into other applications.");
                }
            });
        }
    }
    pub fn activate(self: &Rc<Self>, action: &str, id: i64) {
        if self.update.stopping() {
            return;
        }
        if matches!(action, "show" | "settings" | "new-note") {
            let state = self.clone();
            let next_action = action.to_string();
            if preferences::maybe_offer_super_v(self, move || state.activate(&next_action, id)) {
                return;
            }
        }
        match action {
            "clipboard" => self.open_clipboard(),
            "settings" => ui::settings(self),
            "about" => ui::about(self),
            "new-note" => ui::editor(self, None),
            "edit" => ui::editor(self, Some(id)),
            "view" | "preview" => ui::view(self, id),
            "delete" => ui::delete_item(self, id),
            "pin" => ui::move_item(self, id),
            "rename" => ui::rename_item(self, id),
            "new-group" => ui::new_group(self),
            "new-group-move" => ui::new_group_for_item(self, id),
            action if action.starts_with("move-group-") => {
                if let Ok(group) = action["move-group-".len()..].parse::<i64>() {
                    if group > 1 {
                        let result = self.store.borrow().move_item(id, group);
                        self.report(result);
                    }
                }
            }
            "notes" => {
                let result = self.store.borrow().move_item(id, 1);
                self.report(result);
            }
            "info" => {
                let item = self.store.borrow().get(id);
                match item {
                    Ok(item) => ui::info(self, &item),
                    Err(e) => ui::error(self, &e.to_string()),
                }
            }
            _ => ui::show(self),
        }
    }
}

fn register_service(state: &Rc<State>) -> store::Result<()> {
    let connection = state
        .app
        .dbus_connection()
        .ok_or("No session bus connection")?;
    let info = gio::DBusNodeInfo::for_xml(XML)?
        .lookup_interface(INTERFACE)
        .ok_or("Missing D-Bus interface")?;
    SERVICE_STATE.with(|slot| *slot.borrow_mut() = Some(state.clone()));
    connection
        .register_object(PATH, &info)
        .method_call(
            move |_connection, _sender, _path, _interface, method, parameters, invocation| {
                let state = SERVICE_STATE
                    .with(|slot| slot.borrow().as_ref().expect("service initialized").clone());
                let result: store::Result<glib::Variant> = (|| match method {
                    "GetUpdateStatus" => Ok((state.update.status().to_string(),).to_variant()),
                    "QuitForUpdate" => {
                        let accepted = state.update.begin_shutdown();
                        Ok((accepted,).to_variant())
                    }
                    "Capture" => {
                        if state.update.stopping() {
                            return Ok((false,).to_variant());
                        }
                        let (text, source, source_id, sensitive) = parameters
                            .get::<(String, String, String, bool)>()
                            .ok_or("Invalid capture parameters")?;
                        let accepted = state
                            .store
                            .borrow_mut()
                            .capture(&text, &source, &source_id, sensitive)?;
                        if accepted {
                            state.changed();
                        }
                        Ok((accepted,).to_variant())
                    }
                    "Query" => {
                        let (query,) = parameters
                            .get::<(String,)>()
                            .ok_or("Invalid query parameters")?;
                        let query: model::Query = serde_json::from_str(&query)?;
                        Ok((state.store.borrow().query_json(&query)?,).to_variant())
                    }
                    "GetItem" => {
                        let (id,) = parameters.get::<(i64,)>().ok_or("Invalid item ID")?;
                        Ok((serde_json::to_string(&state.store.borrow().get(id)?)?,).to_variant())
                    }
                    "Activate" => {
                        let (action, id) =
                            parameters.get::<(String, i64)>().ok_or("Invalid action")?;
                        state.activate(&action, id);
                        Ok(().to_variant())
                    }
                    "Pause" => {
                        let (minutes,) =
                            parameters.get::<(u32,)>().ok_or("Invalid pause duration")?;
                        {
                            let mut store = state.store.borrow_mut();
                            store.settings.paused_until = if minutes == 0 {
                                0
                            } else {
                                model::now() + i64::from(minutes.min(1440)) * 60
                            };
                            store.save_settings()?;
                        }
                        state.changed();
                        Ok(().to_variant())
                    }
                    "EditorSaved" => {
                        let (id, name) = parameters
                            .get::<(i64, String)>()
                            .ok_or("Invalid editor notification")?;
                        if !name.starts_with("io.github.OleksiyM.GnomeClipNotes.Editor.")
                            || !gio::Application::id_is_valid(&name)
                        {
                            return Err("Invalid editor instance".into());
                        }
                        state.remote_editors.borrow_mut().insert(id, name);
                        state.changed();
                        Ok(().to_variant())
                    }
                    _ => Err("Unknown method".into()),
                })();
                match result {
                    Ok(value) => {
                        invocation.return_value(Some(&value));
                        if method == "QuitForUpdate" && state.update.stopping() {
                            // Queue the reply before exiting. The installer must
                            // still wait for process exit and the exclusive lock.
                            let app = state.app.clone();
                            _connection.flush(gio::Cancellable::NONE, move |_| app.quit());
                        }
                    }
                    Err(error) => invocation.return_dbus_error(
                        "io.github.OleksiyM.GnomeClipNotes.Error",
                        &error.to_string(),
                    ),
                }
            },
        )
        .build()?;
    Ok(())
}

pub fn run() -> glib::ExitCode {
    i18n::init();
    let args: Vec<String> = std::env::args().collect();
    #[cfg(debug_assertions)]
    if args.iter().any(|arg| arg == "--i18n-probe") {
        println!(
            "{}",
            serde_json::json!({
                "language": i18n::active_language(),
                "simple": i18n::tr("New Note"),
                "plural_one": i18n::ntrf("{count} item", "{count} items", 1, &[]),
                "plural_many": i18n::ntrf("{count} item", "{count} items", 2, &[]),
                "shell": i18n::shell_catalog(),
            })
        );
        return glib::ExitCode::SUCCESS;
    }
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!("GnomeClipNotes\n\nUsage: gnome-clip-notes [OPTION]\n\n  --daemon        Keep the clipboard service running\n  --library       Open the library window\n  --new-note      Open a new Markdown note\n  --settings      Open settings\n  --about         Show application information\n  --backup PATH   Create a consistent SQLite snapshot (new file)\n  --quit          Stop the running application\n  --version       Print version\n\nWithout an option, open the clipboard panel.");
        return glib::ExitCode::SUCCESS;
    }
    if let Some(index) = args.iter().position(|s| s == "--backup") {
        let result = (|| {
            let _runtime_lock = update_lock::RuntimeLock::acquire()?;
            let path = args
                .get(index + 1)
                .ok_or("Usage: gnome-clip-notes --backup PATH")?;
            store::Store::open_for_backup()?.backup(std::path::Path::new(path))
        })();
        return match result {
            Ok(()) => glib::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Backup failed: {error}");
                glib::ExitCode::FAILURE
            }
        };
    }
    if args.iter().any(|s| s == "--version") {
        println!("GnomeClipNotes {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }
    let _runtime_lock = match update_lock::RuntimeLock::acquire() {
        Ok(lock) => lock,
        Err(error) => {
            eprintln!(
                "{}: {error}",
                i18n::tr("Cannot start GnomeClipNotes. An update may be in progress.")
            );
            return glib::ExitCode::FAILURE;
        }
    };
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let state_slot: Rc<RefCell<Option<Rc<State>>>> = Rc::new(RefCell::new(None));
    let slot = state_slot.clone();
    app.connect_startup(move |app| {
        let store = match store::Store::open() {
            Ok(store) => store,
            Err(error) => {
                eprintln!("Cannot open private storage: {error}");
                app.quit();
                return;
            }
        };
        if let Err(error) = store.prune() {
            eprintln!("Retention failed: {error}");
        }
        let state = Rc::new(State {
            app: app.clone(),
            store: RefCell::new(store),
            window: RefCell::new(None),
            refresh: RefCell::new(None),
            editors: RefCell::new(std::collections::HashMap::new()),
            remote_editors: RefCell::new(std::collections::HashMap::new()),
            preview_factory: None,
            update: Rc::new(update::UpdateState::default()),
        });
        ui::init(&state);
        if let Err(error) = register_service(&state) {
            eprintln!("Cannot register service: {error}");
            app.quit();
            return;
        }
        state.changed();
        // Keep the clipboard service alive after its last window closes.
        let hold = app.hold();
        app.connect_shutdown(move |_| {
            let _ = &hold;
        });
        let weak = Rc::downgrade(&state);
        glib::timeout_add_seconds_local(6 * 60 * 60, move || {
            if let Some(state) = weak.upgrade() {
                let result = state.store.borrow().prune();
                match result {
                    Ok(n) if n > 0 => state.changed(),
                    Err(error) => eprintln!("Retention failed: {error}"),
                    _ => {}
                }
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        *slot.borrow_mut() = Some(state);
    });
    let activation_slot = state_slot.clone();
    app.connect_activate(move |_| {
        if let Some(state) = activation_slot.borrow().as_ref() {
            state.open_clipboard();
        }
    });
    app.connect_command_line(move |_app, command| {
        if let Some(state) = state_slot.borrow().as_ref() {
            let args = command.arguments();
            if state.update.stopping() {
                eprintln!("{}", i18n::tr("GnomeClipNotes is stopping for an update."));
                return glib::ExitCode::FAILURE;
            }
            #[cfg(debug_assertions)]
            if args.iter().any(|s| s == "--test-update") {
                update_smoke::run(state);
                return glib::ExitCode::SUCCESS;
            }
            #[cfg(debug_assertions)]
            if args.iter().any(|s| s == "--test-i18n") {
                i18n_smoke::run(state);
                return glib::ExitCode::SUCCESS;
            }
            #[cfg(debug_assertions)]
            if args.iter().any(|s| s == "--test-export") {
                export_smoke::run(state);
                return glib::ExitCode::SUCCESS;
            }
            #[cfg(debug_assertions)]
            if args.iter().any(|s| s == "--test-library-filters") {
                filter_smoke::run(state);
                return glib::ExitCode::SUCCESS;
            }
            #[cfg(debug_assertions)]
            if args.iter().any(|s| s == "--smoke-test") {
                smoke::run(state);
                return glib::ExitCode::SUCCESS;
            }
            if args.iter().any(|s| s == "--quit") {
                state.app.quit();
            } else if !args.iter().any(|s| s == "--daemon") {
                let action = if args.iter().any(|s| s == "--settings") {
                    "settings"
                } else if args.iter().any(|s| s == "--new-note") {
                    "new-note"
                } else if args.iter().any(|s| s == "--about") {
                    "about"
                } else if args.iter().any(|s| s == "--library") {
                    "show"
                } else {
                    "clipboard"
                };
                state.activate(action, 0);
            }
        }
        glib::ExitCode::SUCCESS
    });
    app.run()
}
