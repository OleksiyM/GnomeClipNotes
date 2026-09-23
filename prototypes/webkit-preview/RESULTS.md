# WebKitGTK preview experiment — 2026-09-13

## Decision

Keep WebKitGTK as the preferred Markdown display engine. Do not extend our own
Pango renderer. Feature coverage and on-demand performance are promising, but
**do not integrate this prototype into the persistent clipboard process yet**:
repeated preview destruction leaves sandbox helpers alive in this environment.
The experiment is complete; unconditional production approval is not.

The main application/editor, its Cargo dependencies and installed application
were not changed. WebKitGTK development headers were installed for this separate
prototype; the runtime was already present.

## Measured cost

Intel i5-10310U, 16 GB RAM, Fedora 44, GNOME Shell 50.4, GTK 4.22.5,
libadwaita 1.9.3, WebKitGTK 2.52.5; native GTK4 API (`webkitgtk-6.0`).
Headless GNOME Wayland, WebKit sandbox enabled, normal hardware renderer settings.
These are **prototype measurements**, not the full ClipNotes application's memory.
PSS includes the UI and discovered child processes; it apportions shared pages.

| State | Total PSS |
|---|---:|
| Same editor shell built without WebKit, before Preview | 57.8–58.1 MiB |
| WebKit-linked editor, before Preview | 70.1–70.2 MiB |
| First Preview, 1,079-byte feature fixture | 181.9–182.2 MiB |
| One second after first close | 101.8–101.9 MiB |
| After three closes | 103.1–103.2 MiB |
| Ten seconds after tenth close (one ten-cycle run) | 108.6 MiB |

Three baseline launches: median **164 ms**, range 161–169 ms, to window mapping.
Three WebKit-linked launches: median **228 ms**, range 214–231 ms.
First Preview: median **366 ms**, range 360–387 ms. Reopens: **238–268 ms**.
Thus linking adds about 12 MiB before first use; first Preview adds about 112 MiB.
About 32 MiB remain after the first close; later helper accumulation is additional.

The 999,154-byte repeated Markdown stress fixture took **1.063 s** for first load,
and **0.916–0.973 s** on reopens. Total PSS was **321–335 MiB** while open,
**76.3 MiB** before opening, and **122.6 MiB** after three closes. This is a
DOM-heavy synthetic document, not a typical note or a universal worst case.

Startup measures mapping, not first paint. OS caches were not flushed. These are
snapshots rather than peak-memory measurements. External compositor/portal and
GPU memory are excluded. See [protocol](README.md#measurement-protocol).

## What the lifecycle test found

1. Removing the WebView from GtkStack and dropping Rust references did not release
   the native widget in the original experiment; full WebProcesses accumulated.
2. Explicit terminal disposal after detaching the widget releases it: weak-reference
   checks pass on every close, and the WebProcess disappears.
3. Neither killing just the WebProcess nor reusing one WebContext/NetworkSession
   prevents the remaining helper accumulation. The final ten-cycle run had
   **32 live processes after closing**: UI + one NetworkProcess + ten sets of
   `xdg-dbus-proxy` and two `bwrap` processes. After the first close there were five.
   Closed-state PSS rose from 101.9 to 108.6 MiB over those ten cycles.
4. All 32 recorded PIDs were gone after the entire isolated test session exited.
   This does not distinguish application exit from private D-Bus teardown.

An existing [WebKit report 279913](https://bugs.webkit.org/show_bug.cgi?id=279913)
describes similar helper retention after WebKit's actual worker processes exit.
Its investigation concerns Flatpak/portal behavior; our run is native. This is a
matching symptom and investigation lead, **not proof of the same root cause**.

Before integration: reproduce/check cleanup in a normal desktop session and
resolve the helper lifecycle. If deterministic release requires a short-lived
editor process, benchmark that design and its shutdown independently before
changing the main application. Do not disable the WebKit sandbox, silently accept
unbounded helpers, or add broad process-killing workarounds.

## Functional and security checks

- Seven release unit tests passed; release Clippy with `-D warnings` and rustfmt
  checks passed. Shell runner syntax check passed.
- Light/dark screenshots inspected: headings, nested lists, task checkboxes,
  quotes, inline/fenced code, tables and links render correctly. Quote contrast
  was strengthened during inspection. This is still prototype typography.
- Runtime probe: `SECURITY_PROBE_OK` on the final shared-context build. Hostile
  Markdown and deliberately active test HTML did not contact the loopback canary;
  script-sensitive titles stayed unchanged; HTTP/file navigation was rejected.
- Navigation policy alone was insufficient: the initial HTTP navigation test
  observed a connection before rejection. A fail-closed custom proxy guard now
  prevents this in the tested stack. Its portability requires regression tests.
- External HTTP(S) user-click handoff is implemented using GtkUriLauncher, with
  a tested allow-list. Actual external browser launch was deliberately suppressed
  in automation and is **not end-to-end verified**. Local images, raw author HTML,
  JavaScript, remote resources and arbitrary URI schemes are not supported.

## Reproducible evidence

[measurements.json](measurements.json) preserves all final samples and per-process
PSS/RSS. Final binary SHA-256:
`f428bd2599db9708a345ef12bcaef8c5949b75d4c0bb2b663957a9501d0b1194`.

Raw harness directories under `/tmp/gcn-webkit-prototype.`:

| Run | Suffix |
|---|---|
| Baseline ×3 | `dZ1c9v`, `eg2tWo`, `beVHn2` |
| Regular ×2 | `Eiorli`, `UBsAoM` |
| Regular ten-cycle run | `R4nSn4` |
| Near-1-MB document | `hD1IH8` |
| Final security probe | `BxHGLJ` |
| Final typography screenshots (before context-reuse change) | `Rtgoan` |

Intermediate lifecycle experiments are intentionally excluded from the final
timing/memory table. The three regular launches include the ten-cycle run's first
open; later soak cycles are reported separately.
