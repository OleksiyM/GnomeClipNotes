use crate::model::{kind, MAX_CONTENT};
use htmd::element_handler::{HandlerResult, Handlers};
use htmd::options::{BulletListMarker, Options};
use htmd::Element;

/// Checks if a `<b ...>` tag attribute string represents a non-bold wrapper
/// (such as Google Docs' `<b style="font-weight:normal;" id="docs-internal-guid-...">`).
fn is_non_bold_b_tag(tag_attrs: &str) -> bool {
    let compact: String = tag_attrs
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    compact.contains("docs-internal-guid")
        || compact.contains("font-weight:normal")
        || compact.contains("font-weight:400")
}

/// Inspects HTML to determine whether it contains semantic rich-text tags
/// (headings, links, lists, tables, quotes, code, formatting) rather than just
/// editor syntax highlighting (e.g., `<span style="color:...">` or `<pre><span>` in IDEs).
pub fn has_semantic_html(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();

    // Check for structural & inline formatting tags that clearly indicate rich text.
    // Note: `<pre` alone is intentionally excluded because JetBrains IDEs wrap copied
    // code in `<pre><span style="...">` without `<code>`, whereas web code blocks use `<code>`.
    let semantic_tags = [
        "<a ",
        "<a\t",
        "<a\n",
        "<a\r",
        "<a>",
        "<h1",
        "<h2",
        "<h3",
        "<h4",
        "<h5",
        "<h6",
        "<ul",
        "<ol",
        "<li",
        "<table",
        "<tr",
        "<th",
        "<td",
        "<blockquote",
        "<code>",
        "<code ",
        "<code\t",
        "<code\n",
        "<strong",
        "<b>",
        "<em>",
        "<em ",
        "<i>",
        "<i ",
        "<del",
        "<s>",
        "<s ",
        "<strike",
        "<hr>",
        "<hr ",
        "<hr/",
    ];

    if semantic_tags.iter().any(|tag| lower.contains(tag)) {
        return true;
    }

    // Check `<b ...>` tags that are not Google Docs `font-weight:normal` wrappers
    let mut search_from = 0;
    while let Some(pos) = lower[search_from..].find("<b") {
        let idx = search_from + pos;
        let after = &lower[idx + 2..];
        if after.starts_with(' ') || after.starts_with('\t') || after.starts_with('\n') {
            if let Some(end) = after.find('>') {
                if !is_non_bold_b_tag(&after[..end]) {
                    return true;
                }
            }
        }
        search_from = idx + 2;
    }

    // Google Docs uses `<span style="...font-weight:700...">` and `font-style:italic`
    // inside `docs-internal-guid` wrappers instead of `<b>`/`<i>`.
    if lower.contains("docs-internal-guid") {
        let compact: String = lower.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        if compact.contains("font-weight:700")
            || compact.contains("font-weight:bold")
            || compact.contains("font-style:italic")
        {
            return true;
        }
    }

    false
}

/// Preprocesses clipboard HTML quirks before DOM parsing:
/// 1. Replaces Google Docs' outer `<b style="font-weight:normal"...>` inline wrapper
///    with `<div ...>` so `html5ever` does not duplicate `<b>` across block elements.
/// 2. For tables without `<th>`/`<thead>` (e.g. Google Docs, Google Sheets, Calc),
///    promotes `<td>` cells in the first `<tr>` row to `<th>` if there are at least 2 columns,
///    allowing `htmd` to render a valid GFM pipe table.
fn preprocess_clipboard_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let lower = html.to_ascii_lowercase();
    let mut last = 0;

    // 1. Neutralize `<b ...>` tags that have `font-weight:normal` or `docs-internal-guid`
    let mut cursor = 0;
    while let Some(pos) = lower[cursor..].find("<b") {
        let idx = cursor + pos;
        let after = &lower[idx + 2..];
        if after.starts_with(' ') || after.starts_with('\t') || after.starts_with('\n') {
            if let Some(end) = after.find('>') {
                let attrs = &after[..end];
                if is_non_bold_b_tag(attrs) {
                    out.push_str(&html[last..idx]);
                    out.push_str("<div");
                    last = idx + 2;
                }
            }
        }
        cursor = idx + 2;
    }
    out.push_str(&html[last..]);

    // 2. Promote first row `<td>` to `<th>` in tables that lack `<th>`/`<thead>`
    let out_lower = out.to_ascii_lowercase();
    if out_lower.contains("<table") && !out_lower.contains("<th") {
        if let Some(tr_start) = out_lower.find("<tr") {
            if let Some(tr_end_rel) = out_lower[tr_start..].find("</tr>") {
                let tr_end = tr_start + tr_end_rel + 5;
                let first_row_lower = &out_lower[tr_start..tr_end];
                let td_count = first_row_lower.matches("<td").count();
                if td_count >= 2 {
                    let mut promoted_row = String::with_capacity(tr_end - tr_start);
                    let row_orig = &out[tr_start..tr_end];
                    let mut r_last = 0;
                    let mut r_cur = 0;
                    while r_cur < first_row_lower.len() {
                        if first_row_lower[r_cur..].starts_with("<td") {
                            let next_char = first_row_lower[r_cur + 3..].chars().next();
                            if matches!(next_char, Some('>' | ' ' | '\t' | '\n' | '\r' | '/')) {
                                promoted_row.push_str(&row_orig[r_last..r_cur]);
                                promoted_row.push_str("<th");
                                r_last = r_cur + 3;
                                r_cur += 3;
                                continue;
                            }
                        } else if first_row_lower[r_cur..].starts_with("</td") {
                            let next_char = first_row_lower[r_cur + 4..].chars().next();
                            if matches!(next_char, Some('>' | ' ' | '\t' | '\n' | '\r')) {
                                promoted_row.push_str(&row_orig[r_last..r_cur]);
                                promoted_row.push_str("</th");
                                r_last = r_cur + 4;
                                r_cur += 4;
                                continue;
                            }
                        }
                        r_cur += 1;
                    }
                    promoted_row.push_str(&row_orig[r_last..]);
                    let mut with_headers = String::with_capacity(out.len());
                    with_headers.push_str(&out[..tr_start]);
                    with_headers.push_str(&promoted_row);
                    with_headers.push_str(&out[tr_end..]);
                    return with_headers;
                }
            }
        }
    }

    out
}

fn split_surrounding_whitespace(s: &str) -> (&str, &str, &str) {
    let trimmed_start = s.trim_start();
    let leading = &s[..s.len() - trimmed_start.len()];
    let trimmed = trimmed_start.trim_end();
    let trailing = &trimmed_start[trimmed.len()..];
    (leading, trimmed, trailing)
}

/// Converts HTML to Markdown using `htmd`.
/// If HTML has no semantic markup, or conversion fails or produces empty output,
/// returns `None` so the caller can fall back to plain text.
pub fn convert_html_to_markdown(html: &str) -> Option<String> {
    if !has_semantic_html(html) {
        return None;
    }

    let is_google_docs = html.to_ascii_lowercase().contains("docs-internal-guid");
    let prepared = preprocess_clipboard_html(html);

    let options = Options {
        bullet_list_marker: BulletListMarker::Dash,
        ul_bullet_spacing: 1,
        ol_number_spacing: 1,
        ..Default::default()
    };

    let converter = htmd::HtmlToMarkdown::builder()
        .options(options)
        .skip_tags(vec![
            "script", "style", "head", "meta", "noscript", "template", "svg", "img", "picture",
            "source", "video", "audio", "canvas", "iframe", "button", "nav", "footer", "form",
            "input", "select", "textarea",
        ])
        .add_handler(vec!["a"], |handlers: &dyn Handlers, element: Element| {
            let content = handlers.walk_children(element.node).content;
            let (leading, core, trailing) = split_surrounding_whitespace(&content);
            if core.is_empty() {
                return None;
            }

            let href = element
                .attrs
                .iter()
                .find(|a| &*a.name.local == "href")
                .map(|a| a.value.trim().to_string());

            let Some(href) = href.filter(|h| !h.is_empty()) else {
                return Some(content.into());
            };

            let href_lower = href.to_ascii_lowercase();
            if href_lower.starts_with("javascript:")
                || href_lower.starts_with("vbscript:")
                || href_lower.starts_with("data:")
            {
                return Some(content.into());
            }

            // Strip decorative heading permalink symbols (e.g., `§`, `¶`, `#`)
            if href.starts_with('#') {
                if matches!(core, "#" | "§" | "¶" | "🔗") {
                    return None;
                }
                return Some(content.into());
            }

            // Avoid redundant `[https://...](https://...)` when text is already the exact URL
            if core == href || core.trim_end_matches('/') == href.trim_end_matches('/') {
                return Some(content.into());
            }

            let escaped_href = href.replace('(', "\\(").replace(')', "\\)");
            let normalized_core = core.lines().map(str::trim).collect::<Vec<_>>().join(" ");
            Some(HandlerResult::from(format!(
                "{leading}[{normalized_core}]({escaped_href}){trailing}"
            )))
        })
        .add_handler(
            vec!["span"],
            move |handlers: &dyn Handlers, element: Element| {
                let content = handlers.walk_children(element.node).content;
                if !is_google_docs {
                    return Some(content.into());
                }
                let (leading, core, trailing) = split_surrounding_whitespace(&content);
                if core.is_empty() {
                    return Some(content.into());
                }
                let style = element
                    .attrs
                    .iter()
                    .find(|a| &*a.name.local == "style")
                    .map(|a| {
                        a.value
                            .chars()
                            .filter(|c| !c.is_ascii_whitespace())
                            .collect::<String>()
                            .to_ascii_lowercase()
                    })
                    .unwrap_or_default();

                let is_bold =
                    style.contains("font-weight:700") || style.contains("font-weight:bold");
                let is_italic = style.contains("font-style:italic");

                let formatted = match (is_bold, is_italic) {
                    (true, true) => format!("{leading}***{core}***{trailing}"),
                    (true, false) => format!("{leading}**{core}**{trailing}"),
                    (false, true) => format!("{leading}*{core}*{trailing}"),
                    (false, false) => content,
                };
                Some(formatted.into())
            },
        )
        .build();

    match converter.convert(&prepared) {
        Ok(md) => {
            let trimmed = md.trim();
            if trimmed.is_empty() || trimmed.len() > MAX_CONTENT {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Err(_) => None,
    }
}

/// Chooses the best representation between plain text and HTML.
/// When `convert_rich_text` is true and HTML contains semantic tags,
/// converts HTML to Markdown (unless `text` is already a single bare URL).
/// Otherwise returns the untouched plain text.
pub fn capture_content(text: &str, html: Option<&str>, convert_rich_text: bool) -> String {
    if convert_rich_text && kind(text) != "link" {
        if let Some(h) = html {
            if let Some(md) = convert_html_to_markdown(h) {
                return md;
            }
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_article_with_links_and_headers() {
        let html = r##"
            <h2><a class="anchor" href="#rust-2026">§</a>Rust 2026</h2>
            <p>Visit <a href="https://www.rust-lang.org" title="Rust Website">Rust Official</a> for <strong>fast</strong> code.</p>
            <ul>
                <li>First</li>
                <li>Second with <i>italics</i> and <code>inline_fn()</code></li>
            </ul>
            <button>Copy</button>
            <img src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAUA" alt="avatar">
        "##;
        assert!(has_semantic_html(html));
        let md = convert_html_to_markdown(html).expect("Markdown conversion should succeed");
        assert!(md.contains("## Rust 2026"));
        assert!(!md.contains('§'));
        assert!(md.contains("[Rust Official](https://www.rust-lang.org)"));
        assert!(md.contains("**fast**"));
        assert!(md.contains("- First"));
        assert!(md.contains("`inline_fn()`"));
        assert!(!md.contains("Copy"));
        assert!(!md.contains("data:image"));
    }

    #[test]
    fn test_table_conversion_with_and_without_th() {
        // Standard HTML table with <thead>/<th>
        let html = r#"
            <table>
                <thead><tr><th>Col A</th><th>Col B</th></tr></thead>
                <tbody><tr><td>Val 1</td><td>Val 2</td></tr></tbody>
            </table>
        "#;
        let md = convert_html_to_markdown(html).expect("Table conversion should succeed");
        assert!(md.contains("| Col A | Col B |"));
        assert!(md.contains("| Val 1 | Val 2 |"));

        // Google Docs / Google Sheets style table with only <tbody><tr><td>
        let gdocs_table = r#"
            <b style="font-weight:normal;" id="docs-internal-guid-123">
            <table>
                <tbody>
                    <tr><td><p>Feature</p></td><td><p>Status</p></td></tr>
                    <tr><td><p>Rich Capture</p></td><td><p>Ready</p></td></tr>
                </tbody>
            </table>
            </b>
        "#;
        let md_gdocs =
            convert_html_to_markdown(gdocs_table).expect("Google Docs table should convert");
        assert!(md_gdocs.contains("| Feature"));
        assert!(md_gdocs.contains("| Rich Capture | Ready"));
        assert!(!md_gdocs.contains("**"));
    }

    #[test]
    fn test_vscode_and_jetbrains_code_snippets_are_not_converted() {
        // VS Code copies code with spans and colors (including italic comments / bold keywords)
        let vscode_html = r#"<meta charset='utf-8'><div style="color: #d4d4d4;font-family: monospace;"><div><span style="color: #569cd6; font-weight: bold;">let</span> <span style="color: #9cdcfe;">x</span> = <span style="color: #b5cea8;">a</span> * <span style="color: #b5cea8;">b</span>;</div><div><span style="color: #6a9955; font-style: italic;">// [pointer] * 2</span></div></div>"#;
        let text = "let x = a * b;\n// [pointer] * 2";

        assert!(!has_semantic_html(vscode_html));
        assert_eq!(capture_content(text, Some(vscode_html), true), text);

        // JetBrains IDEs wrap code in <pre><span> without <code>
        let jetbrains_html = r#"<html><body><pre style="font-family:'JetBrains Mono';"><span style="color:#cf8e6d;font-weight:bold;">fn</span> main() { let a = b * c[0]; }</pre></body></html>"#;
        let jb_text = "fn main() { let a = b * c[0]; }";
        assert!(!has_semantic_html(jetbrains_html));
        assert_eq!(
            capture_content(jb_text, Some(jetbrains_html), true),
            jb_text
        );
    }

    #[test]
    fn test_google_docs_plain_and_formatted_text() {
        // Unformatted text copied from Google Docs must stay plain text, NOT bolded
        let gdocs_plain_html = r#"<meta charset="utf-8"><b style="font-weight:normal;" id="docs-internal-guid-abc"><span style="font-size:11pt;font-weight:400;font-style:normal;">Plain text with * and [brackets]</span></b>"#;
        let plain_text = "Plain text with * and [brackets]";
        assert!(!has_semantic_html(gdocs_plain_html));
        assert_eq!(
            capture_content(plain_text, Some(gdocs_plain_html), true),
            plain_text
        );

        // Formatted text copied from Google Docs preserves bold and italic spans
        let gdocs_rich_html = r#"<meta charset="utf-8"><b style="font-weight:normal;" id="docs-internal-guid-abc"><p><span style="font-weight:700;">Important</span> and <span style="font-style:italic;">subtle</span> note.</p></b>"#;
        assert!(has_semantic_html(gdocs_rich_html));
        let md = capture_content("Important and subtle note.", Some(gdocs_rich_html), true);
        assert_eq!(md, "**Important** and *subtle* note.");
    }

    #[test]
    fn test_bare_url_preserves_link_kind() {
        let url = "https://github.com/OleksiyM/GnomeClipNotes";
        let html = r#"<a href="https://github.com/OleksiyM/GnomeClipNotes">https://github.com/OleksiyM/GnomeClipNotes</a>"#;
        let result = capture_content(url, Some(html), true);
        assert_eq!(result, url);
        assert_eq!(kind(&result), "link");
    }

    #[test]
    fn test_disabled_setting_keeps_plain_text() {
        let html = "<h2>Heading</h2><p><a href=\"https://example.com\">Link</a></p>";
        let text = "Heading\nLink";
        let result = capture_content(text, Some(html), false);
        assert_eq!(result, text);
    }

    #[test]
    fn test_blockquote_and_code_blocks() {
        let html = r#"
            <blockquote>A wise quote.</blockquote>
            <pre><code class="language-rust">let answer = 42 * 2;
let arr = &amp;values[0..1];</code></pre>
        "#;
        assert!(has_semantic_html(html));
        let md = convert_html_to_markdown(html).expect("Conversion should succeed");
        assert!(md.contains("> A wise quote."));
        assert!(md.contains("```rust"));
        // Code inside <pre><code> should NOT escape * or [
        assert!(md.contains("42 * 2"));
        assert!(md.contains("&values[0..1]"));
    }

    #[test]
    fn test_empty_or_broken_html() {
        assert_eq!(capture_content("plain text", Some(""), true), "plain text");
        assert_eq!(
            capture_content("plain text", Some("   "), true),
            "plain text"
        );
        assert_eq!(
            capture_content("plain text", Some("<<<>>>"), true),
            "plain text"
        );
        assert_eq!(capture_content("plain text", None, true), "plain text");
    }
}
