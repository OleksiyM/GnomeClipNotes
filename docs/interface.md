# Interface design

The primary interface is the bottom clipboard panel, not the library window.
Opening the app or using its shortcut brings up the panel. The library remains
available for browsing larger collections.

The Shell-owned Open Library shortcut defaults to Super+B. Settings → Shortcuts
offers Ctrl+Alt+B as an alternative and Disabled; it brings forward the existing
library window rather than creating duplicates. Other desktop bindings are not
changed to reserve either combination.

## GNOME Shell panel

- Use Shell's native surface and button styles, rather than imitating GTK.
- Arrow navigation transfers keyboard focus to the selected card. Delete and F8
  request deletion of that card with confirmation. Delete in a focused nonempty
  search edits text; an empty search does not swallow the card command.
- Keep search, filters, collections, and page navigation together above cards;
  there is no bottom footer. Search takes remaining space, with a usable minimum.
- Give collections symbolic icons. Move excess folders to More based on their
  measured widths. Keep History and Notes visible; on narrower panels move the
  collection strip onto a second row rather than squeeze the search field.
- Show up to nine cards; use fewer on narrower monitors so text stays readable.
- Give each card a source icon and name, multiline preview, type, and shortcut.
- Use dropdown menus for choices, rather than cycling through hidden values.
- The application-source filter lists sources of existing items across all
  collections, not an accumulated application registry. Refresh replaces that
  snapshot; deleting the last item of the selected source resets to All apps.
  The source list scrolls within the space above the panel, with a top margin
  and a fixed All apps row below it. Keyboard focus scrolls its item into view.
- Shell menus use `PopupMenu` rows and separators, including the tray and its
  pause submenu. Filters and the folder overflow mark the current choice.
  Keep native geometry; adapt state/separator colors when the overlay theme
  differs from the Shell theme.
- Retain keyboard and pointer input tests, including nested context-menu grabs.
- Icon-only panel buttons keep accessible names and show delayed Shell-style
  labels on hover or keyboard focus. Labels do not take focus or affect layout;
  dismiss them on activation, menu opening, owner destruction and panel close.
- Custom dates expand inside the date menu, without launching Library. From/To
  use explicit YYYY-MM-DD local dates, both endpoints included. Valid ranges
  apply live; incomplete/invalid input keeps the last applied range. Enter in
  From moves to To; Enter in a valid To closes the menu, never pastes a card.
  Choices and draft date text survive menu dismissal for the extension's lifetime.
  Opening the preset list alone must not reactivate an older custom range.
- Both Library and overlay date fields provide a calendar button and Today.
  Opening shows the entered date, or today for empty/invalid input, without
  filling the field. Browsing months is provisional; only an explicit day or
  Today writes the corresponding field. Cancel leaves manual input intact.
  Library uses GtkCalendar; the overlay reuses Shell's Calendar without an
  event source. Its small adapter lives in `extension/datePicker.js`, with
  regression coverage for Shell's distinction between browsing and day clicks.

## Application windows

- Card menus in Library and the overlay use the same move destinations:
  Move to Notes, then Move to folder with up to ten custom folders in collection
  order. More Folders… appears only above ten and opens the existing chooser;
  New Folder… always appears below the separator (no separator for an empty list).
  Creating a destination and moving the item is one database transaction: a
  failed move must not leave an empty folder behind. Cancelling changes nothing.
- The library uses `AdwOverlaySplitView`, `AdwToolbarView`, and a breakpoint.
  Its sidebar becomes an overlay on narrow windows.
- Library cards have bounded, whitespace-normalized previews; full text remains
  available in the editor. Page capacity is measured from the viewport and card
  size, so Previous/Next page through fitting rows in History, Notes, and folders.
  Fetch one extra result to detect the final page without opening an empty one.
- Header bars contain a few relevant actions. Preferences and About live in
  the main menu, not as competing top-level buttons.
- Main and card-action menus use `GMenu`/`GtkPopoverMenu`, not flat buttons in
  a box. Separate navigation, organization, and deletion into menu sections.
  Main-menu shortcut hints follow the actual extension settings live; these
  remain Shell-owned bindings rather than competing GTK accelerators.
  The filter form stays a plain popover; text fields and editor text retain
  their built-in GTK context menus and editing shortcuts.
- Library filters apply immediately, without Apply/Cancel. Outside click and
  Escape dismiss the popover without reverting choices, including after a
  nested dropdown closes. Consume the dismissing click inside the library so
  it does not activate a card underneath. `Filters (N)` counts active type,
  date and source filters; Clear filters resets these but keeps search text.
  Incomplete or invalid custom dates show an inline hint and retain the last
  valid date range; other filters and search remain live during date entry.
- The editor uses a subtly tinted, outlined document surface, system-relative
  text sizing, preview, and a save action. Its only title is in the header:
  `New Note`, the saved name, or `Note` when saved without a name.
- Use `AdwWindowTitle` at the native header size, including the dirty marker;
  do not apply large content-heading styles to the editor's window title.
- A leading `●` and Save sensitivity share the same comparison against saved
  content, including undo back to that content. Saving leaves the editor open.
  First save asks for an optional name in an initially empty field; later saves
  never ask. Rename remains a library action. There is no bottom status strip.
- Editor and Preview are explicit text-labelled pages controlled by a native
  `GtkStackSwitcher` below the header. Keep the theme's normal selected-tab
  styling and sizing; do not recolor it with the app accent or replace it with
  an icon-only toggle. This works with the existing libadwaita 1.5 minimum.
- Unsaved edits still require an explicit discard confirmation when closing;
  automatic draft recovery is not implemented by this editor change.
- Settings uses an `AdwDialog` with `AdwNavigationSplitView`: a sidebar on wide
  layouts, a section list and native back navigation on narrow layouts. General,
  History, Privacy, Shortcuts, Folders and Data keep native `AdwPreferencesPage`,
  `AdwSwitchRow` and `AdwComboRow` content, with a bounded reading width.
  The selected section survives list refreshes (ignored apps and folders).
- History retention uses a restrained-width scale with a separate selected
  period, Apply button and currently applied period. Dragging never saves or
  deletes data. Existing shorter-period confirmation remains mandatory.
- Shortcuts separates configurable Shell bindings from contextual, read-only
  reference rows. No local library accelerators or preview-save binding are
  invented: Ctrl+S is documented specifically while typing in the editor.
- Confirmations use `AdwAlertDialog`. About uses a small `AdwDialog` with native
  rows: identity, installed version and explicit update check, Website/GitHub/X,
  then Legal. Update results stay in About; opening it never starts a request.
  Legal shows the bundled MIT license. Links use the system browser; a launch
  failure stays a toast rather than another blocking dialog.
- Item Details uses native `.property` rows: values are primary, selectable
  text, with quieter field names. Its height comes from the content, not a fixed
  empty reserve. Long values wrap; scrolling remains available in small windows.
- Colors come from the platform. Test both light and dark appearances, along
  with narrow layouts; avoid fixed-size promotional typography.

## Design references

- [GNOME Human Interface Guidelines](https://developer.gnome.org/hig/)
- [Typography](https://developer.gnome.org/hig/guidelines/typography.html)
- [Header bars](https://developer.gnome.org/hig/patterns/containers/header-bars.html)
- [GtkStackSwitcher](https://docs.gtk.org/gtk4/class.StackSwitcher.html)
- [Menus](https://developer.gnome.org/hig/patterns/controls/menus.html)
- [GtkPopoverMenu](https://docs.gtk.org/gtk4/class.PopoverMenu.html)
- [UI styling](https://developer.gnome.org/hig/guidelines/ui-styling.html)
- [Libadwaita widget gallery](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/widget-gallery.html)
- [Adaptive layouts](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/adaptive-layouts.html)
- [Adaptive dialog migration](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/migrating-to-adaptive-dialogs.html)

The current bindings expose modern APIs while the native minimum remains
libadwaita 1.5 / GTK 4.12. Using a recent Rust crate does not require using
unreleased native widgets.
