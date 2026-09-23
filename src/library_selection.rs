//! Persistent item identity is separate from transient GTK keyboard focus.
use crate::{item_shortcuts, State};
use adw::prelude::*;
use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

pub struct Selection {
    list: gtk::FlowBox,
    state: Weak<State>,
    id: Cell<Option<i64>>,
    rebuilding: Cell<bool>,
    edge: Cell<i32>,
    focus: gtk::EventControllerFocus,
    previous: glib::WeakRef<gtk::Button>,
    next: glib::WeakRef<gtk::Button>,
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

    pub fn begin_refresh(&self) -> bool {
        self.rebuilding.set(true);
        self.focus.contains_focus()
    }

    pub fn append(self: &Rc<Self>, id: i64, card: &gtk::Box) {
        let child = gtk::FlowBoxChild::new();
        child.set_widget_name(&format!("library-item-{id}"));
        child.set_child(Some(card));
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
        if let Some(action) = item_shortcuts::action(key, modifiers) {
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
