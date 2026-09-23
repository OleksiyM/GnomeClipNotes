//! Manual, metadata-only update check. Never downloads/executes release assets.
use gio::prelude::*;
use glib::translate::IntoGlib;
use soup::prelude::*;

pub const WEBSITE: &str = "https://oleksiym.github.io/GnomeClipNotes/";
pub const REPOSITORY: &str = "https://github.com/OleksiyM/GnomeClipNotes";
const ENDPOINT: &str = "https://api.github.com/repos/OleksiyM/GnomeClipNotes/releases/latest";
const MAX_BYTES: usize = 512 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Current,
    Available(String),
    NewerBuild,
    NoRelease,
    RateLimited,
    Invalid,
    Failed,
    Timeout,
}

fn version(text: &str) -> Option<[u64; 3]> {
    let parts: Vec<_> = text.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut parsed = [0; 3];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty()
            || (part.len() > 1 && part.starts_with('0'))
            || !part.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        parsed[i] = part.parse().ok()?;
    }
    Some(parsed)
}

fn decode(body: &[u8], current: &str) -> Outcome {
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
        draft: bool,
        prerelease: bool,
    }
    if body.len() > MAX_BYTES {
        return Outcome::Invalid;
    }
    let Ok(release) = serde_json::from_slice::<Release>(body) else {
        return Outcome::Invalid;
    };
    if release.draft || release.prerelease {
        return Outcome::Invalid;
    }
    let Some(tag) = release.tag_name.strip_prefix('v') else {
        return Outcome::Invalid;
    };
    let (Some(latest), Some(current)) = (version(tag), version(current)) else {
        return Outcome::Invalid;
    };
    match latest.cmp(&current) {
        std::cmp::Ordering::Greater => Outcome::Available(tag.to_owned()),
        std::cmp::Ordering::Equal => Outcome::Current,
        std::cmp::Ordering::Less => Outcome::NewerBuild,
    }
}

pub fn release_url(version_text: &str) -> Option<String> {
    version(version_text)?;
    Some(format!("{REPOSITORY}/releases/tag/v{version_text}"))
}

async fn request(session: &soup::Session) -> Outcome {
    let Ok(message) = soup::Message::new("GET", ENDPOINT) else {
        return Outcome::Failed;
    };
    message.set_flags(soup::MessageFlags::NO_REDIRECT | soup::MessageFlags::DO_NOT_USE_AUTH_CACHE);
    let Some(headers) = message.request_headers() else {
        return Outcome::Failed;
    };
    headers.append("Accept", "application/vnd.github+json");
    headers.append("X-GitHub-Api-Version", "2022-11-28");
    let Ok(stream) = session.send_future(&message, glib::Priority::DEFAULT).await else {
        return Outcome::Failed;
    };
    if let Some(outcome) = status_error(message.status().into_glib()) {
        return outcome;
    }
    read_response(&stream, env!("CARGO_PKG_VERSION")).await
}

fn status_error(status: i32) -> Option<Outcome> {
    match status {
        200 => None,
        404 => Some(Outcome::NoRelease),
        403 | 429 => Some(Outcome::RateLimited),
        _ => Some(Outcome::Failed),
    }
}

async fn read_response(stream: &impl IsA<gio::InputStream>, current: &str) -> Outcome {
    let mut body = Vec::new();
    loop {
        let Ok(chunk) = stream
            .read_bytes_future(8192, glib::Priority::DEFAULT)
            .await
        else {
            return Outcome::Failed;
        };
        if chunk.is_empty() {
            break;
        }
        if body.len() + chunk.len() > MAX_BYTES {
            return Outcome::Invalid;
        }
        body.extend_from_slice(chunk.as_ref());
    }
    decode(&body, current)
}

pub async fn check() -> Outcome {
    // Also abort when closing the dialog drops this future mid-request.
    struct SessionGuard(soup::Session);
    impl Drop for SessionGuard {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let session = SessionGuard(
        soup::Session::builder()
            .timeout(15)
            .user_agent("GnomeClipNotes-update-check")
            .build(),
    );
    // Includes DNS, response headers and streaming body, not just idle I/O time.
    let result =
        glib::future_with_timeout(std::time::Duration::from_secs(20), request(&session.0)).await;
    result.unwrap_or(Outcome::Timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body(tag: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"tag_name":tag,"draft":false,"prerelease":false}))
            .unwrap()
    }
    #[test]
    fn numeric_versions_not_lexical() {
        assert_eq!(
            decode(&body("v1.10.0"), "1.9.9"),
            Outcome::Available("1.10.0".into())
        );
        assert_eq!(decode(&body("v1.0.0"), "1.0.0"), Outcome::Current);
        assert_eq!(decode(&body("v1.0.0"), "1.1.0"), Outcome::NewerBuild);
    }
    #[test]
    fn refuses_unsafe_tags_and_nonstable_releases() {
        for tag in [
            "v1.0.0-beta",
            "v1.0.0+build",
            "v01.0.0",
            "1.0.0",
            "v1.0",
            "v1.0.0/../../bad",
            "v18446744073709551616.0.0",
        ] {
            assert_eq!(decode(&body(tag), "1.0.0"), Outcome::Invalid);
        }
        assert_eq!(
            decode(
                br#"{"tag_name":"v2.0.0","draft":true,"prerelease":false}"#,
                "1.0.0"
            ),
            Outcome::Invalid
        );
        assert_eq!(
            decode(
                br#"{"tag_name":"v2.0.0","draft":false,"prerelease":true}"#,
                "1.0.0"
            ),
            Outcome::Invalid
        );
        assert_eq!(decode(b"{}", "1.0.0"), Outcome::Invalid);
        assert_eq!(
            decode(&vec![b' '; MAX_BYTES + 1], "1.0.0"),
            Outcome::Invalid
        );
    }
    #[test]
    fn download_target_is_constructed_locally() {
        assert_eq!(
            release_url("1.2.3").unwrap(),
            "https://github.com/OleksiyM/GnomeClipNotes/releases/tag/v1.2.3"
        );
        assert!(release_url("https://evil.example").is_none());
        assert!(release_url("1.2.3?token=secret").is_none());
    }

    #[test]
    fn unsuccessful_http_status_is_never_up_to_date() {
        assert_eq!(status_error(200), None);
        assert_eq!(status_error(404), Some(Outcome::NoRelease));
        for status in [403, 429] {
            assert_eq!(status_error(status), Some(Outcome::RateLimited));
        }
        for status in [204, 301, 302, 401, 500, 503] {
            assert_eq!(status_error(status), Some(Outcome::Failed));
        }
    }

    #[test]
    fn streaming_response_is_bounded_and_validated_offline() {
        let context = glib::MainContext::new();
        context
            .with_thread_default(|| {
                context.block_on(async {
                    for (bytes, expected) in [
                        (body("v1.0.0"), Outcome::Current),
                        (Vec::new(), Outcome::Invalid),
                        (b"<html>unavailable</html>".to_vec(), Outcome::Invalid),
                        (vec![b' '; MAX_BYTES + 1], Outcome::Invalid),
                    ] {
                        let stream =
                            gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
                        assert_eq!(read_response(&stream, "1.0.0").await, expected);
                    }
                    let stream = gio::MemoryInputStream::new();
                    stream.close(None::<&gio::Cancellable>).unwrap();
                    assert_eq!(read_response(&stream, "1.0.0").await, Outcome::Failed);
                })
            })
            .unwrap();
    }
}
