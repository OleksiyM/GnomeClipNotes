use adw::prelude::*;

pub(crate) struct DatePicker {
    pub row: gtk::Box,
    pub entry: gtk::Entry,
}

impl DatePicker {
    pub fn new(name: &str, placeholder: &str) -> Self {
        let entry = gtk::Entry::builder()
            .placeholder_text(crate::i18n::tr(placeholder))
            .width_chars(15)
            .hexpand(true)
            .build();
        entry.set_widget_name(name);

        let calendar = gtk::Calendar::new();
        calendar.set_widget_name(&format!("{name}-calendar"));
        let today = gtk::Button::with_label(&crate::i18n::tr("Today"));
        today.set_widget_name(&format!("{name}-today"));
        today.add_css_class("flat");
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.append(&calendar);
        content.append(&today);
        crate::ui::margins(&content, 6);
        let popover = gtk::Popover::builder().child(&content).build();
        popover.set_widget_name(&format!("{name}-calendar-popover"));

        let chooser = gtk::MenuButton::builder()
            .icon_name("x-office-calendar-symbolic")
            .tooltip_text(crate::i18n::tr("Choose date"))
            .popover(&popover)
            .build();
        chooser.set_widget_name(&format!("{name}-calendar-button"));
        chooser.add_css_class("flat");

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.add_css_class("linked");
        row.append(&entry);
        row.append(&chooser);

        {
            let entry = entry.downgrade();
            let calendar = calendar.downgrade();
            chooser.connect_active_notify(move |button| {
                if !button.is_active() {
                    return;
                }
                let (Some(entry), Some(calendar)) = (entry.upgrade(), calendar.upgrade()) else {
                    return;
                };
                let shown = parse_date(&entry.text())
                    .or_else(|| glib::DateTime::now_local().ok())
                    .expect("A local calendar date");
                calendar.select_day(&shown);
            });
        }
        // GtkCalendar does not promise day-selected for the already selected
        // day. Its day labels are native internal widgets (documented CSS
        // class: day-number), so add the fallback only to those exact hit
        // targets. Header labels, weekday names and whitespace cannot commit.
        for day_label in descendants(&calendar.clone().upcast())
            .into_iter()
            .filter(|widget| widget.has_css_class("day-number"))
        {
            let click = gtk::GestureClick::new();
            click.set_button(1);
            let entry = entry.downgrade();
            let calendar = calendar.downgrade();
            let popover = popover.downgrade();
            click.connect_released(move |_, _, _, _| {
                let (Some(entry), Some(calendar), Some(popover)) =
                    (entry.upgrade(), calendar.upgrade(), popover.upgrade())
                else {
                    return;
                };
                if popover.is_visible() {
                    commit(&entry, &popover, &calendar.date());
                }
            });
            day_label.add_controller(click);
        }

        // day-selected also fires during navigation/initialization, so it is
        // deliberately not a commit signal. Keyboard browsing stays provisional
        // until Enter/Space on the calendar itself (not its header buttons).
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        {
            let entry = entry.downgrade();
            let calendar = calendar.downgrade();
            let popover = popover.downgrade();
            keys.connect_key_pressed(move |_, key, _, modifiers| {
                if !matches!(
                    key,
                    gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter | gtk::gdk::Key::space
                ) || modifiers.intersects(
                    gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::ALT_MASK,
                ) {
                    return glib::Propagation::Proceed;
                }
                if let (Some(entry), Some(calendar), Some(popover)) =
                    (entry.upgrade(), calendar.upgrade(), popover.upgrade())
                {
                    if calendar.has_focus() && popover.is_visible() {
                        commit(&entry, &popover, &calendar.date());
                        return glib::Propagation::Stop;
                    }
                }
                glib::Propagation::Proceed
            });
        }
        calendar.add_controller(keys);

        {
            let entry = entry.downgrade();
            let popover = popover.downgrade();
            today.connect_clicked(move |_| {
                if let (Some(entry), Some(popover), Ok(now)) = (
                    entry.upgrade(),
                    popover.upgrade(),
                    glib::DateTime::now_local(),
                ) {
                    commit(&entry, &popover, &now);
                }
            });
        }

        Self { row, entry }
    }
}

fn parse_date(text: &str) -> Option<glib::DateTime> {
    let bytes = text.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return None;
    }
    let year = text[0..4].parse().ok()?;
    let month = text[5..7].parse().ok()?;
    let day = text[8..10].parse().ok()?;
    glib::DateTime::from_local(year, month, day, 12, 0, 0.0).ok()
}

fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut result = Vec::new();
    let mut child = root.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        result.extend(descendants(&widget));
        result.push(widget);
    }
    result
}

fn commit(entry: &gtk::Entry, popover: &gtk::Popover, date: &glib::DateTime) {
    let (year, month, day) = date.ymd();
    entry.set_text(&format!("{year:04}-{month:02}-{day:02}"));
    popover.popdown();
}

#[cfg(test)]
mod tests {
    use super::parse_date;

    #[test]
    fn calendar_initialization_requires_a_real_iso_date() {
        for text in [
            "",
            "202",
            "+001-01-01",
            "2026-2-01",
            "2026-02-29",
            "2024-02-30",
            "0000-01-01",
            "абвг-01-01",
        ] {
            assert!(parse_date(text).is_none(), "{text}");
        }
        assert_eq!(parse_date("2024-02-29").unwrap().ymd(), (2024, 2, 29));
    }
}
