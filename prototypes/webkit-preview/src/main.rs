#[cfg(feature = "web")]
mod document;
#[cfg(feature = "web")]
mod security;

use adw::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
    time::{Duration, Instant},
};
#[cfg(feature = "web")]
use webkit::prelude::*;

#[cfg(feature = "web")]
const CSP: &str = "default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src 'none'; connect-src 'none'; font-src 'none'; media-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

#[cfg(feature = "web")]
thread_local! {
    // One lazy process pool/session. A new context on every toggle also creates
    // a new sandbox D-Bus proxy, whose lifetime can exceed that of the WebView.
    static WEB_ENV: std::cell::OnceCell<(webkit::WebContext, webkit::NetworkSession)> = const { std::cell::OnceCell::new() };
}

#[cfg(feature = "web")]
fn web_environment() -> (webkit::WebContext, webkit::NetworkSession) {
    WEB_ENV.with(|slot| {
        slot.get_or_init(|| {
            let context = webkit::WebContext::new();
            context.set_cache_model(webkit::CacheModel::DocumentViewer);
            let session = webkit::NetworkSession::new_ephemeral();
            // No DIRECT fallback or bypass hosts; fail before opening a socket,
            // including speculative preconnect before navigation policy runs.
            session.set_proxy_settings(
                webkit::NetworkProxyMode::Custom,
                Some(&webkit::NetworkProxySettings::new(
                    Some("gcn-disabled://127.0.0.1"),
                    &[],
                )),
            );
            session.connect_download_started(|_, download| download.cancel());
            (context, session)
        })
        .clone()
    })
}

#[derive(Default)]
struct Metrics {
    seen: HashSet<u32>,
}
impl Metrics {
    fn sample(&mut self, phase: &str) {
        let root = std::process::id();
        let mut processes = HashMap::new();
        for entry in std::fs::read_dir("/proc").unwrap().flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else {
                continue;
            };
            let parent = status
                .lines()
                .find_map(|line| line.strip_prefix("PPid:")?.trim().parse::<u32>().ok())
                .unwrap_or(0);
            processes.insert(pid, parent);
        }
        self.seen.insert(root);
        loop {
            let before = self.seen.len();
            for (&pid, parent) in &processes {
                if self.seen.contains(parent) {
                    self.seen.insert(pid);
                }
            }
            if self.seen.len() == before {
                break;
            }
        }
        let mut rows = Vec::new();
        let (mut pss, mut rss, mut missing) = (0u64, 0u64, 0u32);
        for &pid in &self.seen {
            if !processes.contains_key(&pid) {
                continue;
            }
            let Ok(memory) = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) else {
                missing += 1;
                continue;
            };
            let value = |prefix: &str| {
                memory
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix(prefix)?
                            .split_whitespace()
                            .next()?
                            .parse::<u64>()
                            .ok()
                    })
                    .unwrap_or(0)
            };
            let (p, r) = (value("Pss:"), value("Rss:"));
            pss += p;
            rss += r;
            rows.push(serde_json::json!({"pid":pid,"name":std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim(),"pss_kib":p,"rss_kib":r}));
        }
        println!(
            "{}",
            serde_json::json!({"phase":phase,"webkit":cfg!(feature="web"),"pss_kib":pss,"rss_kib":rss,"unreadable_processes":missing,"processes":rows})
        );
        assert_eq!(
            missing, 0,
            "Memory report must include every live child process"
        );
    }
}

#[cfg(feature = "web")]
fn external_link(uri: &str, gesture: bool, kind: webkit::NavigationType) -> bool {
    gesture
        && kind == webkit::NavigationType::LinkClicked
        && glib::Uri::parse(uri, glib::UriFlags::NONE).is_ok_and(|uri| {
            matches!(uri.scheme().to_ascii_lowercase().as_str(), "https" | "http")
                && uri.host().is_some()
        })
}

#[cfg(feature = "web")]
fn preview(
    parent: &adw::ApplicationWindow,
    automated: bool,
    loaded: Rc<Cell<bool>>,
    start: Instant,
) -> webkit::WebView {
    let settings = webkit::Settings::new();
    settings.set_enable_javascript(false);
    settings.set_enable_javascript_markup(false);
    settings.set_auto_load_images(false);
    settings.set_enable_html5_local_storage(false);
    settings.set_enable_page_cache(false);
    settings.set_allow_file_access_from_file_urls(false);
    settings.set_allow_universal_access_from_file_urls(false);
    settings.set_enable_media(false);
    settings.set_enable_media_stream(false);
    settings.set_enable_webgl(false);
    settings.set_javascript_can_access_clipboard(false);
    settings.set_javascript_can_open_windows_automatically(false);
    let (context, session) = web_environment();
    let view = webkit::WebView::builder()
        .settings(&settings)
        .web_context(&context)
        .network_session(&session)
        .default_content_security_policy(CSP)
        .vexpand(true)
        .hexpand(true)
        .build();
    let parent = parent.downgrade();
    view.connect_decide_policy(move |_, decision, kind| {
        if matches!(
            kind,
            webkit::PolicyDecisionType::NavigationAction
                | webkit::PolicyDecisionType::NewWindowAction
        ) {
            let navigation = decision
                .downcast_ref::<webkit::NavigationPolicyDecision>()
                .unwrap();
            if let Some(action) = navigation.navigation_action() {
                let uri = action.request().and_then(|r| r.uri()).unwrap_or_default();
                if uri == "about:blank" && kind == webkit::PolicyDecisionType::NavigationAction {
                    decision.use_();
                } else {
                    decision.ignore();
                    if external_link(&uri, action.is_user_gesture(), action.navigation_type()) {
                        if automated {
                            println!("EXTERNAL_LINK {uri}");
                        } else {
                            let launcher = gtk::UriLauncher::new(&uri);
                            launcher.launch(
                                parent.upgrade().as_ref(),
                                None::<&gio::Cancellable>,
                                |result| {
                                    if let Err(error) = result {
                                        eprintln!("Cannot open external link: {error}");
                                    }
                                },
                            );
                        }
                    } else {
                        println!("BLOCKED_NAVIGATION {uri}");
                    }
                }
            } else {
                decision.ignore();
            }
            return true;
        }
        false
    });
    view.connect_permission_request(|_, request| {
        request.deny();
        true
    });
    view.connect_load_changed(move |_, event| {
        if event == webkit::LoadEvent::Finished {
            loaded.set(true);
            println!(
                "{}",
                serde_json::json!({"document_loaded_ms":start.elapsed().as_secs_f64()*1000.0})
            );
        }
    });
    view.connect_load_failed(|_, _, uri, error| {
        eprintln!("LOAD_FAILED {uri}: {error}");
        false
    });
    view.connect_resource_load_started(|_, _, request| {
        println!("RESOURCE {}", request.uri().unwrap_or_default());
    });
    view
}

fn main() {
    let start = Instant::now();
    let args: Vec<String> = std::env::args().collect();
    let automated = args.iter().any(|s| s == "--auto");
    if automated {
        // Panics in spawned GLib futures otherwise leave an idle app running.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            previous(info);
            std::process::exit(1);
        }));
    }
    let large = args.iter().any(|s| s == "--large");
    let security_probe = args.iter().any(|s| s == "--security");
    let screenshots = args.iter().any(|s| s == "--screenshots");
    let keep_processes = args.iter().any(|s| s == "--keep-processes");
    let cycles = if args.iter().any(|s| s == "--soak") {
        10
    } else {
        3
    };
    let output = std::env::var("GCN_PROTOTYPE_OUTPUT").unwrap_or_else(|_| "/tmp".into());
    let source = include_str!("../fixtures/features.md");
    let source = if large {
        source.repeat(1_000_000 / source.len())
    } else {
        source.to_string()
    };
    let app = adw::Application::builder()
        .application_id("org.example.GcnWebkitPrototype")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        let window = adw::ApplicationWindow::builder().application(app).title("Markdown preview prototype")
            .default_width(780).default_height(720).build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let toggle = gtk::ToggleButton::with_label("Preview");
        header.pack_end(&toggle);
        toolbar.add_top_bar(&header);
        let pages = gtk::Stack::new();
        let text = gtk::TextView::builder().monospace(true).wrap_mode(gtk::WrapMode::WordChar)
            .left_margin(24).right_margin(24).top_margin(24).bottom_margin(24).build();
        text.buffer().set_text(&source);
        let editor = gtk::ScrolledWindow::builder().child(&text).build();
        pages.add_named(&editor, Some("editor"));
        toolbar.set_content(Some(&pages));
        window.set_content(Some(&toolbar));
        let loaded = Rc::new(Cell::new(false));
        let preview_widget = Rc::new(RefCell::new(None::<gtk::Widget>));
        {
            let pages = pages.clone(); let window = window.downgrade(); let text = text.clone();
            let loaded = loaded.clone(); let slot = preview_widget.clone();
            toggle.connect_toggled(move |toggle| {
                if toggle.is_active() {
                    let source = text.buffer().text(&text.buffer().start_iter(), &text.buffer().end_iter(), false);
                    loaded.set(false);
                    #[cfg(feature = "web")]
                    let widget: gtk::Widget = {
                        let opened = Instant::now();
                        let view = preview(&window.upgrade().unwrap(), automated, loaded.clone(), opened);
                        pages.add_named(&view, Some("preview"));
                        pages.set_visible_child_name("preview");
                        view.load_html(&document::render(&source, adw::StyleManager::default().is_dark()), Some("about:blank"));
                        view.upcast()
                    };
                    #[cfg(not(feature = "web"))]
                    let widget: gtk::Widget = {
                        let _ = (&window, automated);
                        let label = gtk::Label::new(Some(&source));
                        label.set_wrap(true);
                        pages.add_named(&label, Some("preview"));
                        pages.set_visible_child_name("preview");
                        loaded.set(true);
                        label.upcast()
                    };
                    *slot.borrow_mut() = Some(widget);
                } else {
                    pages.set_visible_child_name("editor");
                    if let Some(widget) = slot.borrow_mut().take() {
                        #[cfg(feature = "web")]
                        {
                            let view = widget.downcast_ref::<webkit::WebView>().unwrap();
                            view.stop_loading();
                        }
                        let _ = keep_processes;
                        let weak = widget.downgrade();
                        pages.remove(&widget);
                        assert!(widget.parent().is_none());
                        if !keep_processes {
                            // SAFETY: detached terminal widget, exclusively owned by
                            // this local value; no strong refs escape the preview slot.
                            // Callbacks never use the view after this point. Dispose
                            // closes WebKit's page/proxies; killing its process doesn't.
                            unsafe { widget.run_dispose(); }
                        }
                        drop(widget);
                        glib::timeout_add_local_once(Duration::from_millis(500), move || {
                            let released = weak.upgrade().is_none();
                            println!("{}", serde_json::json!({"view_released":released}));
                            if automated && !keep_processes { assert!(released, "Closed preview still retained"); }
                        });
                    }
                }
            });
        }
        window.connect_map(move |_| {
            let launch_ms = std::env::var("GCN_START_NS").ok().and_then(|s| s.parse::<u128>().ok()).map(|ns| {
                (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() - ns) as f64 / 1e6
            });
            println!("{}", serde_json::json!({"startup_to_map_ms":launch_ms,"main_to_map_ms":start.elapsed().as_secs_f64()*1000.0}));
        });
        window.present();
        if security_probe {
            #[cfg(feature = "web")]
            {
                let app = app.clone();
                glib::MainContext::default().spawn_local(async move {
                    security::verify(&window, &pages).await;
                    app.quit();
                });
            }
            return;
        }
        if automated {
            let app = app.clone(); let loaded = loaded.clone(); let output = output.clone();
            glib::MainContext::default().spawn_local(async move {
                let _ = (&output, screenshots);
                let mut metrics = Metrics::default();
                glib::timeout_future(Duration::from_secs(1)).await;
                metrics.sample("before_preview");
                if !cfg!(feature="web") { app.quit(); return; }
                for cycle in 0..cycles {
                    adw::StyleManager::default().set_color_scheme(if cycle == 1 { adw::ColorScheme::ForceDark } else { adw::ColorScheme::ForceLight });
                    toggle.set_active(true);
                    let deadline = Instant::now();
                    while !loaded.get() {
                        assert!(deadline.elapsed() < Duration::from_secs(30), "Preview load timeout");
                        glib::timeout_future(Duration::from_millis(25)).await;
                    }
                    glib::timeout_future(Duration::from_secs(1)).await;
                    metrics.sample(&format!("open_{cycle}"));
                    #[cfg(feature="web")]
                    if screenshots && cycle < 2 && !large {
                        let view = preview_widget.borrow().as_ref().unwrap().clone().downcast::<webkit::WebView>().unwrap();
                        let texture = view.snapshot_future(webkit::SnapshotRegion::Visible, webkit::SnapshotOptions::NONE).await.unwrap();
                        texture.save_to_png(format!("{output}/preview-{cycle}.png")).unwrap();
                    }
                    toggle.set_active(false);
                    glib::timeout_future(Duration::from_secs(1)).await;
                    metrics.sample(&format!("closed_{cycle}_1s"));
                    glib::timeout_future(Duration::from_secs(4)).await;
                    metrics.sample(&format!("closed_{cycle}_5s"));
                }
                glib::timeout_future(Duration::from_secs(5)).await;
                metrics.sample("closed_final_10s");
                app.quit();
            });
        }
    });
    app.run_with_args::<&str>(&[]);
}

#[cfg(all(test, feature = "web"))]
mod tests {
    use super::*;
    #[test]
    fn links_require_user_click_and_http_scheme() {
        assert!(external_link(
            "https://example.org/path",
            true,
            webkit::NavigationType::LinkClicked
        ));
        for uri in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,test",
            "mailto:test@example.org",
            "about:blank",
        ] {
            assert!(!external_link(
                uri,
                true,
                webkit::NavigationType::LinkClicked
            ));
        }
        assert!(!external_link(
            "https://example.org",
            false,
            webkit::NavigationType::LinkClicked
        ));
        assert!(!external_link(
            "https://example.org",
            true,
            webkit::NavigationType::Other
        ));
    }
}
