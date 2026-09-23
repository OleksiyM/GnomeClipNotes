//! Real pointer regression tests in the private headless GNOME session.
use crate::{smoke::find, ui, State};
use adw::prelude::*;
use std::{path::PathBuf, rc::Rc, time::Duration};

async fn settle() {
    glib::timeout_future(Duration::from_millis(200)).await;
}

async fn click(window: &adw::ApplicationWindow, widget: &impl IsA<gtk::Widget>) {
    let native = widget.as_ref().native().unwrap();
    let bounds = widget
        .as_ref()
        .compute_bounds(&native.clone().upcast::<gtk::Widget>())
        .expect("Widget bounds");
    let (dx, dy) = native.surface_transform();
    let mut x = f64::from(bounds.x() + bounds.width() / 2.0) + dx;
    let mut y = f64::from(bounds.y() + bounds.height() / 2.0) + dy;
    let mut surface = native.surface().unwrap();
    while let Some(popup) = surface.downcast_ref::<gtk::gdk::Popup>() {
        x += f64::from(popup.position_x());
        y += f64::from(popup.position_y());
        surface = popup.parent().unwrap();
    }
    assert_eq!(Some(surface), window.surface());
    let (dx, dy) = window.surface_transform();
    pointer(window, (x - dx) as f32, (y - dy) as f32).await;
}

async fn pointer(window: &adw::ApplicationWindow, x: f32, y: f32) {
    let bus = window.application().unwrap().dbus_connection().unwrap();
    let point = serde_json::json!({"x":x,"y":y}).to_string();
    bus.call_future(
        Some("org.gnome.Shell"),
        "/org/example/ClipNotesTestDriver",
        "org.example.ClipNotesTestDriver",
        "FilterClick",
        Some(&(point,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        3000,
    )
    .await
    .unwrap();
    settle().await;
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

pub fn run(state: &Rc<State>) {
    let dir =
        PathBuf::from(std::env::var("GCN_SHELL_TEST_DIR").expect("Private shell test required"));
    assert!(dir
        .to_string_lossy()
        .starts_with("/tmp/gnome-clip-notes-shell."));
    assert_eq!(
        crate::store::xdg_path("XDG_DATA_HOME", ".local/share"),
        dir.join("data")
    );
    let failure = dir.join("filters-failed");
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::write(&failure, info.to_string());
        previous(info);
        std::process::exit(1);
    }));
    for text in ["Filter test plain text", "https://example.org/filter-test"] {
        state
            .store
            .borrow_mut()
            .capture(text, "Filter test", "", false)
            .unwrap();
    }
    ui::show(state);
    let state = state.clone();
    glib::MainContext::default().spawn_local(async move {
        let window = state.window.borrow().as_ref().unwrap().clone();
        window.set_title(Some("ClipNotes filters test"));
        window.present();
        // Wait for the compositor's initial configure/map before screen clicks.
        glib::timeout_future(Duration::from_millis(900)).await;
        let named = |name: &str| find(window.upcast_ref(), &|w| w.widget_name() == name).unwrap();
        let button = named("library-filters")
            .downcast::<gtk::MenuButton>()
            .unwrap();
        let popover = button.popover().unwrap();
        let kind = named("library-filter-kind")
            .downcast::<gtk::DropDown>()
            .unwrap();
        let dates = named("library-filter-dates")
            .downcast::<gtk::DropDown>()
            .unwrap();
        let source = named("library-filter-source")
            .downcast::<gtk::DropDown>()
            .unwrap();
        let clear = named("library-clear-filters")
            .downcast::<gtk::Button>()
            .unwrap();
        let from = named("library-filter-from")
            .downcast::<gtk::Entry>()
            .unwrap();
        let until = named("library-filter-until")
            .downcast::<gtk::Entry>()
            .unwrap();
        let from_calendar_button = named("library-filter-from-calendar-button")
            .downcast::<gtk::MenuButton>()
            .unwrap();
        let from_calendar = named("library-filter-from-calendar")
            .downcast::<gtk::Calendar>()
            .unwrap();
        let from_calendar_popover = named("library-filter-from-calendar-popover")
            .downcast::<gtk::Popover>()
            .unwrap();
        let from_today = named("library-filter-from-today")
            .downcast::<gtk::Button>()
            .unwrap();
        let hint = named("library-filter-date-hint");
        let list = named("library-cards");
        let ids = || {
            let mut result = vec![];
            let mut child = list.first_child();
            while let Some(row) = child {
                child = row.next_sibling();
                result.push(row.widget_name().to_string());
            }
            result
        };
        for (choose, outside_x) in [(false, 40.0), (true, 40.0), (true, -60.0)] {
            kind.set_selected(0);
            click(&window, &button).await;
            if !popover.is_visible() {
                window
                    .application()
                    .unwrap()
                    .dbus_connection()
                    .unwrap()
                    .call_future(
                        Some("org.gnome.Shell"),
                        "/org/example/ClipNotesTestDriver",
                        "org.example.ClipNotesTestDriver",
                        "Screenshot",
                        Some(
                            &(dir.join("filters-click-failed.png").to_str().unwrap(),).to_variant(),
                        ),
                        None,
                        gio::DBusCallFlags::NONE,
                        3000,
                    )
                    .await
                    .unwrap();
            }
            assert!(popover.is_visible());
            if choose {
                click(&window, &kind).await;
                window
                    .application()
                    .unwrap()
                    .dbus_connection()
                    .unwrap()
                    .call_future(
                        Some("org.gnome.Shell"),
                        "/org/example/ClipNotesTestDriver",
                        "org.example.ClipNotesTestDriver",
                        "Screenshot",
                        Some(&(dir.join("filters-nested.png").to_str().unwrap(),).to_variant()),
                        None,
                        gio::DBusCallFlags::NONE,
                        3000,
                    )
                    .await
                    .unwrap();
                let nested = find(kind.upcast_ref(), &|w| {
                    w.is::<gtk::Popover>() && w.is_visible()
                })
                .expect("Nested type chooser");
                let text_row = find(&nested, &|w| {
                    w.is_mapped()
                        && w.downcast_ref::<gtk::Label>()
                            .is_some_and(|l| l.text() == "Text")
                })
                .unwrap();
                click(&window, &text_row).await;
                assert_eq!(kind.selected(), 1);
                assert!(
                    popover.is_visible(),
                    "Choosing a filter keeps its panel open"
                );
                assert_eq!(button.label().as_deref(), Some("Filters (1)"));
            }
            pointer(&window, outside_x, window.height() as f32 - 40.0).await;
            assert!(
                !popover.is_visible(),
                "Outside click closes filters (after nested selection: {choose})"
            );
        }
        click(&window, &button).await;
        click(&window, &clear).await;
        assert_eq!(kind.selected(), 0);
        assert_eq!(source.selected(), 0);
        assert_eq!(button.label().as_deref(), Some("Filters"));
        assert!(!clear.is_sensitive());
        let before = ids();
        assert!(!before.is_empty());
        dates.set_selected(5);

        // Opening only initializes the view; neither valid/invalid input nor
        // the currently applied result set is touched. Calendar navigation is
        // likewise provisional until a day is explicitly chosen.
        from.set_text("202");
        let before_calendar = ids();
        settle().await;
        click(&window, &from_calendar_button).await;
        assert!(from_calendar_popover.is_visible());
        window
            .application()
            .unwrap()
            .dbus_connection()
            .unwrap()
            .call_future(
                Some("org.gnome.Shell"),
                "/org/example/ClipNotesTestDriver",
                "org.example.ClipNotesTestDriver",
                "Screenshot",
                Some(&(dir.join("calendar-picker.png").to_str().unwrap(),).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                3000,
            )
            .await
            .unwrap();
        assert_eq!(from.text().as_str(), "202");
        assert_eq!(ids(), before_calendar);
        let local_today = glib::DateTime::now_local().unwrap();
        assert_eq!(from_calendar.date().ymd(), local_today.ymd());

        let calendar_children = descendants(from_calendar.upcast_ref());
        let weekday = calendar_children
            .iter()
            .find(|widget| widget.has_css_class("day-name"))
            .expect("GtkCalendar weekday label");
        click(&window, weekday).await;
        assert_eq!(from.text().as_str(), "202");
        assert_eq!(ids(), before_calendar);
        assert!(from_calendar_popover.is_visible(), "Weekday label is not a date");
        let year_label = calendar_children
            .iter()
            .find(|widget| widget.has_css_class("year"))
            .expect("GtkCalendar year label");
        click(&window, year_label).await;
        assert_eq!(from.text().as_str(), "202");
        assert!(from_calendar_popover.is_visible(), "Header label is not a date");

        // Exercise a real header button from January 31. GTK clamps the
        // selected day while moving to February; that adjustment must not be
        // mistaken for an explicit day choice.
        from_calendar_popover.popdown();
        from.set_text("2025-01-31");
        settle().await;
        let next_buttons: Vec<_> = calendar_children
            .iter()
            .filter(|widget| {
                widget.downcast_ref::<gtk::Button>().is_some()
                    && descendants(widget).iter().any(|child| {
                        child.downcast_ref::<gtk::Image>().is_some_and(|image| {
                            image.icon_name().is_some_and(|name| name.contains("end"))
                        })
                    })
            })
            .cloned()
            .collect();
        assert!(!next_buttons.is_empty(), "GtkCalendar forward header buttons");
        let mut reached_february = false;
        for header_button in next_buttons {
            if !from_calendar_popover.is_visible() {
                click(&window, &from_calendar_button).await;
            }
            click(&window, &header_button).await;
            assert_eq!(from.text().as_str(), "2025-01-31");
            assert!(from_calendar_popover.is_visible(), "Navigation keeps calendar open");
            if from_calendar.date().ymd().1 == 2 {
                reached_february = true;
                break;
            }
            from_calendar_popover.popdown();
        }
        assert!(reached_february, "Clicked GtkCalendar next-month button");

        let day_15 = descendants(from_calendar.upcast_ref())
            .into_iter()
            .find(|widget| {
                widget.has_css_class("day-number")
                    && !widget.has_css_class("other-month")
                    && widget
                        .downcast_ref::<gtk::Label>()
                        .is_some_and(|label| label.text() == "15")
            })
            .expect("February 15 day cell");
        click(&window, &day_15).await;
        assert_eq!(from.text().as_str(), "2025-02-15");
        assert!(!from_calendar_popover.is_visible());
        assert!(popover.is_visible(), "Date selection keeps filters open");

        // A real click on the already-selected day must commit too, even when
        // GtkCalendar omits day-selected for an unchanged selection.
        click(&window, &from_calendar_button).await;
        let selected_day = descendants(from_calendar.upcast_ref())
            .into_iter()
            .find(|widget| {
                widget.has_css_class("day-number")
                    && widget.state_flags().contains(gtk::StateFlags::SELECTED)
            })
            .expect("Selected GtkCalendar day cell");
        click(&window, &selected_day).await;
        assert_eq!(from.text().as_str(), "2025-02-15");
        assert!(!from_calendar_popover.is_visible());
        assert!(popover.is_visible(), "Repeated date selection keeps filters open");

        click(&window, &from_calendar_button).await;
        assert!(from_calendar.grab_focus());
        let before_keyboard = from.text();
        for key in ["right", "enter"] {
            window.application().unwrap().dbus_connection().unwrap().call_future(
                Some("org.gnome.Shell"), "/org/example/ClipNotesTestDriver",
                "org.example.ClipNotesTestDriver", "Key", Some(&(key,).to_variant()),
                None, gio::DBusCallFlags::NONE, 3000,
            ).await.unwrap();
            settle().await;
            if key == "right" {
                assert_eq!(from.text(), before_keyboard, "Keyboard browsing is provisional");
                assert!(from_calendar_popover.is_visible());
            }
        }
        let (year, month, day) = from_calendar.date().ymd();
        assert_eq!(from.text().as_str(), format!("{year:04}-{month:02}-{day:02}"));
        assert!(!from_calendar_popover.is_visible(), "Enter confirms calendar day");

        click(&window, &from_calendar_button).await;
        click(&window, &from_today).await;
        let (year, month, day) = glib::DateTime::now_local().unwrap().ymd();
        assert_eq!(from.text().as_str(), format!("{year:04}-{month:02}-{day:02}"));
        assert!(!from_calendar_popover.is_visible());
        assert!(popover.is_visible(), "Today keeps filters open");

        click(&window, &from_calendar_button).await;
        assert!(from_calendar_popover.is_visible());
        pointer(&window, 40.0, window.height() as f32 - 40.0).await;
        assert!(!from_calendar_popover.is_visible());
        assert!(!popover.is_visible(), "Outside click dismisses nested calendar and filters");
        click(&window, &button).await;

        from.set_text("202");
        assert_eq!(ids(), before, "Partial dates preserve the previous results");
        assert!(hint.is_visible());
        from.set_text("2000-01-01");
        until.set_text("2099-12-31");
        assert!(!hint.is_visible());
        assert_eq!(button.label().as_deref(), Some("Filters (1)"));
        let valid = ids();
        until.set_text("1999-01-01");
        assert!(hint.is_visible());
        assert_eq!(ids(), valid, "Reversed range keeps last applied dates");
        let search = find(window.upcast_ref(), &|w| w.is::<gtk::SearchEntry>())
            .unwrap()
            .downcast::<gtk::SearchEntry>()
            .unwrap();
        search.set_text("Filter test plain text");
        settle().await;
        click(&window, &clear).await;
        assert_eq!(
            search.text().as_str(),
            "Filter test plain text",
            "Clear filters keeps search text"
        );
        assert!(from.text().is_empty() && until.text().is_empty());
        assert_eq!(dates.selected(), 0);
        assert!(!hint.is_visible());
        window
            .application()
            .unwrap()
            .dbus_connection()
            .unwrap()
            .call_future(
                Some("org.gnome.Shell"),
                "/org/example/ClipNotesTestDriver",
                "org.example.ClipNotesTestDriver",
                "Key",
                Some(&("escape",).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                3000,
            )
            .await
            .unwrap();
        settle().await;
        assert!(
            !popover.is_visible(),
            "Escape dismisses without resetting filters"
        );
        window.close();
        std::fs::write(dir.join("filters-passed"), "FILTERS_OK\n").unwrap();
        println!("PASS live filters, native calendar, nested selection/outside click, reset and incomplete dates");
    });
}
