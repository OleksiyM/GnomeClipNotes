use crate::{
    i18n::{ntrf, tr, trf},
    State,
};
use adw::prelude::*;
use std::{collections::HashSet, path::PathBuf, rc::Rc};

pub(crate) struct ExportDialog {
    pub dialog: adw::Dialog,
    state: Rc<State>,
    stack: gtk::Stack,
    choices: Vec<(i64, gtk::CheckButton)>,
    metadata: gtk::CheckButton,
    delete_items: gtk::CheckButton,
    delete_folders: gtk::CheckButton,
    start: gtk::Button,
    progress: gtk::Label,
}

pub fn show(state: &Rc<State>, parent: &impl IsA<gtk::Widget>) {
    match ExportDialog::new(state) {
        Ok(export) => export.dialog.present(Some(parent)),
        Err(error) => crate::ui::error(state, &error),
    }
}

fn check_row(title: &str, subtitle: &str, name: &str) -> (adw::ActionRow, gtk::CheckButton) {
    let check = gtk::CheckButton::new();
    check.set_widget_name(name);
    check.set_valign(gtk::Align::Center);
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .use_markup(false)
        .build();
    row.add_prefix(&check);
    row.set_activatable_widget(Some(&check));
    check.update_property(&[gtk::accessible::Property::Label(title)]);
    (row, check)
}

impl ExportDialog {
    pub(crate) fn new(state: &Rc<State>) -> Result<Rc<Self>, String> {
        let groups = state.store.borrow().groups().map_err(|e| e.to_string())?;
        let dialog = adw::Dialog::builder()
            .title(tr("Export Markdown"))
            .content_width(540)
            .content_height(620)
            .build();
        dialog.set_widget_name("export-dialog");
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        let stack = gtk::Stack::new();
        stack.set_vhomogeneous(false);
        let page = adw::PreferencesPage::new();
        let collections = adw::PreferencesGroup::builder().title(tr("Collections"))
            .description(tr("One Markdown file per collection. All saved items are included, regardless of filters. History is excluded."))
            .build();
        let mut choices = Vec::new();
        let mut has_items = false;
        for group in groups.into_iter().filter(|g| g.id > 0) {
            let count: i64 = state
                .store
                .borrow()
                .db
                .query_row(
                    "SELECT COUNT(*) FROM items WHERE group_id=?1",
                    [group.id],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let (row, check) = check_row(
                &if group.id == 1 {
                    tr("Notes")
                } else {
                    group.name.clone()
                },
                &if count == 0 {
                    tr("Empty")
                } else {
                    item_count(count as usize)
                },
                &format!("export-group-{}", group.id),
            );
            row.set_sensitive(count > 0);
            has_items |= count > 0;
            check.set_active(group.id == 1 && count > 0);
            collections.add(&row);
            choices.push((group.id, check));
        }
        if !has_items {
            collections.set_description(Some(&tr(
                "There are no saved items to export. History is not included.",
            )));
        }
        page.add(&collections);
        let format = adw::PreferencesGroup::builder()
            .title(tr("Document"))
            .build();
        let (row, metadata) = check_row(
            &tr("Additional metadata"),
            &tr(
                "Include source, origin and internal identifiers. Basic dates are always included.",
            ),
            "export-metadata",
        );
        format.add(&row);
        page.add(&format);
        let after = adw::PreferencesGroup::builder().title(tr("After export"))
            .description(tr("Deletion requires separate confirmation after all files are saved. These options are never remembered.")) .build();
        let (row, delete_items) = check_row(
            &tr("Delete exported items"),
            &tr("Changed items and items open in an editor will be kept."),
            "export-delete-items",
        );
        after.add(&row);
        let (folder_row, delete_folders) = check_row(
            &tr("Delete exported folders if empty"),
            &tr("Only selected custom folders. The Notes collection is never deleted."),
            "export-delete-folders",
        );
        after.add(&folder_row);
        page.add(&after);
        stack.add_named(&page, Some("options"));
        let progress_box = gtk::Box::new(gtk::Orientation::Vertical, 16);
        progress_box.set_valign(gtk::Align::Center);
        crate::ui::margins(&progress_box, 24);
        let spinner = gtk::Spinner::new();
        spinner.start();
        progress_box.append(&spinner);
        let progress = gtk::Label::new(Some(&tr("Saving Markdown files…")));
        progress.set_wrap(true);
        progress_box.append(&progress);
        stack.add_named(&progress_box, Some("progress"));
        toolbar.set_content(Some(&stack));
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        crate::ui::margins(&footer, 12);
        let start = gtk::Button::with_label(&tr("Choose Folder and Export…"));
        start.set_widget_name("export-start");
        start.add_css_class("suggested-action");
        start.set_hexpand(true);
        footer.append(&start);
        toolbar.add_bottom_bar(&footer);
        dialog.set_child(Some(&toolbar));
        let this = Rc::new(Self {
            dialog,
            state: state.clone(),
            stack,
            choices,
            metadata,
            delete_items,
            delete_folders,
            start,
            progress,
        });
        let sync: Rc<dyn Fn()> = Rc::new({
            let weak = Rc::downgrade(&this);
            move || {
                if let Some(this) = weak.upgrade() {
                    this.start
                        .set_sensitive(this.choices.iter().any(|(_, c)| c.is_active()));
                    let can_delete_folders = this.delete_items.is_active()
                        && this.choices.iter().any(|(id, c)| *id > 1 && c.is_active());
                    folder_row.set_sensitive(can_delete_folders);
                    if !can_delete_folders {
                        this.delete_folders.set_active(false);
                    }
                }
            }
        });
        for check in this
            .choices
            .iter()
            .map(|(_, c)| c)
            .chain([&this.delete_items])
        {
            let sync = sync.clone();
            check.connect_toggled(move |_| sync());
        }
        sync();
        {
            let weak = Rc::downgrade(&this);
            this.start.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    glib::MainContext::default().spawn_local(async move {
                        this.choose_folder().await;
                    });
                }
            });
        }
        // The explicit keepalive is released on either close or parent teardown.
        let owner = Rc::new(std::cell::RefCell::new(Some(this.clone())));
        let closed_owner = owner.clone();
        this.dialog.connect_closed(move |_| {
            closed_owner.borrow_mut().take();
        });
        this.dialog.connect_destroy(move |_| {
            owner.borrow_mut().take();
        });
        Ok(this)
    }

    async fn choose_folder(self: Rc<Self>) {
        self.start.set_sensitive(false);
        let chooser = gtk::FileDialog::builder()
            .title(tr("Choose Export Folder"))
            .accept_label(tr("Export Here"))
            .build();
        if let Some(path) = self.initial_folder() {
            chooser.set_initial_folder(Some(&gio::File::for_path(path)));
        }
        let parent = self.dialog.root().and_downcast::<gtk::Window>();
        match chooser.select_folder_future(parent.as_ref()).await {
            Ok(folder) => {
                if let Some(path) = folder.path() {
                    self.run_to(path).await;
                    return;
                } else {
                    crate::ui::error(
                        &self.state,
                        &tr("Choose a local folder or a mounted drive for export."),
                    );
                }
            }
            Err(error)
                if error.matches(gtk::DialogError::Dismissed)
                    || error.matches(gio::IOErrorEnum::Cancelled) => {}
            Err(error) => crate::ui::error(&self.state, &error.to_string()),
        }
        self.start.set_sensitive(true);
    }

    fn initial_folder(&self) -> Option<PathBuf> {
        self.state
            .store
            .borrow()
            .settings
            .last_export_folder
            .as_ref()
            .and_then(|uri| gio::File::for_uri(uri).path())
            .filter(|path| path.is_dir())
            .or_else(|| glib::user_special_dir(glib::UserDirectory::Documents))
    }

    fn protected(&self) -> Vec<i64> {
        self.state
            .editors
            .borrow()
            .iter()
            .filter(|(_, window)| window.upgrade().is_some())
            .map(|(id, _)| *id)
            .chain(self.state.remote_editors.borrow().keys().copied())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    pub(crate) async fn run_to(self: Rc<Self>, directory: PathBuf) {
        if !self.dialog.is_visible() {
            return;
        }
        let ids: Vec<_> = self
            .choices
            .iter()
            .filter(|(_, check)| check.is_active())
            .map(|(id, _)| *id)
            .collect();
        let delete_items = self.delete_items.is_active();
        let delete_folders = delete_items && self.delete_folders.is_active();
        let options = crate::export::ExportOptions {
            include_metadata: self.metadata.is_active(),
        };
        self.busy(&tr("Saving Markdown files…"));
        let snapshot = match self.state.store.borrow_mut().snapshot_markdown_export(&ids) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.finish(&tr("Export could not start"), &error.to_string(), None);
                return;
            }
        };
        if snapshot.collections.iter().all(|c| c.items.is_empty()) {
            self.finish(&tr("Nothing to export"), &tr("The selected collections are now empty. No files were created and nothing was deleted."), None);
            return;
        }
        let path = directory.clone();
        let worker = gio::spawn_blocking(move || {
            let written = crate::export::write_markdown_export(&snapshot, &path, options);
            (snapshot, written)
        })
        .await;
        let (snapshot, written) = match worker {
            Ok(value) => value,
            Err(_) => {
                self.finish(&tr("Export interrupted"), &tr("The export worker stopped unexpectedly. Some files may have been created in the selected folder. Nothing was deleted from the application."), Some(&directory));
                return;
            }
        };
        let mut summary = write_summary(&snapshot, &written, &directory);
        if !written.complete(&snapshot) {
            summary.push_str("\n\n");
            summary.push_str(&tr("Export is incomplete. Nothing was deleted from the application. Partial files, if listed above, must not be treated as complete exports."));
            self.finish(&tr("Export incomplete"), &summary, Some(&directory));
            return;
        }
        {
            let mut store = self.state.store.borrow_mut();
            let previous = store
                .settings
                .last_export_folder
                .replace(gio::File::for_path(&directory).uri().to_string());
            if let Err(error) = store.save_settings() {
                store.settings.last_export_folder = previous;
                summary.push_str("\n\n");
                summary.push_str(&trf(
                    "Files were saved, but the export folder could not be remembered: {error}",
                    &[("error", &error.to_string())],
                ));
            }
        }
        if !delete_items {
            summary.push_str("\n\n");
            summary.push_str(&tr("All items and folders were kept in the application."));
            self.finish(&tr("Export complete"), &summary, Some(&directory));
            return;
        }
        self.progress
            .set_text(&tr("Checking which exported items can be removed…"));
        let preview = match self.state.store.borrow().preview_export_cleanup(
            &snapshot,
            &written,
            &self.protected(),
            delete_folders,
        ) {
            Ok(preview) => preview,
            Err(error) => {
                summary.push_str("\n\n");
                summary.push_str(&trf(
                    "Cleanup could not be checked. Nothing was deleted: {error}",
                    &[("error", &error.to_string())],
                ));
                self.finish(
                    &tr("Export complete — items kept"),
                    &summary,
                    Some(&directory),
                );
                return;
            }
        };
        let details = preview
            .collections
            .iter()
            .map(|c| {
                let collection = collection_name(&snapshot, c.group_id);
                if c.folder_eligible {
                    let count = c.eligible_items.to_string();
                    ntrf(
                        "{collection}: delete {count} item and its empty folder",
                        "{collection}: delete {count} items and their empty folder",
                        c.eligible_items as u64,
                        &[("collection", &collection), ("count", &count)],
                    )
                } else {
                    let count = c.eligible_items.to_string();
                    ntrf(
                        "{collection}: delete {count} item",
                        "{collection}: delete {count} items",
                        c.eligible_items as u64,
                        &[("collection", &collection), ("count", &count)],
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        if preview.eligible_item_ids.is_empty() && preview.eligible_folder_ids.is_empty() {
            summary.push_str("\n\n");
            summary.push_str(&tr("Nothing could be removed safely."));
            summary.push('\n');
            summary.push_str(&skipped_summary(&snapshot, &preview.skipped));
            self.finish(
                &tr("Export complete — items kept"),
                &summary,
                Some(&directory),
            );
            return;
        }
        let mut confirmation_lines = Vec::new();
        if !preview.eligible_item_ids.is_empty() {
            let count = preview.eligible_item_ids.len().to_string();
            confirmation_lines.push(ntrf(
                "Delete {count} exported item from the application.",
                "Delete {count} exported items from the application.",
                preview.eligible_item_ids.len() as u64,
                &[("count", &count)],
            ));
        }
        if !preview.eligible_folder_ids.is_empty() {
            let count = preview.eligible_folder_ids.len().to_string();
            confirmation_lines.push(ntrf(
                "Delete {count} empty custom folder from the application.",
                "Delete {count} empty custom folders from the application.",
                preview.eligible_folder_ids.len() as u64,
                &[("count", &count)],
            ));
        }
        confirmation_lines.push(tr(
            "Exported files will remain on disk. Deletion from the application cannot be undone.",
        ));
        let confirm = adw::AlertDialog::builder()
            .heading(tr("Delete exported items?"))
            .body(confirmation_lines.join("\n"))
            .build();
        confirm.set_widget_name("export-confirm-cleanup");
        let skipped = skipped_summary(&snapshot, &preview.skipped);
        let label = gtk::Label::new(Some(&trf(
            "{details}\n\n{skipped}",
            &[("details", &details), ("skipped", &skipped)],
        )));
        label.set_wrap(true);
        label.set_xalign(0.0);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .max_content_height(240)
            .propagate_natural_height(true)
            .child(&label)
            .build();
        confirm.set_extra_child(Some(&scroll));
        confirm.add_response("keep", &tr("Keep in App"));
        confirm.add_response("delete", &tr("Delete Exported Items"));
        confirm.set_default_response(Some("keep"));
        confirm.set_close_response("keep");
        confirm.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        if confirm.choose_future(Some(&self.dialog)).await != "delete" {
            summary.push_str("\n\n");
            summary.push_str(&tr(
                "Cleanup was cancelled. All items and folders were kept in the application.",
            ));
            self.finish(&tr("Export complete"), &summary, Some(&directory));
            return;
        }
        self.progress
            .set_text(&tr("Rechecking files and removing confirmed items…"));
        let cleanup = self.state.store.borrow_mut().cleanup_exported_snapshot(
            &snapshot,
            &written,
            &preview,
            &self.protected(),
        );
        match cleanup {
            Ok(report) => {
                if !report.deleted_item_ids.is_empty() || !report.deleted_folder_ids.is_empty() {
                    self.state.changed();
                }
                let item_count = report.deleted_item_ids.len().to_string();
                let folder_count = report.deleted_folder_ids.len().to_string();
                summary.push_str("\n\n");
                summary.push_str(&ntrf(
                    "Deleted {count} item.",
                    "Deleted {count} items.",
                    report.deleted_item_ids.len() as u64,
                    &[("count", &item_count)],
                ));
                summary.push('\n');
                summary.push_str(&ntrf(
                    "Deleted {count} custom folder.",
                    "Deleted {count} custom folders.",
                    report.deleted_folder_ids.len() as u64,
                    &[("count", &folder_count)],
                ));
                summary.push('\n');
                summary.push_str(&skipped_summary(&snapshot, &report.skipped));
                if !report.errors.is_empty() {
                    summary.push('\n');
                    summary.push_str(&trf(
                        "Cleanup errors:\n{errors}",
                        &[("errors", &report.errors.join("\n"))],
                    ));
                }
                self.finish(&tr("Export complete"), &summary, Some(&directory));
            }
            Err(error) => {
                summary.push_str("\n\n");
                summary.push_str(&trf(
                    "Cleanup failed; the database transaction was rolled back. Exported files remain saved. Error: {error}",
                    &[("error", &error.to_string())],
                ));
                self.finish(
                    &tr("Export complete — cleanup failed"),
                    &summary,
                    Some(&directory),
                );
            }
        }
    }

    fn busy(&self, message: &str) {
        self.dialog.set_can_close(false);
        self.start.set_visible(false);
        self.progress.set_text(message);
        self.stack.set_visible_child_name("progress");
    }

    fn finish(&self, heading: &str, message: &str, path: Option<&std::path::Path>) {
        self.dialog.set_can_close(true);
        self.dialog.set_content_height(360);
        self.start.set_visible(false);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
        crate::ui::margins(&content, 24);
        let title = gtk::Label::new(Some(heading));
        title.add_css_class("title-2");
        title.set_wrap(true);
        title.set_xalign(0.0);
        content.append(&title);
        let result = gtk::Label::new(Some(message));
        result.set_widget_name("export-result");
        result.set_wrap(true);
        result.set_selectable(true);
        result.set_xalign(0.0);
        content.append(&result);
        if let Some(path) = path {
            let open = gtk::Button::with_label(&tr("Open Export Folder"));
            open.set_widget_name("export-open-folder");
            open.set_halign(gtk::Align::Start);
            let file = gio::File::for_path(path);
            let launch_error = gtk::Label::new(None);
            launch_error.set_widget_name("export-open-folder-error");
            launch_error.set_wrap(true);
            launch_error.set_xalign(0.0);
            launch_error.set_selectable(true);
            launch_error.add_css_class("error");
            launch_error.set_visible(false);
            let weak = self.dialog.downgrade();
            let error_label = launch_error.clone();
            open.connect_clicked(move |button| {
                if let Some(dialog) = weak.upgrade() {
                    let parent = dialog.root().and_downcast::<gtk::Window>();
                    button.set_sensitive(false);
                    error_label.set_visible(false);
                    let button = button.downgrade();
                    let label = error_label.downgrade();
                    gtk::FileLauncher::new(Some(&file)).launch(
                        parent.as_ref(),
                        gio::Cancellable::NONE,
                        move |result| {
                            if let (Some(button), Some(label)) = (button.upgrade(), label.upgrade())
                            {
                                folder_launch_finished(&button, &label, result);
                            }
                        },
                    );
                }
            });
            content.append(&open);
            content.append(&launch_error);
        }
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&content)
            .build();
        self.stack.add_named(&scroll, Some("result"));
        self.stack.set_visible_child_name("result");
    }
}

pub(crate) fn folder_launch_finished(
    button: &gtk::Button,
    label: &gtk::Label,
    result: Result<(), glib::Error>,
) {
    button.set_sensitive(true);
    label.set_visible(false);
    if let Err(error) = result {
        if error.matches(gtk::DialogError::Dismissed)
            || error.matches(gtk::DialogError::Cancelled)
            || error.matches(gio::IOErrorEnum::Cancelled)
        {
            return;
        }
        label.set_text(&trf(
            "Could not open the export folder. Exported files are unchanged.\n{error}",
            &[("error", &error.to_string())],
        ));
        label.set_visible(true);
    }
}

fn collection_name(snapshot: &crate::export::ExportSnapshot, id: i64) -> String {
    snapshot
        .collections
        .iter()
        .find(|c| c.group_id == id)
        .map(|c| {
            if c.group_id == 1 {
                tr("Notes")
            } else {
                c.group_name.clone()
            }
        })
        .unwrap_or_else(|| tr("Collection"))
}

fn item_count(count: usize) -> String {
    let value = count.to_string();
    ntrf(
        "{count} item",
        "{count} items",
        count as u64,
        &[("count", &value)],
    )
}

fn write_summary(
    snapshot: &crate::export::ExportSnapshot,
    report: &crate::export::ExportWriteReport,
    directory: &std::path::Path,
) -> String {
    let destination = directory.display().to_string();
    let mut lines = vec![trf(
        "Destination: {destination}",
        &[("destination", &destination)],
    )];
    for collection in snapshot.collections.iter().filter(|c| c.items.is_empty()) {
        lines.push(trf(
            "{collection}: Empty — skipped; no file created",
            &[(
                "collection",
                &collection_name(snapshot, collection.group_id),
            )],
        ));
    }
    for file in &report.files {
        let count = snapshot
            .collections
            .iter()
            .find(|c| c.group_id == file.group_id)
            .map_or(0, |c| c.items.len());
        let collection = collection_name(snapshot, file.group_id);
        let file_name = file
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let count_value = count.to_string();
        lines.push(ntrf(
            "{collection}: {count} item → {file}",
            "{collection}: {count} items → {file}",
            count as u64,
            &[
                ("collection", collection.as_str()),
                ("count", count_value.as_str()),
                ("file", file_name.as_str()),
            ],
        ));
    }
    for failure in &report.errors {
        lines.push(trf(
            "{file}: {error}",
            &[
                ("file", &failure.intended_name),
                ("error", &failure.error.to_string()),
            ],
        ));
        if let Some(path) = &failure.partial_path {
            lines.push(trf(
                "Incomplete file: {path}",
                &[("path", &path.display().to_string())],
            ));
        }
    }
    if let Some(error) = &report.directory_sync_error {
        lines.push(trf(
            "Could not confirm files were saved safely: {error}",
            &[("error", &error.to_string())],
        ));
    }
    lines.join("\n")
}

fn skipped_summary(
    snapshot: &crate::export::ExportSnapshot,
    skipped: &[crate::export::CleanupSkip],
) -> String {
    let mut counts = std::collections::BTreeMap::new();
    for skip in skipped {
        *counts
            .entry((skip.group_id, skip.item_id.is_some(), skip.reason.as_str()))
            .or_insert(0usize) += 1;
    }
    counts
        .into_iter()
        .map(|((id, item, reason), count)| {
            let collection = collection_name(snapshot, id);
            let count_value = count.to_string();
            if item {
                ntrf(
                    "{collection}: {count} item kept — {reason}",
                    "{collection}: {count} items kept — {reason}",
                    count as u64,
                    &[
                        ("collection", collection.as_str()),
                        ("count", count_value.as_str()),
                        ("reason", reason),
                    ],
                )
            } else {
                ntrf(
                    "{collection}: {count} folder kept — {reason}",
                    "{collection}: {count} folders kept — {reason}",
                    count as u64,
                    &[
                        ("collection", collection.as_str()),
                        ("count", count_value.as_str()),
                        ("reason", reason),
                    ],
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
