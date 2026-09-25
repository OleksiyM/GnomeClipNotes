import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Meta from 'gi://Meta';
import Pango from 'gi://Pango';
import Shell from 'gi://Shell';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import {gettext as _, trf, setCatalog, resetCatalog} from './i18n.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {customDateRange, localDate, formatLocalDate} from './dateRange.js';
import {DateCalendar} from './datePicker.js';
import {textFormats} from './clipboardFormats.js';

const SERVICE = 'io.github.OleksiyM.GnomeClipNotes';
const SERVICE_PATH = '/io/github/OleksiyM/GnomeClipNotes';
const SERVICE_IFACE = 'io.github.OleksiyM.GnomeClipNotes.Service';
const MAX_BYTES = 1024 * 1024;

const SERVICE_XML = `<node><interface name="${SERVICE_IFACE}">
 <method name="Capture"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="b" direction="in"/><arg type="b" direction="out"/></method>
 <method name="Query"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
 <method name="GetItem"><arg type="x" direction="in"/><arg type="s" direction="out"/></method>
 <method name="Activate"><arg type="s" direction="in"/><arg type="x" direction="in"/></method>
 <method name="Pause"><arg type="u" direction="in"/></method>
 <signal name="Changed"/><signal name="PasteRequested"><arg type="s"/><arg type="b"/></signal>
</interface></node>`;

const BRIDGE_XML = `<node><interface name="io.github.OleksiyM.GnomeClipNotes.Bridge">
 <method name="Paste"><arg type="s" direction="in"/><arg type="b" direction="in"/></method>
 <method name="Toggle"/><method name="GetStatus"><arg type="s" direction="out"/></method>
</interface></node>`;

const ServiceProxy = Gio.DBusProxy.makeProxyWrapper(SERVICE_XML);

function busCall(destination, path, iface, method, parameters, replyType, cancellable = null) {
    return new Promise((resolve, reject) => {
        Gio.DBus.session.call(destination, path, iface, method, parameters,
            new GLib.VariantType(replyType), Gio.DBusCallFlags.NO_AUTO_START, 5000,
            cancellable, (connection, result) => {
                try { resolve(connection.call_finish(result).deepUnpack()); }
                catch (error) { reject(error); }
            });
    });
}

const ITEM_SHORTCUTS = {paste: 'Enter', view: 'F3', edit: 'F4', info: 'Alt+Enter', rename: 'F2', delete: 'F8 / Del'};

function ellipsize(text, limit) {
    const clean = String(text ?? '').replace(/\s+/g, ' ').trim();
    return clean.length <= limit ? clean : `${clean.slice(0, limit - 1)}…`;
}

function clippedLabel(text, styleClass) {
    const label = new St.Label({text, style_class: styleClass, x_expand: true});
    label.clutter_text.ellipsize = Pango.EllipsizeMode.END;
    label.clutter_text.single_line_mode = true;
    return label;
}

class Overlay {
    constructor(extension, service, previous = null) {
        this._extension = extension;
        this._service = service;
        this._menuManager = new PopupMenu.PopupMenuManager(this);
        this._items = [];
        this._pageSize = 6;
        this._selected = 0;
        this._query = {search: '', group_id: 0, kind: '', source: '', since: 0, until: 0, limit: 9, offset: 0};
        if (previous) this._query = {...previous._query};
        this._customDates = previous?._customDates ? {...previous._customDates} : null;
        this._actor = new St.BoxLayout({vertical: true, style_class: 'popup-menu-content gcn-overlay', reactive: true});
        this._actor.connect('destroy',()=>{this._actorDestroyed=true;});
        this._keyId = this._actor.connect('captured-event', (_a, event) => {
            if (!this.visible || this._context?.isOpen || event.type() !== Clutter.EventType.KEY_PRESS)
                return Clutter.EVENT_PROPAGATE;
            const focus = global.stage.get_key_focus();
            if (!focus || !this._actor.contains(focus)) return Clutter.EVENT_PROPAGATE;
            return this._onKey(event);
        });
        this._interfaceSettings = new Gio.Settings({schema_id: 'org.gnome.desktop.interface'});
        this._themeId = this._interfaceSettings.connect('changed::color-scheme', () => this._syncTheme());
        this._syncTheme();

        const toolbar = new St.BoxLayout({style_class: 'gcn-toolbar', x_expand: true});
        this._toolbar = toolbar;
        this._search = new St.Entry({text: this._query.search, hint_text: _('Search clipboard and notes'), style_class: 'gcn-search', x_expand: true, can_focus: true});
        this._search.get_clutter_text().connect('text-changed', () => {
            this._query.search = this._search.get_text();
            this._query.offset = 0;
            this._scheduleRefresh();
        });
        // Clutter.Text consumes Return to emit activate, so it never reaches
        // the overlay's bubbling key-press handler while search has focus.
        this._search.get_clutter_text().connect('activate', () => this._pasteIndex(this._selected));
        toolbar.add_child(this._search);
        this._kindButton = this._cycleButton(_('All types'), [[_('All types'), ''], [_('Text'), 'text'], [_('Links'), 'link']], value => { this._query.kind = value; this._query.offset = 0; this.refresh(); }, null, this._query.kind);
        toolbar.add_child(this._kindButton);
        this._sourceOptions = [[_('All apps'), '']];
        if (this._query.source) this._sourceOptions.push([this._query.source, this._query.source]);
        this._sourceButton = this._cycleButton(_('All apps'), this._sourceOptions, source => { this._query.source = source; this._query.offset = 0; this.refresh(); }, null, this._query.source);
        toolbar.add_child(this._sourceButton);
        this._dateButton = this._cycleButton(_('Any time'), [[_('Any time'), 'any'], [_('Today'), 'today'], [_('Yesterday'), 'yesterday'], [_('This week'), 'week'], [_('Last 30 days'), 'month'], [_('Custom…'), 'custom']], range => {
            const now = new Date();
            const localDay = offset => Math.floor(new Date(now.getFullYear(), now.getMonth(), now.getDate() + offset).getTime() / 1000);
            const midnight = localDay(0);
            const mondayOffset = (now.getDay() + 6) % 7;
            this._query.since = range === 'today' ? midnight : range === 'yesterday' ? localDay(-1) : range === 'week' ? localDay(-mondayOffset) : range === 'month' ? Math.floor(Date.now() / 1000) - 30 * 86400 : 0;
            this._query.until = range === 'today' ? localDay(1) - 1 : range === 'yesterday' ? midnight - 1 : 0;
            this._query.offset = 0;
            this.refresh();
        }, (menu, select) => this._customDateItem(menu, select), previous?._dateButton?._selectedValue ?? 'any');
        toolbar.add_child(this._dateButton);
        this._groups = new St.BoxLayout({style_class: 'gcn-groups', y_align: Clutter.ActorAlign.CENTER});
        toolbar.add_child(this._groups);
        toolbar.add_child(this._iconButton('document-new-symbolic', _('New Note'), () => this._activateApp('new-note', 0)));
        toolbar.add_child(this._iconButton('emblem-system-symbolic', _('Settings'), () => this._activateApp('settings', 0)));
        toolbar.add_child(this._iconButton('window-close-symbolic', _('Close'), () => this.hide()));
        this._actor.add_child(toolbar);
        // The cards own remaining height, including loading/empty states.
        this._cards = new St.BoxLayout({style_class: 'gcn-cards', x_expand: true, y_expand: true});
        this._actor.add_child(this._cards);
        const paging = new St.BoxLayout({style_class: 'gcn-paging', y_align: Clutter.ActorAlign.CENTER});
        this._pageLabel = new St.Label({style_class: 'gcn-hint', y_align: Clutter.ActorAlign.CENTER});
        paging.add_child(this._pageLabel);
        const previousButton = this._iconButton('go-previous-symbolic', _('Previous Items'), () => this._changePage(-1));
        const next = this._iconButton('go-next-symbolic', _('Next Items'), () => this._changePage(1));
        this._previousButton = previousButton;
        this._nextButton = next;
        paging.add_child(previousButton); paging.add_child(next);
        toolbar.insert_child_at_index(paging, toolbar.get_n_children() - 1);
        Main.layoutManager.addChrome(this._actor, {affectsStruts: false, trackFullscreen: false});
        this._actor.hide();
    }

    _syncTheme() {
        const theme = this._settings?.theme ?? this._extension._runtimeSettings?.theme ?? 'system';
        const dark = theme === 'dark' || (theme === 'system' &&
            this._interfaceSettings.get_string('color-scheme') === 'prefer-dark');
        this._themeClass = dark ? 'gcn-dark' : 'gcn-light';
        for (const actor of [this._actor, this._context?.actor, this._datePicker?.actor].filter(Boolean)) {
            actor.remove_style_class_name(dark ? 'gcn-light' : 'gcn-dark');
            actor.add_style_class_name(this._themeClass);
        }
    }

    _cycleButton(initial, values, changed, customItem = null, selected = values[0][1]) {
        const button = new St.Button({style_class: 'button gcn-filter', can_focus: true});
        button._selectedValue = selected;
        const content = new St.BoxLayout({style_class: 'gcn-filter-content'});
        const label = clippedLabel(selected === 'custom' ? _('Custom range') : values.find(option => option[1] === selected)?.[0] ?? initial, '');
        content.add_child(label);
        content.add_child(new St.Icon({icon_name: 'pan-down-symbolic', icon_size: 12}));
        button.set_child(content);
        button._setSelectedValue = value => {
            selected = value;
            button._selectedValue = value;
            label.text = values.find(option => option[1] === value)?.[0] ?? initial;
        };
        button.connect('clicked', () => {
            if (button === this._sourceButton) {
                this._openSourceMenu();
                return;
            }
            this._closeContext();
            const menu = new PopupMenu.PopupMenu(button, 0.5, St.Side.BOTTOM);
            this._context = menu;
            Main.uiGroup.add_child(menu.actor);
            for (const [title, value] of values) {
                if (value === 'custom' && customItem) {
                    const item = customItem(menu, () => {
                        selected = 'custom';
                        button._selectedValue = selected;
                        label.text = _('Custom range');
                    });
                    if (value === selected) item.setOrnament(PopupMenu.Ornament.DOT);
                    continue;
                }
                const item = menu.addAction(title, () => {
                    selected = value;
                    button._selectedValue = selected;
                    label.text = title;
                    changed(value);
                });
                if (value === selected)
                    item.setOrnament(PopupMenu.Ornament.DOT);
            }
            this._watchContext(menu);
            menu.open();
        });
        return button;
    }

    _openSourceMenu() {
        this._closeContext();
        const button = this._sourceButton;
        const menu = new PopupMenu.PopupMenu(button, 0.5, St.Side.BOTTOM);
        this._context = menu;
        Main.uiGroup.add_child(menu.actor);
        const section = new PopupMenu.PopupMenuSection();
        const scroll = new St.ScrollView({overlay_scrollbars: false});
        scroll.set_policy(St.PolicyType.NEVER, St.PolicyType.AUTOMATIC);
        scroll.set_child(section.box);
        section.actor = scroll;
        scroll._delegate = section;
        menu.addMenuItem(section);
        menu._sourceSection = section;
        menu._sourceScroll = scroll;
        const monitor = Main.layoutManager.findMonitorForActor(button);
        const area = Main.layoutManager.getWorkAreaForMonitor(monitor?.index ?? Main.layoutManager.primaryIndex);
        const scale = St.ThemeContext.get_for_stage(global.stage).scale_factor;
        const [, y] = button.get_transformed_position();
        // Reserve room for the fixed reset row, menu padding/arrow and a clear
        // margin below the work area's top. CSS lengths are logical pixels.
        const height = Math.max(48, Math.min(420, (y - area.y) / scale - 120));
        scroll.set_style(`max-height: ${height}px; max-width: 320px;`);
        const select = value => {
            this._query.source = value;
            this._query.offset = 0;
            button._setSelectedValue(value);
            this.refresh();
        };
        for (const [title, value] of this._sourceOptions.slice(1)) {
            const item = section.addAction(title, () => select(value));
            item.label.clutter_text.ellipsize = Pango.EllipsizeMode.END;
            if (value === this._query.source) item.setOrnament(PopupMenu.Ornament.DOT);
            item.connect('key-focus-in', () => {
                const adjustment = scroll.vadjustment ?? scroll.vscroll.adjustment;
                const box = item.get_allocation_box();
                if (box.y1 < adjustment.value) adjustment.value = box.y1;
                else if (box.y2 > adjustment.value + adjustment.page_size)
                    adjustment.value = box.y2 - adjustment.page_size;
            });
        }
        if (section.numMenuItems) menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        menu._sourceReset = menu.addAction(_('All apps'), () => select(''));
        if (!this._query.source) menu._sourceReset.setOrnament(PopupMenu.Ornament.DOT);
        this._watchContext(menu);
        menu.open();
    }

    _customDateItem(menu, select) {
        const item = new PopupMenu.PopupSubMenuMenuItem(_('Custom…'));
        menu.addMenuItem(item);
        const row = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        const form = new St.BoxLayout({vertical: true, style_class: 'gcn-date-form', x_expand: true});
        row.add_child(form);
        item.menu.addMenuItem(row);
        const field = (title, text) => {
            const label = new St.Label({text: title});
            const entry = new St.Entry({text, hint_text: _('YYYY-MM-DD'), can_focus: true,
                style_class: 'gcn-date-entry', x_expand: true, accessible_name: title});
            entry.label_actor = label;
            const calendarButton = new St.Button({can_focus: true, accessible_name: _('Choose date'),
                child: new St.Icon({icon_name: 'x-office-calendar-symbolic', icon_size: 16})});
            entry.set_secondary_icon(calendarButton);
            calendarButton.connect('clicked', () => this._openDatePicker(calendarButton, entry));
            entry._calendarButton = calendarButton;
            form.add_child(label);
            form.add_child(entry);
            return entry;
        };
        const from = field(_('From'), this._customDates?.from ?? '');
        const to = field(_('To'), this._customDates?.to ?? '');
        const hint = new St.Label({style_class: 'gcn-date-hint'});
        hint.clutter_text.line_wrap = true;
        hint.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        form.add_child(hint);
        const update = (apply = true) => {
            const start = from.get_text();
            const end = to.get_text();
            this._customDates = {from: start, to: end};
            const range = customDateRange(start, end);
            hint.text = range.error === 'reversed'
                ? _('From must not be after To. Previous filter stays active.')
                : range.error
                    ? _('Enter both dates as YYYY-MM-DD. Previous filter stays active.')
                    : _('Both dates included. Changes apply immediately.');
            if (!range.error && apply) {
                this._query.since = range.since;
                this._query.until = range.until;
                this._query.offset = 0;
                select();
                item.setOrnament(PopupMenu.Ornament.DOT);
                for (const other of menu._getMenuItems()) {
                    if (other !== item) other.setOrnament(PopupMenu.Ornament.NONE);
                }
                this.refresh();
            }
            return !range.error;
        };
        from.clutter_text.connect('text-changed', () => update());
        to.clutter_text.connect('text-changed', () => update());
        from.clutter_text.connect('activate', () => to.clutter_text.grab_key_focus());
        to.clutter_text.connect('activate', () => { if (update()) menu.close(); });
        let focusId = 0;
        const cancelFocus = () => {
            if (focusId) GLib.source_remove(focusId);
            focusId = 0;
        };
        from.connect('notify::mapped', () => {
            cancelFocus();
            if (from.mapped) focusId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 260, () => {
                focusId = 0;
                const focus = global.stage.get_key_focus();
                // Wait for submenu allocation/input focus, and don't steal
                // focus if the user has already chosen another field.
                if (this._context === menu && item.menu.isOpen &&
                    (!focus || focus === item || focus === item.menu.actor))
                    from.clutter_text.grab_key_focus();
                return GLib.SOURCE_REMOVE;
            });
        });
        from.connect('destroy', cancelFocus);
        item.menu.connect('open-state-changed', (_menu, open) => {
            if (open) update();
        });
        // Only entering Custom applies its stored range, not opening the
        // preset menu while another date filter is selected.
        update(false);
        menu._dateFields = {from, to, hint};
        return item;
    }

    _iconButton(icon, name, action) {
        const button = new St.Button({style_class: 'button gcn-icon-button', can_focus: true, accessible_name: name,
            child: new St.Icon({icon_name: icon, icon_size: 16})});
        button.connect('notify::hover', () => {
            if (button.hover) this._queueTooltip(button, name);
            else if (this._tooltipButton === button) this._hideTooltip();
        });
        button.connect('key-focus-in', () => this._queueTooltip(button, name));
        button.connect('key-focus-out', () => {
            if (this._tooltipButton === button) this._hideTooltip();
        });
        button.connect('destroy', () => {
            if (this._tooltipButton === button) this._hideTooltip();
        });
        button.connect('clicked', () => { this._hideTooltip(); action(); });
        return button;
    }

    _openDatePicker(button, entry) {
        if (this._datePicker?.sourceActor === button) { this._datePicker.close(); return; }
        this._datePicker?.close();
        const parent = this._context;
        if (!parent?.isOpen) return;
        const menu = new PopupMenu.PopupMenu(button, 0.5, St.Side.TOP);
        this._datePicker = menu;
        menu.actor.add_style_class_name(this._themeClass);
        menu.actor.add_style_class_name('gcn-date-picker');
        Main.uiGroup.add_child(menu.actor);
        const commit = date => {
            entry.set_text(formatLocalDate(date));
            menu.close();
        };
        const calendar = new DateCalendar(commit);
        calendar.setDate(localDate(entry.get_text().trim()) ?? new Date());
        const row = new PopupMenu.PopupBaseMenuItem({reactive: false, can_focus: false});
        row.add_child(calendar);
        menu.addMenuItem(row);
        menu.addAction(_('Today'), () => commit(new Date()));
        // Independent nested grab: sharing the parent's menu manager would
        // close the From/To form as soon as this calendar opens.
        const manager = new PopupMenu.PopupMenuManager(this);
        menu.actor.connect('captured-event', (_actor, event) => {
            if ([Clutter.EventType.BUTTON_PRESS, Clutter.EventType.TOUCH_BEGIN].includes(event.type()) &&
                !menu.actor.contains(global.stage.get_event_actor(event))) {
                menu.close();
                return Clutter.EVENT_STOP;
            }
            return Clutter.EVENT_PROPAGATE;
        });
        manager.addMenu(menu);
        const parentId = parent.connect('open-state-changed', (_parent, open) => { if (!open) menu.close(); });
        menu.connect('open-state-changed', (_menu, open) => {
            if (!open) {
                parent.disconnect(parentId);
                if (this._datePicker === menu) this._datePicker = null;
                if (parent.isOpen && button.mapped) button.grab_key_focus();
                this._destroyMenuSoon(menu);
            }
        });
        menu._calendar = calendar;
        menu.open();
    }

    _queueTooltip(button, text) {
        this._hideTooltip();
        if (!this.visible || this._context?.isOpen) return;
        this._tooltipButton = button;
        this._tooltipId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 500, () => {
            this._tooltipId = 0;
            if (!this.visible || !button.mapped || this._context?.isOpen) {
                this._hideTooltip();
                return GLib.SOURCE_REMOVE;
            }
            // Use the same label surface as Shell's dash; it must not take
            // focus, intercept pointer events or change toolbar allocation.
            const label = new St.Label({text, style_class: 'dash-label', reactive: false});
            this._tooltip = label;
            Main.uiGroup.add_child(label);
            const [x, y] = button.get_transformed_position();
            const [width] = button.get_transformed_size();
            const monitor = Main.layoutManager.findMonitorForActor(button) ?? Main.layoutManager.primaryMonitor;
            const gap = label.get_theme_node().get_length('-y-offset');
            label.set_position(
                Math.max(monitor.x, Math.min(x + (width - label.width) / 2, monitor.x + monitor.width - label.width)),
                Math.max(monitor.y, y - label.height - gap));
            return GLib.SOURCE_REMOVE;
        });
    }

    _hideTooltip() {
        if (this._tooltipId) GLib.source_remove(this._tooltipId);
        this._tooltipId = 0;
        this._tooltipButton = null;
        this._tooltip?.destroy();
        this._tooltip = null;
    }

    get visible() { return !this._actorDestroyed && this._actor.visible; }

    async show() {
        if (this.visible || this._opening || this._extension._isLocked()) return;
        const request=this._openRequest=(this._openRequest??0)+1;
        this._opening=true;
        try {
            if (!await this._extension.ensureService() || request!==this._openRequest || this._destroyed) return;
            const localized = this._extension._refreshLocalizedOverlay();
            if (localized !== this) return localized.show();
            this._lastError = null;
            this._items = [];
            this._selected = 0;
            this._renderError(_('Loading…'));
            this._previousWindow = this._extension._lastTargetWindow;
            const monitor = Main.layoutManager.currentMonitor ?? Main.layoutManager.primaryMonitor;
            const scale = St.ThemeContext.get_for_stage(global.stage).scale_factor;
            const width = monitor.width - 32 * scale;
            const height = Math.min(340 * scale, monitor.height - Main.panel.height - 24 * scale);
            this._pageSize = Math.min(9, Math.max(1, Math.floor((width - 36 * scale) / (208 * scale))));
            this._query.limit = this._pageSize;
            this._query.offset = 0;
            this._actor.set_position(monitor.x + 16 * scale, monitor.y + monitor.height - height - 12 * scale);
            this._actor.set_size(width, height);
            this._actor.show();
            this._grab = Main.pushModal(this._actor, {actionMode: Shell.ActionMode.NORMAL});
            // GNOME 50 replaced seat-state flags with revocable grabs.
            const hasGrab = this._grab && (typeof this._grab.is_revoked === 'function'
                ? !this._grab.is_revoked()
                : Boolean(this._grab.get_seat_state() & Clutter.GrabState.KEYBOARD));
            if (!hasGrab) {
                this.hide();
                return;
            }
            global.stage.set_key_focus(this._search.get_clutter_text());
            await this.refresh();
        } catch (error) {
            this._lastError = `${error.message}`;
            console.error(`Gnome Clip Notes overlay: ${error.message}\n${error.stack ?? ''}`);
            this.hide();
        } finally {
            if(request===this._openRequest)this._opening=false;
        }
    }

    hide() {
        this._hideTooltip();
        this._openRequest=(this._openRequest??0)+1;
        this._opening=false;
        this._closeContext();
        if (!this.visible) return;
        if(this._grab)Main.popModal(this._grab);
        this._grab = null;
        this._actor.hide();
        global.stage.set_key_focus(null);
    }

    toggle() { (this.visible||this._opening) ? this.hide() : this.show(); }

    _scheduleRefresh() {
        if (this._refreshId) GLib.source_remove(this._refreshId);
        this._refreshId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 120, () => { this._refreshId = 0; this.refresh(); return GLib.SOURCE_REMOVE; });
    }

    async refresh() {
        const serial = (this._serial ?? 0) + 1;
        this._serial = serial;
        try {
            // One extra item distinguishes a full final page from a next page.
            const [json] = await this._service.QueryAsync(JSON.stringify({...this._query, limit: this._pageSize + 1}));
            if (this._destroyed || serial !== this._serial || !this.visible) return;
            const result = JSON.parse(json);
            this._hasNext = Array.isArray(result.items) && result.items.length > this._pageSize;
            this._items = Array.isArray(result.items) ? result.items.slice(0, this._pageSize) : [];
            const options = [[_('All apps'), '']];
            for (const entry of result.sources ?? []) {
                const source = String(typeof entry === 'string' ? entry : entry?.name ?? entry?.source ?? '').trim();
                if (source && !options.some(option => option[1] === source))
                    options.push([source, source]);
            }
            const sourcesChanged = JSON.stringify(options) !== JSON.stringify(this._sourceOptions);
            // Preserve the array used by the button, but replace its contents.
            this._sourceOptions.splice(0, this._sourceOptions.length, ...options);
            if (this._query.source && !options.some(option => option[1] === this._query.source)) {
                this._query.source = '';
                this._query.offset = 0;
                this._sourceButton._setSelectedValue('');
                if (this._context?._sourceSection) this._openSourceMenu();
                return this.refresh();
            }
            if (sourcesChanged && this._context?._sourceSection) this._openSourceMenu();
            this._settings = result.settings ?? {};
            this._syncTheme();
            this._renderGroups(result.groups ?? []);
            this._selected = Math.min(this._selected, Math.max(0, this._items.length - 1));
            this._renderCards();
            this._pageLabel.text = this._items.length ? `${this._query.offset + 1}–${this._query.offset + this._items.length}` : '';
            this._previousButton.reactive = this._query.offset > 0;
            this._previousButton.opacity = this._query.offset > 0 ? 255 : 80;
            this._nextButton.reactive = this._hasNext;
            this._nextButton.opacity = this._hasNext ? 255 : 80;
        } catch (error) {
            if (!this._destroyed && serial === this._serial) this._renderError(_('Gnome Clip Notes is starting…'));
            console.debug(`Gnome Clip Notes query: ${error.message}`);
        }
    }

    _renderGroups(groups) {
        this._moveGroups = groups.filter(group => group.id > 1);
        this._groups.destroy_all_children();
        const all = [{id: 0, name: _('History')}, {id: 1, name: _('Notes')}, ...groups.filter(g => g.id > 1)];
        const seen = new Set();
        const unique = all.filter(group => !seen.has(group.id) && seen.add(group.id));
        const buttons = unique.map(group => {
            const button = new St.Button({style_class: 'button gcn-chip', toggle_mode: true, checked: this._query.group_id === group.id, can_focus: true, accessible_name: group.name});
            const content = new St.BoxLayout({style_class:'gcn-group-content'});
            content.add_child(new St.Icon({icon_name:group.id===0?'document-open-recent-symbolic':group.id===1?'document-edit-symbolic':'folder-symbolic',icon_size:16}));
            content.add_child(clippedLabel(group.name, 'gcn-group-label'));
            button.set_child(content);
            button.connect('clicked', () => this._chooseGroup(group.id));
            this._groups.add_child(button);
            return button;
        });
        const more = new St.Button({label: _('More…'), style_class: 'button gcn-chip', can_focus:true, toggle_mode:true});
        this._groups.add_child(more);
        const add = this._iconButton('list-add-symbolic', _('New Folder'), () => this._activateApp('new-group', 0));
        this._groups.add_child(add);
        const scale=St.ThemeContext.get_for_stage(global.stage).scale_factor;
        const gap=8*scale;
        const natural=actor=>actor.get_preferred_width(-1)[1];
        const fixed=this._toolbar.get_children().filter(a=>a!==this._search&&a!==this._groups);
        const available=this._actor.width-48*scale;
        let budget=available-200*scale-fixed.reduce((sum,a)=>sum+natural(a),0)-(fixed.length+1)*gap;
        const minimum=buttons.slice(0,2).reduce((sum,b)=>sum+natural(b)+gap,0)+natural(more)+gap+natural(add);
        this._groupsSecondRow=budget<minimum;
        const desiredParent=this._groupsSecondRow?this._actor:this._toolbar;
        if(this._groups.get_parent()!==desiredParent){
            this._groups.get_parent().remove_child(this._groups);
            desiredParent.insert_child_at_index(this._groups,this._groupsSecondRow?1:4);
        }
        if(this._groupsSecondRow)budget=available;
        let used=natural(add);
        let visibleCount=0;
        for(let i=0;i<buttons.length;i++){
            const needed=natural(buttons[i])+gap;
            const reserve=i<buttons.length-1?natural(more)+gap:0;
            if(used+needed+reserve>budget&&i>=2)break;
            used+=needed;
            visibleCount++;
        }
        buttons.forEach((button,i)=>button.visible=i<visibleCount);
        const hidden=unique.slice(visibleCount);
        more.visible=hidden.length>0;
        more.checked=hidden.some(g=>g.id===this._query.group_id);
        more.connect('clicked',()=>this._openGroupMenu(more,hidden));
        this._visibleGroupCount=visibleCount;
        this._hiddenGroupCount=hidden.length;
        this._moreGroups=more;
    }

    _chooseGroup(id) {
        this._query.group_id = id;
        this._query.offset = 0;
        this._selected = 0;
        this.refresh();
    }

    _openGroupMenu(source, groups) {
        this._closeContext();
        const menu = new PopupMenu.PopupMenu(source, 0.5, St.Side.BOTTOM);
        this._context = menu;
        Main.uiGroup.add_child(menu.actor);
        for (const group of groups) {
            const item = menu.addAction(group.name, () => this._chooseGroup(group.id));
            if (group.id === this._query.group_id)
                item.setOrnament(PopupMenu.Ornament.DOT);
        }
        this._watchContext(menu);
        menu.open();
    }

    _renderError(message) {
        this._cards.destroy_all_children();
        const empty = new St.BoxLayout({vertical:true, style_class:'gcn-empty', x_expand:true, x_align:Clutter.ActorAlign.CENTER, y_align:Clutter.ActorAlign.CENTER});
        empty.add_child(new St.Icon({icon_name:'edit-paste-symbolic',icon_size:40,style_class:'gcn-empty-icon'}));
        empty.add_child(new St.Label({text:message,style_class:'gcn-empty-title',x_align:Clutter.ActorAlign.CENTER}));
        if (!this._items.length && message === _('Nothing here yet'))
            empty.add_child(new St.Label({text:_('Copy text, create a note, or adjust your filters.'),style_class:'gcn-hint'}));
        this._cards.add_child(empty);
    }

    _renderCards() {
        const focus = global.stage.get_key_focus();
        const hadCardFocus = focus && this._cards.contains(focus);
        this._cards.destroy_all_children();
        if (!this._items.length) {
            this._renderError(_('Nothing here yet'));
            if (hadCardFocus) this._search.get_clutter_text().grab_key_focus();
            return;
        }
        const scale=St.ThemeContext.get_for_stage(global.stage).scale_factor;
        const cardWidth=Math.floor((this._actor.width - 48*scale - (this._pageSize - 1)*12*scale)/this._pageSize);
        this._items.forEach((item, index) => {
            const card = new St.Button({style_class: `button gcn-card${index === this._selected ? ' gcn-card-selected' : ''}`, width:cardWidth, x_expand: false, y_align:Clutter.ActorAlign.START, can_focus: true});
            const box = new St.BoxLayout({vertical: true,x_expand:true, style_class:'gcn-card-inner'});
            const source = new St.BoxLayout({style_class:'gcn-card-source'});
            const app = item.source_id ? Shell.AppSystem.get_default().lookup_app(item.source_id) : null;
            source.add_child(app?.create_icon_texture(20) ?? new St.Icon({icon_name:item.kind==='link'?'insert-link-symbolic':'text-x-generic-symbolic',icon_size:20}));
            source.add_child(clippedLabel(item.source || _('Notes'), 'gcn-source-label'));
            box.add_child(source);
            if(item.title)box.add_child(clippedLabel(ellipsize(item.title, 32), 'gcn-card-title'));
            const preview=clippedLabel(String(item.content??'').trim().slice(0,500), 'gcn-card-content');
            preview.clutter_text.single_line_mode=false;
            preview.clutter_text.line_wrap=true;
            preview.clutter_text.line_wrap_mode=Pango.WrapMode.WORD_CHAR;
            preview.height=Math.max(48,Math.min(item.title?108:132,this._actor.height/scale-190-(item.title?24:0)-(this._groupsSecondRow?52:0)))*scale;
            box.add_child(preview);
            const meta = new St.BoxLayout({style_class:'gcn-card-bottom'});
            meta.add_child(clippedLabel(item.kind==='link'?_('Link'):_('Text'), 'gcn-card-meta'));
            meta.add_child(new St.Label({text:`${index+1}`,style_class:'gcn-shortcut'}));
            box.add_child(meta);
            card.set_child(box);
            card.connect('clicked', () => this._pasteIndex(index));
            card.connect('key-focus-in', () => this._select(index));
            card.connect('button-press-event', (_a, event) => {
                if ([1, 3].includes(event.get_button())) { this._select(index); card.grab_key_focus(); }
                if (event.get_button() === 3) { this._openContext(card, item); return Clutter.EVENT_STOP; }
                return Clutter.EVENT_PROPAGATE;
            });
            this._cards.add_child(card);
        });
        if (hadCardFocus) this._cards.get_children()[this._selected]?.grab_key_focus();
    }

    _openContext(source, item) {
        this._closeContext();
        const menu = new PopupMenu.PopupMenu(source, 0.5, St.Side.BOTTOM);
        this._context = menu;
        Main.uiGroup.add_child(menu.actor);
        const monitor = Main.layoutManager.findMonitorForActor(source);
        const area = Main.layoutManager.getWorkAreaForMonitor(monitor?.index ?? Main.layoutManager.primaryIndex);
        const scale = St.ThemeContext.get_for_stage(global.stage).scale_factor;
        const [, y] = source.get_transformed_position();
        // Shell enables native submenu scrolling when the top menu has a
        // maximum height. Keep the whole menu above the card and on screen.
        menu.actor.set_style(`max-height: ${Math.max(160, (y - area.y) / scale - 16)}px;`);
        const add = (label, action, callback = () => this._activateApp(action, item.id)) => {
            const row = menu.addAction(label, callback);
            if (ITEM_SHORTCUTS[action]) {
                row.label.x_expand = true;
                row.add_child(new St.Label({text: ITEM_SHORTCUTS[action], style_class: 'gcn-menu-shortcut', y_align: Clutter.ActorAlign.CENTER, x_align: Clutter.ActorAlign.END}));
            }
            return row;
        };
        add(_('Paste'), 'paste', () => this._extension.paste(item.content, this._settings?.paste_mode !== 'clipboard', this._previousWindow));
        menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        add(_('View'), 'view'); add(_('Edit'), 'edit'); add(_('Info'), 'info'); add(_('Rename…'), 'rename');
        menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        add(_('Move to Notes'), 'notes');
        const folders = this._moveGroups ?? [];
        const move = new PopupMenu.PopupSubMenuMenuItem(_('Move to folder'));
        menu.addMenuItem(move);
        for (const group of folders.slice(0, 10)) {
            const row = move.menu.addAction(group.name, () => this._activateApp(`move-group-${group.id}`, item.id));
            row.label.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        }
        if (folders.length) move.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        if (folders.length > 10)
            move.menu.addAction(_('More Folders…'), () => this._activateApp('pin', item.id));
        move.menu.addAction(_('New Folder…'), () => this._activateApp('new-group-move', item.id));
        for (const row of move.menu._getMenuItems()) {
            if (!row.can_focus) continue;
            row.connect('key-focus-in', () => {
                const adjustment = move.menu.actor.vadjustment ?? move.menu.actor.vscroll.adjustment;
                const box = row.get_allocation_box();
                if (box.y1 < adjustment.value) adjustment.value = box.y1;
                else if (box.y2 > adjustment.value + adjustment.page_size)
                    adjustment.value = box.y2 - adjustment.page_size;
            });
        }
        menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        add(_('Delete…'), 'delete');
        this._watchContext(menu);
        menu.open();
    }

    _watchContext(menu) {
        this._hideTooltip();
        menu.actor.add_style_class_name(this._themeClass);
        // The menu is a sibling of the overlay in uiGroup. It needs its own
        // nested modal grab; the overlay's grab excludes sibling actors.
        this._menuManager.addMenu(menu);
        menu.connect('open-state-changed', (_menu, open) => {
            if (!open && this._context === menu) {
                this._context = null;
                this._destroyMenuSoon(menu);
            }
        });
    }

    _closeContext() {
        this._datePicker?.close();
        const menu = this._context;
        this._context = null;
        if (menu) { menu.close(); this._destroyMenuSoon(menu); }
    }

    _destroyMenuSoon(menu) {
        this._pendingMenus ??= new Map();
        const id = GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            this._pendingMenus.delete(id);
            menu.destroy();
            return GLib.SOURCE_REMOVE;
        });
        this._pendingMenus.set(id, menu);
    }

    _activateApp(action, id) {
        this._closeContext();
        this.hide();
        this._extension.activate(action, id);
    }

    async _changePage(direction) {
        if (this._changingPage || (direction > 0 ? !this._hasNext : this._query.offset === 0)) return;
        this._changingPage = true;
        this._query.offset = Math.max(0, this._query.offset + direction * this._pageSize);
        this._selected = direction > 0 ? 0 : this._pageSize - 1;
        try { await this.refresh(); }
        finally { this._changingPage = false; }
    }

    _navigate(direction) {
        if (this._changingPage || !this._items.length) return;
        // Navigation acts on cards, not on the search cursor. Move real focus
        // as well as the highlight so Delete has the same target as F8.
        this._cards.get_children()[this._selected]?.grab_key_focus();
        const index = this._selected + direction;
        if (index < 0 || index >= this._items.length) this._changePage(direction);
        else { this._select(index); this._cards.get_children()[this._selected]?.grab_key_focus(); }
    }

    _select(index) {
        this._selected = Math.max(0, Math.min(this._items.length - 1, index));
        this._cards.get_children().forEach((card, i) => {
            if (i === this._selected) card.add_style_class_name('gcn-card-selected');
            else card.remove_style_class_name('gcn-card-selected');
        });
    }
    _pasteIndex(index, active = null) {
        if (this._changingPage) return;
        const item = this._items[index];
        const shouldPaste = active ?? this._settings?.paste_mode !== 'clipboard';
        if (item) this._extension.paste(item.content, shouldPaste, this._previousWindow);
    }
    _onKey(event) {
        const key = event.get_key_symbol();
        const modifiers = event.get_state();
        const commandModifiers = modifiers & (Clutter.ModifierType.SHIFT_MASK | Clutter.ModifierType.CONTROL_MASK | Clutter.ModifierType.MOD1_MASK | Clutter.ModifierType.SUPER_MASK | Clutter.ModifierType.META_MASK | Clutter.ModifierType.HYPER_MASK);
        let action = null;
        if (commandModifiers === Clutter.ModifierType.MOD1_MASK && [Clutter.KEY_Return, Clutter.KEY_KP_Enter].includes(key)) action = 'info';
        if (!commandModifiers) {
            action = ({[Clutter.KEY_F3]: 'view', [Clutter.KEY_F4]: 'edit', [Clutter.KEY_F2]: 'rename', [Clutter.KEY_F8]: 'delete', [Clutter.KEY_Delete]: 'delete', [Clutter.KEY_KP_Delete]: 'delete'})[key] ?? null;
        }
        const inSearch = global.stage.get_key_focus() === this._search.get_clutter_text();
        if (inSearch && this._search.get_text().length > 0 && [Clutter.KEY_Delete, Clutter.KEY_KP_Delete].includes(key)) return Clutter.EVENT_PROPAGATE;
        if (action) {
            const item = this._items[this._selected];
            if (item && !this._changingPage) this._activateApp(action, item.id);
            return Clutter.EVENT_STOP;
        }
        // XKB hardware code: evdev KEY_C 46 + 8; independent of Cyrillic layout.
        if ((modifiers & Clutter.ModifierType.CONTROL_MASK) &&
            !(modifiers & (Clutter.ModifierType.SHIFT_MASK | Clutter.ModifierType.MOD1_MASK | Clutter.ModifierType.SUPER_MASK)) &&
            (key === Clutter.KEY_c || event.get_key_code() === 54)) {
            const item = this._items[this._selected];
            if (item && !this._changingPage) this._extension.copy(item.content);
            return Clutter.EVENT_STOP;
        }
        if (key === Clutter.KEY_Escape) { this.hide(); return Clutter.EVENT_STOP; }
        if (key === Clutter.KEY_Left || key === Clutter.KEY_Up) { this._navigate(-1); return Clutter.EVENT_STOP; }
        if (key === Clutter.KEY_Right || key === Clutter.KEY_Down) { this._navigate(1); return Clutter.EVENT_STOP; }
        if (key === Clutter.KEY_Return || key === Clutter.KEY_KP_Enter) { this._pasteIndex(this._selected); return Clutter.EVENT_STOP; }
        if (event.get_state() & Clutter.ModifierType.MOD1_MASK && key >= Clutter.KEY_1 && key <= Clutter.KEY_9) { this._pasteIndex(key - Clutter.KEY_1); return Clutter.EVENT_STOP; }
        return Clutter.EVENT_PROPAGATE;
    }

    destroy() {
        this._destroyed = true;
        this._hideTooltip();
        this._serial = (this._serial ?? 0) + 1;
        if (this.visible) this.hide();
        if (this._refreshId) GLib.source_remove(this._refreshId);
        if (this._keyId && !this._actorDestroyed) this._actor.disconnect(this._keyId);
        this._interfaceSettings.disconnect(this._themeId);
        this._closeContext();
        for (const [id, menu] of this._pendingMenus ?? []) {
            GLib.source_remove(id);
            menu.destroy();
        }
        this._pendingMenus?.clear();
        if(!this._actorDestroyed)this._actor.destroy();
    }
}

const Indicator = GObject.registerClass(class Indicator extends PanelMenu.Button {
    _init(extension) {
        super._init(0, _('Gnome Clip Notes'));
        this._extension = extension;
        this.add_child(new St.Icon({icon_name: 'edit-paste-symbolic', style_class: 'system-status-icon'}));
        this.menu.addAction(_('Open Clipboard'), () => extension._overlay.toggle());
        this.menu.addAction(_('Open Library'), () => extension.activate('show', 0));
        this.menu.addAction(_('New Note'), () => extension.activate('new-note', 0));
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        const pause = new PopupMenu.PopupSubMenuMenuItem(_('Pause Capture'));
        for (const [label, minutes] of [[_('15 minutes'), 15], [_('30 minutes'), 30], [_('1 hour'), 60], [_('3 hours'), 180], [_('8 hours'), 480]])
            pause.menu.addAction(label, () => extension.pause(minutes));
        this.menu.addMenuItem(pause);
        this._pauseStatus = new PopupMenu.PopupMenuItem('', {reactive: false});
        this.menu.addMenuItem(this._pauseStatus);
        this._resume = this.menu.addAction(_('Resume Capture'), () => extension.pause(0));
        this._serviceMenu = new PopupMenu.PopupSubMenuMenuItem(_('Service'));
        this._startService = this._serviceMenu.menu.addAction(_('Start'), () => extension.controlService('start'));
        this._stopService = this._serviceMenu.menu.addAction(_('Stop'), () => extension.controlService('stop'));
        this._restartService = this._serviceMenu.menu.addAction(_('Restart'), () => extension.controlService('restart'));
        this._serviceMenu.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._serviceStatus = new PopupMenu.PopupMenuItem('', {reactive: false, can_focus: false});
        this._serviceMenu.menu.addMenuItem(this._serviceStatus);
        this.menu.addMenuItem(this._serviceMenu);
        this.updateServiceStatus();
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this.menu.addAction(_('Settings'), () => extension.activate('settings', 0));
        this.menu.addAction(_('About'), () => extension.activate('about', 0));
        this.menu.connect('open-state-changed', (_menu, open) => {
            if (open) extension.refreshPassiveStatus();
        });
    }

    updateStatus(settings) {
        const paused = Number(settings?.paused_until ?? 0) > Math.floor(Date.now() / 1000);
        const until = new Date(Number(settings?.paused_until ?? 0) * 1000);
        this._pauseStatus.label.text = paused ? trf('Paused until {time}', {time: until.toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'})}) : '';
        this._pauseStatus.visible = paused;
        this._resume.visible = paused;
        this.updateServiceStatus();
    }

    setStopped() {
        this._pauseStatus.visible = false;
        this._resume.visible = false;
        this.updateServiceStatus();
    }

    updateServiceStatus() {
        const action = this._extension._serviceAction;
        const running = Boolean(this._extension._service?.g_name_owner);
        this._startService.sensitive = !action && !running;
        this._stopService.sensitive = this._restartService.sensitive = !action && running;
        this._serviceStatus.label.text = action === 'start' ? _('Starting service…')
            : action === 'stop' ? _('Stopping service…')
            : action === 'restart' ? _('Restarting service…')
            : running ? _('Service running') : _('Service stopped');
    }
});

export default class GnomeClipNotesExtension extends Extension {
    enable() {
        resetCatalog();
        this._overlayNeedsLocalization = false;
        this._lifecycle = Symbol('enabled');
        this._alive = true;
        this._serviceAction = null;
        this._serviceControlCancellable = null;
        this._captureAttempts=0;
        this._privacy = null;
        this._settings = this.getSettings();
        this._pendingDelays = new Map();
        this._service = new ServiceProxy(Gio.DBus.session, SERVICE, SERVICE_PATH, null, null,
            Gio.DBusProxyFlags.DO_NOT_AUTO_START);
        this._overlay = new Overlay(this, this._service);
        this._indicator = new Indicator(this);
        this._indicator.connect('destroy',()=>{this._indicator=null;});
        Main.panel.addToStatusArea(this.uuid, this._indicator);
        this._clipboard = St.Clipboard.get_default();
        this._selection = global.display.get_selection();
        this._ownerChangedId = this._selection.connect('owner-changed', (_s, type, owner) => this._onOwnerChanged(type, owner));
        this._focusId = global.display.connect('notify::focus-window', () => this._rememberTargetWindow());
        this._sessionId = Main.sessionMode.connect('updated', () => {
            if (this._isLocked()) {
                this._captureGeneration = (this._captureGeneration ?? 0) + 1;
                this._captureCancellable?.cancel();
                this._expectOwnOwner = false;
                this._overlay.hide();
            }
        });
        this._ownerNotifyId = this._service.connect('notify::g-name-owner', () => this._onServiceOwnerChanged());
        this._serviceSignalId = this._service.connectSignal('Changed', () => {
            this._reloadServiceSettings(8);
            if (this._overlay.visible) this._overlay.refresh();
        });
        this._pasteSignalId = this._service.connectSignal('PasteRequested', (_p, _sender, [text, active]) => this.paste(text, active));
        this._installBindings();
        this._bridge = Gio.DBusExportedObject.wrapJSObject(BRIDGE_XML, {
            Paste: (text, active) => this.paste(text, active),
            Toggle: () => this._overlay.toggle(),
            GetStatus: () => JSON.stringify(this._diagnostics()),
        });
        this._bridge.export(Gio.DBus.session, '/io/github/OleksiyM/GnomeClipNotes/Bridge');
        this._rememberTargetWindow();
        this._onServiceOwnerChanged();
    }

    _installBindings() {
        const flags = Meta.KeyBindingFlags.NONE;
        Main.wm.addKeybinding('activate-shortcut', this._settings, flags, Shell.ActionMode.NORMAL, () => this._overlay.toggle());
        Main.wm.addKeybinding('note-shortcut', this._settings, flags, Shell.ActionMode.NORMAL, () => this.activate('new-note', 0));
        Main.wm.addKeybinding('library-shortcut', this._settings, flags, Shell.ActionMode.NORMAL, () => this.activate('show', 0));
        for (let i = 1; i <= 9; i++)
            Main.wm.addKeybinding(`quick-paste-${i}`, this._settings, flags, Shell.ActionMode.NORMAL, () => this._quickPaste(i - 1));
    }

    _isLocked() {
        return !this._alive || Main.sessionMode.isLocked || Main.sessionMode.isGreeter;
    }

    _rememberTargetWindow() {
        if (this._isLocked() || this._overlay?.visible) return;
        const win = global.display.focus_window;
        if (!win) return;
        const app = Shell.WindowTracker.get_default().get_window_app(win);
        const identity = `${app?.get_id() ?? ''} ${app?.get_name() ?? ''}`;
        if (!/io\.github\.OleksiyM\.GnomeClipNotes|Gnome Clip Notes/i.test(identity))
            this._lastTargetWindow = win;
    }

    _diagnostics() {
        return {
            enabled: this._alive,
            overlay_visible: this._overlay?.visible ?? false,
            overlay_items: this._overlay?._items.length ?? 0,
            overlay_modal: Boolean(this._overlay?._grab),
            overlay_error: this._overlay?._lastError ?? null,
            capture_ready: Boolean(this._privacy && this._service?.g_name_owner),
            service_available: Boolean(this._service?.g_name_owner),
            capture_attempts: this._captureAttempts,
            locked: this._isLocked(),
            last_capture_status: this._lastCaptureStatus ?? 'idle',
            last_capture_mime: this._lastCaptureMime ?? null,
            latest_mime_types: this._lastMimeTypes ?? [],
            latest_source: this._lastCaptureSource ?? {id: '', name: ''},
        };
    }

    _onServiceOwnerChanged() {
        if (!this._alive) return;
        this._indicator?.updateServiceStatus();
        if (this._service.g_name_owner) {
            this._reloadServiceSettings(20);
        } else {
            this._privacy = this._runtimeSettings = null;
            this._indicator?.setStopped();
            if (this._overlay?.visible) this._overlay._renderError(_('Service stopped'));
        }
    }

    _delay(milliseconds, lifecycle) {
        return new Promise(resolve => {
            const id = GLib.timeout_add(GLib.PRIORITY_DEFAULT, milliseconds, () => {
                this._pendingDelays.delete(id);
                resolve(this._alive && lifecycle === this._lifecycle);
                return GLib.SOURCE_REMOVE;
            });
            this._pendingDelays.set(id, resolve);
        });
    }

    async _reloadServiceSettings(retries = 0) {
        const lifecycle = this._lifecycle;
        const query = {search: '', group_id: 0, kind: '', source: '', since: 0, until: 0, limit: 1, offset: 0, metadata_only: true};
        for (let attempt = 0; attempt <= retries; attempt++) {
            if (!this._alive || lifecycle !== this._lifecycle || !this._service.g_name_owner) return false;
            const owner = this._service.g_name_owner;
            try {
                const [json] = await this._service.QueryAsync(JSON.stringify(query));
                // A reply queued by the old process must not restore its status
                // or privacy cache after stop/restart.
                if (!this._alive || lifecycle !== this._lifecycle || owner !== this._service.g_name_owner) return false;
                const result = JSON.parse(json);
                const settings = result.settings ?? {};
                this._privacy = {
                    ignore_sensitive: settings.ignore_sensitive !== false,
                    ignored_apps: Array.isArray(settings.ignored_apps) ? settings.ignored_apps.map(String).filter(Boolean) : [],
                };
                this._runtimeSettings = settings;
                if (setCatalog(result.localization)) {
                    this._overlayNeedsLocalization = true;
                    this._indicator?.destroy();
                    this._indicator = new Indicator(this);
                    this._indicator.connect('destroy', () => { this._indicator = null; });
                    Main.panel.addToStatusArea(this.uuid, this._indicator);
                    // show() handles its own replacement after ensureService;
                    // replacing it here would cancel that in-flight request.
                    if (!this._overlay._opening) {
                        const wasVisible = this._overlay.visible;
                        const overlay = this._refreshLocalizedOverlay();
                        if (wasVisible) overlay.show();
                    }
                }
                this._indicator?.updateStatus(settings);
                return true;
            } catch (error) {
                if (attempt === retries) {
                    if (this._alive && lifecycle === this._lifecycle)
                        console.debug(`Gnome Clip Notes settings: ${error.message}`);
                    return false;
                }
                if (!await this._delay(100, lifecycle)) return false;
            }
        }
        return false;
    }

    _refreshLocalizedOverlay() {
        if (this._overlayNeedsLocalization) {
            this._overlayNeedsLocalization = false;
            const previous = this._overlay;
            previous.destroy();
            this._overlay = new Overlay(this, this._service, previous);
        }
        return this._overlay;
    }

    async _startService() {
        await new Promise((resolve, reject) => {
            Gio.DBus.session.call('org.freedesktop.DBus', '/org/freedesktop/DBus',
                'org.freedesktop.DBus', 'StartServiceByName', new GLib.Variant('(su)', [SERVICE, 0]),
                new GLib.VariantType('(u)'), Gio.DBusCallFlags.NONE, 5000, null, (connection, result) => {
                    try { connection.call_finish(result); resolve(); } catch (error) { reject(error); }
                });
        });
    }

    async ensureService() {
        if (this._isLocked() || this._serviceAction) return false;
        if (this._ensurePromise) return this._ensurePromise;
        const lifecycle = this._lifecycle;
        this._ensurePromise = (async () => {
            try {
                if (!this._service.g_name_owner) await this._startService();
                for (let attempt = 0; attempt < 30 && !this._service.g_name_owner; attempt++)
                    if (!await this._delay(100, lifecycle)) return false;
                return await this._reloadServiceSettings(20);
            } catch (error) {
                if (this._alive && lifecycle === this._lifecycle)
                    console.warn(`Gnome Clip Notes start service: ${error.message}`);
                return false;
            } finally {
                if (lifecycle === this._lifecycle) this._ensurePromise = null;
            }
        })();
        return this._ensurePromise;
    }

    async activate(action, id = 0) {
        if (await this.ensureService()) this._service.ActivateRemote(action, id);
    }

    async controlService(action) {
        if (this._isLocked() || this._serviceAction) return;
        if (!['start', 'stop', 'restart'].includes(action)) return;
        let owner = this._service.g_name_owner;
        if ((action === 'start') === Boolean(owner)) return;
        const lifecycle = this._lifecycle;
        const cancellable = new Gio.Cancellable();
        this._serviceControlCancellable = cancellable;
        this._serviceAction = action;
        this._indicator?.updateServiceStatus();
        this._overlay?.hide();
        try {
            // Finish an already requested overlay/library activation before
            // controlling its process; do not silently ignore an enabled Start.
            if (this._ensurePromise) await this._ensurePromise;
            if (!this._alive || lifecycle !== this._lifecycle || this._isLocked()) return;
            owner = this._service.g_name_owner;
            if ((action === 'start') === Boolean(owner)) return;
            if (owner) {
                // Address the inspected unique bus owner, never a replacement
                // process and never an auto-activated service.
                const [pid] = await busCall('org.freedesktop.DBus', '/org/freedesktop/DBus',
                    'org.freedesktop.DBus', 'GetConnectionUnixProcessID',
                    new GLib.Variant('(s)', [owner]), '(u)', cancellable);
                if (!this._alive || lifecycle !== this._lifecycle || owner !== this._service.g_name_owner) return;
                const [accepted] = await busCall(owner, SERVICE_PATH, SERVICE_IFACE,
                    'QuitForUpdate', null, '(b)', cancellable);
                if (!accepted) throw new Error(_('Save and close all note editors before stopping or restarting GnomeClipNotes.'));
                // Name loss is not process exit. Linux /proc tracks this exact
                // bus owner's PID; do not guess by executable/process name.
                const process = Gio.File.new_for_path(`/proc/${pid}`);
                const deadline = GLib.get_monotonic_time() + 10 * GLib.USEC_PER_SEC;
                while (process.query_exists(null) || this._service?.g_name_owner === owner) {
                    if (GLib.get_monotonic_time() >= deadline)
                        throw new Error(_('The service has not finished stopping. Try again shortly.'));
                    if (!await this._delay(100, lifecycle)) return;
                }
            }
            if (!this._alive || lifecycle !== this._lifecycle || this._isLocked()) return;
            if (this._service.g_name_owner)
                throw new Error(_('Another service instance started. Check its status before trying again.'));
            if (action !== 'stop') {
                await this._startService();
                for (let attempt = 0; attempt < 30 && !this._service?.g_name_owner; attempt++)
                    if (!await this._delay(100, lifecycle)) return;
                if (!this._alive || lifecycle !== this._lifecycle) return;
                if (!this._service.g_name_owner || !await this._reloadServiceSettings(20))
                    throw new Error(_('The service did not become ready. Try starting it again.'));
            }
        } catch (error) {
            if (this._alive && lifecycle === this._lifecycle && !this._isLocked())
                Main.notify(_('GnomeClipNotes'), error.message);
        } finally {
            if (lifecycle === this._lifecycle) {
                this._serviceAction = this._serviceControlCancellable = null;
                this.refreshPassiveStatus();
            }
        }
    }

    async pause(minutes) {
        if (await this.ensureService()) this._service.PauseRemote(minutes);
    }

    refreshPassiveStatus() {
        this._indicator?.updateServiceStatus();
        if (this._service?.g_name_owner) this._reloadServiceSettings(2);
        else this._indicator?.setStopped();
    }

    async _quickPaste(index) {
        if (this._isLocked()) return;
        if (!await this.ensureService()) return;
        const lifecycle = this._lifecycle;
        try {
            const query = {search: '', group_id: 0, kind: '', source: '', since: 0, until: 0, limit: 9, offset: 0};
            const [json] = await this._service.QueryAsync(JSON.stringify(query));
            if (!this._alive || lifecycle !== this._lifecycle) return;
            const item = JSON.parse(json).items?.[index];
            if (item) this.paste(item.content, this._runtimeSettings?.paste_mode !== 'clipboard', this._lastTargetWindow);
        } catch (error) { console.warn(`Gnome Clip Notes quick paste: ${error.message}`); }
    }

    _onOwnerChanged(type, owner) {
        if (type !== Meta.SelectionType.SELECTION_CLIPBOARD) return;
        const previousStatus = this._lastCaptureStatus;
        const previousMime = this._lastCaptureMime;
        this._lastCaptureStatus = 'owner-changed';
        this._lastCaptureMime = null;
        const generation = (this._captureGeneration ?? 0) + 1;
        this._captureGeneration = generation;
        this._currentOwner = owner;
        this._captureCancellable?.cancel();
        if (this._isLocked()) { this._lastCaptureStatus = 'ignored:locked'; return; }
        if (!owner) { this._lastCaptureStatus = 'ignored:no-owner'; return; }
        if (this._expectOwnOwner) {
            this._expectOwnOwner = false;
            this._ownOwner = owner;
            this._lastCaptureStatus = previousStatus === 'accepted' ? 'accepted' : 'ignored:own-owner';
            this._lastCaptureMime = previousStatus === 'accepted' ? previousMime : null;
            return;
        }
        if (owner === this._ownOwner) {
            this._lastCaptureStatus = previousStatus === 'accepted' ? 'accepted' : 'ignored:own-owner';
            this._lastCaptureMime = previousStatus === 'accepted' ? previousMime : null;
            return;
        }
        if (!this._service.g_name_owner) { this._lastCaptureStatus = 'ignored:service-stopped'; return; }
        if (!this._privacy) { this._lastCaptureStatus = 'ignored:settings-not-ready'; return; }
        if (Number(this._runtimeSettings?.paused_until ?? 0) > Math.floor(Date.now() / 1000)) { this._lastCaptureStatus = 'ignored:paused'; return; }
        const app = this._focusedApp();
        const mimeTypes = owner.get_mimetypes?.() ?? this._selection.get_mimetypes(type) ?? [];
        this._lastMimeTypes = [...mimeTypes].map(String);
        this._lastCaptureSource = app;
        const identity = `${app.id} ${app.name}`;
        const sensitiveMime = mimeTypes.some(m => /password|secret|sensitive|keepass|1password|x-kde-passwordManagerHint/i.test(m));
        const ignored = this._privacy.ignored_apps.some(value => identity.toLowerCase().includes(value.toLowerCase()));
        const sensitiveApp = /keepass|bitwarden|1password|seahorse|password/i.test(identity);
        if ((this._privacy.ignore_sensitive && (sensitiveMime || sensitiveApp)) || ignored) { this._lastCaptureStatus = 'ignored:privacy'; return; }
        const formats = textFormats(mimeTypes);
        if (!formats.length) { this._lastCaptureStatus = 'ignored:mime'; return; }
        const lifecycle = this._lifecycle;
        const cancellable = new Gio.Cancellable();
        this._captureCancellable = cancellable;
        const selection = this._selection;
        const current = () => this._alive && lifecycle === this._lifecycle &&
            generation === this._captureGeneration && owner === this._currentOwner &&
            !cancellable.is_cancelled() && !this._isLocked();
        const permitted = () => {
            if (!current()) return false;
            if (!this._service.g_name_owner) { this._lastCaptureStatus = 'ignored:service-stopped'; return false; }
            if (!this._privacy) { this._lastCaptureStatus = 'ignored:settings-not-ready'; return false; }
            if (Number(this._runtimeSettings?.paused_until ?? 0) > Math.floor(Date.now() / 1000)) { this._lastCaptureStatus = 'ignored:paused'; return false; }
            if ((this._privacy.ignore_sensitive && (sensitiveMime || sensitiveApp)) ||
                this._privacy.ignored_apps.some(value => identity.toLowerCase().includes(value.toLowerCase()))) {
                this._lastCaptureStatus = 'ignored:privacy';
                return false;
            }
            return true;
        };
        const attempt = index => {
            if (!permitted()) return;
            const mime = formats[index];
            this._lastCaptureMime = mime;
            this._lastCaptureStatus = 'read';
            this._captureAttempts++;
            const output = Gio.MemoryOutputStream.new_resizable();
            const failed = error => {
                const oversized = output.get_data_size() > MAX_BYTES;
                try { output.close(null); } catch (_) {}
                if (!permitted()) return;
                // A size-policy rejection is not a reason to try another
                // representation. Never concatenate partial failed transfers.
                if (oversized) { this._lastCaptureStatus = 'ignored:too-large'; return; }
                if (index + 1 < formats.length) { attempt(index + 1); return; }
                this._lastCaptureStatus = `error:${error.message}`;
                console.debug(`Gnome Clip Notes bounded capture: ${error.message}`);
            };
            try {
                selection.transfer_async(type, mime, MAX_BYTES + 1, output, cancellable, (_selection, result) => {
                    let bytes;
                    try {
                        selection.transfer_finish(result);
                        output.close(null);
                        bytes = output.steal_as_bytes();
                    } catch (error) { failed(error); return; }
                    if (!permitted()) return;
                    try {
                        if (bytes.get_size() > MAX_BYTES) { this._lastCaptureStatus = 'ignored:too-large'; return; }
                        const text = new TextDecoder('utf-8', {fatal: true}).decode(bytes.get_data());
                        if (text.includes('\0')) { this._lastCaptureStatus = 'ignored:invalid-text'; return; }
                        if (!text) { this._lastCaptureStatus = 'ignored:empty'; return; }
                        this._service.CaptureRemote(text, app.name, app.id, false, (reply, error) => {
                            if (!current()) return;
                            if (error) {
                                this._lastCaptureStatus = `error:${error.message}`;
                                console.debug(`Gnome Clip Notes capture: ${error.message}`);
                                return;
                            }
                            if (reply?.[0] === true) {
                                this._lastCaptureStatus = 'accepted';
                                this._takeClipboardOwnership(text, generation, owner);
                            } else {
                                this._lastCaptureStatus = 'ignored:service-rejected';
                            }
                        });
                    } catch (error) {
                        this._lastCaptureStatus = `error:${error.message}`;
                        console.debug(`Gnome Clip Notes clipboard text: ${error.message}`);
                    }
                });
            } catch (error) {
                failed(error);
            }
        };
        attempt(0);
    }

    _takeClipboardOwnership(text, generation, owner) {
        if (!this._alive || this._isLocked() || generation !== this._captureGeneration || owner !== this._currentOwner) return;
        this._expectOwnOwner = true;
        this._clipboard.set_text(St.ClipboardType.CLIPBOARD, text);
    }

    _focusedApp() {
        const win = global.display.focus_window;
        const app = win ? Shell.WindowTracker.get_default().get_window_app(win) : null;
        return {id: app?.get_id() ?? '', name: app?.get_name() ?? win?.get_wm_class() ?? ''};
    }

    copy(text) {
        if (this._isLocked() || typeof text !== 'string' || new TextEncoder().encode(text).length > MAX_BYTES) return false;
        this._captureGeneration = (this._captureGeneration ?? 0) + 1;
        this._expectOwnOwner = true;
        this._clipboard.set_text(St.ClipboardType.CLIPBOARD, text);
        return true;
    }

    paste(text, active = true, targetWindow = null) {
        if (this._isLocked() || typeof text !== 'string' || new TextEncoder().encode(text).length > MAX_BYTES) return;
        this._overlay?.hide();
        this.copy(text);
        if (!active) return;
        const window = targetWindow ?? this._lastTargetWindow;
        if (!window) return;
        const time = global.get_current_time();
        window?.activate(time);
        if (this._pasteTimeoutId) GLib.source_remove(this._pasteTimeoutId);
        this._pasteTimeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 80, () => {
            this._pasteTimeoutId = 0;
            this._waitAndPaste(window, 0);
            return GLib.SOURCE_REMOVE;
        });
    }

    _waitAndPaste(window, attempt) {
        if (!this._alive || this._isLocked() || global.display.focus_window !== window) return;
        const modifiers = global.get_pointer()[2];
        const held = Clutter.ModifierType.SHIFT_MASK | Clutter.ModifierType.CONTROL_MASK |
            Clutter.ModifierType.MOD1_MASK | Clutter.ModifierType.SUPER_MASK;
        if ((modifiers & held) && attempt < 20) {
            this._pasteTimeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 25, () => {
                this._pasteTimeoutId = 0;
                this._waitAndPaste(window, attempt + 1);
                return GLib.SOURCE_REMOVE;
            });
            return;
        }
        if (!(modifiers & held)) this._sendPasteKeys(window);
    }

    _sendPasteKeys(window) {
        let keyboard;
        const pressed=[];
        try {
            // Keep the virtual device alive while Mutter dispatches queued input.
            keyboard = this._keyboard ??= Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
            const app = Shell.WindowTracker.get_default().get_window_app(window);
            const terminal = /terminal|console|kitty|alacritty|wezterm|foot|konsole/i.test(`${app?.get_id() ?? ''} ${app?.get_name() ?? ''} ${window.get_wm_class() ?? ''}`);
            // Linux evdev codes: Ctrl, optional Shift, V. Keyvals fail when
            // the active layout has no Latin V (for example Russian/Ukrainian).
            for(const key of [29,...(terminal?[42]:[]),47]){
                pressed.push(key);
                keyboard.notify_key(GLib.get_monotonic_time(),key,Clutter.KeyState.PRESSED);
            }
        } catch (error) {
            console.warn(`Gnome Clip Notes paste: ${error.message}`);
        } finally {
            for(const key of pressed.reverse()){
                try{keyboard.notify_key(GLib.get_monotonic_time(),key,Clutter.KeyState.RELEASED);}catch(error){console.warn(`Gnome Clip Notes key release: ${error.message}`);}
            }
        }
    }

    disable() {
        this._lifecycle = Symbol('disabled');
        this._alive = false;
        this._serviceControlCancellable?.cancel();
        this._captureGeneration = (this._captureGeneration ?? 0) + 1;
        this._captureCancellable?.cancel();
        for (const name of ['activate-shortcut', 'note-shortcut', 'library-shortcut', ...Array.from({length: 9}, (_, i) => `quick-paste-${i + 1}`)])
            Main.wm.removeKeybinding(name);
        if (this._ownerChangedId) this._selection.disconnect(this._ownerChangedId);
        if (this._focusId) global.display.disconnect(this._focusId);
        if (this._sessionId) Main.sessionMode.disconnect(this._sessionId);
        if (this._serviceSignalId) this._service.disconnectSignal(this._serviceSignalId);
        if (this._pasteSignalId) this._service.disconnectSignal(this._pasteSignalId);
        if (this._ownerNotifyId) this._service.disconnect(this._ownerNotifyId);
        if (this._pasteTimeoutId) GLib.source_remove(this._pasteTimeoutId);
        for (const [id, resolve] of this._pendingDelays ?? []) {
            GLib.source_remove(id);
            resolve(false);
        }
        this._pendingDelays?.clear();
        this._bridge?.unexport();
        this._overlay?.destroy();
        this._indicator?.destroy();
        this._ownOwner = this._currentOwner = this._lastTargetWindow = null;
        this._keyboard=null;
        this._runtimeSettings = this._privacy = null;
        this._ensurePromise = this._pendingDelays = null;
        this._bridge = this._overlay = this._indicator = this._selection = this._clipboard = this._service = this._settings = null;
    }
}
