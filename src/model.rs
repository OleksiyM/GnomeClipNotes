use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_CONTENT: usize = 1024 * 1024;
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub kind: String,
    pub origin: String,
    pub source: String,
    pub source_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub copied_at: i64,
    pub group_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub children: Vec<Subcategory>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subcategory {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemClassification {
    pub category_id: Option<i64>,
    pub subcategory_id: Option<i64>,
    pub comment: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Query {
    pub metadata_only: bool,
    pub search: String,
    /// -1 searches all collections; 0 History, 1 Notes, >=2 custom folders.
    pub group_id: i64,
    pub kind: String,
    pub source: String,
    pub since: i64,
    pub until: i64,
    /// None: all categories; Some(0): unclassified; positive: a category ID.
    pub category_id: Option<i64>,
    pub subcategory_id: Option<i64>,
    /// Literal case-insensitive search in the user's classification comment only.
    pub comment: String,
    pub limit: i64,
    pub offset: i64,
}

impl Query {
    /// Paging and metadata transport do not change the set being selected.
    pub fn same_filter(&self, other: &Self) -> bool {
        self.search == other.search
            && self.group_id == other.group_id
            && self.kind == other.kind
            && self.source == other.source
            && self.since == other.since
            && self.until == other.until
            && self.category_id == other.category_id
            && self.subcategory_id == other.subcategory_id
            && self.comment == other.comment
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub paste_mode: String,
    pub ignore_sensitive: bool,
    pub ignored_apps: Vec<String>,
    pub paused_until: i64,
    pub activate_shortcut: String,
    pub note_shortcut: String,
    pub library_shortcut: String,
    pub quick_paste: bool,
    pub retention_days: i64,
    pub open_at_login: bool,
    pub theme: String,
    pub language: String,
    pub preview_mode: String,
    pub super_v_conflict_dismissed: bool,
    pub last_export_folder: Option<String>,
    pub classification_enabled: bool,
    pub comments_enabled: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            paste_mode: "active".into(),
            ignore_sensitive: true,
            ignored_apps: vec![
                "keepass".into(),
                "bitwarden".into(),
                "1password".into(),
                "seahorse".into(),
                "protonpass".into(),
                "org.gnome.World.Secrets".into(),
            ],
            paused_until: 0,
            activate_shortcut: "<Super>v".into(),
            note_shortcut: "<Super><Shift>n".into(),
            library_shortcut: "<Super>b".into(),
            quick_paste: true,
            retention_days: 30,
            open_at_login: true,
            theme: "system".into(),
            language: "system".into(),
            preview_mode: "webkit".into(),
            super_v_conflict_dismissed: false,
            last_export_folder: None,
            classification_enabled: false,
            comments_enabled: false,
        }
    }
}

pub fn kind(content: &str) -> &'static str {
    let text = content.trim();
    if !text.chars().any(char::is_whitespace)
        && (text.starts_with("https://") || text.starts_with("http://"))
        && text
            .split_once("://")
            .is_some_and(|(_, host)| !host.is_empty())
    {
        "link"
    } else {
        "text"
    }
}

pub fn ignored(settings: &Settings, source: &str, source_id: &str, sensitive: bool) -> bool {
    settings.paused_until > now()
        || (settings.ignore_sensitive && sensitive)
        || settings
            .ignored_apps
            .iter()
            .filter(|s| !s.trim().is_empty())
            .any(|app| {
                let app = app.to_lowercase();
                source.to_lowercase().contains(&app) || source_id.to_lowercase().contains(&app)
            })
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn settings_without_preview_mode_use_webkit() {
        let settings: Settings = serde_json::from_str(
            r#"{"paste_mode":"active","ignore_sensitive":true,"ignored_apps":[],"paused_until":0,"activate_shortcut":"","note_shortcut":"","quick_paste":false,"retention_days":30,"open_at_login":true,"theme":"system","super_v_conflict_dismissed":false}"#,
        )
        .unwrap();
        assert_eq!(settings.preview_mode, "webkit");
        assert_eq!(settings.library_shortcut, "<Super>b");
        assert!(!settings.classification_enabled);
        assert!(!settings.comments_enabled);
    }
}
