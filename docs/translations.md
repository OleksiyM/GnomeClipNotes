# Translations

English strings remain in the Rust and JavaScript sources and are the fallback.
The application resolves gettext messages; GNOME Shell receives already translated
message metadata and does not change the Shell process locale.
The first release offers System default and English only. System default follows
the session language when a shipped catalog exists and otherwise remains English.

Run `scripts/translations.py update` after changing user-visible text. It extracts
all production Rust sources and `extension/*.js`, updates `po/gnome-clip-notes.pot`
and `po/shell-messages.json`, and records the exact source set in `po/POTFILES.in`.
`scripts/translations.py check` verifies that generated files are current, checks
GNU gettext format metadata and named placeholders, and rejects missing catalogs.

Rust extraction (`update` and the full `check`) requires **GNU gettext 0.24 or
newer**. Ubuntu 24.04's distribution tools can build existing catalogs but cannot
extract Rust messages. Use a system with a recent gettext for source-message
changes; do not replace Rust extraction with a different language parser.

`scripts/translations.py check-catalogs` validates the checked-in catalogs and
named placeholders without re-extracting source strings. Archive builds use this
check with the distribution's gettext, then compile catalogs normally. Fedora CI
additionally runs the full `check`, including source/template freshness; passing
the catalog-only check does not establish that the extracted messages are current.

To add a language, add its locale to `po/LINGUAS`, create `po/<locale>.po`, and add
the same locale and its native display name to `po/languages.json`. English is
always the first registry entry and is not listed in `LINGUAS`. Build installable
catalogs with:

```sh
scripts/translations.py build --output target/locales
```

Cargo also compiles the listed catalogs into `OUT_DIR/locales` and exports that
directory as `GCN_BUILD_LOCALE_DIR` for uninstalled development builds. The local
installer copies `target/locales` beneath the application and Shell extension
locale directories. Release archives include these compiled catalogs, so installing
a prebuilt archive does not require gettext extraction tools. A source-tree install
compiles catalogs only when a locale listed in `LINGUAS` is missing; that path needs
`msgfmt`. `update` and the full `check` need the recent extraction toolchain;
`check-catalogs` needs `msginit`/`msgfmt`, and `pseudo` additionally needs `msgfilter`.

A developer-only catalog can be generated in an explicitly temporary location:

```sh
scripts/translations.py pseudo --output /tmp/gcn-pseudo
```

`en_XA` is deliberately absent from both `LINGUAS` and `languages.json`, so it is
never packaged or offered as a real language.

`scripts/test-i18n-runtime.sh` exercises the debug `--i18n-probe` in separate
processes. It verifies System default with the pseudolocale, explicit English,
an unknown saved language, an unsupported system locale, plural selection, and
the shared Rust/Shell message policy. The test uses only a guarded
`/tmp/gnome-clip-notes-i18n.XXXXXX` profile and retains that directory on both
success and failure for diagnosis. Its pseudolocale case requires an installed
`en_US.UTF-8` locale: gettext intentionally ignores `LANGUAGE` while
`LC_ALL=C.UTF-8`. The C-locale fallback is therefore checked in a separate case.

The placeholder validator maps every third-and-later target plural form back to
the English `msgid_plural`. `scripts/translations.py selftest` covers a temporary
three-form catalog and proves that a broken placeholder in its third form is
rejected; `check` runs this regression test automatically.

## Message contract

Use `tr("A complete message")` for plain text, `trf("Could not open {file}",
&[("file", name)])` for named values, and `ntrf("{count} item", "{count} items",
count, &[])` for quantities. `ntrf` supplies `{count}` itself. Translators may reorder
placeholders but must preserve their names. Values are substituted once, not
interpreted as further templates. GTK labels receive plain text; escape translated
text explicitly before embedding it in HTML or Pango markup.

Never assemble grammatical sentences from translated words, or select plural
forms with a Rust/JavaScript `count == 1` check. Independent quantities (such as
characters and words) each need their own plural lookup. `ptr(context, message)`
is available for ambiguous short words; `mark(message)` marks literals stored in
tables for translation at the point of display. User note titles, Markdown,
custom folder names and source application names are never translation keys.
Database identifiers, action names, settings values and `Notes.md` stay stable.

The English source is the fallback and remains next to its use in code. The
generated POT is the external translator-facing catalog, not a second manually
maintained copy of English. Only shipped, validated PO files enter `LINGUAS` and
the language picker; do not advertise untranslated languages.

## Runtime boundary

The process initializes gettext before GTK/GIO starts threads. Language preference
is read independently of the database; legacy settings default to `system`.
English bypasses lookup for the application domain. Changing the preference saves
it for the next main-process launch, without reinitializing gettext in a running
process or closing editors. Full/WebKit helpers receive the main process's active
language explicitly, so opening one before restart cannot mix UI policies.

The service sends a cached application-domain message map only in D-Bus metadata
queries. The extension uses this private map, keeps it if the service stops, and
refreshes its UI when the effective catalog changes. Search and custom filters
survive that refresh. Before the first service response, labels fall back to
English. Shell never changes its locale or `LANGUAGE`. There are currently no
count-dependent Shell messages; future ones must use service-side plural selection
instead of an English-only JavaScript plural rule.

Language selection does not change the system time zone or stored timestamps.
Native system calendars, portals and OS error text may still follow the desktop
locale. GNU gettext deliberately does not translate in C/POSIX sessions. Manual
non-English languages require an installed non-C message locale; adding the first
such language must test this on each supported distribution as well as normal
localized GNOME sessions.

`bash scripts/test-i18n-ui.sh` renders Library, every Settings page, a Native
editor and Export with elongated pseudolocale text in a private Xvfb session.
It checks user-data preservation and deferred language changes and keeps PNGs
under its temporary profile. `bash scripts/test-shell.sh` also exercises catalog
replacement during overlay opening, filter preservation and an unchanged Shell
locale. Neither test uses the user's database or running desktop session.
