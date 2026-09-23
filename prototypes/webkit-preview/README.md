# Isolated WebKitGTK Markdown preview

This is a separate experiment, not the application's active editor. It never
opens the ClipNotes database or attaches to its application instance.

Pipeline: `pulldown-cmark 0.12 → filtered events → HTML → WebKitGTK 6.0`.
No handwritten text layout, Markdown grammar, or table/list rendering engine.
Supported fixture: headings, nested lists, tables, read-only task lists, quotes,
inline/fenced code, emphasis, strikethrough, and HTTP(S) links.

## Production outcome

The experiment informed the application's two preview modes: Full (WebKitGTK)
and a deliberately limited Native GTK preview. It did **not** justify embedding
WebKit in the persistent clipboard service. Repeated disposal left sandbox
helpers alive on the tested stack; see the dated [results](RESULTS.md).

Production Full preview instead lives in a separate, short-lived editor process.
One WebView is reused while that editor window is open. Closing the window ends
its systemd user service and remaining processes in that service's control group;
returning to the Editor tab alone does not unload WebKit. Native does not start a
WebKit process. Implementation
and verification are described in [Note preview](../../docs/preview.md).

Keep this prototype as experimental evidence, not as a second implementation to
ship or maintain alongside the active editor. Its measured PSS/startup numbers
describe the prototype, not the final application. Production tests own a separate
fixture under `tests/fixtures/` and do not require this directory to build.

## Run

Requires the main project's GTK4/libadwaita development dependencies plus
`webkitgtk-6.0` (Fedora: `webkitgtk6.0-devel`; Rust bindings: `webkit6`).

```sh
cargo build --release --locked --manifest-path prototypes/webkit-preview/Cargo.toml
bash prototypes/webkit-preview/run-session.sh --security
bash prototypes/webkit-preview/run-session.sh
bash prototypes/webkit-preview/run-session.sh --large
bash prototypes/webkit-preview/run-session.sh --screenshots
```

The harness creates an isolated headless GNOME Wayland session, private D-Bus and
temporary XDG directories. Results are printed as `/tmp/gcn-webkit-prototype.*`.
WebKit's own process sandbox remains enabled. The user's current application,
notes, clipboard and browser are not operated by the harness.

For interactive inspection, launch `target/release/gcn-webkit-preview-prototype`
from this directory **without** `--auto`. It contains an editable fixture and a
Preview toggle. Clicking a permitted link then uses GTK's external URI launcher.
Automated mode logs the handoff instead of opening a real browser.

## Security boundary

- Author raw HTML is displayed as escaped text, never interpreted.
- Images become alt-text placeholders; their URLs are discarded.
- Only HTTP(S) and fragment anchors survive Markdown filtering. External launch
  additionally requires a genuine user gesture, link-click navigation, a parsed
  HTTP(S) URI and a host. Other navigations and permission requests are denied.
- JavaScript and JavaScript markup, automatic images, media, WebGL, local storage,
  page cache, clipboard scripting and file-URL access are disabled.
- Both the HTML document and WebView have restrictive CSP. No network styles,
  fonts, images, frames, scripts, forms, or base URL overrides are allowed.
- A private ephemeral session cancels downloads. Its custom proxy has an
  unsupported `gcn-disabled` protocol, no bypass hosts and no DIRECT fallback.
  This fails before opening a network socket on the tested WebKit/libsoup/GIO
  stack, including speculative preconnect that happens before navigation policy.
  It is a tested fail-closed guard, **not** a documented OS-level offline sandbox;
  rerun the security regression test after engine updates.

The runtime security probe checks a loopback canary with a positive control,
hostile Markdown, deliberately unsanitized test HTML, unchanged script-sensitive
document titles, HTTP/file navigation denial and zero canary connections.
It exercises the combined protections, not every CSP directive independently.
It does not prove engine vulnerability immunity, all possible delayed requests,
or end-to-end external browser launch. Fragment target generation is not part of
this prototype.

## Measurement protocol

Release builds; one process at a time; no screenshots/security probes during
performance samples. Shell and portals start before timing the application.
Startup means process launch to GTK window mapping, **not** first rendered frame
or system cold boot. `document_loaded_ms` includes view creation, conversion and
WebKit's Finished load event, not a first-paint measurement.

Samples: before first Preview; one second after each of three opens; one and five
seconds after each close; ten seconds after the final close. The regular fixture
is 1,079 UTF-8 bytes. `--large` repeats it to 999,154 bytes, exercising many DOM
nodes rather than one long paragraph. Light/dark/light cycles are tested.

Memory is summed PSS from `/proc/*/smaps_rollup` for the application and all
discovered descendants, including WebKit network/web processes, bwrap and D-Bus
proxies. RSS is logged too, but double-counts shared libraries. Missing readable
processes fail the run. The compositor, separately activated portal services and
GPU allocations are not included. Tracking is sufficient for these short runs,
not robust against arbitrary PID reuse or children reparented before discovery.
Three cycles detect gross retention, not all long-term leaks.

For a no-WebKit baseline, build with `--no-default-features`, retain that binary,
then use `GCN_PROTOTYPE_BINARY=/absolute/path/to/baseline` with the same harness.
The GTK editor/window is otherwise the same. It exits after the pre-preview sample.

`--keep-processes` is a diagnostic regression mode that removes the view without
explicit disposal; do not use its retained-memory numbers as the final design.
`--soak` runs ten open/close cycles instead of three.

The WebView is created on demand and explicitly disposed after detachment when
Preview closes. Its Rust references must not escape the owned slot, and it must
never be used after disposal. One lazy WebContext and ephemeral NetworkSession
are reused across opens. This intentionally retains shared WebKit infrastructure
after first use, but not the rendered document/process. Reusing the context did
**not** eliminate sandbox helper accumulation on the tested stack: this remains
an integration blocker, described in the results. The prototype is not a
production-ready lifecycle implementation.

Results and the integration decision are recorded in [RESULTS.md](RESULTS.md).
