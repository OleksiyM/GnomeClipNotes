//! Optional, folder-independent Library metadata. No UI or inferred taxonomy.
use crate::{
    i18n::tr,
    model::{Category, ItemClassification, Subcategory, MAX_CONTENT},
    store::{Result, Store},
};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn migrate(db: &mut Connection) -> Result<()> {
    let tx = db.transaction()?;
    tx.execute_batch(
        "CREATE TABLE categories(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE CHECK(length(trim(name))>0),
            position INTEGER NOT NULL);
         CREATE TABLE subcategories(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
            name TEXT NOT NULL CHECK(length(trim(name))>0),
            position INTEGER NOT NULL,
            UNIQUE(category_id,name), UNIQUE(id,category_id));
         CREATE TABLE item_classifications(
            item_id INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
            category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
            subcategory_id INTEGER REFERENCES subcategories(id) ON DELETE SET NULL,
            comment TEXT NOT NULL DEFAULT '',
            CHECK(subcategory_id IS NULL OR category_id IS NOT NULL),
            FOREIGN KEY(subcategory_id,category_id) REFERENCES subcategories(id,category_id));
         CREATE INDEX classifications_category ON item_classifications(category_id);
         CREATE INDEX classifications_subcategory ON item_classifications(subcategory_id);
         CREATE TRIGGER classification_revision_insert AFTER INSERT ON item_classifications
            BEGIN UPDATE item_export_revisions SET token=randomblob(16) WHERE item_id=new.item_id; END;
         CREATE TRIGGER classification_revision_update AFTER UPDATE ON item_classifications
            BEGIN UPDATE item_export_revisions SET token=randomblob(16) WHERE item_id=new.item_id; END;
         CREATE TRIGGER classification_revision_delete AFTER DELETE ON item_classifications
            BEGIN UPDATE item_export_revisions SET token=randomblob(16) WHERE item_id=old.item_id; END;
         CREATE TRIGGER category_clear_assignments BEFORE DELETE ON categories
            BEGIN UPDATE item_classifications SET category_id=NULL,subcategory_id=NULL WHERE category_id=old.id; END;
         CREATE TRIGGER category_name_revision AFTER UPDATE OF name ON categories
            WHEN old.name<>new.name
            BEGIN UPDATE item_export_revisions SET token=randomblob(16)
                WHERE item_id IN (SELECT item_id FROM item_classifications WHERE category_id=new.id); END;
         CREATE TRIGGER subcategory_name_revision AFTER UPDATE OF name ON subcategories
            WHEN old.name<>new.name
            BEGIN UPDATE item_export_revisions SET token=randomblob(16)
                WHERE item_id IN (SELECT item_id FROM item_classifications WHERE subcategory_id=new.id); END;
         PRAGMA user_version=3;",
    )?;
    tx.commit()?;
    Ok(())
}

fn name(value: &str) -> Result<&str> {
    let value = value.trim();
    if value.is_empty() || value.contains('\0') || value.chars().count() > 512 {
        return Err(tr("Use a name of 1–512 characters without NUL characters.").into());
    }
    Ok(value)
}

fn require_row(changed: usize) -> Result<()> {
    if changed == 0 {
        return Err(tr("This item or category no longer exists.").into());
    }
    Ok(())
}

fn write_assignment(db: &Connection, id: i64, value: &ItemClassification) -> Result<()> {
    if value.comment.len() > MAX_CONTENT || value.comment.contains('\0') {
        return Err(tr("Text must be at most 1 MiB and contain no NUL characters").into());
    }
    // Do not touch body/capture/retention. Foreign keys enforce child ownership.
    db.execute(
        "INSERT INTO item_classifications(item_id,category_id,subcategory_id,comment) VALUES(?1,?2,?3,?4)
         ON CONFLICT(item_id) DO UPDATE SET category_id=excluded.category_id,
            subcategory_id=excluded.subcategory_id,comment=excluded.comment
         WHERE category_id IS NOT excluded.category_id OR subcategory_id IS NOT excluded.subcategory_id OR comment<>excluded.comment",
        params![id, value.category_id, value.subcategory_id, value.comment],
    )?;
    Ok(())
}

impl Store {
    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut categories = self
            .db
            .prepare("SELECT id,name FROM categories ORDER BY position,id")?
            .query_map([], |r| {
                Ok(Category {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    children: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut children = self.db.prepare(
            "SELECT id,name FROM subcategories WHERE category_id=?1 ORDER BY position,id",
        )?;
        for category in &mut categories {
            category.children = children
                .query_map([category.id], |r| {
                    Ok(Subcategory {
                        id: r.get(0)?,
                        name: r.get(1)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
        }
        Ok(categories)
    }

    pub fn create_category(&self, value: &str) -> Result<i64> {
        self.db.execute(
            "INSERT INTO categories(name,position) VALUES(?1,(SELECT COALESCE(MAX(position),0)+1 FROM categories))",
            [name(value)?],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn create_subcategory(&self, category: i64, value: &str) -> Result<i64> {
        self.db.execute(
            "INSERT INTO subcategories(category_id,name,position) VALUES(?1,?2,(SELECT COALESCE(MAX(position),0)+1 FROM subcategories WHERE category_id=?1))",
            params![category, name(value)?],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn rename_category(&self, id: i64, value: &str) -> Result<()> {
        require_row(self.db.execute(
            "UPDATE categories SET name=?1 WHERE id=?2",
            params![name(value)?, id],
        )?)
    }

    pub fn rename_subcategory(&self, id: i64, value: &str) -> Result<()> {
        require_row(self.db.execute(
            "UPDATE subcategories SET name=?1 WHERE id=?2",
            params![name(value)?, id],
        )?)
    }

    pub fn category_item_count(&self, id: i64) -> Result<usize> {
        Ok(self.db.query_row(
            "SELECT count(*) FROM item_classifications WHERE category_id=?1",
            [id],
            |r| r.get(0),
        )?)
    }

    pub fn subcategory_item_count(&self, id: i64) -> Result<usize> {
        Ok(self.db.query_row(
            "SELECT count(*) FROM item_classifications WHERE subcategory_id=?1",
            [id],
            |r| r.get(0),
        )?)
    }

    /// SQLite clears assignments atomically; item bodies and comments survive.
    pub fn delete_category(&self, id: i64) -> Result<()> {
        require_row(
            self.db
                .execute("DELETE FROM categories WHERE id=?1", [id])?,
        )
    }

    pub fn delete_subcategory(&self, id: i64) -> Result<()> {
        require_row(
            self.db
                .execute("DELETE FROM subcategories WHERE id=?1", [id])?,
        )
    }

    /// Full ordered ID list from Settings. Reject a stale/incomplete dictionary.
    pub fn reorder_categories(&mut self, ids: &[i64]) -> Result<()> {
        reorder(&mut self.db, None, ids)
    }

    pub fn reorder_subcategories(&mut self, category: i64, ids: &[i64]) -> Result<()> {
        reorder(&mut self.db, Some(category), ids)
    }

    pub fn item_classification(&self, id: i64) -> Result<ItemClassification> {
        Ok(self.db.query_row(
            "SELECT a.category_id,a.subcategory_id,COALESCE(a.comment,'') FROM items i
             LEFT JOIN item_classifications a ON a.item_id=i.id WHERE i.id=?1",
            [id],
            |r| {
                Ok(ItemClassification {
                    category_id: r.get(0)?,
                    subcategory_id: r.get(1)?,
                    comment: r.get(2)?,
                })
            },
        )?)
    }

    /// Category/child/comment are saved together, never on opening a dialog.
    pub fn set_item_classification(&self, id: i64, value: &ItemClassification) -> Result<()> {
        write_assignment(&self.db, id, value)
    }

    pub fn clear_item_category(&self, id: i64) -> Result<()> {
        // Also reject a deleted item, even if it never had an annotation.
        self.item_classification(id)?;
        self.db.execute(
            "UPDATE item_classifications SET category_id=NULL,subcategory_id=NULL WHERE item_id=?1 AND category_id IS NOT NULL",
            [id],
        )?;
        Ok(())
    }

    /// A comment dialog must not overwrite an item changed/replaced while open.
    pub fn save_classification_revision(
        &mut self,
        id: i64,
        value: &ItemClassification,
        revision: &[u8],
    ) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: Option<Vec<u8>> = tx
            .query_row(
                "SELECT token FROM item_export_revisions WHERE item_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if current.as_deref() != Some(revision) {
            return Err(tr("This item changed while the dialog was open. Close it and try again; your comment has not been saved.").into());
        }
        write_assignment(&tx, id, value)?;
        tx.commit()?;
        Ok(())
    }
}

fn reorder(db: &mut Connection, parent: Option<i64>, ids: &[i64]) -> Result<()> {
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (select, update) = if parent.is_some() {
        (
            "SELECT id FROM subcategories WHERE category_id=?1 ORDER BY id",
            "UPDATE subcategories SET position=?1 WHERE id=?2",
        )
    } else {
        (
            "SELECT id FROM categories WHERE ?1 IS NULL ORDER BY id",
            "UPDATE categories SET position=?1 WHERE id=?2",
        )
    };
    if let Some(id) = parent {
        require_row(usize::from(
            tx.query_row("SELECT 1 FROM categories WHERE id=?1", [id], |_| Ok(()))
                .optional()?
                .is_some(),
        ))?;
    }
    let existing = tx
        .prepare(select)?
        .query_map([parent], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut sorted = ids.to_vec();
    sorted.sort_unstable();
    if sorted != existing {
        return Err(tr("The category list changed. Try again.").into());
    }
    for (position, id) in ids.iter().enumerate() {
        tx.execute(update, params![position as i64, id])?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Query, Settings};

    fn store() -> Store {
        let mut s = Store {
            db: Connection::open_in_memory().unwrap(),
            settings: Settings::default(),
            config_path: Default::default(),
        };
        s.migrate().unwrap();
        s
    }

    #[test]
    fn removal_preserves_comments_and_items_but_clears_only_the_requested_level() {
        let mut s = store();
        let first = s.save_item(None, "", "First body").unwrap();
        let second = s.save_item(None, "", "Second body").unwrap();
        let category = s.create_category("Projects").unwrap();
        let child = s
            .create_subcategory(category, "What should I try?")
            .unwrap();
        let assigned = ItemClassification {
            category_id: Some(category),
            subcategory_id: Some(child),
            comment: "Try the native preview.\nThen compare.".into(),
        };
        for id in [first, second] {
            s.set_item_classification(id, &assigned).unwrap();
        }
        assert_eq!(s.category_item_count(category).unwrap(), 2);
        assert_eq!(s.subcategory_item_count(child).unwrap(), 2);
        s.clear_item_category(first).unwrap();
        let unclassified = ItemClassification {
            comment: assigned.comment.clone(),
            ..Default::default()
        };
        assert_eq!(s.item_classification(first).unwrap(), unclassified);
        assert_eq!(s.item_classification(second).unwrap(), assigned);
        s.delete_subcategory(child).unwrap();
        assert_eq!(
            s.item_classification(second).unwrap(),
            ItemClassification {
                subcategory_id: None,
                ..assigned.clone()
            }
        );
        // Parent deletion also works with a currently assigned child.
        let replacement = s.create_subcategory(category, "New question").unwrap();
        assert_ne!(child, replacement);
        s.set_item_classification(
            second,
            &ItemClassification {
                subcategory_id: Some(replacement),
                ..assigned
            },
        )
        .unwrap();
        s.delete_category(category).unwrap();
        assert_eq!(s.item_classification(second).unwrap(), unclassified);
        assert!(s.categories().unwrap().is_empty());
        assert_eq!(s.get(first).unwrap().content, "First body");
        assert_eq!(s.get(second).unwrap().content, "Second body");
        assert_ne!(s.create_category("Projects").unwrap(), category);
        // Deleting the note itself leaves no orphan annotation.
        s.delete(first).unwrap();
        assert!(s.item_classification(first).is_err());
        assert_eq!(
            s.db.query_row("SELECT count(*) FROM item_classifications", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn invalid_assignment_is_atomic_and_parent_child_relationship_is_enforced() {
        let mut s = store();
        let id = s.save_item(None, "", "body").unwrap();
        let a = s.create_category("A").unwrap();
        let b = s.create_category("B").unwrap();
        let child = s.create_subcategory(a, "Question").unwrap();
        let value = ItemClassification {
            category_id: Some(a),
            subcategory_id: Some(child),
            comment: "Original answer".into(),
        };
        s.set_item_classification(id, &value).unwrap();
        for invalid in [
            ItemClassification {
                category_id: Some(b),
                ..value.clone()
            },
            ItemClassification {
                category_id: None,
                ..value.clone()
            },
            ItemClassification {
                subcategory_id: Some(9999),
                ..value.clone()
            },
            ItemClassification {
                comment: "bad\0text".into(),
                ..value.clone()
            },
            ItemClassification {
                comment: "x".repeat(MAX_CONTENT + 1),
                ..value.clone()
            },
        ] {
            assert!(s.set_item_classification(id, &invalid).is_err());
            assert_eq!(s.item_classification(id).unwrap(), value);
        }
        assert!(s.set_item_classification(9999, &value).is_err());
        // A dictionary entry cannot silently move to another parent either.
        assert!(s
            .db
            .execute(
                "UPDATE subcategories SET category_id=?1 WHERE id=?2",
                params![b, child]
            )
            .is_err());
        let selection = s.selected_items(&[id]).unwrap();
        s.set_item_classification(id, &value).unwrap();
        assert_eq!(
            selection[0].revision,
            s.selected_items(&[id]).unwrap()[0].revision
        );
        s.save_classification_revision(id, &value, &selection[0].revision)
            .unwrap();
        s.save_item(Some(id), "", "Changed while comment dialog was open")
            .unwrap();
        assert!(s
            .save_classification_revision(
                id,
                &ItemClassification {
                    comment: "Stale answer".into(),
                    ..value.clone()
                },
                &selection[0].revision
            )
            .is_err());
        assert_eq!(s.item_classification(id).unwrap(), value);
    }

    #[test]
    fn classification_filters_share_paging_and_select_all_and_keep_searches_separate() {
        let mut s = store();
        let category = s.create_category("Projects").unwrap();
        let child = s.create_subcategory(category, "Next step?").unwrap();
        s.create_group("Ideas").unwrap();
        let folder = s.groups().unwrap().last().unwrap().id;
        let mut ids = Vec::new();
        for group in [0, 1, folder] {
            let id = s.save_item(None, "Title", "shared body").unwrap();
            s.move_item(id, group).unwrap();
            s.set_item_classification(
                id,
                &ItemClassification {
                    category_id: Some(category),
                    subcategory_id: Some(child),
                    comment: "Проверить тариф 100%_literal".into(),
                },
            )
            .unwrap();
            ids.push(id);
        }
        let unassigned = s.save_item(None, "", "shared body").unwrap();
        let query = Query {
            group_id: -1,
            search: "shared".into(),
            category_id: Some(category),
            subcategory_id: Some(child),
            comment: "ПРОВЕРИТЬ".into(),
            limit: 1,
            ..Default::default()
        };
        let expected: Vec<_> = ids.iter().rev().copied().collect();
        assert_eq!(s.matching_item_ids(&query).unwrap(), expected);
        for (offset, id) in expected.iter().enumerate() {
            let page = s
                .query(&Query {
                    offset: offset as i64,
                    ..query.clone()
                })
                .unwrap();
            assert_eq!(
                page.iter().map(|item| item.id).collect::<Vec<_>>(),
                vec![*id]
            );
        }
        assert_eq!(
            s.matching_item_ids(&Query {
                group_id: folder,
                ..query.clone()
            })
            .unwrap(),
            vec![ids[2]]
        );
        assert_eq!(
            s.matching_item_ids(&Query {
                comment: "%_".into(),
                ..query.clone()
            })
            .unwrap(),
            expected
        );
        assert!(s
            .matching_item_ids(&Query {
                search: "ПРОВЕРИТЬ".into(),
                ..query.clone()
            })
            .unwrap()
            .is_empty());
        assert!(s
            .matching_item_ids(&Query {
                comment: "shared body".into(),
                ..query.clone()
            })
            .unwrap()
            .is_empty());
        s.clear_item_category(ids[0]).unwrap();
        let no_category = Query {
            group_id: -1,
            category_id: Some(0),
            ..Default::default()
        };
        assert_eq!(
            s.matching_item_ids(&no_category).unwrap(),
            vec![unassigned, ids[0]]
        );
        assert_eq!(
            s.matching_item_ids(&Query {
                comment: "Проверить".into(),
                ..no_category
            })
            .unwrap(),
            vec![ids[0]]
        );
        assert!(query.same_filter(&Query {
            offset: 100,
            limit: 200,
            ..query.clone()
        }));
        for changed in [
            Query {
                category_id: None,
                ..query.clone()
            },
            Query {
                subcategory_id: None,
                ..query.clone()
            },
            Query {
                comment: String::new(),
                ..query.clone()
            },
        ] {
            assert!(!query.same_filter(&changed));
        }
        // Legacy Shell requests still see all matching content, no category filter.
        let legacy: Query = serde_json::from_str(r#"{"group_id":-1,"search":"shared"}"#).unwrap();
        assert_eq!(s.query(&legacy).unwrap().len(), 4);
    }

    #[test]
    fn moves_edits_and_combine_do_not_reinterpret_classification_or_retention() {
        let mut s = store();
        s.capture("history text", "Terminal", "", false).unwrap();
        let id = s.query(&Query::default()).unwrap()[0].id;
        let category = s.create_category("Ideas").unwrap();
        let annotation = ItemClassification {
            category_id: Some(category),
            comment: "Keep my answer".into(),
            ..Default::default()
        };
        s.set_item_classification(id, &annotation).unwrap();
        assert_eq!(s.get(id).unwrap().group_id, 0);
        s.create_group_for_item(id, "Folder").unwrap();
        let folder = s.get(id).unwrap().group_id;
        s.save_item(Some(id), "Renamed", "edited body").unwrap();
        assert_eq!(s.item_classification(id).unwrap(), annotation);
        s.delete_group(folder).unwrap();
        assert_eq!(s.get(id).unwrap().group_id, 1);
        assert_eq!(s.item_classification(id).unwrap(), annotation);
        let second = s.save_item(None, "", "another body").unwrap();
        let (combined, _) = s.combine_items(&[id, second]).unwrap();
        assert_eq!(
            s.item_classification(combined).unwrap(),
            ItemClassification::default()
        );
        assert_eq!(s.item_classification(id).unwrap(), annotation);
        // Hiding the feature cannot erase saved metadata.
        s.settings.classification_enabled = true;
        s.settings.classification_enabled = false;
        assert_eq!(s.item_classification(id).unwrap(), annotation);
    }

    #[test]
    fn dictionary_edits_invalidate_destructive_snapshots_but_reordering_does_not() {
        let mut s = store();
        let id = s.save_item(None, "", "body").unwrap();
        let other = s.save_item(None, "", "unrelated body").unwrap();
        let a = s.create_category("A").unwrap();
        let b = s.create_category("B").unwrap();
        let child = s.create_subcategory(a, "Question").unwrap();
        let child2 = s.create_subcategory(a, "Another question").unwrap();
        s.set_item_classification(
            id,
            &ItemClassification {
                category_id: Some(a),
                subcategory_id: Some(child),
                comment: "Answer".into(),
            },
        )
        .unwrap();
        let selected = s.selected_items(&[id]).unwrap();
        let unrelated = s.selected_items(&[other]).unwrap();
        s.reorder_categories(&[b, a]).unwrap();
        s.reorder_subcategories(a, &[child2, child]).unwrap();
        assert_eq!(
            selected[0].revision,
            s.selected_items(&[id]).unwrap()[0].revision
        );
        assert_eq!(
            s.categories()
                .unwrap()
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            vec![b, a]
        );
        assert_eq!(s.categories().unwrap()[1].children[0].id, child2);
        assert!(s.reorder_categories(&[a, a]).is_err());
        assert!(s.reorder_subcategories(a, &[child]).is_err());
        assert_eq!(s.categories().unwrap()[0].id, b);
        s.rename_category(a, "Updated A").unwrap();
        s.rename_category(a, "A").unwrap();
        assert!(s.delete_selected_items(&selected, &[]).is_err());
        let selected = s.selected_items(&[id]).unwrap();
        s.rename_subcategory(child, "Updated question").unwrap();
        assert!(s.move_selected_items(&selected, 1).is_err());
        let selected = s.selected_items(&[id]).unwrap();
        s.delete_subcategory(child).unwrap();
        assert!(s.delete_selected_items(&selected, &[]).is_err());
        assert_eq!(
            unrelated[0].revision,
            s.selected_items(&[other]).unwrap()[0].revision
        );
        assert_eq!(s.get(id).unwrap().content, "body");
    }
}
