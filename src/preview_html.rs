//! Rendering of untrusted Markdown for the WebKit preview.
//!
//! The parser is deliberately the only source of document markup. Raw HTML is
//! turned into text, links are reduced to a small allow-list, and images become
//! text placeholders before `pulldown_cmark`'s HTML renderer sees the events.

use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};

const CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src 'none'; \
                   base-uri 'none'; form-action 'none'; frame-src 'none'";

/// Render untrusted Markdown as a complete, self-contained HTML document.
pub fn render(markdown: &str, dark: bool) -> String {
    let parser = Parser::new_ext(
        markdown,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS,
    );
    let mut safe_events = Vec::new();
    let mut link_stack = Vec::new();
    let mut image_depth = 0usize;
    let mut image_alt = String::new();

    for event in parser {
        if image_depth > 0 {
            match event {
                Event::Start(Tag::Image { .. }) => image_depth += 1,
                Event::End(TagEnd::Image) => {
                    image_depth -= 1;
                    if image_depth == 0 {
                        let alt = image_alt.split_whitespace().collect::<Vec<_>>().join(" ");
                        let label = if alt.is_empty() {
                            "[image]".to_owned()
                        } else {
                            format!("[image: {alt}]")
                        };
                        safe_events.push(Event::Text(label.into()));
                        image_alt.clear();
                    }
                }
                Event::Text(text) | Event::Code(text) => image_alt.push_str(&text),
                Event::SoftBreak | Event::HardBreak => image_alt.push(' '),
                _ => {}
            }
            continue;
        }

        match event {
            Event::Html(raw) | Event::InlineHtml(raw) => {
                // Treat author-supplied HTML literally; push_html will escape it.
                safe_events.push(Event::Text(raw));
            }
            Event::Start(Tag::Image { .. }) => {
                image_depth = 1;
                image_alt.clear();
            }
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                let href = allowed_href(&dest_url);
                link_stack.push(href.is_some());
                if let Some(href) = href {
                    let href = escape_attribute(href);
                    let title = if title.is_empty() {
                        String::new()
                    } else {
                        format!(" title=\"{}\"", escape_attribute(&title))
                    };
                    let external = if href.starts_with('#') {
                        ""
                    } else {
                        " target=\"_blank\" rel=\"noopener noreferrer\""
                    };
                    safe_events.push(Event::Html(
                        format!("<a href=\"{href}\"{title}{external}>").into(),
                    ));
                }
            }
            Event::End(TagEnd::Link) => {
                if link_stack.pop().unwrap_or(false) {
                    safe_events.push(Event::Html(CowStr::Borrowed("</a>")));
                }
            }
            other => safe_events.push(other),
        }
    }

    let mut body = String::new();
    html::push_html(&mut body, safe_events.into_iter());
    page(&body, dark)
}

fn allowed_href(href: &str) -> Option<&str> {
    if href.starts_with('#') && !href.chars().any(char::is_control) {
        return Some(href);
    }
    let prefix = href.get(..8).unwrap_or(href);
    if prefix
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case("http://"))
        || prefix.eq_ignore_ascii_case("https://")
    {
        Some(href)
    } else {
        None
    }
}

fn escape_attribute(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            c if c.is_control() => escaped.push(' '),
            c => escaped.push(c),
        }
    }
    escaped
}

fn page(body: &str, dark: bool) -> String {
    let scheme = if dark { "dark" } else { "light" };
    format!(
        r#"<!doctype html>
<html lang="en" data-theme="{scheme}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="{CSP}">
<title>{title}</title>
<style>
:root {{ color-scheme: {scheme}; font: 16px/1.55 system-ui, sans-serif; }}
* {{ box-sizing: border-box; }}
body {{ max-width: 52rem; margin: 0 auto; padding: 1.25rem; color: CanvasText; background: Canvas; overflow-wrap: anywhere; }}
h1, h2, h3, h4, h5, h6 {{ line-height: 1.2; margin: 1.4em 0 .55em; }}
p, ul, ol, blockquote, pre, table {{ margin: 0 0 1em; }}
ul, ol {{ padding-inline-start: 1.7em; }}
blockquote {{ margin-inline: 0; padding-inline: 1em; border-inline-start: 3px solid GrayText; color: color-mix(in srgb, CanvasText 70%, Canvas); }}
code, pre {{ font-family: ui-monospace, monospace; background: color-mix(in srgb, CanvasText 8%, Canvas); border-radius: .35rem; }}
code {{ padding: .1em .3em; }}
pre {{ padding: .8rem; overflow-x: auto; overflow-wrap: normal; }}
pre code {{ padding: 0; background: none; white-space: pre; }}
table {{ width: 100%; border-collapse: collapse; display: block; overflow-x: auto; }}
th, td {{ padding: .4rem .6rem; border: 1px solid color-mix(in srgb, CanvasText 25%, Canvas); text-align: start; }}
a {{ color: LinkText; }}
input[type="checkbox"] {{ margin-inline: 0 .45em; }}
</style>
</head>
<body>{body}</body>
</html>"#,
        title = escape_attribute(&gnome_clip_notes::i18n::tr("Markdown preview"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_full_document_and_requested_theme() {
        let output = render("# Title", true);
        assert!(output.starts_with("<!doctype html>"));
        assert!(output.contains("data-theme=\"dark\""));
        assert!(output.contains("<h1>Title</h1>"));
        assert!(output.contains("default-src 'none'"));
        assert!(output.contains("img-src 'none'"));
    }

    #[test]
    fn raw_html_is_visible_text_not_markup() {
        let output = render("<script>alert('x')</script> <b onclick=x>bold</b>", false);
        assert!(!output.contains("<script>"));
        assert!(!output.contains("<b onclick"));
        assert!(output.contains("&lt;script&gt;alert('x')&lt;/script&gt;"));
        assert!(output.contains("&lt;b onclick=x&gt;bold&lt;/b&gt;"));
    }

    #[test]
    fn only_web_and_fragment_links_survive() {
        let source = r#"[web](https://example.com/?a=1&b=\"q\") [anchor](#part)
[js](javascript:alert(1)) [data](data:text/html,x) [file](file:///etc/passwd)"#;
        let output = render(source, false);
        assert!(output.contains("href=\"https://example.com/?a=1&amp;b="));
        assert!(output.contains("target=\"_blank\" rel=\"noopener noreferrer\""));
        assert!(output.contains("href=\"#part\""));
        assert!(!output.contains("href=\"javascript:"));
        assert!(!output.contains("href=\"data:"));
        assert!(!output.contains("href=\"file:"));
        assert!(output.contains("js"));
        assert!(output.contains("data"));
        assert!(output.contains("file"));
    }

    #[test]
    fn images_are_inert_alt_text_placeholders() {
        let output = render("![remote *kitten*](https://example.com/cat.png)", false);
        assert!(output.contains("[image: remote kitten]"));
        assert!(!output.contains("example.com/cat.png"));
        assert!(!output.contains("<img"));
    }

    #[test]
    fn tables_tasks_code_quotes_and_nested_lists_render() {
        let source = r#"> quoted `code`

- [x] done
  1. child & escaped

| A | B |
|---|---|
| 1 | 2 |"#;
        let output = render(source, false);
        assert!(output.contains("<blockquote>"));
        assert!(output.contains("<code>code</code>"));
        assert!(output.contains("type=\"checkbox\""));
        assert!(output.contains("checked=\"\""));
        assert!(output.contains("disabled=\"\""));
        assert!(output.contains("<ol>"));
        assert!(output.contains("child &amp; escaped"));
        assert!(output.contains("<table>"));
        assert!(output.contains("<th>A</th>"));
    }

    #[test]
    fn pathological_quotes_cannot_break_generated_attributes() {
        let output = render(
            r#"[x](<https://example.com/%22%27%3C%3E> "title &'<>\"")"#,
            false,
        );
        assert!(!output.contains("<script"));
        assert!(!output.contains("onclick="));
        assert!(output.contains("href=\"https://example.com/%22%27%3C%3E\""));
        assert_eq!(escape_attribute("a&b'\"<>"), "a&amp;b&#39;&quot;&lt;&gt;");
    }
}
