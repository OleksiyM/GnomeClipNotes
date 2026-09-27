//! Persistent item identity is separate from transient GTK keyboard focus.
use crate::{i18n::tr, item_shortcuts, model::Query, State};
use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::{Rc, Weak},
};

type ActionCallback = Box<dyn Fn(&str)>;

pub struct Selection {
    list: gtk::FlowBox,
    state: Weak<State>,
    id: Cell<Option<i64>>,
    rebuilding: Cell<bool>,
    edge: Cell<i32>,
    focus: gtk::EventControllerFocus,
    previous: glib::WeakRef<gtk::Button>,
    next: glib::WeakRef<gtk::Button>,
    multiple: Cell<bool>,
    checked: RefCell<BTreeSet<i64>>,
    query: RefCell<Query>,
    ready: Cell<bool>,
    on_change: RefCell<Option<Box<dyn Fn()>>>,
    on_action: RefCell<Option<ActionCallback>>,
}

fn item_id(child: &gtk::FlowBoxChild) -> Option<i64> {
    child
        .widget_name()
        .strip_prefix("library-item-")?
        .parse()
        .ok()
}

impl Selection {
    pub fn new(
        list: &gtk::FlowBox,
        state: &Rc<State>,
        previous: &gtk::Button,
        next: &gtk::Button,
    ) -> Rc<Self> {
        let focus = gtk::EventControllerFocus::new();
        list.add_controller(focus.clone());
        let selection = Rc::new(Self {
            list: list.clone(),
            state: Rc::downgrade(state),
            id: Cell::new(None),
            rebuilding: Cell::new(false),
            edge: Cell::new(0),
            focus,
            previous: previous.downgrade(),
            next: next.downgrade(),
            multiple: Cell::new(false),
            checked: RefCell::new(BTreeSet::new()),
            query: RefCell::new(Query::default()),
            ready: Cell::new(true),
            on_change: RefCell::new(None),
            on_action: RefCell::new(None),
        });
        let weak = Rc::downgrade(&selection);
        list.connect_selected_children_changed(move |list| {
            if let Some(s) = weak.upgrade() {
                if !s.rebuilding.get() {
                    s.id.set(list.selected_children().first().and_then(item_id));
                }
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_name(Some("library-item-shortcuts"));
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&selection);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            weak.upgrade()
                .map_or(glib::Propagation::Proceed, |s| s.key(key, modifiers))
        });
        list.add_controller(keys);
        selection
    }

    pub fn is_multiple(&self) -> bool {
        self.multiple.get()
    }

    pub fn ids(&self) -> Vec<i64> {
        self.checked.borrow().iter().copied().collect()
    }

    pub fn is_ready(&self) -> bool {
        self.ready.get()
    }

    // SearchEntry debounces rendering, but old selections must stop being
    // actionable as soon as the user changes the text.
    pub fn search_edited(&self, text: &str) {
        if self.query.borrow().search != text {
            self.invalidate();
        }
    }

    pub fn invalidate(&self) {
        self.ready.set(false);
        self.checked.borrow_mut().clear();
        self.paint();
    }

    pub fn connect_changed(&self, callback: impl Fn() + 'static) {
        *self.on_change.borrow_mut() = Some(Box::new(callback));
    }

    pub fn connect_action(&self, callback: impl Fn(&str) + 'static) {
        *self.on_action.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_multiple(&self, enabled: bool) {
        self.multiple.set(enabled);
        self.checked.borrow_mut().clear();
        if enabled {
            self.list.add_css_class("multiple-selection");
        } else {
            self.list.remove_css_class("multiple-selection");
        }
        self.notify();
        if let Some(state) = self.state.upgrade() {
            state.changed();
        }
    }

    pub fn set_query(&self, query: &Query) {
        self.ready.set(true);
        let old = self.query.borrow();
        let changed = old.search != query.search
            || old.group_id != query.group_id
            || old.kind != query.kind
            || old.source != query.source
            || old.since != query.since
            || old.until != query.until;
        drop(old);
        *self.query.borrow_mut() = query.clone();
        if changed {
            self.checked.borrow_mut().clear();
        }
        // A capture or editor save may refresh the page; only remove selections
        // that no longer match, never auto-select newly arrived items.
        if self.is_multiple() && !self.checked.borrow().is_empty() {
            if let Some(state) = self.state.upgrade() {
                if let Ok(ids) = state.store.borrow().matching_item_ids(query) {
                    let available: BTreeSet<_> = ids.into_iter().collect();
                    self.checked
                        .borrow_mut()
                        .retain(|id| available.contains(id));
                }
            }
        }
        self.notify();
    }

    pub fn select_all(&self) {
        if !self.is_multiple() || !self.is_ready() {
            return;
        }
        if let Some(state) = self.state.upgrade() {
            let result = state.store.borrow().matching_item_ids(&self.query.borrow());
            match result {
                Ok(ids) => *self.checked.borrow_mut() = ids.into_iter().collect(),
                Err(error) => crate::ui::error(&state, &error.to_string()),
            }
        }
        self.paint();
    }

    pub fn deselect_all(&self) {
        if !self.is_multiple() {
            return;
        }
        self.checked.borrow_mut().clear();
        self.paint();
    }

    fn toggle(&self, id: i64) {
        if !self.is_ready() {
            return;
        }
        let mut checked = self.checked.borrow_mut();
        if !checked.remove(&id) {
            checked.insert(id);
        }
        drop(checked);
        self.paint();
    }

    fn notify(&self) {
        if let Some(callback) = self.on_change.borrow().as_ref() {
            callback();
        }
    }

    fn paint(&self) {
        let mut row = self.list.first_child();
        while let Some(widget) = row {
            row = widget.next_sibling();
            if let Ok(child) = widget.downcast::<gtk::FlowBoxChild>() {
                let checked = item_id(&child).is_some_and(|id| self.checked.borrow().contains(&id));
                if checked {
                    child.add_css_class("bulk-selected");
                } else {
                    child.remove_css_class("bulk-selected");
                }
                if let Some(check) = child
                    .child()
                    .and_then(|c| c.first_child())
                    .and_then(|head| head.last_child())
                    .and_downcast::<gtk::CheckButton>()
                {
                    check.set_active(checked);
                }
            }
        }
        self.notify();
    }

    pub fn begin_refresh(&self) -> bool {
        self.rebuilding.set(true);
        self.focus.contains_focus()
    }

    pub fn append(self: &Rc<Self>, id: i64, card: &gtk::Box) {
        let child = gtk::FlowBoxChild::new();
        child.set_widget_name(&format!("library-item-{id}"));
        child.set_child(Some(card));
        if self.is_multiple() {
            let check = gtk::CheckButton::new();
            check.set_tooltip_text(Some(&tr("Select item")));
            check.set_can_target(false);
            check.set_focusable(false);
            check.set_active(self.checked.borrow().contains(&id));
            let weak = Rc::downgrade(self);
            check.connect_toggled(move |check| {
                if let Some(selection) = weak.upgrade() {
                    if selection.checked.borrow().contains(&id) != check.is_active() {
                        selection.toggle(id);
                    }
                }
            });
            if check.is_active() {
                child.add_css_class("bulk-selected");
            }
            if let Some(head) = card.first_child().and_downcast::<gtk::Box>() {
                head.append(&check);
            }
        }
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(self);
        let target = child.downgrade();
        focus.connect_enter(move |_| {
            if let (Some(s), Some(child)) = (weak.upgrade(), target.upgrade()) {
                if !s.rebuilding.get() {
                    s.list.select_child(&child);
                }
            }
        });
        child.add_controller(focus);
        let click = gtk::GestureClick::new();
        click.set_button(0);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        let target = child.downgrade();
        click.connect_pressed(move |gesture, _, _, _| {
            if !matches!(gesture.current_button(), 1 | 3) {
                return;
            }
            if let (Some(s), Some(child)) = (weak.upgrade(), target.upgrade()) {
                s.list.select_child(&child);
                child.grab_focus();
                if s.is_multiple() {
                    if gesture.current_button() == 1 {
                        s.toggle(id);
                    }
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            }
        });
        child.add_controller(click);
        self.list.insert(&child, -1);
    }

    pub fn finish_refresh(&self, had_focus: bool) {
        let edge = self.edge.replace(0);
        let mut target = None;
        let mut child = self.list.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(row) = widget.downcast::<gtk::FlowBoxChild>() {
                if (edge == 1 && target.is_none())
                    || edge == -1
                    || (edge == 0 && item_id(&row) == self.id.get())
                {
                    target = Some(row);
                }
            }
        }
        self.id.set(target.as_ref().and_then(item_id));
        if let Some(target) = target {
            self.list.select_child(&target);
            if had_focus || edge != 0 {
                target.grab_focus();
            }
        }
        self.rebuilding.set(false);
    }

    fn key(&self, key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> glib::Propagation {
        use gtk::gdk::{Key, ModifierType};
        if self.rebuilding.get() || !self.focus.contains_focus() {
            return glib::Propagation::Proceed;
        }
        // A menu, dialog or editable control owns its keys, even inside the grid.
        let mut focused = self
            .list
            .root()
            .and_downcast::<gtk::Window>()
            .and_then(|w| GtkWindowExt::focus(&w));
        while let Some(widget) = focused {
            if widget == self.list {
                break;
            }
            if widget.is::<gtk::Popover>()
                || widget.is::<gtk::Editable>()
                || widget.is::<gtk::TextView>()
            {
                return glib::Propagation::Proceed;
            }
            focused = widget.parent();
        }
        if self.is_multiple() {
            if modifiers.is_empty() {
                match key {
                    Key::space | Key::Return | Key::KP_Enter => {
                        if let Some(id) = self.id.get() {
                            self.toggle(id);
                        }
                        return glib::Propagation::Stop;
                    }
                    Key::Escape => {
                        self.set_multiple(false);
                        return glib::Propagation::Stop;
                    }
                    _ => {}
                }
            }
            if modifiers == ModifierType::CONTROL_MASK && matches!(key, Key::a | Key::A) {
                self.select_all();
                return glib::Propagation::Stop;
            }
            let action =
                if modifiers == ModifierType::CONTROL_MASK && matches!(key, Key::c | Key::C) {
                    Some("copy")
                } else if item_shortcuts::action(key, modifiers) == Some("delete") {
                    Some("delete")
                } else {
                    None
                };
            if let Some(action) = action {
                if let Some(callback) = self.on_action.borrow().as_ref() {
                    callback(action);
                }
                return glib::Propagation::Stop;
            }
            if item_shortcuts::action(key, modifiers).is_some() {
                return glib::Propagation::Stop;
            }
        } else if let Some(action) = item_shortcuts::action(key, modifiers) {
            if let (Some(id), Some(state)) = (self.id.get(), self.state.upgrade()) {
                state.activate(action, id);
            }
            return glib::Propagation::Stop;
        }
        if modifiers.intersects(
            ModifierType::CONTROL_MASK
                | ModifierType::ALT_MASK
                | ModifierType::SHIFT_MASK
                | ModifierType::SUPER_MASK,
        ) {
            return glib::Propagation::Proceed;
        }
        let columns = self.list.max_children_per_line() as i32;
        let delta = match key {
            Key::Left => -1,
            Key::Right => 1,
            Key::Up => -columns,
            Key::Down => columns,
            _ => return glib::Propagation::Proceed,
        };
        let current = self.list.selected_children().first().map(|row| row.index());
        let index = current.map_or(0, |index| index + delta);
        if let Some(child) = (index >= 0)
            .then(|| self.list.child_at_index(index))
            .flatten()
        {
            self.list.select_child(&child);
            child.grab_focus();
        } else if current.is_some() {
            let (button, edge) = if delta < 0 {
                (self.previous.upgrade(), -1)
            } else {
                (self.next.upgrade(), 1)
            };
            if let Some(button) = button.filter(|b| b.is_sensitive()) {
                self.edge.set(edge);
                button.emit_clicked();
            }
        }
        glib::Propagation::Stop
    }
}
