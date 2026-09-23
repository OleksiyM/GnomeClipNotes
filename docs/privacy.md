# Privacy and current limitations

Clipboard history is sensitive by nature. GnomeClipNotes stores captured plain
text locally in SQLite and does not synchronize or upload that content. The
database is not encrypted at rest. Anyone or any process able to read the user
account's application-data directory may read unprotected history and notes.

Password and sensitive-content filtering is best effort. Applications do not
always expose enough metadata to identify a password field, and clipboard text
has no reliable sensitivity marker. Use pause capture or the ignored-app list
before copying secrets. Retention deletes only entries in History. Notes and
all user-created folders are protected, including captured entries moved there.
They have no separate password, concealment, or encryption feature.

Cleanup runs when the background service starts, every six hours while it
runs, and after a confirmed shortening of the retention period. Age is measured
from the entry's last copy time. Forever disables expiry. A month is 30 days;
a year is 365 days. Moving the settings slider does not apply the policy:
Apply commits it, with a deletion count and confirmation for shorter periods.

Only plain text is captured. Markdown notes store Markdown source as plain text.
Rich HTML, images, files, cross-device sync, and clipboard-manager interoperability
are outside the initial release. Automatic paste depends on GNOME Shell and the
target application; sandboxed applications or protected input fields may reject
injected paste events.

Full Markdown preview runs in a separate short-lived WebKitGTK editor process.
It has no JavaScript, raw author HTML, file access or external resource loading.
Clicking a permitted web link deliberately opens the system browser, which has
its own networking and privacy settings. Native preview creates no browser process.
Neither preview changes or uploads the stored Markdown source.

## Manual update check

About shows the installed version and local update status. Only **Check for Updates**
contacts the GitHub API for the latest public stable release of
`OleksiyM/GnomeClipNotes`. Opening the application or About does not
start a check; there is no background polling or automatic installation.

The HTTPS request contains a fixed application user agent and public API headers,
not notes, clipboard text, database identifiers or authentication credentials.
GitHub can see the connection's IP address and request metadata. Responses are
bounded and time-limited; closing the dialog cancels an unfinished check.
**View Release** and the other project links open the system browser, whose
networking and privacy settings apply independently.
