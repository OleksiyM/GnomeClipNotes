//! Full preview runs in a short-lived editor service, never in the clipboard daemon.
use crate::{i18n, store, ui, PreviewFactory, State, APP_ID};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

thread_local! { static SERIAL: Cell<u64> = const { Cell::new(0) }; }

fn activation_token() -> Option<glib::GString> {
    let display = gtk::gdk::Display::default()?;
    let context = display.app_launch_context();
    context.set_icon_name(Some(APP_ID));
    let info = gio::AppInfo::create_from_commandline(
        "gnome-clip-notes-editor",
        Some("GnomeClipNotes"),
        gio::AppInfoCreateFlags::SUPPORTS_STARTUP_NOTIFICATION,
    )
    .ok()?;
    context.startup_notify_id(Some(&info), &[])
}

pub fn present_existing(state: &Rc<State>, id: Option<i64>) -> bool {
    let Some(id) = id else {
        return false;
    };
    let name = state.remote_editors.borrow().get(&id).cloned();
    let Some(name) = name else {
        return false;
    };
    if let Some(bus) = state.app.dbus_connection() {
        let path = format!("/{}", name.replace('.', "/"));
        let mut platform = HashMap::<String, glib::Variant>::new();
        if let Some(token) = activation_token() {
            platform.insert("activation-token".into(), token.as_str().to_variant());
            platform.insert("desktop-startup-id".into(), token.as_str().to_variant());
        }
        bus.call(
            Some(&name),
            &path,
            "org.freedesktop.Application",
            "Activate",
            Some(&(platform,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
            |_| {},
        );
    }
    true
}

pub fn open(state: &Rc<State>, id: Option<i64>, preview: bool) {
    let Some(editor_lifetime) = state.update.track(crate::update::EditorKind::Full) else {
        return;
    };
    let serial = SERIAL.with(|s| {
        let next = s.get() + 1;
        s.set(next);
        next
    });
    let instance = format!("{APP_ID}.Editor.P{}N{serial}", std::process::id());
    let unit = format!(
        "gnome-clip-notes-editor-{}-{serial}.service",
        std::process::id()
    );
    let result = (|| -> Result<gio::Subprocess, Box<dyn std::error::Error>> {
        let exe = std::env::current_exe()?.with_file_name("gnome-clip-notes-editor");
        if !exe.is_file() {
            return Err(i18n::tr("The Full preview helper is missing. Reinstall GnomeClipNotes or choose Lightweight · Native in Settings.").into());
        }
        let mut args = vec![
            "systemd-run".to_string(),
            "--user".into(),
            "--quiet".into(),
            "--wait".into(),
            "--collect".into(),
            "--pipe".into(),
            "--service-type=exec".into(),
            "--property=KillMode=control-group".into(),
            "--property=TimeoutStopSec=3s".into(),
            format!("--unit={unit}"),
        ];
        // Forward only the environment needed for this desktop and private storage,
        // not the shell's complete environment or its credentials.
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_DATA_DIRS",
            "XDG_CONFIG_DIRS",
            "LANG",
            "LC_ALL",
            "LC_MESSAGES",
            "GDK_BACKEND",
            "GSK_RENDERER",
            "GTK_A11Y",
            "GSETTINGS_BACKEND",
        ] {
            if let Ok(value) = std::env::var(key) {
                args.push(format!("--setenv={key}={value}"));
            }
        }
        for (key, fallback) in [
            ("XDG_DATA_HOME", ".local/share"),
            ("XDG_CONFIG_HOME", ".config"),
            ("XDG_CACHE_HOME", ".cache"),
        ] {
            args.push(format!(
                "--setenv={key}={}",
                store::xdg_path(key, fallback).display()
            ));
        }
        // Preserve the system preference, not a gettext LANGUAGE override made
        // inside the daemon. The helper receives the daemon's active UI policy.
        if let Some(language) = i18n::system_language() {
            args.push(format!("--setenv=LANGUAGE={}", language.to_string_lossy()));
        } else {
            args.push("--setenv=LANGUAGE=".into());
        }
        if let Some(token) = activation_token() {
            args.push(format!("--setenv=XDG_ACTIVATION_TOKEN={token}"));
            args.push(format!("--setenv=DESKTOP_STARTUP_ID={token}"));
        }
        args.extend([
            "--".into(),
            exe.to_string_lossy().into_owned(),
            "--instance".into(),
            instance.clone(),
            "--language".into(),
            i18n::active_language().into(),
            "--item".into(),
            id.unwrap_or(0).to_string(),
        ]);
        if preview {
            args.push("--preview".into());
        }
        #[cfg(debug_assertions)]
        if let Ok(dir) = std::env::var("GCN_EDITOR_TEST_DIR") {
            args.extend(["--test-editor".into(), dir]);
        }
        let launcher = gio::SubprocessLauncher::new(
            gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
        );
        #[cfg(debug_assertions)]
        if let Ok(address) = std::env::var("GCN_TEST_SYSTEMD_BUS") {
            launcher.setenv("DBUS_SESSION_BUS_ADDRESS", address, true);
        }
        let refs: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
        Ok(launcher.spawn(&refs)?)
    })();
    match result {
        Ok(child) => {
            if let Some(id) = id {
                state
                    .remote_editors
                    .borrow_mut()
                    .insert(id, instance.clone());
            }
            let weak = Rc::downgrade(state);
            child.clone().communicate_utf8_async(None, gio::Cancellable::NONE, move |result| {
                drop(editor_lifetime);
                if let Some(state) = weak.upgrade() {
                    state.remote_editors.borrow_mut().retain(|_, name| name != &instance);
                    if result.is_err() || !child.is_successful() {
                        ui::error(&state, &ui::tr("The Full preview editor could not run or stopped unexpectedly. Lightweight · Native remains available in Settings."));
                        if let Ok((_, Some(stderr))) = result { eprintln!("Editor service: {stderr}"); }
                    }
                }
            });
        }
        Err(error) => ui::error(state, &error.to_string()),
    }
}

pub fn run(factory: PreviewFactory) -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let preview = args.iter().any(|arg| arg == "--preview");
    let value = |key| {
        args.iter()
            .position(|s| s == key)
            .and_then(|i| args.get(i + 1))
    };
    i18n::init_with_language(value("--language").map(String::as_str));
    let Some(instance) = value("--instance").filter(|s| {
        s.starts_with("io.github.OleksiyM.GnomeClipNotes.Editor.")
            && gio::Application::id_is_valid(s)
    }) else {
        eprintln!(
            "This is the GnomeClipNotes editor helper. Open notes through the main application."
        );
        return glib::ExitCode::FAILURE;
    };
    let id = value("--item")
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|id| *id > 0);
    let _runtime_lock = match crate::update_lock::RuntimeLock::acquire() {
        Ok(lock) => lock,
        Err(error) => {
            eprintln!(
                "{}: {error}",
                i18n::tr("Cannot open the editor. An update may be in progress.")
            );
            return glib::ExitCode::FAILURE;
        }
    };
    #[cfg(debug_assertions)]
    let test_dir = value("--test-editor").cloned();
    let app = adw::Application::builder()
        .application_id(instance)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    let slot = Rc::new(RefCell::new(None::<Rc<State>>));
    let failed = Rc::new(Cell::new(false));
    {
        let slot = slot.clone();
        let failed = failed.clone();
        app.connect_startup(move |app| match store::Store::open() {
            Ok(store) => {
                let state = Rc::new(State {
                    app: app.clone(),
                    store: RefCell::new(store),
                    window: RefCell::new(None),
                    refresh: RefCell::new(None),
                    editors: RefCell::new(HashMap::new()),
                    remote_editors: RefCell::new(HashMap::new()),
                    preview_factory: Some(factory.clone()),
                    update: Rc::new(crate::update::UpdateState::default()),
                });
                ui::init(&state);
                // Theme changes still apply to an already open worker window.
                // Preview mode itself remains fixed until that editor is closed.
                let config = state.store.borrow().config_path.clone();
                if let Some(directory) = config.parent() {
                    if let Ok(monitor) = gio::File::for_path(directory)
                        .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
                    {
                        monitor.connect_changed(move |_, file, other, _| {
                            if file.path().as_ref() != Some(&config)
                                && other.and_then(|f| f.path()).as_ref() != Some(&config)
                            {
                                return;
                            }
                            if let Ok(bytes) = std::fs::read(&config) {
                                if let Ok(settings) =
                                    serde_json::from_slice::<crate::model::Settings>(&bytes)
                                {
                                    ui::apply_theme(&settings.theme);
                                }
                            }
                        });
                        app.connect_shutdown(move |_| {
                            monitor.cancel();
                        });
                    }
                }
                *slot.borrow_mut() = Some(state);
            }
            Err(error) => {
                eprintln!("Cannot open editor storage: {error}");
                failed.set(true);
                app.quit();
            }
        });
    }
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            if let Some(window) = window.downcast_ref::<adw::ApplicationWindow>() {
                ui::present_editor(window);
            }
        } else if let Some(state) = slot.borrow().as_ref() {
            ui::editor_in_process_mode(state, id, preview);
            #[cfg(debug_assertions)]
            if let Some(dir) = test_dir.as_ref() {
                crate::editor_smoke::run(state, dir);
            }
        }
    });
    app.connect_command_line(|app, _| {
        app.activate();
        glib::ExitCode::SUCCESS
    });
    let exit = app.run();
    if failed.get() {
        glib::ExitCode::FAILURE
    } else {
        exit
    }
}
