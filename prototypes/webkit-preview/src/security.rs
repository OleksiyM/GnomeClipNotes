//! Runtime, black-box checks for the hardened WebKit preview.
//!
//! This is an automated prototype probe, not part of Markdown rendering. It
//! intentionally panics on a failed invariant so the isolated harness exits
//! unsuccessfully.

use std::{
    cell::Cell,
    io::ErrorKind,
    net::TcpListener,
    rc::Rc,
    time::{Duration, Instant},
};
use webkit::prelude::*;

const LOAD_TIMEOUT: Duration = Duration::from_secs(20);

/// Exercise renderer sanitization, WebKit settings, CSP, and navigation policy.
///
/// The probe uses only a loopback TCP canary: it never contacts the Internet.
/// It must be called from the GTK main context after `window` has been created.
pub async fn verify(window: &adw::ApplicationWindow, pages: &gtk::Stack) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind security canary");
    listener
        .set_nonblocking(true)
        .expect("make security canary non-blocking");
    let canary = format!("http://{}", listener.local_addr().unwrap());
    let control = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    assert_eq!(drain_canary(&listener), 1, "canary positive control failed");
    drop(control);

    let loaded = Rc::new(Cell::new(false));
    let view = crate::preview(window, true, loaded.clone(), Instant::now());
    pages.add_named(&view, Some("security-probe"));
    pages.set_visible_child(&view);

    let settings = WebViewExt::settings(&view).expect("preview must have WebKit settings");
    assert!(
        !settings.enables_javascript(),
        "JavaScript must be disabled"
    );
    assert!(
        !settings.enables_javascript_markup(),
        "JavaScript markup must be disabled"
    );
    assert!(
        !settings.is_auto_load_images(),
        "image loading must be disabled"
    );
    assert!(
        !settings.allows_file_access_from_file_urls(),
        "file URL access must be disabled"
    );
    assert!(
        !settings.allows_universal_access_from_file_urls(),
        "universal file URL access must be disabled"
    );

    // First layer: hostile author input must become an inert Markdown document.
    let markdown = format!(
        r#"# SECURITY_MARKDOWN_TITLE

<script>document.title='SCRIPTED_MARKDOWN';fetch('{canary}/markdown-script')</script>
<img src="{canary}/markdown-image">
<link rel="stylesheet" href="{canary}/markdown-css">
<iframe src="{canary}/markdown-frame"></iframe>

![remote image]({canary}/markdown-image-syntax)
[script](javascript:document.title='SCRIPTED_LINK')
[file](file:///tmp/gcn-security-canary)
[data](data:text/html,<script>document.title='SCRIPTED_DATA'</script>)
"#
    );
    loaded.set(false);
    view.load_html(
        &crate::document::render(&markdown, false),
        Some("about:blank"),
    );
    wait_for_load(&loaded, &listener).await;
    assert_eq!(
        view.title().as_deref(),
        Some("Markdown preview"),
        "Markdown content changed the document title"
    );
    assert_eq!(drain_canary(&listener), 0, "Markdown triggered network I/O");

    // Second layer: bypass the Markdown sanitizer on purpose. WebKit settings
    // and its default CSP must independently keep active content inert.
    let trusted_probe = format!(
        r#"<!doctype html><html><head><title>TRUSTED_PROBE</title>
<script>document.title='SCRIPTED_TRUSTED';fetch('{canary}/trusted-script')</script>
<link rel="stylesheet" href="{canary}/trusted-css">
<style>@import url("{canary}/trusted-import");</style>
</head><body>
<img src="{canary}/trusted-image">
<iframe src="{canary}/trusted-frame"></iframe>
</body></html>"#
    );
    loaded.set(false);
    view.load_html(&trusted_probe, Some("about:blank"));
    wait_for_load(&loaded, &listener).await;
    assert_eq!(
        view.title().as_deref(),
        Some("TRUSTED_PROBE"),
        "script ran despite disabled JavaScript/CSP"
    );
    assert_eq!(
        drain_canary(&listener),
        0,
        "trusted active-content probe triggered network I/O"
    );

    let stable_uri = view.uri().expect("loaded probe must have a URI");
    assert_eq!(stable_uri.as_str(), "about:blank");

    view.load_uri(&format!("{canary}/top-level"));
    settle(&listener).await;
    assert_eq!(
        view.uri().as_deref(),
        Some(stable_uri.as_str()),
        "programmatic HTTP navigation escaped the preview"
    );
    assert_eq!(drain_canary(&listener), 0, "HTTP navigation reached canary");

    view.load_uri("file:///tmp/gcn-nonexistent-security-canary");
    settle(&listener).await;
    assert_eq!(
        view.uri().as_deref(),
        Some(stable_uri.as_str()),
        "programmatic file navigation escaped the preview"
    );
    assert_eq!(drain_canary(&listener), 0, "file navigation reached canary");

    view.stop_loading();
    pages.remove(&view);
    println!("SECURITY_PROBE_OK");
}

async fn wait_for_load(loaded: &Cell<bool>, listener: &TcpListener) {
    let deadline = Instant::now() + LOAD_TIMEOUT;
    while !loaded.get() {
        assert!(Instant::now() < deadline, "security probe load timeout");
        assert_eq!(
            drain_canary(listener),
            0,
            "network I/O occurred while loading security probe"
        );
        glib::timeout_future(Duration::from_millis(25)).await;
    }
    // Resource loads may be scheduled immediately after the main load event.
    settle(listener).await;
}

async fn settle(listener: &TcpListener) {
    for _ in 0..10 {
        glib::timeout_future(Duration::from_millis(25)).await;
        assert_eq!(drain_canary(listener), 0, "security canary was contacted");
    }
}

fn drain_canary(listener: &TcpListener) -> usize {
    let mut connections = 0;
    loop {
        match listener.accept() {
            Ok((_stream, _address)) => connections += 1,
            Err(error) if error.kind() == ErrorKind::WouldBlock => return connections,
            Err(error) => panic!("security canary accept failed: {error}"),
        }
    }
}
