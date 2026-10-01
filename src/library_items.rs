use crate::{
    export::{export_annotation, ExportAnnotation},
    i18n::tr,
    model::{kind, now, Item, MAX_CONTENT},
    store::{Result, Store},
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone)]
pub struct SelectedItem {
    pub item: Item,
    pub revision: Vec<u8>,
    pub annotation: ExportAnnotation,
}

pub fn joined_content(items: &[SelectedItem]) -> String {
    items
        .iter()
        .map(|selected| selected.item.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

fn read_selected(db: &Connection, ids: &[i64]) -> Result<Vec<SelectedItem>> {
    let unique: BTreeSet<_> = ids.iter().copied().collect();
    let mut stmt = db.prepare("SELECT i.*, r.token FROM items i JOIN item_export_revisions r ON r.item_id=i.id WHERE i.id=?1")?;
    let mut items = Vec::with_capacity(unique.len());
    for id in unique {
        let selected = stmt
            .query_row([id], |r| {
                Ok(SelectedItem {
                    item: Store::row(r)?,
                    revision: r.get(11)?,
                    annotation: export_annotation(db, id)?,
                })
            })
            .optional()?
            .ok_or_else(|| tr("The selection changed. Select items again."))?;
        items.push(selected);
    }
    items.sort_by_key(|selected| (selected.item.created_at, selected.item.id));
    Ok(items)
}

fn check_revisions(db: &Connection, items: &[SelectedItem]) -> Result<()> {
    let mut seen = HashSet::with_capacity(items.len());
    let mut stmt = db.prepare("SELECT token FROM item_export_revisions WHERE item_id=?1")?;
    for selected in items {
        if !seen.insert(selected.item.id)
            || stmt
                .query_row([selected.item.id], |r| r.get::<_, Vec<u8>>(0))
                .optional()?
                .as_ref()
                != Some(&selected.revision)
        {
            return Err(tr("The selection changed. Select items again.").into());
        }
    }
    Ok(())
}

impl Store {
    pub fn selected_items(&mut self, ids: &[i64]) -> Result<Vec<SelectedItem>> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let items = read_selected(&tx, ids)?;
        tx.commit()?;
        Ok(items)
    }

    pub fn combine_items(&mut self, ids: &[i64]) -> Result<(i64, Vec<SelectedItem>)> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let items = read_selected(&tx, ids)?;
        if items.len() < 2 {
            return Err(tr("Select at least two items to combine.").into());
        }
        let size = (items.len() - 1)
            .checked_mul("\n\n---\n\n".len())
            .and_then(|separators| {
                items.iter().try_fold(separators, |len, selected| {
                    len.checked_add(selected.item.content.len())
                })
            });
        if size.is_none_or(|len| len > MAX_CONTENT)
            || items
                .iter()
                .any(|selected| selected.item.content.contains('\0'))
        {
            return Err(tr("Text must be at most 1 MiB and contain no NUL characters").into());
        }
        let content = joined_content(&items);
        if content.trim().is_empty() {
            return Err(tr("A note cannot be empty").into());
        }
        let group = if items[0].item.group_id >= 2
            && items
                .iter()
                .all(|selected| selected.item.group_id == items[0].item.group_id)
        {
            items[0].item.group_id
        } else {
            1
        };
        let timestamp = now();
        tx.execute("INSERT INTO items(title,content,kind,origin,created_at,updated_at,copied_at,group_id) VALUES('',?1,?2,'manual',?3,?3,?3,?4)", params![content, kind(&content), timestamp, group])?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok((id, items))
    }

    pub fn delete_selected_items(
        &mut self,
        items: &[SelectedItem],
        protected_ids: &[i64],
    ) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let protected: HashSet<_> = protected_ids.iter().copied().collect();
        if items
            .iter()
            .any(|selected| protected.contains(&selected.item.id))
        {
            return Err(tr("Close the editor before deleting selected items.").into());
        }
        check_revisions(&tx, items)?;
        for selected in items {
            tx.execute("DELETE FROM items WHERE id=?1", [selected.item.id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn move_selected_items(&mut self, items: &[SelectedItem], group: i64) -> Result<()> {
        if group < 1 {
            return Err(tr("Choose Notes or a folder as the destination.").into());
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx
            .query_row("SELECT 1 FROM groups WHERE id=?1", [group], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
            .is_none()
        {
            return Err(tr("The destination folder no longer exists.").into());
        }
        check_revisions(&tx, items)?;
        let timestamp = now();
        for selected in items {
            tx.execute(
                "UPDATE items SET group_id=?1,updated_at=?2 WHERE id=?3",
                params![group, timestamp, selected.item.id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
