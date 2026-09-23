import GObject from 'gi://GObject';
import * as Calendar from 'resource:///org/gnome/shell/ui/calendar.js';

// Keep Shell-specific adaptation here: its Calendar changes its selected date
// while browsing months, and EmptyEventSource disables day activation. Reuse
// its grid, locale/week settings and navigation without connecting event data.
export const DateCalendar = GObject.registerClass(
class DateCalendar extends Calendar.Calendar {
    _init(onChoose) {
        super._init();
        this._onChoose = onChoose;
        this.setEventSource(new Calendar.EmptyEventSource());
        this.connect('destroy', () => {
            this._onChoose = null;
            this._eventSource.destroy();
            this._settings.run_dispose();
        });
    }

    _rebuildCalendar() {
        super._rebuildCalendar();
        for (const button of this._buttons) button.reactive = true;
    }

    setDate(date) {
        // Shell sets this flag only around explicit day-button activation,
        // not month navigation or initialization. Even today's selected day
        // must commit when clicked (the base signal omits that case).
        const chosen = this._shouldDateGrabFocus;
        super.setDate(date);
        if (chosen) this._onChoose?.(date);
    }
});
