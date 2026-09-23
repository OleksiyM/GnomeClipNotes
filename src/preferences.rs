use crate::{
    i18n::{languages, mark, normalize_language, ntrf, tr, trf},
    model::Settings,
    store, ui, State,
};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    fs,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    rc::Rc,
};

thread_local! {
    static SETTINGS_DIALOG: RefCell<Option<glib::WeakRef<adw::Dialog>>> = const { RefCell::new(None) };
    static SETTINGS_PAGES: RefCell<glib::WeakRef<gtk::Stack>> = RefCell::new(glib::WeakRef::new());
    static CONFLICT_OFFERED: Cell<bool> = const { Cell::new(false) };
}

fn is_super_v(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "<super>v" | "<mod4>v")
}

fn calendar_settings() -> Option<gio::Settings> {
    let schema =
        gio::SettingsSchemaSource::default()?.lookup("org.gnome.shell.keybindings", true)?;
    if !schema.has_key("toggle-message-tray") {
        return None;
    }
    Some(gio::Settings::new_full(
        &schema,
        None::<&gio::SettingsBackend>,
        None,
    ))
}

fn super_v_conflict() -> bool {
    extension_settings().is_ok_and(|s| s.strv("activate-shortcut").iter().any(|v| is_super_v(v)))
        && calendar_settings()
            .is_some_and(|s| s.strv("toggle-message-tray").iter().any(|v| is_super_v(v)))
}

fn release_calendar_shortcut() -> Result<(), String> {
    // Re-read after confirmation; never write back a stale list from the dialog.
    if !super_v_conflict() {
        return Ok(());
    }
    let settings =
        calendar_settings().ok_or_else(|| tr("GNOME calendar shortcuts are unavailable."))?;
    if !settings.is_writable("toggle-message-tray") {
        return Err(tr(
            "Calendar shortcuts are locked by your system administrator.",
        ));
    }
    let remaining: Vec<String> = settings
        .strv("toggle-message-tray")
        .iter()
        .filter(|v| !is_super_v(v))
        .map(|v| v.to_string())
        .collect();
    settings
        .set_strv("toggle-message-tray", remaining.as_slice())
        .map_err(|e| e.to_string())?;
    gio::Settings::sync();
    Ok(())
}

fn offer_super_v(state: &Rc<State>, finished: impl Fn() + 'static) {
    let dialog = adw::AlertDialog::builder()
        .heading(tr("Use Super+V for ClipNotes?"))
        .body(tr("GNOME also uses Super+V for the calendar and notifications. Remove only that calendar shortcut? Other shortcuts, including Super+M if assigned, will be kept. You can always open the calendar by clicking the clock."))
        .build();
    dialog.set_widget_name("super-v-conflict-dialog");
    dialog.add_response("keep", &tr("Keep GNOME Shortcut"));
    dialog.add_response("use", &tr("Use for ClipNotes"));
    dialog.set_default_response(Some("keep"));
    dialog.set_close_response("keep");
    dialog.set_response_appearance("use", adw::ResponseAppearance::Suggested);
    let state_copy = state.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "use" {
            if let Err(message) = release_calendar_shortcut() {
                ui::error(&state_copy, &message);
                return;
            }
        } else if !save_change(&state_copy, |s| s.super_v_conflict_dismissed = true) {
            return;
        }
        finished();
    });
    dialog.present(state.app.active_window().as_ref());
}

pub(crate) fn maybe_offer_super_v(state: &Rc<State>, finished: impl Fn() + 'static) -> bool {
    if state.store.borrow().settings.super_v_conflict_dismissed
        || CONFLICT_OFFERED.with(Cell::get)
        || !super_v_conflict()
    {
        return false;
    }
    CONFLICT_OFFERED.with(|seen| seen.set(true));
    ui::show(state);
    offer_super_v(state, finished);
    true
}

#[cfg(debug_assertions)]
pub(crate) fn verify_shortcut_consent(state: &Rc<State>) {
    // Called only by --smoke-test with isolated XDG paths and memory GSettings.
    assert_eq!(std::env::var("GSETTINGS_BACKEND").as_deref(), Ok("memory"));
    let calendar = calendar_settings().unwrap();
    let extension = extension_settings().unwrap();
    extension
        .set_strv("activate-shortcut", ["<Super>v"])
        .unwrap();
    calendar
        .set_strv("toggle-message-tray", ["<Super>v", "<Super>m"])
        .unwrap();
    assert!(super_v_conflict());
    let window = state.window.borrow().as_ref().unwrap().clone();
    assert!(maybe_offer_super_v(state, || {}));
    let dialog = window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AlertDialog>()
        .unwrap();
    assert_eq!(
        calendar.strv("toggle-message-tray").len(),
        2,
        "Prompt wrote settings before consent"
    );
    dialog.emit_by_name::<()>("response", &[&"keep"]);
    dialog.force_close();
    assert!(super_v_conflict());
    assert!(state.store.borrow().settings.super_v_conflict_dismissed);
    assert!(!maybe_offer_super_v(state, || {}));
    offer_super_v(state, || {});
    let dialog = window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AlertDialog>()
        .unwrap();
    // Simulate an independent edit while the confirmation is open.
    calendar
        .set_strv(
            "toggle-message-tray",
            ["<Super>v", "<Super>m", "<Control>F12"],
        )
        .unwrap();
    dialog.emit_by_name::<()>("response", &[&"use"]);
    dialog.force_close();
    assert_eq!(
        calendar
            .strv("toggle-message-tray")
            .iter()
            .map(|v| v.as_str())
            .collect::<Vec<_>>(),
        vec!["<Super>m", "<Control>F12"]
    );
    assert!(!super_v_conflict());
    calendar
        .set_strv("toggle-message-tray", ["<Super>v", "<Super>m"])
        .unwrap();
    extension
        .set_strv("activate-shortcut", ["<Shift><Alt>v"])
        .unwrap();
    assert!(!super_v_conflict());
    release_calendar_shortcut().unwrap();
    assert_eq!(calendar.strv("toggle-message-tray").len(), 2);
    extension
        .set_strv("activate-shortcut", ["<Super>v"])
        .unwrap();
    release_calendar_shortcut().unwrap();
    assert_eq!(
        calendar
            .strv("toggle-message-tray")
            .iter()
            .map(|v| v.as_str())
            .collect::<Vec<_>>(),
        vec!["<Super>m"]
    );
    let original = state.store.borrow().settings.clone();
    assert!(write_shortcuts(
        state,
        &original.activate_shortcut,
        &original.note_shortcut,
        "<Control><Alt>b",
        original.quick_paste,
    ));
    assert_eq!(
        state.store.borrow().settings.library_shortcut,
        "<Control><Alt>b"
    );
    assert_eq!(
        extension
            .strv("library-shortcut")
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
        vec!["<Control><Alt>b"]
    );
    assert!(write_shortcuts(
        state,
        &original.activate_shortcut,
        &original.note_shortcut,
        "",
        original.quick_paste,
    ));
    assert!(extension.strv("library-shortcut").is_empty());
    assert!(write_shortcuts(
        state,
        &original.activate_shortcut,
        &original.note_shortcut,
        &original.library_shortcut,
        original.quick_paste,
    ));
    println!("PASS shortcut consent, decline, stale settings and alternate shortcut");
}

fn reopen(state: &Rc<State>, dialog: &glib::WeakRef<adw::Dialog>) {
    let page = dialog.upgrade().and_then(|dialog| {
        let page = SETTINGS_PAGES.with(|slot| {
            slot.borrow()
                .upgrade()
                .and_then(|pages| pages.visible_child_name())
        });
        dialog.force_close();
        page
    });
    SETTINGS_DIALOG.with(|slot| *slot.borrow_mut() = None);
    show_page(state, page.as_deref());
}

const ACTIVATE_SHORTCUTS: &[(&str, &str)] = &[
    (mark("Super+V"), "<Super>v"),
    (mark("Shift+Alt+V"), "<Shift><Alt>v"),
    (mark("None"), ""),
];
const NOTE_SHORTCUTS: &[(&str, &str)] = &[
    (mark("Super+Shift+N"), "<Super><Shift>n"),
    (mark("Shift+Alt+N"), "<Shift><Alt>n"),
    (mark("None"), ""),
];
const LIBRARY_SHORTCUTS: &[(&str, &str)] = &[
    (mark("Super+B"), "<Super>b"),
    (mark("Ctrl+Alt+B"), "<Control><Alt>b"),
    (mark("Disabled"), ""),
];
const RETENTION: &[(i64, &str)] = &[
    (1, mark("1 day")),
    (2, mark("2 days")),
    (3, mark("3 days")),
    (4, mark("4 days")),
    (5, mark("5 days")),
    (6, mark("6 days")),
    (7, mark("1 week")),
    (14, mark("2 weeks")),
    (21, mark("3 weeks")),
    (30, mark("1 month")),
    (60, mark("2 months")),
    (90, mark("3 months")),
    (120, mark("4 months")),
    (150, mark("5 months")),
    (180, mark("6 months")),
    (210, mark("7 months")),
    (240, mark("8 months")),
    (270, mark("9 months")),
    (300, mark("10 months")),
    (330, mark("11 months")),
    (365, mark("1 year")),
    (0, mark("Forever")),
];

fn save_change(state: &Rc<State>, mutate: impl FnOnce(&mut Settings)) -> bool {
    let previous = state.store.borrow().settings.clone();
    let result = {
        let mut store = state.store.borrow_mut();
        mutate(&mut store.settings);
        store.save_settings()
    };
    match result {
        Ok(()) => {
            state.changed();
            true
        }
        Err(error) => {
            state.store.borrow_mut().settings = previous;
            ui::error(state, &error.to_string());
            false
        }
    }
}

fn action_row(title: &str, subtitle: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(tr(title))
        .subtitle(tr(subtitle))
        .build();
    row.set_use_markup(false);
    row
}

fn data_row(title: &str, subtitle: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(tr(subtitle))
        .build();
    row.set_use_markup(false);
    row
}

fn switch_row(title: &str, subtitle: &str, active: bool) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(tr(title))
        .subtitle(tr(subtitle))
        .active(active)
        .build();
    row.set_use_markup(false);
    row
}

fn combo(title: &str, values: &[&str], selected: u32) -> adw::ComboRow {
    let labels: Vec<String> = values.iter().map(|value| tr(value)).collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    adw::ComboRow::builder()
        .title(tr(title))
        .model(&gtk::StringList::new(&refs))
        .selected(selected)
        .build()
}

fn shortcut_label(label: &str) -> String {
    match label {
        "View" => tr("View"),
        "Edit" => tr("Edit"),
        "Info" => tr("Info"),
        "Rename…" => tr("Rename…"),
        "Delete…" => tr("Delete…"),
        _ => label.to_string(),
    }
}

fn private_dir(path: &Path) -> store::Result<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

fn autostart_path() -> PathBuf {
    store::xdg_path("XDG_CONFIG_HOME", ".config")
        .join("autostart/io.github.OleksiyM.GnomeClipNotes.desktop")
}

fn configure_autostart(enabled: bool) -> store::Result<()> {
    let path = autostart_path();
    if !enabled {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        return Ok(());
    }
    let binary = std::env::current_exe()?;
    let binary = binary
        .to_str()
        .ok_or("Executable path is not valid UTF-8")?;
    if binary.contains(['\n', '"']) {
        return Err("Executable path contains an unsupported character".into());
    }
    let parent = path.parent().ok_or("Invalid autostart path")?;
    private_dir(parent)?;
    let tmp = path.with_extension("desktop.tmp");
    let contents = format!("[Desktop Entry]\nType=Application\nName=GnomeClipNotes Background Service\nComment=Keep clipboard history available\nExec=\"{binary}\" --daemon\nIcon=io.github.OleksiyM.GnomeClipNotes\nTerminal=false\nNoDisplay=true\nX-GNOME-Autostart-enabled=true\n");
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&tmp)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn set_autostart(state: &Rc<State>, enabled: bool) -> bool {
    let previous = state.store.borrow().settings.open_at_login;
    if let Err(error) = configure_autostart(enabled) {
        ui::error(state, &error.to_string());
        return false;
    }
    if save_change(state, |settings| settings.open_at_login = enabled) {
        return true;
    }
    if let Err(error) = configure_autostart(previous) {
        ui::error(state, &error.to_string());
    }
    false
}

pub(crate) fn extension_settings() -> Result<gio::Settings, String> {
    const ID: &str = "org.gnome.shell.extensions.gnome-clip-notes";
    if let Some(source) = gio::SettingsSchemaSource::default() {
        if let Some(schema) = source.lookup(ID, true) {
            return Ok(gio::Settings::new_full(
                &schema,
                None::<&gio::SettingsBackend>,
                None,
            ));
        }
    }
    let data_home = store::xdg_path("XDG_DATA_HOME", ".local/share");
    let candidates = [
        data_home.join("gnome-shell/extensions/gnome-clip-notes@oleksiym.github.io/schemas"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("extension/schemas"),
    ];
    for directory in candidates {
        if !directory.join("gschemas.compiled").is_file() {
            continue;
        }
        let Some(directory) = directory.to_str() else {
            continue;
        };
        let source = gio::SettingsSchemaSource::from_directory(
            directory,
            gio::SettingsSchemaSource::default().as_ref(),
            false,
        )
        .map_err(|error| error.to_string())?;
        if let Some(schema) = source.lookup(ID, false) {
            return Ok(gio::Settings::new_full(
                &schema,
                None::<&gio::SettingsBackend>,
                None,
            ));
        }
    }
    Err(tr(
        "The GNOME Shell extension schema is not installed. Run the installer first.",
    ))
}

fn write_shortcuts(
    state: &Rc<State>,
    activate: &str,
    note: &str,
    library: &str,
    quick: bool,
) -> bool {
    let settings = match extension_settings() {
        Ok(settings) => settings,
        Err(message) => {
            ui::error(state, &message);
            return false;
        }
    };
    if !settings
        .settings_schema()
        .is_some_and(|schema| schema.has_key("library-shortcut"))
    {
        ui::error(
            state,
            &tr("The installed GNOME Shell extension does not support the Library shortcut. Update the extension first."),
        );
        return false;
    }
    let old_activate: Vec<String> = settings
        .strv("activate-shortcut")
        .iter()
        .map(|value| value.to_string())
        .collect();
    let old_note: Vec<String> = settings
        .strv("note-shortcut")
        .iter()
        .map(|value| value.to_string())
        .collect();
    let old_library: Vec<String> = settings
        .strv("library-shortcut")
        .iter()
        .map(|value| value.to_string())
        .collect();
    let old_quick: Vec<Vec<String>> = (1..=9)
        .map(|number| {
            settings
                .strv(&format!("quick-paste-{number}"))
                .iter()
                .map(|value| value.to_string())
                .collect()
        })
        .collect();
    let activate_values: Vec<&str> = if activate.is_empty() {
        Vec::new()
    } else {
        vec![activate]
    };
    let note_values: Vec<&str> = if note.is_empty() {
        Vec::new()
    } else {
        vec![note]
    };
    let library_values: Vec<&str> = if library.is_empty() {
        Vec::new()
    } else {
        vec![library]
    };
    settings.delay();
    if let Err(error) = settings.set_strv("activate-shortcut", activate_values) {
        settings.revert();
        ui::error(state, &error.to_string());
        return false;
    }
    if let Err(error) = settings.set_strv("note-shortcut", note_values) {
        settings.revert();
        ui::error(state, &error.to_string());
        return false;
    }
    if let Err(error) = settings.set_strv("library-shortcut", library_values) {
        settings.revert();
        ui::error(state, &error.to_string());
        return false;
    }
    for number in 1..=9 {
        let value = format!("<Alt>{number}");
        let values: Vec<&str> = if quick {
            vec![value.as_str()]
        } else {
            Vec::new()
        };
        if let Err(error) = settings.set_strv(&format!("quick-paste-{number}"), values) {
            settings.revert();
            ui::error(state, &error.to_string());
            return false;
        }
    }
    settings.apply();
    let saved = save_change(state, |config| {
        config.activate_shortcut = activate.into();
        config.note_shortcut = note.into();
        config.library_shortcut = library.into();
        config.quick_paste = quick;
    });
    if !saved {
        settings.delay();
        let _ = settings.set_strv("activate-shortcut", old_activate.as_slice());
        let _ = settings.set_strv("note-shortcut", old_note.as_slice());
        let _ = settings.set_strv("library-shortcut", old_library.as_slice());
        for (index, values) in old_quick.iter().enumerate() {
            let _ = settings.set_strv(&format!("quick-paste-{}", index + 1), values.as_slice());
        }
        settings.apply();
    }
    saved
}

fn confirm_retention(state: &Rc<State>, days: i64, accepted: impl Fn() + 'static) {
    let count = match state.store.borrow().expiry_count(days) {
        Ok(count) => count,
        Err(error) => {
            ui::error(state, &error.to_string());
            return;
        }
    };
    let period = RETENTION
        .iter()
        .find(|(value, _)| *value == days)
        .map(|(_, label)| tr(label))
        .unwrap_or_else(|| ntrf("{count} day", "{count} days", days.max(0) as u64, &[]));
    let due = ntrf(
        "{count} History entry is currently due for permanent deletion.",
        "{count} History entries are currently due for permanent deletion.",
        count as u64,
        &[],
    );
    let retention = trf("New History retention: {period}.", &[("period", &period)]);
    let body = [
        retention,
        due,
        tr("Only History is affected. Notes and your folders will not be deleted. Future History entries will expire after the selected period."),
    ]
    .join("\n\n");
    ui::confirm(
        state,
        "Shorten History retention?",
        &body,
        "Apply",
        accepted,
    );
}

fn reset_defaults(state: &Rc<State>) -> bool {
    let previous = state.store.borrow().settings.clone();
    let defaults = Settings::default();
    if let Err(error) = configure_autostart(defaults.open_at_login) {
        ui::error(state, &error.to_string());
        return false;
    }
    if !write_shortcuts(
        state,
        &defaults.activate_shortcut,
        &defaults.note_shortcut,
        &defaults.library_shortcut,
        defaults.quick_paste,
    ) {
        let _ = configure_autostart(previous.open_at_login);
        return false;
    }
    if !save_change(state, |settings| *settings = defaults) {
        let _ = write_shortcuts(
            state,
            &previous.activate_shortcut,
            &previous.note_shortcut,
            &previous.library_shortcut,
            previous.quick_paste,
        );
        let _ = configure_autostart(previous.open_at_login);
        return false;
    }
    let prune_result = state.store.borrow().prune();
    if let Err(error) = prune_result {
        let restore_result = {
            let mut store = state.store.borrow_mut();
            store.settings = previous.clone();
            store.save_settings()
        };
        let _ = write_shortcuts(
            state,
            &previous.activate_shortcut,
            &previous.note_shortcut,
            &previous.library_shortcut,
            previous.quick_paste,
        );
        let _ = configure_autostart(previous.open_at_login);
        let message = match restore_result {
            Ok(()) => error.to_string(),
            Err(restore) => trf(
                "{error}\nCould not restore settings: {restore_error}",
                &[
                    ("error", &error.to_string()),
                    ("restore_error", &restore.to_string()),
                ],
            ),
        };
        ui::error(state, &message);
        return false;
    }
    state.changed();
    true
}

fn choose_installed_app(state: &Rc<State>, dialog: &adw::Dialog) {
    let mut apps: Vec<(String, String)> = gio::AppInfo::all()
        .into_iter()
        .filter_map(|app| {
            let id = app.id()?.to_string();
            Some((app.display_name().to_string(), id))
        })
        .collect();
    apps.sort_by_cached_key(|a| a.0.to_lowercase());
    apps.dedup_by(|a, b| a.1 == b.1);
    if apps.is_empty() {
        ui::error(state, &tr("No installed applications were found."));
        return;
    }
    let names: Vec<&str> = apps.iter().map(|(name, _)| name.as_str()).collect();
    let select = gtk::DropDown::from_strings(&names);
    let alert = adw::AlertDialog::builder()
        .heading(tr("Ignore Installed Application"))
        .extra_child(&select)
        .default_response("add")
        .close_response("cancel")
        .build();
    alert.add_response("cancel", &tr("Cancel"));
    alert.add_response("add", &tr("Add"));
    alert.set_response_appearance("add", adw::ResponseAppearance::Suggested);
    let state = state.clone();
    let dialog_weak = dialog.downgrade();
    alert.connect_response(None, move |_, response| {
        if response != "add" {
            return;
        }
        let Some((_, id)) = apps.get(select.selected() as usize) else {
            return;
        };
        let id = id.clone();
        if save_change(&state, |settings| {
            if !settings.ignored_apps.contains(&id) {
                settings.ignored_apps.push(id);
            }
        }) {
            reopen(&state, &dialog_weak);
        }
    });
    alert.present(Some(dialog));
}

fn add_folders(group: &adw::PreferencesGroup, state: &Rc<State>, dialog: &adw::Dialog) {
    let groups = match state.store.borrow().groups() {
        Ok(groups) => groups,
        Err(error) => {
            ui::error(state, &error.to_string());
            return;
        }
    };
    let folders: Vec<_> = groups.into_iter().filter(|folder| folder.id >= 2).collect();
    let count = folders.len();
    for (index, folder) in folders.into_iter().enumerate() {
        let row = data_row(&folder.name, "");
        let rename = gtk::Button::from_icon_name("document-edit-symbolic");
        rename.set_tooltip_text(Some(&tr("Rename")));
        let up = gtk::Button::from_icon_name("go-up-symbolic");
        up.set_tooltip_text(Some(&tr("Move up")));
        up.set_sensitive(index > 0);
        let down = gtk::Button::from_icon_name("go-down-symbolic");
        down.set_tooltip_text(Some(&tr("Move down")));
        down.set_sensitive(index + 1 < count);
        let delete = gtk::Button::from_icon_name("user-trash-symbolic");
        delete.set_tooltip_text(Some(&tr("Delete")));
        for button in [&rename, &up, &down, &delete] {
            button.set_valign(gtk::Align::Center);
            button.add_css_class("flat");
            row.add_suffix(button);
        }
        {
            let state = state.clone();
            let window = dialog.downgrade();
            let name = folder.name.clone();
            let id = folder.id;
            rename.connect_clicked(move |_| {
                let s = state.clone();
                let w = window.clone();
                ui::prompt(&state, "Rename Folder", &name, move |value| {
                    let result = s.store.borrow().rename_group(id, &value);
                    match result {
                        Ok(()) => {
                            s.changed();
                            reopen(&s, &w)
                        }
                        Err(error) => ui::error(&s, &error.to_string()),
                    }
                });
            });
        }
        for (button, direction) in [(&up, -1), (&down, 1)] {
            let state = state.clone();
            let window = dialog.downgrade();
            let id = folder.id;
            button.connect_clicked(move |_| {
                let result = state.store.borrow_mut().reorder_group(id, direction);
                match result {
                    Ok(()) => {
                        state.changed();
                        reopen(&state, &window)
                    }
                    Err(error) => ui::error(&state, &error.to_string()),
                }
            });
        }
        {
            let state = state.clone();
            let window = dialog.downgrade();
            let id = folder.id;
            delete.connect_clicked(move |_| {
                let s = state.clone();
                let w = window.clone();
                ui::confirm(
                    &state,
                    "Delete this folder?",
                    &tr("Items in this folder will be moved to Notes. The items themselves will not be deleted."),
                    "Delete",
                    move || {
                        let result = s.store.borrow_mut().delete_group(id);
                        match result {
                            Ok(()) => {
                                s.changed();
                                reopen(&s, &w)
                            }
                            Err(error) => ui::error(&s, &error.to_string()),
                        }
                    },
                );
            });
        }
        group.add(&row);
    }
}

pub fn show(state: &Rc<State>) {
    show_page(state, None);
}

fn show_page(state: &Rc<State>, page: Option<&str>) {
    let parent = state.app.active_window();
    if let Some(dialog) =
        SETTINGS_DIALOG.with(|slot| slot.borrow().as_ref().and_then(glib::WeakRef::upgrade))
    {
        dialog.present(parent.as_ref());
        return;
    }
    let dialog = adw::Dialog::builder()
        .title(tr("Settings"))
        .content_width(940)
        .content_height(700)
        .build();
    dialog.set_widget_name("settings-dialog");
    dialog.set_width_request(360);
    dialog.set_height_request(360);
    let pages = gtk::Stack::builder()
        .hhomogeneous(false)
        .vhomogeneous(false)
        .build();
    pages.set_widget_name("settings-pages");
    SETTINGS_PAGES.with(|slot| *slot.borrow_mut() = pages.downgrade());
    SETTINGS_DIALOG.with(|slot| *slot.borrow_mut() = Some(dialog.downgrade()));
    let current = state.store.borrow().settings.clone();

    let general = adw::PreferencesPage::builder()
        .title(tr("General"))
        .name("general")
        .icon_name("preferences-system-symbolic")
        .build();
    let startup = adw::PreferencesGroup::builder()
        .title(tr("Startup and appearance"))
        .build();
    let login = switch_row(
        mark("Open at Login"),
        mark("Keep clipboard capture available after signing in"),
        current.open_at_login,
    );
    {
        let state = state.clone();
        login.connect_active_notify(move |toggle| {
            let wanted = toggle.is_active();
            if wanted != state.store.borrow().settings.open_at_login
                && !set_autostart(&state, wanted)
            {
                toggle.set_active(!wanted);
            }
        });
    }
    startup.add(&login);
    let themes = [mark("System"), mark("Light"), mark("Dark")];
    let theme_index = match current.theme.as_str() {
        "light" => 1,
        "dark" => 2,
        _ => 0,
    };
    let theme = combo(mark("Theme"), &themes, theme_index);
    {
        let state = state.clone();
        let guard = Rc::new(Cell::new(false));
        theme.connect_selected_notify(move |row| {
            if guard.get() {
                return;
            }
            let value = match row.selected() {
                1 => "light",
                2 => "dark",
                _ => "system",
            };
            let old = state.store.borrow().settings.theme.clone();
            if value == old {
                return;
            }
            if save_change(&state, |settings| settings.theme = value.into()) {
                adw::StyleManager::default().set_color_scheme(match value {
                    "light" => adw::ColorScheme::ForceLight,
                    "dark" => adw::ColorScheme::ForceDark,
                    _ => adw::ColorScheme::Default,
                });
            } else {
                guard.set(true);
                row.set_selected(match old.as_str() {
                    "light" => 1,
                    "dark" => 2,
                    _ => 0,
                });
                guard.set(false);
            }
        });
    }
    startup.add(&theme);
    let available_languages = languages();
    let mut language_names = vec![tr("System default")];
    language_names.extend(
        available_languages
            .iter()
            .map(|language| language.name.to_string()),
    );
    let language_refs: Vec<&str> = language_names.iter().map(String::as_str).collect();
    let normalized_language = normalize_language(&current.language);
    let language_index = available_languages
        .iter()
        .position(|language| language.id == normalized_language)
        .map_or(0, |index| index as u32 + 1);
    let language = adw::ComboRow::builder()
        .title(tr("Language"))
        .subtitle(tr("Log out and back in to apply."))
        .model(&gtk::StringList::new(&language_refs))
        .selected(language_index)
        .build();
    {
        let state = state.clone();
        let guard = Rc::new(Cell::new(false));
        language.connect_selected_notify(move |row| {
            if guard.get() {
                return;
            }
            let selected = row.selected();
            let value = if selected == 0 {
                "system".to_string()
            } else {
                available_languages
                    .get(selected as usize - 1)
                    .map(|language| language.id.to_string())
                    .unwrap_or_default()
            };
            let old = state.store.borrow().settings.language.clone();
            if value == old {
                return;
            }
            if !save_change(&state, |settings| settings.language = value) {
                guard.set(true);
                let normalized = normalize_language(&old);
                row.set_selected(
                    available_languages
                        .iter()
                        .position(|language| language.id == normalized)
                        .map_or(0, |index| index as u32 + 1),
                );
                guard.set(false);
            }
        });
    }
    startup.add(&language);
    general.add(&startup);

    let preview_group = adw::PreferencesGroup::builder()
        .title(tr("Note preview"))
        .description(tr("Applies to newly opened note editors."))
        .build();
    let full_preview = gtk::CheckButton::new();
    let native_preview = gtk::CheckButton::new();
    native_preview.set_group(Some(&full_preview));
    let full_row = action_row(
        mark("Full · WebKit"),
        mark("Richer Markdown formatting, including tables. Uses more memory."),
    );
    let native_row = action_row(
        mark("Lightweight · Native"),
        mark("Basic Markdown formatting. Uses less memory."),
    );
    full_row.add_suffix(&full_preview);
    native_row.add_suffix(&native_preview);
    full_row.set_activatable_widget(Some(&full_preview));
    native_row.set_activatable_widget(Some(&native_preview));
    full_preview.set_active(current.preview_mode != "native");
    native_preview.set_active(current.preview_mode == "native");
    let preview_guard = Rc::new(Cell::new(false));
    for (toggle, mode) in [(&full_preview, "webkit"), (&native_preview, "native")] {
        let state = state.clone();
        let full = full_preview.downgrade();
        let native = native_preview.downgrade();
        let guard = preview_guard.clone();
        toggle.connect_toggled(move |toggle| {
            if guard.get() || !toggle.is_active() {
                return;
            }
            let old = state.store.borrow().settings.preview_mode.clone();
            if old == mode {
                return;
            }
            if !save_change(&state, |settings| settings.preview_mode = mode.into()) {
                guard.set(true);
                if let Some(full) = full.upgrade() {
                    full.set_active(old != "native");
                }
                if let Some(native) = native.upgrade() {
                    native.set_active(old == "native");
                }
                guard.set(false);
            }
        });
    }
    preview_group.add(&full_row);
    preview_group.add(&native_row);
    general.add(&preview_group);

    let capture = adw::PreferencesGroup::builder()
        .title(tr("Capture and paste"))
        .build();
    let sensitive = switch_row(
        mark("Ignore Passwords and Sensitive Content"),
        mark("Uses sensitivity hints supplied by the source application"),
        current.ignore_sensitive,
    );
    {
        let state = state.clone();
        let guard = Rc::new(Cell::new(false));
        sensitive.connect_active_notify(move |toggle| {
            if guard.get() {
                return;
            }
            let active = toggle.is_active();
            let old = state.store.borrow().settings.ignore_sensitive;
            if active == old {
                return;
            }
            if !save_change(&state, |settings| settings.ignore_sensitive = active) {
                guard.set(true);
                toggle.set_active(old);
                guard.set(false);
            }
        });
    }
    let paste = combo(
        mark("Paste Item"),
        &[mark("To active application"), mark("To clipboard")],
        if current.paste_mode == "clipboard" {
            1
        } else {
            0
        },
    );
    {
        let state = state.clone();
        let guard = Rc::new(Cell::new(false));
        paste.connect_selected_notify(move |row| {
            if guard.get() {
                return;
            }
            let value = if row.selected() == 1 {
                "clipboard"
            } else {
                "active"
            };
            let old = state.store.borrow().settings.paste_mode.clone();
            if value == old {
                return;
            }
            if !save_change(&state, |settings| settings.paste_mode = value.into()) {
                guard.set(true);
                row.set_selected(if old == "clipboard" { 1 } else { 0 });
                guard.set(false);
            }
        });
    }
    capture.add(&paste);
    let remaining = (current.paused_until - crate::model::now()).max(0);
    let pause_index = if remaining == 0 {
        0
    } else if remaining <= 900 {
        1
    } else if remaining <= 1800 {
        2
    } else if remaining <= 3600 {
        3
    } else if remaining <= 10800 {
        4
    } else {
        5
    };
    let pause = combo(
        mark("Pause Clipboard Capture"),
        &[
            mark("Resume capture"),
            mark("15 minutes"),
            mark("30 minutes"),
            mark("1 hour"),
            mark("3 hours"),
            mark("8 hours"),
        ],
        pause_index,
    );
    let pause_subtitle = if remaining > 0 {
        ntrf(
            "Paused for about {count} more minute",
            "Paused for about {count} more minutes",
            ((remaining + 59) / 60) as u64,
            &[],
        )
    } else {
        tr("Capture is active")
    };
    pause.set_subtitle(&pause_subtitle);
    {
        let state = state.clone();
        let guard = Rc::new(Cell::new(false));
        pause.connect_selected_notify(move |row| {
            if guard.get() {
                return;
            }
            let seconds = match row.selected() {
                1 => 900,
                2 => 1800,
                3 => 3600,
                4 => 10800,
                5 => 28800,
                _ => 0,
            };
            let old = state.store.borrow().settings.paused_until;
            if !save_change(&state, |settings| {
                settings.paused_until = if seconds == 0 {
                    0
                } else {
                    crate::model::now() + seconds
                }
            }) {
                guard.set(true);
                row.set_selected(if old <= crate::model::now() {
                    0
                } else {
                    pause_index
                });
                guard.set(false);
            } else {
                let subtitle = if seconds == 0 {
                    tr("Capture is active")
                } else {
                    ntrf(
                        "Paused for {count} minute",
                        "Paused for {count} minutes",
                        (seconds / 60) as u64,
                        &[],
                    )
                };
                row.set_subtitle(&subtitle);
            }
        });
    }
    capture.add(&pause);
    general.add(&capture);

    let retention_group = adw::PreferencesGroup::builder()
        .title(tr("History retention"))
        .description(tr("Applies only to History. Notes and your folders are never deleted automatically. Months are 30 days; a year is 365 days."))
        .build();
    let history = adw::PreferencesPage::builder()
        .title(tr("History"))
        .name("history")
        .icon_name("document-open-recent-symbolic")
        .build();
    let retention_index = RETENTION
        .iter()
        .position(|(days, _)| *days == current.retention_days)
        .unwrap_or(9);
    let retention = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        0.0,
        (RETENTION.len() - 1) as f64,
        1.0,
    );
    retention.set_widget_name("history-retention-scale");
    retention.update_property(&[gtk::accessible::Property::Label(&tr("Keep History"))]);
    retention.set_round_digits(0);
    retention.set_digits(0);
    retention.set_draw_value(false);
    retention.set_value(retention_index as f64);
    retention.set_hexpand(true);
    let apply = gtk::Button::with_label(&tr("Apply"));
    apply.set_widget_name("apply-history-retention");
    apply.set_sensitive(false);
    apply.set_valign(gtk::Align::Center);
    let row = adw::PreferencesRow::new();
    let controls = gtk::Box::new(gtk::Orientation::Vertical, 8);
    ui::margins(&controls, 12);
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let caption = gtk::Label::new(Some(&tr("Keep History")));
    caption.set_xalign(0.0);
    caption.add_css_class("dim-label");
    let period = gtk::Label::new(Some(&tr(RETENTION[retention_index].1)));
    period.set_widget_name("history-retention-period");
    period.set_xalign(0.0);
    period.add_css_class("heading");
    let summary = gtk::Box::new(gtk::Orientation::Vertical, 4);
    summary.set_hexpand(true);
    summary.append(&caption);
    summary.append(&period);
    top.append(&summary);
    top.append(&apply);
    controls.append(&top);
    let scale_clamp = adw::Clamp::builder()
        .maximum_size(360)
        .tightening_threshold(300)
        .child(&retention)
        .build();
    controls.append(&scale_clamp);
    let current_period = tr(RETENTION[retention_index].1);
    let current_label = gtk::Label::new(Some(&trf(
        "Currently applied: {period}",
        &[("period", &current_period)],
    )));
    current_label.add_css_class("dim-label");
    current_label.set_wrap(true);
    current_label.set_xalign(0.0);
    controls.append(&current_label);
    row.set_child(Some(&controls));
    {
        let state = state.clone();
        let apply = apply.downgrade();
        retention.connect_value_changed(move |scale| {
            period.set_text(&tr(RETENTION[scale.value().round() as usize].1));
            scale.update_property(&[gtk::accessible::Property::ValueText(&tr(RETENTION
                [scale.value().round() as usize]
                .1))]);
            if let Some(apply) = apply.upgrade() {
                apply.set_sensitive(
                    RETENTION[scale.value().round() as usize].0
                        != state.store.borrow().settings.retention_days,
                );
            }
        });
    }
    {
        let state = state.clone();
        let retention = retention.clone();
        let apply_weak = apply.downgrade();
        apply.connect_clicked(move |_| {
            let Some((days, _)) = RETENTION.get(retention.value().round() as usize) else {
                return;
            };
            let days = *days;
            let old = state.store.borrow().settings.retention_days;
            if days == old {
                return;
            }
            let s = state.clone();
            let apply = apply_weak.clone();
            let scale = retention.downgrade();
            let current_label = current_label.clone();
            let commit = move || {
                if save_change(&s, |settings| settings.retention_days = days) {
                    if days > 0 && (old == 0 || days < old) {
                        let result = s.store.borrow().prune();
                        if let Err(error) = result {
                            save_change(&s, |settings| settings.retention_days = old);
                            ui::error(&s, &error.to_string());
                            return;
                        }
                    }
                    s.changed();
                    let index = RETENTION.iter().position(|(d, _)| *d == days).unwrap();
                    let period = tr(RETENTION[index].1);
                    current_label
                        .set_text(&trf("Currently applied: {period}", &[("period", &period)]));
                    if let Some(scale) = scale.upgrade() {
                        scale.set_value(index as f64);
                    }
                    if let Some(apply) = apply.upgrade() {
                        apply.set_sensitive(false);
                    }
                }
            };
            if days > 0 && (old == 0 || days < old) {
                confirm_retention(&state, days, commit);
            } else {
                commit();
            }
        });
    }
    retention_group.add(&row);
    history.add(&retention_group);
    let cleanup = adw::PreferencesGroup::builder().title(tr("Automatic cleanup"))
        .description(tr("Expired History entries are permanently deleted at startup and every 6 hours. A shorter period is applied only after confirmation. Notes and custom folders are kept."))
        .build();
    history.add(&cleanup);

    let privacy = adw::PreferencesPage::builder()
        .title(tr("Privacy"))
        .name("privacy")
        .icon_name("security-high-symbolic")
        .build();
    let protection = adw::PreferencesGroup::builder()
        .title(tr("Sensitive content"))
        .build();
    protection.add(&sensitive);
    privacy.add(&protection);
    let ignored = adw::PreferencesGroup::builder()
        .title(tr("Ignored applications"))
        .description(tr(
            "Clipboard changes from matching application identifiers are not stored",
        ))
        .build();
    for identifier in &current.ignored_apps {
        let row = data_row(identifier, mark("Application identifier"));
        let remove = gtk::Button::from_icon_name("list-remove-symbolic");
        remove.set_valign(gtk::Align::Center);
        row.add_suffix(&remove);
        let state = state.clone();
        let window = dialog.downgrade();
        let id = identifier.clone();
        remove.connect_clicked(move |_| {
            if save_change(&state, |settings| {
                settings.ignored_apps.retain(|value| value != &id)
            }) {
                reopen(&state, &window);
            }
        });
        ignored.add(&row);
    }
    let installed = action_row(
        mark("Add Installed Application"),
        mark("Choose from desktop applications visible to this user"),
    );
    let choose = gtk::Button::with_label(&tr("Choose…"));
    choose.set_valign(gtk::Align::Center);
    installed.add_suffix(&choose);
    {
        let state = state.clone();
        let window = dialog.downgrade();
        choose.connect_clicked(move |_| {
            if let Some(dialog) = window.upgrade() {
                choose_installed_app(&state, &dialog);
            }
        });
    }
    ignored.add(&installed);
    let manual = action_row(
        mark("Add Identifier Manually"),
        mark("Useful for applications not listed above"),
    );
    let add = gtk::Button::with_label(&tr("Add…"));
    add.set_valign(gtk::Align::Center);
    manual.add_suffix(&add);
    {
        let state = state.clone();
        let window = dialog.downgrade();
        add.connect_clicked(move |_| {
            let s = state.clone();
            let w = window.clone();
            ui::prompt(&state, "Application Identifier", "", move |value| {
                let value = value.trim().to_string();
                if value.is_empty() {
                    ui::error(&s, &tr("Application identifier cannot be empty."));
                    return;
                }
                if save_change(&s, |settings| {
                    if !settings.ignored_apps.contains(&value) {
                        settings.ignored_apps.push(value);
                    }
                }) {
                    reopen(&s, &w);
                }
            });
        });
    }
    ignored.add(&manual);
    privacy.add(&ignored);

    let shortcuts_page = adw::PreferencesPage::builder()
        .title(tr("Shortcuts"))
        .name("shortcuts")
        .icon_name("preferences-desktop-keyboard-shortcuts-symbolic")
        .build();
    let shortcut_group = adw::PreferencesGroup::builder()
        .title(tr("GNOME Shell shortcuts"))
        .description(tr(
            "Changes are written to the installed Shell extension settings",
        ))
        .build();
    let activate_index = ACTIVATE_SHORTCUTS
        .iter()
        .position(|(_, value)| *value == current.activate_shortcut)
        .unwrap_or(0) as u32;
    let activate_labels: Vec<&str> = ACTIVATE_SHORTCUTS.iter().map(|(label, _)| *label).collect();
    let activate = combo(mark("Activate"), &activate_labels, activate_index);
    {
        let state = state.clone();
        let guard = Rc::new(Cell::new(false));
        activate.connect_selected_notify(move |row| {
            if guard.get() {
                return;
            }
            let Some((_, value)) = ACTIVATE_SHORTCUTS.get(row.selected() as usize) else {
                return;
            };
            let settings = state.store.borrow().settings.clone();
            if *value == settings.activate_shortcut {
                return;
            }
            if !write_shortcuts(
                &state,
                value,
                &settings.note_shortcut,
                &settings.library_shortcut,
                settings.quick_paste,
            ) {
                guard.set(true);
                row.set_selected(
                    ACTIVATE_SHORTCUTS
                        .iter()
                        .position(|(_, v)| *v == settings.activate_shortcut)
                        .unwrap_or(0) as u32,
                );
                guard.set(false);
            }
        });
    }
    shortcut_group.add(&activate);
    let conflict = action_row(
        mark("Super+V conflicts with GNOME"),
        mark("The calendar also uses this shortcut. You can resolve the conflict or choose another Activate shortcut above."),
    );
    conflict.set_widget_name("super-v-conflict-row");
    conflict.set_visible(super_v_conflict());
    let resolve = gtk::Button::with_label(&tr("Resolve…"));
    resolve.set_valign(gtk::Align::Center);
    conflict.add_suffix(&resolve);
    conflict.set_activatable_widget(Some(&resolve));
    {
        let state = state.clone();
        let row = conflict.downgrade();
        resolve.connect_clicked(move |_| {
            let row = row.clone();
            offer_super_v(&state, move || {
                if let Some(row) = row.upgrade() {
                    row.set_visible(super_v_conflict());
                }
            });
        });
    }
    for watched in [calendar_settings(), extension_settings().ok()]
        .into_iter()
        .flatten()
    {
        let row = conflict.downgrade();
        let handler = watched.connect_changed(None, move |_, _| {
            if let Some(row) = row.upgrade() {
                row.set_visible(super_v_conflict());
            }
        });
        let handler = RefCell::new(Some(handler));
        conflict.connect_destroy(move |_| {
            if let Some(handler) = handler.borrow_mut().take() {
                watched.disconnect(handler);
            }
        });
    }
    shortcut_group.add(&conflict);
    let note_index = NOTE_SHORTCUTS
        .iter()
        .position(|(_, value)| *value == current.note_shortcut)
        .unwrap_or(0) as u32;
    let note_labels: Vec<&str> = NOTE_SHORTCUTS.iter().map(|(label, _)| *label).collect();
    let note = combo(mark("Create Note"), &note_labels, note_index);
    {
        let state = state.clone();
        let guard = Rc::new(Cell::new(false));
        note.connect_selected_notify(move |row| {
            if guard.get() {
                return;
            }
            let Some((_, value)) = NOTE_SHORTCUTS.get(row.selected() as usize) else {
                return;
            };
            let settings = state.store.borrow().settings.clone();
            if *value == settings.note_shortcut {
                return;
            }
            if !write_shortcuts(
                &state,
                &settings.activate_shortcut,
                value,
                &settings.library_shortcut,
                settings.quick_paste,
            ) {
                guard.set(true);
                row.set_selected(
                    NOTE_SHORTCUTS
                        .iter()
                        .position(|(_, v)| *v == settings.note_shortcut)
                        .unwrap_or(0) as u32,
                );
                guard.set(false);
            }
        });
    }
    shortcut_group.add(&note);
    let library_index = LIBRARY_SHORTCUTS
        .iter()
        .position(|(_, value)| *value == current.library_shortcut)
        .unwrap_or(0) as u32;
    let library_labels: Vec<&str> = LIBRARY_SHORTCUTS.iter().map(|(label, _)| *label).collect();
    let library = combo(mark("Open Library"), &library_labels, library_index);
    {
        let state = state.clone();
        let guard = Rc::new(Cell::new(false));
        library.connect_selected_notify(move |row| {
            if guard.get() {
                return;
            }
            let Some((_, value)) = LIBRARY_SHORTCUTS.get(row.selected() as usize) else {
                return;
            };
            let settings = state.store.borrow().settings.clone();
            if *value == settings.library_shortcut {
                return;
            }
            if !write_shortcuts(
                &state,
                &settings.activate_shortcut,
                &settings.note_shortcut,
                value,
                settings.quick_paste,
            ) {
                guard.set(true);
                row.set_selected(
                    LIBRARY_SHORTCUTS
                        .iter()
                        .position(|(_, v)| *v == settings.library_shortcut)
                        .unwrap_or(0) as u32,
                );
                guard.set(false);
            }
        });
    }
    shortcut_group.add(&library);
    let quick = switch_row(
        mark("Quick Paste Alt+1…9"),
        mark("Paste History items 1–9"),
        current.quick_paste,
    );
    {
        let state = state.clone();
        let guard = Rc::new(Cell::new(false));
        quick.connect_active_notify(move |toggle| {
            if guard.get() {
                return;
            }
            let settings = state.store.borrow().settings.clone();
            if toggle.is_active() == settings.quick_paste {
                return;
            }
            if !write_shortcuts(
                &state,
                &settings.activate_shortcut,
                &settings.note_shortcut,
                &settings.library_shortcut,
                toggle.is_active(),
            ) {
                guard.set(true);
                toggle.set_active(settings.quick_paste);
                guard.set(false);
            }
        });
    }
    shortcut_group.add(&quick);
    shortcuts_page.add(&shortcut_group);
    shortcut_reference(&shortcuts_page);

    let folders_page = adw::PreferencesPage::builder()
        .title(tr("Folders"))
        .name("folders")
        .icon_name("folder-symbolic")
        .build();
    let folders = adw::PreferencesGroup::builder()
        .title(tr("Custom folders"))
        .description(tr("History and Notes are built in. Deleting a custom folder moves its items to Notes; the items are not deleted."))
        .build();
    add_folders(&folders, state, &dialog);
    let add_folder = action_row(
        mark("New Folder"),
        mark("Create another place for pinned items"),
    );
    let add_button = gtk::Button::with_label(&tr("Add…"));
    add_button.set_valign(gtk::Align::Center);
    add_folder.add_suffix(&add_button);
    {
        let state = state.clone();
        let window = dialog.downgrade();
        add_button.connect_clicked(move |_| {
            let s = state.clone();
            let w = window.clone();
            ui::prompt(&state, "New Folder", "", move |name| {
                let result = s.store.borrow().create_group(&name);
                match result {
                    Ok(()) => {
                        s.changed();
                        reopen(&s, &w)
                    }
                    Err(error) => ui::error(&s, &error.to_string()),
                }
            });
        });
    }
    folders.add(&add_folder);
    folders_page.add(&folders);

    let data_page = adw::PreferencesPage::builder()
        .title(tr("Data"))
        .name("data")
        .icon_name("document-save-symbolic")
        .build();
    let export_group = adw::PreferencesGroup::builder()
        .title(tr("Markdown export"))
        .description(tr("Save Notes and custom folders as readable Markdown documents. History is not exported."))
        .build();
    let export_row = action_row(
        mark("Export Notes and Folders"),
        mark("One Markdown file per collection, ready to read and edit"),
    );
    let export_button = gtk::Button::with_label(&tr("Export…"));
    export_button.set_widget_name("settings-export");
    export_button.set_valign(gtk::Align::Center);
    export_row.add_suffix(&export_button);
    {
        let state = state.clone();
        let parent = dialog.downgrade();
        export_button.connect_clicked(move |_| {
            if let Some(parent) = parent.upgrade() {
                crate::export_ui::show(&state, &parent);
            }
        });
    }
    export_group.add(&export_row);
    data_page.add(&export_group);

    let reset_group = adw::PreferencesGroup::builder()
        .title(tr("Defaults"))
        .build();
    let reset = action_row(
        mark("Reset All Settings"),
        mark("Restore capture, privacy, retention, theme, language, and shortcut defaults"),
    );
    let reset_button = gtk::Button::with_label(&tr("Reset…"));
    reset_button.add_css_class("destructive-action");
    reset_button.set_valign(gtk::Align::Center);
    reset.add_suffix(&reset_button);
    {
        let state = state.clone();
        let window = dialog.downgrade();
        reset_button.connect_clicked(move |_| {
            let default_retention = Settings::default().retention_days;
            let old = state.store.borrow().settings.retention_days;
            let apply = {
                let s = state.clone();
                let w = window.clone();
                move || {
                    if reset_defaults(&s) {
                        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);
                        reopen(&s, &w);
                    }
                }
            };
            if default_retention > 0 && (old == 0 || default_retention < old) {
                confirm_retention(&state, default_retention, apply)
            } else {
                ui::confirm(
                    &state,
                    "Reset all settings?",
                    &tr("This restores every setting to its original value."),
                    "Reset",
                    apply,
                )
            }
        });
    }
    reset_group.add(&reset);
    general.add(&reset_group);
    build_navigation(
        &dialog,
        &pages,
        &[
            general,
            history,
            privacy,
            shortcuts_page,
            folders_page,
            data_page,
        ],
        page,
    );
    dialog.present(parent.as_ref());
}

fn shortcut_reference(page: &adw::PreferencesPage) {
    let group = adw::PreferencesGroup::builder()
        .title(tr("Selected card"))
        .description(tr("In the library, click a card or focus it with the keyboard; use arrows to move across cards and pages. In the overlay, commands use the highlighted card. Delete edits nonempty search text; otherwise it acts on the highlighted card. Deletion always asks for confirmation."))
        .build();
    for (label, _, accelerator) in crate::item_shortcuts::COMMANDS {
        let row = adw::ActionRow::builder()
            .title(shortcut_label(label))
            .build();
        let (key, modifiers) = gtk::accelerator_parse(*accelerator).unwrap();
        let hint = gtk::Label::new(Some(&gtk::accelerator_get_label(key, modifiers)));
        hint.add_css_class("dim-label");
        row.add_suffix(&hint);
        group.add(&row);
    }
    let alternate = adw::ActionRow::builder()
        .title(tr("Delete (alternative)"))
        .build();
    let hint = gtk::Label::new(Some("F8"));
    hint.add_css_class("dim-label");
    alternate.add_suffix(&hint);
    group.add(&alternate);
    page.add(&group);
    for (title, description, entries) in [
        (mark("Clipboard overlay"), mark("When the overlay has focus and its context menu is closed."), vec![
            (mark("Previous / next card, across pages"), "← / → / ↑ / ↓"),
            (mark("Paste selected card"), "Return"),
            (mark("Copy selected card without pasting"), "<Control>c"),
            (mark("Paste visible card 1–9"), "Alt+1…9"),
            (mark("Close overlay"), "Escape"),
        ]),
        (mark("Note editor"), mark("While typing in the Editor tab. Save is available only when there are changes."), vec![
            (mark("Save note"), "<Control>s"),
            (mark("Continue a Markdown list"), "Return"),
        ]),
        (mark("Text editing"), mark("Standard GTK shortcuts in editable text fields, including library search and the note editor. In the overlay, Ctrl+C copies the selected card instead."), vec![
            (mark("Copy selected text"), "<Control>c"),
            (mark("Cut selected text"), "<Control>x"),
            (mark("Paste text"), "<Control>v"),
            (mark("Select all text"), "<Control>a"),
            (mark("Undo in the note editor"), "<Control>z"),
            (mark("Redo in the note editor"), "<Control><Shift>z"),
        ]),
    ] {
        let group = adw::PreferencesGroup::builder().title(tr(title)).description(tr(description)).build();
        for (label, accelerator) in entries {
            let row = adw::ActionRow::builder().title(tr(label)).build();
            row.set_use_markup(false);
            row.set_title_lines(0);
            let formatted = gtk::accelerator_parse(accelerator)
                .map(|(key, mods)| gtk::accelerator_get_label(key, mods).to_string())
                .unwrap_or_else(|| accelerator.to_string());
            let shortcut = gtk::Label::new(Some(&formatted));
            shortcut.add_css_class("dim-label");
            shortcut.set_xalign(1.0);
            shortcut.set_valign(gtk::Align::Center);
            row.add_suffix(&shortcut);
            group.add(&row);
        }
        page.add(&group);
    }
}

fn build_navigation(
    dialog: &adw::Dialog,
    pages: &gtk::Stack,
    sections: &[adw::PreferencesPage],
    initial: Option<&str>,
) {
    let sidebar_toolbar = adw::ToolbarView::new();
    sidebar_toolbar.add_top_bar(&adw::HeaderBar::new());
    let sidebar = gtk::ListBox::new();
    sidebar.set_widget_name("settings-navigation");
    sidebar.add_css_class("navigation-sidebar");
    let sidebar_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&sidebar)
        .build();
    sidebar_toolbar.set_content(Some(&sidebar_scroll));
    let sidebar_page = adw::NavigationPage::new(&sidebar_toolbar, &tr("Settings"));
    let content_toolbar = adw::ToolbarView::new();
    let content_header = adw::HeaderBar::new();
    content_toolbar.add_top_bar(&content_header);
    let clamp = adw::Clamp::builder()
        .maximum_size(640)
        .tightening_threshold(480)
        .child(pages)
        .build();
    content_toolbar.set_content(Some(&clamp));
    let content_page = adw::NavigationPage::new(&content_toolbar, &tr("General"));
    let split = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .min_sidebar_width(190.0)
        .max_sidebar_width(220.0)
        .sidebar_width_fraction(0.24)
        .build();
    split.set_widget_name("settings-split");
    for section in sections {
        let name = section.name().unwrap();
        pages.add_named(section, Some(&name));
        let row = gtk::ListBoxRow::new();
        row.set_widget_name(&format!("settings-section-{name}"));
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        ui::margins(&line, 6);
        line.append(&gtk::Image::from_icon_name(
            section
                .icon_name()
                .as_deref()
                .unwrap_or("preferences-system-symbolic"),
        ));
        let label = gtk::Label::new(Some(&section.title()));
        label.set_xalign(0.0);
        line.append(&label);
        row.set_child(Some(&line));
        sidebar.append(&row);
    }
    let weak_pages = pages.downgrade();
    let weak_content = content_page.downgrade();
    let weak_split = split.downgrade();
    let sections = sections.to_vec();
    sidebar.connect_row_activated(move |_, row| {
        if let (Some(pages), Some(content), Some(split)) = (
            weak_pages.upgrade(),
            weak_content.upgrade(),
            weak_split.upgrade(),
        ) {
            let section = &sections[row.index() as usize];
            pages.set_visible_child(section);
            content.set_title(&section.title());
            split.set_show_content(true);
        }
    });
    // Keep sidebar selection and the header in sync for programmatic page changes too.
    let weak_sidebar = sidebar.downgrade();
    let weak_content = content_page.downgrade();
    pages.connect_visible_child_notify(move |pages| {
        if let Some(section) = pages.visible_child().and_downcast::<adw::PreferencesPage>() {
            if let Some(content) = weak_content.upgrade() {
                content.set_title(&section.title());
            }
            if let Some(sidebar) = weak_sidebar.upgrade() {
                let name = format!("settings-section-{}", section.name().unwrap());
                let mut child = sidebar.first_child();
                while let Some(row) = child {
                    if row.widget_name() == name {
                        sidebar.select_row(row.downcast_ref::<gtk::ListBoxRow>());
                        break;
                    }
                    child = row.next_sibling();
                }
            }
        }
    });
    pages.set_visible_child_name(initial.unwrap_or("general"));
    if let Some(section) = pages.visible_child().and_downcast::<adw::PreferencesPage>() {
        content_page.set_title(&section.title());
    }
    // Initial General may already be visible, so notify once to select its row.
    pages.notify("visible-child");
    split.set_show_content(true);
    let breakpoint =
        adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 720sp").unwrap());
    breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
    dialog.add_breakpoint(breakpoint);
    dialog.set_child(Some(&split));
}
