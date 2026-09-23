//! One application-domain localization policy per process. Initialize before GTK
//! or worker threads. Shell receives resolved messages, never process locale edits.
use std::{
    ffi::{c_char, CString},
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    sync::OnceLock,
};

const DOMAIN: &str = "gnome-clip-notes";
unsafe extern "C" {
    fn textdomain(domain: *const c_char) -> *mut c_char;
    fn bindtextdomain(domain: *const c_char, directory: *const c_char) -> *mut c_char;
    fn bind_textdomain_codeset(domain: *const c_char, codeset: *const c_char) -> *mut c_char;
}

#[derive(serde::Deserialize)]
pub struct Language {
    pub id: String,
    pub name: String,
}

struct Policy {
    requested: String,
    english: bool,
    system_language: Option<std::ffi::OsString>,
}
static POLICY: OnceLock<Policy> = OnceLock::new();

pub fn languages() -> &'static [Language] {
    static LANGUAGES: OnceLock<Vec<Language>> = OnceLock::new();
    LANGUAGES.get_or_init(|| {
        serde_json::from_str(include_str!("../po/languages.json"))
            .expect("validated language registry")
    })
}

pub fn normalize_language(language: &str) -> &str {
    if language == "system" || languages().iter().any(|entry| entry.id == language) {
        language
    } else {
        "en"
    }
}

pub fn active_language() -> &'static str {
    POLICY
        .get()
        .map_or("system", |policy| policy.requested.as_str())
}

/// Call only from the single-threaded process entry point.
pub fn init() {
    init_with_language(None);
}

pub(crate) fn system_language() -> Option<std::ffi::OsString> {
    POLICY.get().map_or_else(
        || std::env::var_os("LANGUAGE"),
        |policy| policy.system_language.clone(),
    )
}

pub(crate) fn init_with_language(language: Option<&str>) {
    if POLICY.get().is_some() {
        return;
    }
    // Reading this field must not require opening the database or starting GTK.
    let config =
        crate::store::xdg_path("XDG_CONFIG_HOME", ".config").join("gnome-clip-notes/settings.json");
    let settings = std::fs::read(config)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let requested = normalize_language(
        language
            .or_else(|| {
                settings
                    .as_ref()
                    .and_then(|value| value.get("language"))
                    .and_then(serde_json::Value::as_str)
            })
            .unwrap_or("system"),
    )
    .to_owned();
    let system_language = std::env::var_os("LANGUAGE");
    let manual_catalog = requested != "system" && requested != "en";
    let domain = CString::new(DOMAIN).unwrap();
    // These changes are private to this executable, never the GNOME Shell.
    unsafe {
        if manual_catalog {
            std::env::set_var("LANGUAGE", &requested);
        }
        libc::setlocale(libc::LC_ALL, c"".as_ptr());
        if manual_catalog {
            // gettext intentionally ignores LANGUAGE in the C/POSIX locale,
            // including glibc's C.UTF-8. Leave real system locales unchanged;
            // for a C session try an installed UTF-8 message locale only.
            let locale = libc::setlocale(libc::LC_MESSAGES, std::ptr::null());
            let is_c = !locale.is_null()
                && std::ffi::CStr::from_ptr(locale)
                    .to_bytes()
                    .starts_with(b"C");
            if is_c {
                let candidate = CString::new(format!("{requested}.UTF-8")).unwrap();
                if libc::setlocale(libc::LC_MESSAGES, candidate.as_ptr()).is_null() {
                    libc::setlocale(libc::LC_MESSAGES, c"en_US.UTF-8".as_ptr());
                }
            }
        }
        if let Some(directory) =
            catalog_directory().and_then(|path| CString::new(path.as_os_str().as_bytes()).ok())
        {
            bindtextdomain(domain.as_ptr(), directory.as_ptr());
        }
        bind_textdomain_codeset(domain.as_ptr(), c"UTF-8".as_ptr());
        textdomain(domain.as_ptr());
    }
    let _ = POLICY.set(Policy {
        english: requested == "en",
        requested,
        system_language,
    });
}

fn catalog_directory() -> Option<PathBuf> {
    // Developer pseudolocales require an isolated temporary profile and are not
    // available through settings, registry, or release builds.
    #[cfg(debug_assertions)]
    if let Some(root) = std::env::var_os("GCN_I18N_TEST_DIR").map(PathBuf::from) {
        if root.parent() == Some(std::path::Path::new("/tmp"))
            && root
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("gnome-clip-notes-i18n."))
            && std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) == Some(root.join("config"))
        {
            return Some(root.join("locale"));
        }
    }
    let installed = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("share/locale")));
    installed
        .filter(|path| path.is_dir())
        .or_else(|| option_env!("GCN_BUILD_LOCALE_DIR").map(PathBuf::from))
}

pub fn tr(message: &str) -> String {
    if message.is_empty() {
        return String::new();
    }
    if POLICY.get().is_some_and(|policy| policy.english) {
        message.into()
    } else {
        glib::dgettext(Some(DOMAIN), message).to_string()
    }
}

pub fn ptr(context: &str, message: &str) -> String {
    if message.is_empty() {
        return String::new();
    }
    if POLICY.get().is_some_and(|policy| policy.english) {
        message.into()
    } else {
        glib::dpgettext2(Some(DOMAIN), context, message).to_string()
    }
}

pub fn ntr(singular: &str, plural: &str, count: u64) -> String {
    if POLICY.get().is_some_and(|policy| policy.english) {
        if count == 1 {
            singular.into()
        } else {
            plural.into()
        }
    } else {
        glib::dngettext(Some(DOMAIN), singular, plural, count as libc::c_ulong).to_string()
    }
}

/// Extraction marker for messages stored in a table and translated at use time.
pub const fn mark(message: &'static str) -> &'static str {
    message
}

pub fn trf(message: &str, args: &[(&str, &str)]) -> String {
    format_or_fallback(&tr(message), message, args)
}

pub fn ntrf(singular: &str, plural: &str, count: u64, args: &[(&str, &str)]) -> String {
    let number = count.to_string();
    let mut values = vec![("count", number.as_str())];
    values.extend(args.iter().copied().filter(|(key, _)| *key != "count"));
    format_or_fallback(
        &ntr(singular, plural, count),
        if count == 1 { singular } else { plural },
        &values,
    )
}

fn format_or_fallback(translated: &str, source: &str, args: &[(&str, &str)]) -> String {
    interpolate(translated, args)
        .or_else(|| interpolate(source, args))
        .unwrap_or_else(|| source.into())
}

// Single-pass substitution: inserted user values are never templates or markup.
fn interpolate(message: &str, args: &[(&str, &str)]) -> Option<String> {
    let mut result = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                result.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                result.push('}');
            }
            '{' => {
                let mut key = String::new();
                loop {
                    match chars.next()? {
                        '}' => break,
                        ch if ch.is_ascii_alphanumeric() || ch == '_' => key.push(ch),
                        _ => return None,
                    }
                }
                result.push_str(args.iter().find(|(name, _)| *name == key)?.1);
            }
            '}' => return None,
            ch => result.push(ch),
        }
    }
    Some(result)
}

/// Only metadata queries carry this catalog, not every clipboard refresh.
pub fn shell_catalog() -> &'static serde_json::Value {
    static CATALOG: OnceLock<serde_json::Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let ids: Vec<String> = serde_json::from_str(include_str!("../po/shell-messages.json"))
            .expect("validated Shell message catalog");
        let messages: serde_json::Map<String, serde_json::Value> = ids
            .into_iter()
            .map(|id| {
                let translated = tr(&id);
                (id, translated.into())
            })
            .collect();
        serde_json::json!({"language": active_language(), "messages": messages})
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_values_can_be_reordered_and_are_not_reprocessed() {
        assert_eq!(
            interpolate(
                "{second}: {first}",
                &[("first", "{second}"), ("second", "<b>Имя</b>")]
            ),
            Some("<b>Имя</b>: {second}".into())
        );
        assert_eq!(interpolate("{{name}}", &[]), Some("{name}".into()));
    }

    #[test]
    fn broken_translation_falls_back_to_complete_source() {
        assert_eq!(
            format_or_fallback(
                "Delete {unknown}",
                "Delete {count} items",
                &[("count", "2")]
            ),
            "Delete 2 items"
        );
        assert_eq!(interpolate("unclosed {name", &[("name", "X")]), None);
    }

    #[test]
    fn only_shipped_languages_can_be_selected() {
        assert_eq!(normalize_language("system"), "system");
        assert_eq!(normalize_language("en"), "en");
        assert_eq!(normalize_language("en_XA"), "en");
        assert_eq!(normalize_language("../../unknown"), "en");
    }

    #[test]
    fn plural_templates_supply_the_count() {
        let plural_format = ntrf;
        assert_eq!(
            plural_format("Delete {count} item", "Delete {count} items", 2, &[]),
            "Delete 2 items"
        );
    }
}
