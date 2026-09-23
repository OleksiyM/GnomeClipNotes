//! Only linked by the short-lived editor executable.
use adw::prelude::*;
use std::{cell::RefCell, rc::Rc};
use webkit::prelude::*;

const CSP: &str = "default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src 'none'; connect-src 'none'; font-src 'none'; media-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

fn allowed_link(uri: &str, gesture: bool, kind: webkit::NavigationType) -> bool {
    gesture
        && kind == webkit::NavigationType::LinkClicked
        && glib::Uri::parse(uri, glib::UriFlags::NONE).is_ok_and(|uri| {
            matches!(uri.scheme().to_ascii_lowercase().as_str(), "http" | "https")
                && uri.host().is_some_and(|host| !host.is_empty())
        })
}

pub fn build(parent: &adw::ApplicationWindow) -> gnome_clip_notes::Preview {
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
    let context = webkit::WebContext::new();
    context.set_cache_model(webkit::CacheModel::DocumentViewer);
    let session = webkit::NetworkSession::new_ephemeral();
    session.set_proxy_settings(
        webkit::NetworkProxyMode::Custom,
        Some(&webkit::NetworkProxySettings::new(
            Some("gcn-disabled://127.0.0.1"),
            &[],
        )),
    );
    session.connect_download_started(|_, download| download.cancel());
    let view = webkit::WebView::builder()
        .settings(&settings)
        .web_context(&context)
        .network_session(&session)
        .default_content_security_policy(CSP)
        .hexpand(true)
        .vexpand(true)
        .build();
    let parent = parent.downgrade();
    view.connect_decide_policy(move |_, decision, kind| {
        if matches!(
            kind,
            webkit::PolicyDecisionType::NavigationAction
                | webkit::PolicyDecisionType::NewWindowAction
        ) {
            let action = decision
                .downcast_ref::<webkit::NavigationPolicyDecision>()
                .and_then(|d| d.navigation_action());
            if let Some(action) = action {
                let uri = action.request().and_then(|r| r.uri()).unwrap_or_default();
                if (uri == "about:blank" || uri.starts_with("about:blank#"))
                    && kind == webkit::PolicyDecisionType::NavigationAction
                {
                    decision.use_();
                } else {
                    decision.ignore();
                    if allowed_link(&uri, action.is_user_gesture(), action.navigation_type()) {
                        gtk::UriLauncher::new(&uri).launch(
                            parent.upgrade().as_ref(),
                            gio::Cancellable::NONE,
                            |result| {
                                if let Err(error) = result {
                                    eprintln!("Cannot open link: {error}");
                                }
                            },
                        );
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
    let pages = gtk::Stack::new();
    pages.set_vexpand(true);
    pages.add_named(&view, Some("document"));
    let message = adw::StatusPage::builder()
        .icon_name("dialog-warning-symbolic")
        .title(gnome_clip_notes::i18n::tr("Preview stopped"))
        .description(gnome_clip_notes::i18n::tr(
            "Switch to Editor and back to Preview to try again. Your note has not changed.",
        ))
        .build();
    pages.add_named(&message, Some("error"));
    let weak = pages.downgrade();
    view.connect_web_process_terminated(move |_, _| {
        if let Some(pages) = weak.upgrade() {
            pages.set_visible_child_name("error");
        }
    });
    let weak = pages.downgrade();
    view.connect_load_failed(move |_, _, _, _| {
        if let Some(pages) = weak.upgrade() {
            pages.set_visible_child_name("error");
        }
        true
    });
    let source = Rc::new(RefCell::new(String::new()));
    {
        let view = view.downgrade();
        let source = source.clone();
        let style = adw::StyleManager::default();
        style.connect_dark_notify(move |style| {
            if let Some(view) = view.upgrade() {
                view.load_html(
                    &crate::document::render(&source.borrow(), style.is_dark()),
                    Some("about:blank"),
                );
            }
        });
    }
    let widget = pages.clone().upcast();
    gnome_clip_notes::Preview {
        widget,
        set_content: Box::new(move |text| {
            *source.borrow_mut() = text.to_string();
            pages.set_visible_child_name("document");
            view.load_html(
                &crate::document::render(text, adw::StyleManager::default().is_dark()),
                Some("about:blank"),
            );
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_user_clicked_web_links_launch() {
        assert!(allowed_link(
            "https://example.org",
            true,
            webkit::NavigationType::LinkClicked
        ));
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,x",
            "https://",
        ] {
            assert!(!allowed_link(
                url,
                true,
                webkit::NavigationType::LinkClicked
            ));
        }
        assert!(!allowed_link(
            "https://example.org",
            false,
            webkit::NavigationType::LinkClicked
        ));
    }
}
