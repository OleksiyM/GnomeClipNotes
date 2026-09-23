use crate::{i18n::tr, model::*};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct Store {
    pub db: Connection,
    pub settings: Settings,
    pub config_path: PathBuf,
}

pub fn xdg_path(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME must be set")).join(fallback)
        })
}

fn private_dir(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

impl Store {
    pub fn open_for_backup() -> Result<Self> {
        let path = xdg_path("XDG_DATA_HOME", ".local/share").join("gnome-clip-notes/data.db");
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(Self {
            db,
            settings: Settings::default(),
            config_path: PathBuf::new(),
        })
    }
    pub fn open() -> Result<Self> {
        let data = xdg_path("XDG_DATA_HOME", ".local/share").join("gnome-clip-notes");
        let config = xdg_path("XDG_CONFIG_HOME", ".config").join("gnome-clip-notes");
        private_dir(&data)?;
        private_dir(&config)?;
        let config_path = config.join("settings.json");
        let settings = if config_path.exists() {
            serde_json::from_slice(&fs::read(&config_path)?)?
        } else {
            Settings::default()
        };
        let db_path = data.join("data.db");
        let db = Connection::open(&db_path)?;
        fs::set_permissions(&db_path, fs::Permissions::from_mode(0o600))?;
        let mut store = Self {
            db,
            settings,
            config_path,
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        self.db.create_scalar_function(
            "unicode_lower",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8
                | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |context| Ok(context.get::<String>(0)?.to_lowercase()),
        )?;
        self.db.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = self.db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 2 {
            return Err(
                tr("Database was created by a newer application; refusing to open it").into(),
            );
        }
        self.db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA secure_delete=ON;",
        )?;
        if version == 0 {
            let tx = self.db.transaction()?;
            tx.execute_batch("CREATE TABLE groups(id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, position INTEGER NOT NULL);
                INSERT INTO groups VALUES(0,'History',0),(1,'Notes',1);
                CREATE TABLE items(id INTEGER PRIMARY KEY, title TEXT NOT NULL DEFAULT '', content TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('text','link')), origin TEXT NOT NULL CHECK(origin IN ('clipboard','manual')),
                source TEXT NOT NULL DEFAULT '', source_id TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL, copied_at INTEGER NOT NULL, group_id INTEGER NOT NULL DEFAULT 0 REFERENCES groups(id));
                CREATE INDEX items_group_copied ON items(group_id,copied_at DESC);
                CREATE INDEX items_created ON items(created_at);
                PRAGMA user_version=1;")?;
            tx.commit()?;
        }
        if version < 2 {
            let tx = self.db.transaction()?;
            tx.execute_batch("CREATE TABLE item_export_revisions(item_id INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE, token BLOB NOT NULL);
                CREATE TABLE group_export_revisions(group_id INTEGER PRIMARY KEY REFERENCES groups(id) ON DELETE CASCADE, token BLOB NOT NULL);
                INSERT INTO item_export_revisions SELECT id,randomblob(16) FROM items;
                INSERT INTO group_export_revisions SELECT id,randomblob(16) FROM groups;
                CREATE TRIGGER item_export_revision_insert AFTER INSERT ON items BEGIN INSERT INTO item_export_revisions VALUES(new.id,randomblob(16)); END;
                CREATE TRIGGER item_export_revision_update AFTER UPDATE ON items BEGIN UPDATE item_export_revisions SET token=randomblob(16) WHERE item_id=new.id; END;
                CREATE TRIGGER group_export_revision_insert AFTER INSERT ON groups BEGIN INSERT INTO group_export_revisions VALUES(new.id,randomblob(16)); END;
                CREATE TRIGGER group_export_revision_update AFTER UPDATE ON groups BEGIN UPDATE group_export_revisions SET token=randomblob(16) WHERE group_id=new.id; END;
                PRAGMA user_version=2;")?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn save_settings(&self) -> Result<()> {
        let tmp = self.config_path.with_extension("json.tmp");
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(&serde_json::to_vec_pretty(&self.settings)?)?;
        file.sync_all()?;
        fs::rename(tmp, &self.config_path)?;
        Ok(())
    }

    pub fn capture(
        &mut self,
        content: &str,
        source: &str,
        source_id: &str,
        sensitive: bool,
    ) -> Result<bool> {
        if content.trim().is_empty()
            || content.len() > MAX_CONTENT
            || content.contains('\0')
            || ignored(&self.settings, source, source_id, sensitive)
        {
            return Ok(false);
        }
        let existing: Option<i64> = self
            .db
            .query_row(
                "SELECT id FROM items WHERE group_id=0 AND content=?1 LIMIT 1",
                [content],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            self.db.execute(
                "UPDATE items SET copied_at=?1,source=?2,source_id=?3 WHERE id=?4",
                params![now(), source, source_id, id],
            )?;
        } else {
            self.db.execute("INSERT INTO items(content,kind,origin,source,source_id,created_at,updated_at,copied_at) VALUES(?1,?2,'clipboard',?3,?4,?5,?5,?5)", params![content,kind(content),source,source_id,now()])?;
        }
        Ok(true)
    }

    pub(crate) fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
        Ok(Item {
            id: r.get(0)?,
            title: r.get(1)?,
            content: r.get(2)?,
            kind: r.get(3)?,
            origin: r.get(4)?,
            source: r.get(5)?,
            source_id: r.get(6)?,
            created_at: r.get(7)?,
            updated_at: r.get(8)?,
            copied_at: r.get(9)?,
            group_id: r.get(10)?,
        })
    }
    pub fn get(&self, id: i64) -> Result<Item> {
        Ok(self
            .db
            .query_row("SELECT * FROM items WHERE id=?1", [id], Self::row)?)
    }
    pub fn groups(&self) -> Result<Vec<Group>> {
        Ok(self
            .db
            .prepare("SELECT id,name FROM groups ORDER BY position,id")?
            .query_map([], |r| {
                Ok(Group {
                    id: r.get(0)?,
                    name: r.get(1)?,
                })
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn query(&self, query: &Query) -> Result<Vec<Item>> {
        let limit = if query.limit <= 0 {
            60
        } else {
            query.limit.min(200)
        };
        let pattern = query.search.to_lowercase();
        let mut stmt = self.db.prepare("SELECT * FROM items WHERE group_id=?1 AND (instr(unicode_lower(content),?2)>0 OR instr(unicode_lower(title),?2)>0)
            AND (?3='' OR kind=?3) AND (?4='' OR source=?4) AND (?5=0 OR created_at>=?5) AND (?6=0 OR created_at<=?6)
            ORDER BY copied_at DESC,id DESC LIMIT ?7 OFFSET ?8")?;
        let items = stmt
            .query_map(
                params![
                    query.group_id,
                    pattern,
                    query.kind,
                    query.source,
                    query.since,
                    query.until,
                    limit,
                    query.offset.max(0)
                ],
                Self::row,
            )?
            .collect::<rusqlite::Result<_>>()?;
        Ok(items)
    }
    pub fn query_json(&self, query: &Query) -> Result<String> {
        let sources = self.sources()?;
        let items = if query.metadata_only {
            Vec::new()
        } else {
            self.query(query)?
        };
        let mut result = json!({"items":items,"groups":self.groups()?,"settings":self.settings,"sources":sources});
        if query.metadata_only {
            result["localization"] = crate::i18n::shell_catalog().clone();
        }
        Ok(result.to_string())
    }
    pub fn sources(&self) -> Result<Vec<String>> {
        let sources: Vec<String> = self
            .db
            .prepare("SELECT DISTINCT source FROM items WHERE source<>'' ORDER BY source")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(sources)
    }
    pub fn save_item(&mut self, id: Option<i64>, title: &str, content: &str) -> Result<i64> {
        if content.len() > MAX_CONTENT || content.contains('\0') {
            return Err(tr("Text must be at most 1 MiB and contain no NUL characters").into());
        }
        if content.trim().is_empty() {
            return Err(tr("A note cannot be empty").into());
        }
        if let Some(id) = id {
            let changed = self.db.execute(
                "UPDATE items SET title=?1,content=?2,kind=?3,updated_at=?4 WHERE id=?5",
                params![title, content, kind(content), now(), id],
            )?;
            if changed == 0 {
                return Err(tr("This item was removed. Copy your text into a new note before closing the editor.").into());
            }
            Ok(id)
        } else {
            self.db.execute("INSERT INTO items(title,content,kind,origin,created_at,updated_at,copied_at,group_id) VALUES(?1,?2,?3,'manual',?4,?4,?4,1)", params![title,content,kind(content),now()])?;
            Ok(self.db.last_insert_rowid())
        }
    }
    pub fn rename(&self, id: i64, title: &str) -> Result<()> {
        self.db.execute(
            "UPDATE items SET title=?1,updated_at=?2 WHERE id=?3",
            params![title, now(), id],
        )?;
        Ok(())
    }
    pub fn delete(&self, id: i64) -> Result<()> {
        self.db.execute("DELETE FROM items WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn move_item(&self, id: i64, group: i64) -> Result<()> {
        self.db.execute(
            "UPDATE items SET group_id=?1,updated_at=?2 WHERE id=?3",
            params![group, now(), id],
        )?;
        Ok(())
    }
    pub fn create_group(&self, name: &str) -> Result<()> {
        if name.trim().is_empty() || name.len() > 100 {
            return Err(tr("Choose a folder name of 1–100 bytes").into());
        }
        self.db.execute("INSERT INTO groups(name,position) VALUES(?1,(SELECT COALESCE(MAX(position),1)+1 FROM groups))",[name.trim()])?;
        Ok(())
    }
    pub fn create_group_for_item(&mut self, id: i64, name: &str) -> Result<()> {
        if name.trim().is_empty() || name.len() > 100 {
            return Err(tr("Choose a folder name of 1–100 bytes").into());
        }
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO groups(name,position) VALUES(?1,(SELECT COALESCE(MAX(position),1)+1 FROM groups))", [name.trim()])?;
        let group = tx.last_insert_rowid();
        let changed = tx.execute(
            "UPDATE items SET group_id=?1,updated_at=?2 WHERE id=?3",
            params![group, now(), id],
        )?;
        if changed == 0 {
            return Err(tr("This item no longer exists. No folder was created.").into());
        }
        tx.commit()?;
        Ok(())
    }
    pub fn rename_group(&self, id: i64, name: &str) -> Result<()> {
        if id < 2 || name.trim().is_empty() || name.len() > 100 {
            return Err(tr("This folder cannot be renamed").into());
        }
        self.db.execute(
            "UPDATE groups SET name=?1 WHERE id=?2",
            params![name.trim(), id],
        )?;
        Ok(())
    }
    pub fn delete_group(&mut self, id: i64) -> Result<()> {
        if id < 2 {
            return Err(tr("Built-in folders cannot be deleted").into());
        }
        let tx = self.db.transaction()?;
        tx.execute("UPDATE items SET group_id=1 WHERE group_id=?1", [id])?;
        tx.execute("DELETE FROM groups WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn reorder_group(&mut self, id: i64, direction: i64) -> Result<()> {
        if id < 2 {
            return Ok(());
        }
        let groups = self.groups()?;
        if let Some(index) = groups.iter().position(|g| g.id == id) {
            let other = index as i64 + direction;
            if other < 2 || other >= groups.len() as i64 {
                return Ok(());
            }
            let tx = self.db.transaction()?;
            for (position, g) in groups.iter().enumerate() {
                let position = if position == index {
                    other
                } else if position == other as usize {
                    index as i64
                } else {
                    position as i64
                };
                tx.execute(
                    "UPDATE groups SET position=?1 WHERE id=?2",
                    params![position, g.id],
                )?;
            }
            tx.commit()?;
        }
        Ok(())
    }
    pub fn expiry_count(&self, days: i64) -> Result<i64> {
        if days <= 0 {
            return Ok(0);
        }
        Ok(self.db.query_row(
            "SELECT count(*) FROM items WHERE group_id=0 AND copied_at<?1",
            [now() - days * 86400],
            |r| r.get(0),
        )?)
    }
    pub fn prune(&self) -> Result<usize> {
        if self.settings.retention_days <= 0 {
            return Ok(0);
        }
        Ok(self.db.execute(
            "DELETE FROM items WHERE group_id=0 AND copied_at<?1",
            [now() - self.settings.retention_days * 86400],
        )?)
    }
    pub fn backup(&self, path: &Path) -> Result<()> {
        if path.exists() {
            return Err(tr("Backup destination already exists; choose a new file").into());
        }
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)?;
        self.db.backup(rusqlite::DatabaseName::Main, path, None)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> Store {
        let mut s = Store {
            db: Connection::open_in_memory().unwrap(),
            settings: Settings::default(),
            config_path: PathBuf::new(),
        };
        s.migrate().unwrap();
        s
    }
    #[test]
    fn capture_privacy_dedup_and_literal_search() {
        let mut s = store();
        assert!(!s.capture("secret", "KeePassXC", "", false).unwrap());
        assert!(!s.capture("secret", "Browser", "", true).unwrap());
        assert!(!s
            .capture(&"x".repeat(MAX_CONTENT + 1), "", "", false)
            .unwrap());
        s.capture("100%_hello", "Editor", "editor", false).unwrap();
        s.capture("100%_hello", "Editor", "editor", false).unwrap();
        s.capture("100xxhello", "Editor", "editor", false).unwrap();
        let q = Query {
            search: "%_".into(),
            ..Default::default()
        };
        assert_eq!(s.query(&q).unwrap().len(), 1);
        assert_eq!(s.query(&Query::default()).unwrap().len(), 2);
        s.capture("Привет, мир", "Editor", "", false).unwrap();
        assert_eq!(
            s.query(&Query {
                search: "ПРИВЕТ".into(),
                ..Default::default()
            })
            .unwrap()
            .len(),
            1
        );
    }

    #[test]
    fn sources_disappear_only_after_last_item_and_prune() {
        let mut s = store();
        s.capture("first", "Shared app", "first", false).unwrap();
        s.capture("second", "Shared app", "second", false).unwrap();
        assert_eq!(s.sources().unwrap(), vec!["Shared app"]);

        let first = s
            .query(&Query {
                search: "first".into(),
                ..Default::default()
            })
            .unwrap()[0]
            .id;
        let second = s
            .query(&Query {
                search: "second".into(),
                ..Default::default()
            })
            .unwrap()[0]
            .id;
        s.delete(first).unwrap();
        assert_eq!(s.sources().unwrap(), vec!["Shared app"]);
        s.delete(second).unwrap();
        assert!(s.sources().unwrap().is_empty());

        s.capture("expires", "History app", "history", false)
            .unwrap();
        s.capture("notes", "Notes app", "notes", false).unwrap();
        s.capture("custom", "Custom app", "custom", false).unwrap();
        let notes = s
            .query(&Query {
                search: "notes".into(),
                ..Default::default()
            })
            .unwrap()[0]
            .id;
        let custom = s
            .query(&Query {
                search: "custom".into(),
                ..Default::default()
            })
            .unwrap()[0]
            .id;
        s.move_item(notes, 1).unwrap();
        s.create_group("Custom folder").unwrap();
        let custom_group = s.groups().unwrap().last().unwrap().id;
        s.move_item(custom, custom_group).unwrap();

        s.settings.retention_days = 1;
        s.db.execute("UPDATE items SET copied_at=0", []).unwrap();
        assert_eq!(s.prune().unwrap(), 1);
        assert_eq!(s.sources().unwrap(), vec!["Custom app", "Notes app"]);
    }

    #[test]
    fn notes_survive_retention_and_folder_deletion() {
        let mut s = store();
        let note = s.save_item(None, "Note", "Keep me").unwrap();
        s.capture("Expire me", "", "", false).unwrap();
        s.create_group("Pinned").unwrap();
        let pinned_group = s.groups().unwrap().last().unwrap().id;
        s.capture("Keep pinned capture", "", "", false).unwrap();
        let pinned = s
            .query(&Query {
                search: "Keep pinned capture".into(),
                ..Default::default()
            })
            .unwrap()[0]
            .id;
        s.move_item(pinned, pinned_group).unwrap();
        let custom_note = s
            .save_item(None, "Custom note", "Keep in my folder")
            .unwrap();
        s.move_item(custom_note, pinned_group).unwrap();
        s.db.execute("UPDATE items SET copied_at=0", []).unwrap();
        assert_eq!(s.expiry_count(1).unwrap(), 1);
        assert_eq!(s.prune().unwrap(), 1);
        assert!(s.get(pinned).is_ok());
        assert!(s.get(custom_note).is_ok());
        s.capture("Keep forever", "", "", false).unwrap();
        s.db.execute("UPDATE items SET copied_at=0", []).unwrap();
        s.settings.retention_days = 0;
        assert_eq!(s.expiry_count(0).unwrap(), 0);
        assert_eq!(s.prune().unwrap(), 0);
        assert_eq!(s.get(note).unwrap().content, "Keep me");
        s.create_group("Work").unwrap();
        let group = s.groups().unwrap().last().unwrap().id;
        s.move_item(note, group).unwrap();
        s.delete_group(group).unwrap();
        assert_eq!(s.get(note).unwrap().group_id, 1);
        assert!(s.delete_group(1).is_err());
    }
    #[test]
    fn create_group_for_item_is_atomic() {
        let mut s = store();
        let id = s.save_item(None, "Note", "Keep me").unwrap();
        s.create_group_for_item(id, "  New folder  ").unwrap();
        let group = s.groups().unwrap().last().unwrap().clone();
        assert_eq!(group.name, "New folder");
        let position: i64 =
            s.db.query_row("SELECT position FROM groups WHERE id=?1", [group.id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(position, 2);
        assert_eq!(s.get(id).unwrap().group_id, group.id);

        for (missing_id, name) in [(id + 100, "Missing"), (id, " "), (id, "New folder")] {
            assert!(s.create_group_for_item(missing_id, name).is_err());
        }
        s.db.execute_batch("CREATE TRIGGER reject_move BEFORE UPDATE OF group_id ON items BEGIN SELECT RAISE(ABORT, 'move failed'); END;").unwrap();
        assert!(s.create_group_for_item(id, "Move fails").is_err());
        assert_eq!(s.groups().unwrap().len(), 3);
        assert_eq!(s.get(id).unwrap().group_id, group.id);
    }
    #[test]
    fn migration_and_edit_metadata() {
        let mut s = store();
        s.migrate().unwrap();
        let id = s.save_item(None, "", "https://example.org").unwrap();
        assert_eq!(s.get(id).unwrap().kind, "link");
        s.save_item(Some(id), "Example", "plain text").unwrap();
        let item = s.get(id).unwrap();
        assert_eq!(item.origin, "manual");
        assert_eq!(item.kind, "text");
        s.delete(id).unwrap();
        assert!(s.save_item(Some(id), "Lost item", "Unsaved text").is_err());
        s.db.execute_batch("PRAGMA user_version=999").unwrap();
        assert!(s.migrate().is_err());
    }

    #[test]
    fn migrate_v1_preserves_data_creates_revisions_and_is_idempotent() {
        let mut s = Store {
            db: Connection::open_in_memory().unwrap(),
            settings: Settings::default(),
            config_path: PathBuf::new(),
        };
        s.db.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE groups(id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, position INTEGER NOT NULL);
             INSERT INTO groups VALUES(0,'History',0),(1,'Notes',1),(2,'Archive',2);
             CREATE TABLE items(id INTEGER PRIMARY KEY, title TEXT NOT NULL DEFAULT '', content TEXT NOT NULL,
             kind TEXT NOT NULL CHECK(kind IN ('text','link')), origin TEXT NOT NULL CHECK(origin IN ('clipboard','manual')),
             source TEXT NOT NULL DEFAULT '', source_id TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL, copied_at INTEGER NOT NULL, group_id INTEGER NOT NULL DEFAULT 0 REFERENCES groups(id));
             INSERT INTO items VALUES(42,'Old title','Old content','text','manual','test','v1',10,11,12,2);
             PRAGMA user_version=1;",
        )
        .unwrap();

        s.migrate().unwrap();
        assert_eq!(s.get(42).unwrap().content, "Old content");
        let first_revision: Vec<u8> =
            s.db.query_row(
                "SELECT token FROM item_export_revisions WHERE item_id=42",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(first_revision.len(), 16);
        assert_eq!(
            s.db.query_row("SELECT count(*) FROM group_export_revisions", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );

        s.migrate().unwrap();
        let second_revision: Vec<u8> =
            s.db.query_row(
                "SELECT token FROM item_export_revisions WHERE item_id=42",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(first_revision, second_revision);
        assert_eq!(s.get(42).unwrap().content, "Old content");
        assert_eq!(
            s.db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn consistent_backup_preserves_notes_and_refuses_overwrite() {
        let mut s = store();
        let id = s
            .save_item(None, "Backup fixture", "Keep this note")
            .unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("gcn-backup-test-{}-{unique}", std::process::id()));
        private_dir(&dir).unwrap();
        let path = dir.join("snapshot.db");
        s.backup(&path).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let snapshot = Connection::open(&path).unwrap();
        assert_eq!(
            snapshot
                .query_row("SELECT content FROM items WHERE id=?1", [id], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "Keep this note"
        );
        assert_eq!(
            snapshot
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert!(s.backup(&path).is_err());
        drop(snapshot);
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
