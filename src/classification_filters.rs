use crate::{
    i18n::tr,
    model::{Category, Query},
};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub struct Filters {
    pub widget: gtk::Box,
    category: gtk::DropDown,
    child: gtk::DropDown,
    comment: gtk::Entry,
    categories: RefCell<Vec<Category>>,
    updating: Cell<bool>,
}

fn dropdown(name: &str) -> gtk::DropDown {
    // A custom display factory does not tell GtkDropDown which text to search.
    let control = gtk::DropDown::builder()
        .enable_search(true)
        .expression(gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            None::<gtk::Expression>,
            "string",
        ))
        .search_match_mode(gtk::StringFilterMatchMode::Substring)
        .build();
    control.set_widget_name(name);
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let item = object.downcast_ref::<gtk::ListItem>().unwrap();
        item.set_child(Some(
            &gtk::Label::builder()
                .xalign(0.0)
                .max_width_chars(26)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build(),
        ));
    });
    factory.connect_bind(|_, object| {
        let item = object.downcast_ref::<gtk::ListItem>().unwrap();
        if let (Some(label), Some(text)) = (
            item.child().and_downcast::<gtk::Label>(),
            item.item().and_downcast::<gtk::StringObject>(),
        ) {
            label.set_text(&text.string());
            label.set_tooltip_text(Some(&text.string()));
        }
    });
    control.set_factory(Some(&factory));
    control
}

impl Filters {
    pub fn new() -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        widget.set_widget_name("library-classification-filters");
        widget.set_visible(false);
        let category = dropdown("library-filter-category");
        let child = dropdown("library-filter-subcategory");
        let comment = gtk::Entry::builder()
            .placeholder_text(tr("Comment contains…"))
            .build();
        comment.set_widget_name("library-filter-comment");
        category.set_visible(false);
        child.set_visible(false);
        comment.set_visible(false);
        category.update_property(&[gtk::accessible::Property::Label(&tr("Category"))]);
        child.update_property(&[gtk::accessible::Property::Label(&tr("Subcategory"))]);
        comment.update_property(&[gtk::accessible::Property::Label(&tr("Comment contains…"))]);
        widget.append(&category);
        widget.append(&child);
        widget.append(&comment);
        Rc::new(Self {
            widget,
            category,
            child,
            comment,
            categories: RefCell::new(Vec::new()),
            updating: Cell::new(false),
        })
    }

    fn category_id(&self) -> Option<i64> {
        match self.category.selected() {
            0 => None,
            1 => Some(0),
            n => self.categories.borrow().get(n as usize - 2).map(|c| c.id),
        }
    }

    fn child_id(&self) -> Option<i64> {
        let category = self.category_id();
        let index = self.child.selected().checked_sub(1)? as usize;
        self.categories
            .borrow()
            .iter()
            .find(|c| Some(c.id) == category)?
            .children
            .get(index)
            .map(|c| c.id)
    }

    fn rebuild_children(&self, selected: Option<i64>) {
        let category = self.category_id();
        let categories = self.categories.borrow();
        let children = categories
            .iter()
            .find(|c| Some(c.id) == category)
            .map(|c| c.children.as_slice())
            .unwrap_or_default();
        let mut names = vec![tr("All subcategories")];
        names.extend(children.iter().map(|c| c.name.clone()));
        self.child.set_model(Some(&gtk::StringList::new(
            &names.iter().map(String::as_str).collect::<Vec<_>>(),
        )));
        self.child.set_selected(
            children
                .iter()
                .position(|c| Some(c.id) == selected)
                .map_or(0, |i| i as u32 + 1),
        );
        self.child.set_visible(!children.is_empty());
    }

    /// Reconcile by stable IDs, not row positions; removed choices reset filters.
    pub fn sync(
        &self,
        categories_enabled: bool,
        comments_enabled: bool,
        categories: Vec<Category>,
    ) -> bool {
        let before = self.query();
        // Feature state is the widget's own property, not whether its popover
        // is currently mapped. Closing Filters must not clear the query.
        if self.category.get_visible() == categories_enabled
            && self.comment.get_visible() == comments_enabled
            && *self.categories.borrow() == categories
        {
            return false;
        }
        self.updating.set(true);
        self.widget
            .set_visible(categories_enabled || comments_enabled);
        self.category.set_visible(categories_enabled);
        self.comment.set_visible(comments_enabled);
        let category = before.category_id;
        let child = before.subcategory_id;
        *self.categories.borrow_mut() = categories;
        let mut names = vec![tr("All categories"), tr("No category")];
        names.extend(self.categories.borrow().iter().map(|c| c.name.clone()));
        self.category.set_model(Some(&gtk::StringList::new(
            &names.iter().map(String::as_str).collect::<Vec<_>>(),
        )));
        let selected = if !categories_enabled {
            0
        } else if category == Some(0) {
            1
        } else {
            self.categories
                .borrow()
                .iter()
                .position(|c| Some(c.id) == category)
                .map_or(0, |i| i as u32 + 2)
        };
        self.category.set_selected(selected);
        self.rebuild_children(if categories_enabled { child } else { None });
        if !comments_enabled {
            self.comment.set_text("");
        }
        self.updating.set(false);
        !before.same_filter(&self.query())
    }

    pub fn query(&self) -> Query {
        if !self.widget.get_visible() {
            return Query::default();
        }
        Query {
            category_id: if self.category.get_visible() {
                self.category_id()
            } else {
                None
            },
            subcategory_id: if self.category.get_visible() {
                self.child_id()
            } else {
                None
            },
            comment: if self.comment.get_visible() {
                self.comment.text().to_string()
            } else {
                String::new()
            },
            ..Default::default()
        }
    }

    pub fn count(&self) -> u8 {
        let q = self.query();
        u8::from(q.category_id.is_some())
            + u8::from(q.subcategory_id.is_some())
            + u8::from(!q.comment.is_empty())
    }

    pub fn clear(&self) {
        self.updating.set(true);
        self.category.set_selected(0);
        self.rebuild_children(None);
        self.comment.set_text("");
        self.updating.set(false);
    }

    pub fn connect_changed(self: &Rc<Self>, changed: impl Fn() + 'static) {
        let changed: Rc<dyn Fn()> = Rc::new(changed);
        let weak = Rc::downgrade(self);
        let callback = changed.clone();
        self.category.connect_selected_notify(move |_| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            if this.updating.get() {
                return;
            }
            this.updating.set(true);
            this.rebuild_children(None);
            this.updating.set(false);
            callback();
        });
        let weak = Rc::downgrade(self);
        let callback = changed.clone();
        self.child.connect_selected_notify(move |_| {
            if let Some(this) = weak.upgrade() {
                if !this.updating.get() {
                    callback();
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.comment.connect_changed(move |_| {
            if let Some(this) = weak.upgrade() {
                if !this.updating.get() {
                    changed();
                }
            }
        });
    }
}
