use crate::{
    i18n::{tr, trf},
    model::Item,
    store::Store,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub type ExportResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportCollection {
    pub group_id: i64,
    pub group_name: String,
    pub group_revision: Vec<u8>,
    pub items: Vec<Item>,
    pub item_revisions: Vec<Vec<u8>>,
}
#[derive(Debug, Clone)]
pub struct ExportSnapshot {
    pub collections: Vec<ExportCollection>,
    // Freeze the system zone for both rendering and later file verification.
    pub time_zone: glib::TimeZone,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ExportOptions {
    pub include_metadata: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedFile {
    pub group_id: i64,
    pub path: PathBuf,
    pub byte_len: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFileError {
    pub group_id: i64,
    pub intended_name: String,
    pub partial_path: Option<PathBuf>,
    pub error: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportWriteReport {
    pub options: ExportOptions,
    pub files: Vec<ExportedFile>,
    pub errors: Vec<ExportFileError>,
    pub directory_sync_error: Option<String>,
}
impl ExportWriteReport {
    /// Cleanup must not be offered unless every selected collection is durable.
    pub fn complete(&self, snapshot: &ExportSnapshot) -> bool {
        self.errors.is_empty()
            && self.directory_sync_error.is_none()
            && !self.files.is_empty()
            && self.files.len()
                == snapshot
                    .collections
                    .iter()
                    .filter(|c| !c.items.is_empty())
                    .count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CleanupSkip {
    pub item_id: Option<i64>,
    pub group_id: i64,
    pub reason: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollectionCleanupCount {
    pub group_id: i64,
    pub exported: usize,
    pub eligible_items: usize,
    pub skipped_items: usize,
    pub folder_eligible: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanupPreview {
    pub collections: Vec<CollectionCleanupCount>,
    pub eligible_item_ids: Vec<i64>,
    pub eligible_folder_ids: Vec<i64>,
    pub skipped: Vec<CleanupSkip>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanupReport {
    pub deleted_item_ids: Vec<i64>,
    pub deleted_folder_ids: Vec<i64>,
    pub skipped: Vec<CleanupSkip>,
    pub errors: Vec<String>,
}

impl Store {
    /// Captures the complete selected collections in one SQLite read transaction.
    pub fn snapshot_markdown_export(&mut self, group_ids: &[i64]) -> ExportResult<ExportSnapshot> {
        if group_ids.is_empty() {
            return Err(tr("Select at least one collection").into());
        }
        let ids: BTreeSet<i64> = group_ids.iter().copied().collect();
        if ids.iter().any(|id| *id <= 0) {
            return Err(tr("History cannot be exported; collection IDs must be positive").into());
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let mut collections = Vec::with_capacity(ids.len());
        for id in ids {
            let group: Option<(String,Vec<u8>)> = tx.query_row("SELECT g.name,r.token FROM groups g JOIN group_export_revisions r ON r.group_id=g.id WHERE g.id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
            let (name, group_revision) = group.ok_or_else(|| {
                trf(
                    "Collection {id} no longer exists",
                    &[("id", &id.to_string())],
                )
            })?;
            let mut stmt = tx.prepare("SELECT i.id,i.title,i.content,i.kind,i.origin,i.source,i.source_id,i.created_at,i.updated_at,i.copied_at,i.group_id,r.token FROM items i JOIN item_export_revisions r ON r.item_id=i.id WHERE i.group_id=?1 ORDER BY i.created_at,i.id")?;
            let rows = stmt
                .query_map([id], |r| Ok((Store::row(r)?, r.get::<_, Vec<u8>>(11)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let (items, item_revisions) = rows.into_iter().unzip();
            collections.push(ExportCollection {
                group_id: id,
                group_name: name,
                group_revision,
                items,
                item_revisions,
            });
        }
        tx.commit()?;
        Ok(ExportSnapshot {
            collections,
            time_zone: glib::TimeZone::local(),
        })
    }

    pub fn preview_export_cleanup(
        &self,
        snapshot: &ExportSnapshot,
        written: &ExportWriteReport,
        protected_ids: &[i64],
        delete_empty_folders: bool,
    ) -> ExportResult<CleanupPreview> {
        let valid_files = verified_groups(snapshot, written);
        let protected: HashSet<i64> = protected_ids.iter().copied().collect();
        let mut out = CleanupPreview::default();
        for c in &snapshot.collections {
            if c.items.is_empty() {
                continue;
            }
            let mut count = CollectionCleanupCount {
                group_id: c.group_id,
                exported: c.items.len(),
                ..Default::default()
            };
            for (index, old) in c.items.iter().enumerate() {
                let reason = if !valid_files.contains(&c.group_id) {
                    Some(tr("export file is missing, changed, or was not written"))
                } else if protected.contains(&old.id) {
                    Some(tr("item is open in an editor"))
                } else {
                    changed_reason(&self.db, old, c.item_revisions.get(index))?
                };
                if let Some(reason) = reason {
                    count.skipped_items += 1;
                    out.skipped.push(CleanupSkip {
                        item_id: Some(old.id),
                        group_id: c.group_id,
                        reason,
                    });
                } else {
                    count.eligible_items += 1;
                    out.eligible_item_ids.push(old.id);
                }
            }
            if delete_empty_folders && c.group_id > 1 && valid_files.contains(&c.group_id) {
                let same_name: bool = self.db.query_row("SELECT EXISTS(SELECT 1 FROM groups g JOIN group_export_revisions r ON r.group_id=g.id WHERE g.id=?1 AND g.name=?2 AND r.token=?3)", params![c.group_id,c.group_name,c.group_revision], |r| r.get(0))?;
                let remaining: i64 = self.db.query_row("SELECT count(*) FROM items WHERE group_id=?1 AND id NOT IN (SELECT value FROM json_each(?2))", params![c.group_id, serde_json::to_string(&out.eligible_item_ids)?], |r| r.get(0))?;
                count.folder_eligible = same_name && remaining == 0;
                if count.folder_eligible {
                    out.eligible_folder_ids.push(c.group_id);
                } else {
                    out.skipped.push(CleanupSkip {
                        item_id: None,
                        group_id: c.group_id,
                        reason: if !same_name {
                            tr("folder was changed or deleted")
                        } else {
                            tr("folder contains items that will be kept")
                        },
                    });
                }
            } else if delete_empty_folders && c.group_id > 1 {
                out.skipped.push(CleanupSkip {
                    item_id: None,
                    group_id: c.group_id,
                    reason: tr("export file is missing, changed, or was not written"),
                });
            }
            out.collections.push(count);
        }
        Ok(out)
    }

    /// Deletes only IDs present in the confirmed preview, after an in-transaction recheck.
    pub fn cleanup_exported_snapshot(
        &mut self,
        snapshot: &ExportSnapshot,
        written: &ExportWriteReport,
        confirmed: &CleanupPreview,
        protected_ids: &[i64],
    ) -> ExportResult<CleanupReport> {
        let valid_files = verified_groups(snapshot, written);
        let allowed: HashSet<i64> = confirmed.eligible_item_ids.iter().copied().collect();
        let protected: HashSet<i64> = protected_ids.iter().copied().collect();
        let allowed_folders: HashSet<i64> = confirmed.eligible_folder_ids.iter().copied().collect();
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut out = CleanupReport {
            skipped: confirmed.skipped.clone(),
            ..Default::default()
        };
        for c in &snapshot.collections {
            if c.items.is_empty() {
                continue;
            }
            for (index, old) in c.items.iter().enumerate() {
                if !allowed.contains(&old.id) {
                    continue;
                }
                let reason = if !valid_files.contains(&c.group_id) {
                    Some(tr("export file is missing or changed"))
                } else if protected.contains(&old.id) {
                    Some(tr("item is open in an editor"))
                } else {
                    changed_reason(&tx, old, c.item_revisions.get(index))?
                };
                if let Some(reason) = reason {
                    out.skipped.push(CleanupSkip {
                        item_id: Some(old.id),
                        group_id: c.group_id,
                        reason,
                    });
                    continue;
                }
                if tx.execute("DELETE FROM items WHERE id=?1", [old.id])? == 1 {
                    out.deleted_item_ids.push(old.id);
                }
            }
            if allowed_folders.contains(&c.group_id) && c.group_id > 1 {
                if !valid_files.contains(&c.group_id) {
                    out.skipped.push(CleanupSkip {
                        item_id: None,
                        group_id: c.group_id,
                        reason: tr("export file is missing or changed"),
                    });
                    continue;
                }
                let deleted = tx.execute("DELETE FROM groups WHERE id=?1 AND name=?2 AND EXISTS(SELECT 1 FROM group_export_revisions WHERE group_id=?1 AND token=?3) AND NOT EXISTS(SELECT 1 FROM items WHERE group_id=?1)", params![c.group_id,c.group_name,c.group_revision])?;
                if deleted == 1 {
                    out.deleted_folder_ids.push(c.group_id);
                } else {
                    out.skipped.push(CleanupSkip {
                        item_id: None,
                        group_id: c.group_id,
                        reason: tr("folder changed or is not empty"),
                    });
                }
            }
        }
        tx.commit()?;
        Ok(out)
    }
}

fn changed_reason(
    db: &rusqlite::Connection,
    old: &Item,
    revision: Option<&Vec<u8>>,
) -> ExportResult<Option<String>> {
    let current: Option<(Item,Vec<u8>)> = db.query_row("SELECT i.id,i.title,i.content,i.kind,i.origin,i.source,i.source_id,i.created_at,i.updated_at,i.copied_at,i.group_id,r.token FROM items i JOIN item_export_revisions r ON r.item_id=i.id WHERE i.id=?1", [old.id], |r|Ok((Store::row(r)?,r.get(11)?))).optional()?;
    Ok(match current {
        None => Some(tr("item was deleted")),
        Some((now, token)) if revision != Some(&token) || !same_item(old, &now) => {
            Some(if now.group_id != old.group_id {
                tr("item was moved")
            } else {
                tr("item was modified")
            })
        }
        Some(_) => None,
    })
}
fn same_item(a: &Item, b: &Item) -> bool {
    a.id == b.id
        && a.title == b.title
        && a.content == b.content
        && a.kind == b.kind
        && a.origin == b.origin
        && a.source == b.source
        && a.source_id == b.source_id
        && a.created_at == b.created_at
        && a.updated_at == b.updated_at
        && a.copied_at == b.copied_at
        && a.group_id == b.group_id
}

pub fn write_markdown_export(
    snapshot: &ExportSnapshot,
    directory: &Path,
    options: ExportOptions,
) -> ExportWriteReport {
    let mut report = ExportWriteReport {
        options,
        ..Default::default()
    };
    for c in &snapshot.collections {
        if c.items.is_empty() {
            continue;
        }
        let base = if c.group_id == 1 {
            "Notes".into()
        } else {
            safe_name(&c.group_name)
        };
        let body = render_collection(c, options, &snapshot.time_zone);
        match create_unique(directory, &base) {
            Ok((mut file, path)) => match file
                .write_all(body.as_bytes())
                .and_then(|_| file.sync_all())
            {
                Ok(()) => report.files.push(ExportedFile {
                    group_id: c.group_id,
                    path,
                    byte_len: body.len() as u64,
                }),
                Err(e) => report.errors.push(ExportFileError {
                    group_id: c.group_id,
                    intended_name: format!("{base}.md"),
                    partial_path: Some(path),
                    error: e.to_string(),
                }),
            },
            Err(e) => report.errors.push(ExportFileError {
                group_id: c.group_id,
                intended_name: format!("{base}.md"),
                partial_path: None,
                error: e.to_string(),
            }),
        }
    }
    if !report.files.is_empty() {
        if let Err(e) = File::open(directory).and_then(|f| f.sync_all()) {
            report.directory_sync_error = Some(e.to_string());
        }
    }
    report
}

fn create_unique(dir: &Path, base: &str) -> std::io::Result<(File, PathBuf)> {
    for n in 1..=10_000 {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!(" ({n})")
        };
        let path = dir.join(format!(
            "{}{}.md",
            truncate_utf8(base, 220usize.saturating_sub(suffix.len())),
            suffix
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(f) => return Ok((f, path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "too many filename collisions",
    ))
}
fn safe_name(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '\"' | '<' | '>' | '|')
            {
                ' '
            } else {
                c
            }
        })
        .collect();
    let s = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches('.')
        .trim()
        .to_string();
    if s.is_empty() {
        "Collection".into()
    } else {
        truncate_utf8(&s, 220).to_string()
    }
}
fn truncate_utf8(s: &str, max: usize) -> &str {
    let mut n = max.min(s.len());
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    &s[..n]
}

pub fn render_collection(
    c: &ExportCollection,
    options: ExportOptions,
    time_zone: &glib::TimeZone,
) -> String {
    let timestamp = |seconds| timestamp(seconds, time_zone);
    let collection_heading = if c.group_id == 1 {
        tr("Notes")
    } else {
        heading(&c.group_name)
    };
    let mut out = format!("# {collection_heading}\n");
    for (i, item) in c.items.iter().enumerate() {
        if i > 0 {
            out.push_str("\n---\n");
        }
        let title = if item.title.trim().is_empty() {
            trf("Note {number}", &[("number", &(i + 1).to_string())])
        } else {
            heading(&item.title)
        };
        let created = timestamp(item.created_at);
        out.push_str(&format!(
            "\n## {title}\n\n{}",
            trf("Created: {date}", &[("date", &created)])
        ));
        if item.updated_at != item.created_at {
            let modified = timestamp(item.updated_at);
            out.push_str(&trf(" · Modified: {date}", &[("date", &modified)]));
        }
        out.push('\n');
        if options.include_metadata {
            let item_id = item.id.to_string();
            let collection_id = item.group_id.to_string();
            let kind = inline(&item.kind);
            let origin = inline(&item.origin);
            let source = inline(&item.source);
            let source_id = inline(&item.source_id);
            let copied = timestamp(item.copied_at);
            out.push_str(&trf(
                "\n- Item ID: {item_id}\n- Collection ID: {collection_id}\n- Type: {kind}\n- Origin: {origin}\n- Source: {source}\n- Source ID: {source_id}\n- Last copied: {date}\n",
                &[
                    ("item_id", &item_id),
                    ("collection_id", &collection_id),
                    ("kind", &kind),
                    ("origin", &origin),
                    ("source", &source),
                    ("source_id", &source_id),
                    ("date", &copied),
                ],
            ));
        }
        out.push('\n');
        out.push_str(&item.content);
        if !item.content.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}
fn inline(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '!' | '|' => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}
fn heading(value: &str) -> String {
    inline(value.trim())
}
fn timestamp(seconds: i64, time_zone: &glib::TimeZone) -> String {
    glib::DateTime::from_unix_utc(seconds)
        .and_then(|d| d.to_timezone(time_zone))
        .and_then(|d| d.format("%Y-%m-%d %H:%M:%S UTC%:z"))
        .map(|s| s.to_string())
        .unwrap_or_else(|_| format!("{seconds} (Unix time)"))
}
fn verified_groups(snapshot: &ExportSnapshot, written: &ExportWriteReport) -> HashSet<i64> {
    // A partial multi-collection export never authorizes deletion from any collection.
    if !written.complete(snapshot) {
        return HashSet::new();
    }
    written
        .files
        .iter()
        .filter_map(|file| {
            let c = snapshot
                .collections
                .iter()
                .find(|c| c.group_id == file.group_id)?;
            let expected = render_collection(c, written.options, &snapshot.time_zone);
            if file.byte_len != expected.len() as u64 {
                return None;
            }
            if file_matches(&file.path, expected.as_bytes()).unwrap_or(false) {
                Some(file.group_id)
            } else {
                None
            }
        })
        .collect()
}
fn file_matches(path: &Path, expected: &[u8]) -> std::io::Result<bool> {
    // Never follow a replaced export into a symlink, FIFO or device.
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() != expected.len() as u64 {
        return Ok(false);
    }
    let mut offset = 0;
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if expected.get(offset..offset + n) != Some(&buf[..n]) {
            return Ok(false);
        }
        offset += n;
    }
    Ok(offset == expected.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Settings;
    use rusqlite::Connection;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn store() -> Store {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON;
          CREATE TABLE groups(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE,position INTEGER NOT NULL);
          INSERT INTO groups VALUES(0,'History',0),(1,'Notes',1),(2,'Проекты / 2026',2);
          CREATE TABLE items(id INTEGER PRIMARY KEY,title TEXT NOT NULL DEFAULT '',content TEXT NOT NULL,kind TEXT NOT NULL,origin TEXT NOT NULL,source TEXT NOT NULL DEFAULT '',source_id TEXT NOT NULL DEFAULT '',created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,copied_at INTEGER NOT NULL,group_id INTEGER NOT NULL REFERENCES groups(id));
          CREATE TABLE item_export_revisions(item_id INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,token BLOB NOT NULL);
          CREATE TABLE group_export_revisions(group_id INTEGER PRIMARY KEY REFERENCES groups(id) ON DELETE CASCADE,token BLOB NOT NULL);
          INSERT INTO group_export_revisions SELECT id,randomblob(16) FROM groups;
          CREATE TRIGGER item_export_revision_insert AFTER INSERT ON items BEGIN INSERT INTO item_export_revisions VALUES(new.id,randomblob(16)); END;
          CREATE TRIGGER item_export_revision_update AFTER UPDATE ON items BEGIN UPDATE item_export_revisions SET token=randomblob(16) WHERE item_id=new.id; END;
          CREATE TRIGGER group_export_revision_insert AFTER INSERT ON groups BEGIN INSERT INTO group_export_revisions VALUES(new.id,randomblob(16)); END;
          CREATE TRIGGER group_export_revision_update AFTER UPDATE ON groups BEGIN UPDATE group_export_revisions SET token=randomblob(16) WHERE group_id=new.id; END;").unwrap();
        Store {
            db,
            settings: Settings::default(),
            config_path: PathBuf::new(),
        }
    }
    fn add(s: &Store, id: i64, group: i64, title: &str, content: &str, created: i64) {
        s.db.execute(
            "INSERT INTO items VALUES(?1,?2,?3,'text','manual','src','sid',?4,?4,?4,?5)",
            params![id, title, content, created, group],
        )
        .unwrap();
    }
    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "gcn-export-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        p
    }

    #[test]
    fn export_dates_use_the_selected_zone_and_date_specific_offsets() {
        let zone = glib::TimeZone::from_identifier(Some("Europe/Kyiv")).unwrap();
        let summer = glib::DateTime::from_iso8601("2026-09-14T13:33:56Z", None)
            .unwrap()
            .to_unix();
        let winter = glib::DateTime::from_iso8601("2026-01-14T13:33:56Z", None)
            .unwrap()
            .to_unix();
        assert_eq!(timestamp(summer, &zone), "2026-09-14 16:33:56 UTC+03:00");
        assert_eq!(timestamp(winter, &zone), "2026-01-14 15:33:56 UTC+02:00");
        assert_eq!(
            timestamp(0, &glib::TimeZone::from_offset(19800)),
            "1970-01-01 05:30:00 UTC+05:30"
        );
        assert_eq!(
            timestamp(0, &glib::TimeZone::from_offset(-12600)),
            "1969-12-31 20:30:00 UTC-03:30"
        );
        let mut s = store();
        let snapshot = s.snapshot_markdown_export(&[1]).unwrap();
        assert_eq!(
            snapshot.time_zone.identifier(),
            glib::TimeZone::local().identifier()
        );
    }

    #[test]
    fn empty_collections_create_no_files_and_cannot_be_cleaned_up() {
        let mut s = store();
        let snapshot = s.snapshot_markdown_export(&[1, 2]).unwrap();
        let dir = temp();
        let report = write_markdown_export(&snapshot, &dir, ExportOptions::default());
        assert!(report.files.is_empty());
        assert!(!report.complete(&snapshot));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let preview = s
            .preview_export_cleanup(&snapshot, &report, &[], true)
            .unwrap();
        assert!(preview.eligible_folder_ids.is_empty());
        let cleanup = s
            .cleanup_exported_snapshot(&snapshot, &report, &preview, &[])
            .unwrap();
        assert!(cleanup.deleted_folder_ids.is_empty());
        add(&s, 1, 1, "Note", "Content", 0);
        let snapshot = s.snapshot_markdown_export(&[1, 2]).unwrap();
        let report = write_markdown_export(&snapshot, &dir, ExportOptions::default());
        assert!(report.complete(&snapshot));
        assert_eq!(report.files.len(), 1);
        let preview = s
            .preview_export_cleanup(&snapshot, &report, &[], true)
            .unwrap();
        assert_eq!(preview.eligible_item_ids, vec![1]);
        assert!(preview.eligible_folder_ids.is_empty());
        s.cleanup_exported_snapshot(&snapshot, &report, &preview, &[])
            .unwrap();
        assert!(s.groups().unwrap().iter().any(|g| g.id == 2));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn metadata_is_optional_and_headings_are_literal_markdown() {
        let mut s = store();
        add(&s, 1, 1, "<title> **literal**", "Original\n\n`body`", 0);
        add(&s, 2, 1, "", "Second item", 60);
        s.db.execute("UPDATE items SET updated_at=30 WHERE id=1", [])
            .unwrap();
        let snapshot = s.snapshot_markdown_export(&[1]).unwrap();
        let collection = &snapshot.collections[0];
        let plain = render_collection(collection, ExportOptions::default(), &glib::TimeZone::utc());
        assert!(plain.contains("## &lt;title&gt; \\*\\*literal\\*\\*"));
        assert!(plain.contains(
            "Created: 1970-01-01 00:00:00 UTC+00:00 · Modified: 1970-01-01 00:00:30 UTC+00:00"
        ));
        assert!(plain.contains("Original\n\n`body`\n\n---\n\n## Note 2"));
        assert!(!plain.contains("Item ID:"));
        let detailed = render_collection(
            collection,
            ExportOptions {
                include_metadata: true,
            },
            &glib::TimeZone::utc(),
        );
        assert!(detailed.contains("- Item ID: 1\n"));
        assert!(detailed.contains("- Source ID: sid\n"));
    }

    #[test]
    fn replaced_files_block_confirmed_cleanup_including_folder_deletion() {
        let mut s = store();
        add(&s, 1, 2, "One", "Body", 0);
        let snapshot = s.snapshot_markdown_export(&[2]).unwrap();
        let dir = temp();
        let written = write_markdown_export(&snapshot, &dir, ExportOptions::default());
        let confirmed = s
            .preview_export_cleanup(&snapshot, &written, &[], true)
            .unwrap();
        assert_eq!(confirmed.eligible_folder_ids, vec![2]);
        let path = &written.files[0].path;
        let original = std::fs::read(path).unwrap();
        let mut changed = original.clone();
        changed[0] = b'!';
        std::fs::write(path, changed).unwrap();
        let result = s
            .cleanup_exported_snapshot(&snapshot, &written, &confirmed, &[])
            .unwrap();
        assert!(result.deleted_item_ids.is_empty());
        assert!(result.deleted_folder_ids.is_empty());
        assert_eq!(result.skipped.len(), 2);
        // Even an exact copy behind a replacement symlink is not trusted for cleanup.
        let copy = dir.join("copy.md");
        std::fs::write(&copy, &original).unwrap();
        std::fs::remove_file(path).unwrap();
        std::os::unix::fs::symlink(&copy, path).unwrap();
        assert!(!file_matches(path, &original).unwrap_or(false));
        let result = s
            .cleanup_exported_snapshot(&snapshot, &written, &confirmed, &[])
            .unwrap();
        assert!(result.deleted_item_ids.is_empty());
        assert!(s.get(1).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleanup_database_error_rolls_back_all_deletions() {
        let mut s = store();
        add(&s, 1, 1, "One", "one", 0);
        add(&s, 2, 1, "Two", "two", 1);
        let snapshot = s.snapshot_markdown_export(&[1]).unwrap();
        let dir = temp();
        let written = write_markdown_export(&snapshot, &dir, ExportOptions::default());
        let confirmed = s
            .preview_export_cleanup(&snapshot, &written, &[], false)
            .unwrap();
        s.db.execute_batch("CREATE TRIGGER fail_cleanup BEFORE DELETE ON items WHEN old.id=2 BEGIN SELECT RAISE(ABORT, 'simulated failure'); END;").unwrap();
        assert!(s
            .cleanup_exported_snapshot(&snapshot, &written, &confirmed, &[])
            .is_err());
        assert!(s.get(1).is_ok());
        assert!(s.get(2).is_ok());
        assert!(written.files[0].path.is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn complete_snapshot_is_unlimited_sorted_and_history_rejected() {
        let mut s = store();
        for id in (1..=225).rev() {
            add(&s, id, 1, "", &format!("body {id}"), id / 2);
        }
        assert!(s.snapshot_markdown_export(&[0]).is_err());
        let snap = s.snapshot_markdown_export(&[1]).unwrap();
        assert_eq!(snap.collections[0].items.len(), 225);
        assert!(snap.collections[0]
            .items
            .windows(2)
            .all(|w| (w[0].created_at, w[0].id) <= (w[1].created_at, w[1].id)));
    }
    #[test]
    fn unicode_names_collide_without_overwriting_and_markdown_is_preserved() {
        let mut s = store();
        add(&s, 1, 2, "", "**raw**\n\n- list", 0);
        let snap = s.snapshot_markdown_export(&[2]).unwrap();
        let dir = temp();
        std::fs::write(dir.join("Проекты 2026.md"), "mine").unwrap();
        let r = write_markdown_export(&snap, &dir, ExportOptions::default());
        assert!(r.complete(&snap));
        assert!(r.files[0].path.ends_with("Проекты 2026 (2).md"));
        assert_eq!(
            std::fs::read_to_string(dir.join("Проекты 2026.md")).unwrap(),
            "mine"
        );
        assert!(std::fs::read_to_string(&r.files[0].path)
            .unwrap()
            .contains("**raw**\n\n- list"));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn cleanup_rechecks_full_state_protection_tamper_and_new_items() {
        let mut s = store();
        add(&s, 1, 2, "A", "one", 10);
        add(&s, 2, 2, "B", "two", 10);
        let snap = s.snapshot_markdown_export(&[2]).unwrap();
        let dir = temp();
        let r = write_markdown_export(&snap, &dir, ExportOptions::default());
        let p = s.preview_export_cleanup(&snap, &r, &[2], true).unwrap();
        assert_eq!(p.eligible_item_ids, vec![1]);
        assert!(!p.collections[0].folder_eligible);
        // A same-second content edit is still detected; a new item keeps the folder.
        s.db.execute("UPDATE items SET content='changed' WHERE id=1", [])
            .unwrap();
        add(&s, 3, 2, "C", "new", 11);
        let done = s.cleanup_exported_snapshot(&snap, &r, &p, &[2]).unwrap();
        assert!(done.deleted_item_ids.is_empty());
        assert!(s.get(1).is_ok());
        assert!(s.get(2).is_ok());
        assert!(s.get(3).is_ok());
        std::fs::write(&r.files[0].path, "tampered").unwrap();
        let p2 = s.preview_export_cleanup(&snap, &r, &[], false).unwrap();
        assert!(p2.eligible_item_ids.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleanup_does_not_delete_a_new_item_reusing_a_snapshot_id() {
        let mut s = store();
        add(&s, 7, 2, "same title", "same content", 10);
        let snap = s.snapshot_markdown_export(&[2]).unwrap();
        let dir = temp();
        let written = write_markdown_export(&snap, &dir, ExportOptions::default());
        let confirmed = s
            .preview_export_cleanup(&snap, &written, &[], false)
            .unwrap();
        assert_eq!(confirmed.eligible_item_ids, vec![7]);

        s.db.execute("DELETE FROM items WHERE id=7", []).unwrap();
        add(&s, 7, 2, "same title", "same content", 10);
        let report = s
            .cleanup_exported_snapshot(&snap, &written, &confirmed, &[])
            .unwrap();
        assert!(report.deleted_item_ids.is_empty());
        assert_eq!(s.get(7).unwrap().content, "same content");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleanup_does_not_delete_an_item_that_was_edited_and_reverted() {
        let mut s = store();
        add(&s, 8, 2, "title", "original", 10);
        let snap = s.snapshot_markdown_export(&[2]).unwrap();
        let dir = temp();
        let written = write_markdown_export(&snap, &dir, ExportOptions::default());
        let confirmed = s
            .preview_export_cleanup(&snap, &written, &[], false)
            .unwrap();
        s.db.execute("UPDATE items SET content='changed' WHERE id=8", [])
            .unwrap();
        s.db.execute("UPDATE items SET content='original' WHERE id=8", [])
            .unwrap();
        let report = s
            .cleanup_exported_snapshot(&snap, &written, &confirmed, &[])
            .unwrap();
        assert!(report.deleted_item_ids.is_empty());
        assert_eq!(s.get(8).unwrap().content, "original");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn partial_write_never_allows_cleanup_and_notes_folder_survives() {
        let mut s = store();
        add(&s, 1, 1, "", "note", 1);
        add(&s, 2, 2, "", "custom", 2);
        let snap = s.snapshot_markdown_export(&[1, 2]).unwrap();
        let missing = std::env::temp_dir()
            .join(format!(
                "gcn-export-no-parent-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ))
            .join("child");
        let r = write_markdown_export(&snap, &missing, ExportOptions::default());
        assert!(!r.complete(&snap));
        let p = s.preview_export_cleanup(&snap, &r, &[], true).unwrap();
        assert!(p.eligible_item_ids.is_empty());
        let dir = temp();
        let r = write_markdown_export(&snap, &dir, ExportOptions::default());
        let p = s.preview_export_cleanup(&snap, &r, &[], true).unwrap();
        let done = s.cleanup_exported_snapshot(&snap, &r, &p, &[]).unwrap();
        assert_eq!(done.deleted_item_ids.len(), 2);
        assert!(s.groups().unwrap().iter().any(|g| g.id == 1));
        assert!(!s.groups().unwrap().iter().any(|g| g.id == 2));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleanup_preserves_new_items_new_folders_and_notes() {
        let mut s = store();
        add(&s, 1, 1, "", "old note", 1);
        add(&s, 2, 2, "", "old custom", 2);
        let snap = s.snapshot_markdown_export(&[1, 2]).unwrap();
        let dir = temp();
        let written = write_markdown_export(&snap, &dir, ExportOptions::default());
        let confirmed = s
            .preview_export_cleanup(&snap, &written, &[], true)
            .unwrap();
        add(&s, 3, 1, "", "new note", 3);
        add(&s, 4, 2, "", "new custom", 4);
        s.create_group("Added after snapshot").unwrap();
        let added_group = s.groups().unwrap().last().unwrap().id;
        let report = s
            .cleanup_exported_snapshot(&snap, &written, &confirmed, &[])
            .unwrap();
        assert!(report.deleted_item_ids.contains(&1));
        assert!(report.deleted_item_ids.contains(&2));
        assert!(s.get(3).is_ok());
        assert!(s.get(4).is_ok());
        assert!(s.groups().unwrap().iter().any(|g| g.id == 1));
        assert!(s.groups().unwrap().iter().any(|g| g.id == added_group));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
