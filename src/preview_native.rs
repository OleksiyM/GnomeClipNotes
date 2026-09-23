use gtk::prelude::*;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

pub const CSS: &str = r#"
.markdown-preview {
  padding: 28px 32px;
}
.markdown-preview-block {
  color: @view_fg_color;
}
.markdown-preview-h1 { font-size: 1.8em; font-weight: 800; margin-top: 18px; margin-bottom: 8px; }
.markdown-preview-h2 { font-size: 1.5em; font-weight: 750; margin-top: 16px; margin-bottom: 7px; }
.markdown-preview-h3 { font-size: 1.25em; font-weight: 700; margin-top: 14px; margin-bottom: 6px; }
.markdown-preview-h4,
.markdown-preview-h5,
.markdown-preview-h6 { font-size: 1.08em; font-weight: 700; margin-top: 12px; margin-bottom: 5px; }
.markdown-preview-paragraph { margin-bottom: 10px; }
.markdown-preview-list { margin-left: 12px; margin-bottom: 5px; }
.markdown-preview-quote {
  background: alpha(@card_bg_color, .65);
  border-left: 3px solid alpha(@view_fg_color, .28);
  border-radius: 6px;
  padding: 8px 12px;
  margin: 4px 0 10px 0;
}
.markdown-preview-code {
  font-family: monospace;
  background: @card_bg_color;
  border: 1px solid alpha(@view_fg_color, .14);
  border-radius: 9px;
  padding: 12px 14px;
  margin: 4px 0 12px 0;
}
.markdown-preview-rule { color: alpha(@view_fg_color, .35); margin: 8px 0 12px 0; }
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Paragraph,
    Heading(u8),
    Quote,
    List,
    Code,
    Rule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    kind: Kind,
    text: String,
    markup: bool,
}

pub fn build(markdown: &str) -> gtk::Widget {
    let document = gtk::Box::new(gtk::Orientation::Vertical, 0);
    document.add_css_class("markdown-preview");
    document.set_hexpand(true);

    let blocks = parse(markdown);
    // Keep this mode lightweight even for a pathological one-megabyte note.
    // Never truncate content or create tens of thousands of GTK widgets.
    if blocks.len() > 512 {
        let text = gtk::TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(gtk::WrapMode::WordChar)
            .monospace(true)
            .top_margin(24)
            .bottom_margin(24)
            .left_margin(32)
            .right_margin(32)
            .build();
        text.set_tooltip_text(Some(&crate::i18n::tr(
            "Large note shown as plain text in Lightweight preview.",
        )));
        text.buffer().set_text(markdown);
        return text.upcast();
    }
    for block in blocks {
        let label = gtk::Label::builder()
            .xalign(0.0)
            .yalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .selectable(true)
            .hexpand(true)
            .build();
        label.add_css_class("markdown-preview-block");
        match block.kind {
            Kind::Paragraph => label.add_css_class("markdown-preview-paragraph"),
            Kind::Heading(level) => label.add_css_class(&format!("markdown-preview-h{level}")),
            Kind::Quote => label.add_css_class("markdown-preview-quote"),
            Kind::List => label.add_css_class("markdown-preview-list"),
            Kind::Code => label.add_css_class("markdown-preview-code"),
            Kind::Rule => label.add_css_class("markdown-preview-rule"),
        }
        if block.markup {
            label.set_markup(&block.text);
        } else {
            label.set_text(&block.text);
        }
        label.connect_activate_link(|label, uri| {
            if safe_uri(uri) {
                let launcher = gtk::UriLauncher::new(uri);
                launcher.launch(
                    label.root().and_downcast::<gtk::Window>().as_ref(),
                    None::<&gio::Cancellable>,
                    |_| {},
                );
            }
            glib::Propagation::Stop
        });
        document.append(&label);
    }
    document.upcast()
}

fn safe_uri(uri: &str) -> bool {
    !uri.chars().any(char::is_control)
        && glib::Uri::parse(uri, glib::UriFlags::NONE).is_ok_and(|uri| {
            matches!(uri.scheme().to_ascii_lowercase().as_str(), "http" | "https")
                && uri.host().is_some_and(|host| !host.is_empty())
        })
}

fn escape(text: &str) -> String {
    glib::markup_escape_text(text).to_string()
}

fn parse(markdown: &str) -> Vec<Block> {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut kind: Option<Kind> = None;
    let mut quote_depth = 0usize;
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut link_safe = Vec::new();

    let flush = |blocks: &mut Vec<Block>, text: &mut String, kind: &mut Option<Kind>| {
        if let Some(block_kind) = kind.take() {
            if !text.is_empty() || matches!(block_kind, Kind::Paragraph) {
                blocks.push(Block {
                    kind: block_kind,
                    text: std::mem::take(text),
                    markup: true,
                });
            }
        }
    };

    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    kind = Some(if !lists.is_empty() {
                        Kind::List
                    } else if quote_depth > 0 {
                        Kind::Quote
                    } else {
                        Kind::Paragraph
                    });
                }
                Tag::Heading { level, .. } => kind = Some(Kind::Heading(level as u8)),
                Tag::BlockQuote(_) => quote_depth += 1,
                Tag::CodeBlock(_) => {
                    flush(&mut blocks, &mut text, &mut kind);
                    kind = Some(Kind::Code);
                }
                Tag::List(start) => lists.push(start),
                Tag::Item => {
                    kind = Some(Kind::List);
                    let indent = "  ".repeat(lists.len().saturating_sub(1));
                    text.push_str(&escape(&indent));
                    if let Some(Some(number)) = lists.last_mut() {
                        text.push_str(&format!("{number}. "));
                        *number += 1;
                    } else {
                        text.push_str("• ");
                    }
                }
                Tag::Strong => text.push_str("<b>"),
                Tag::Emphasis => text.push_str("<i>"),
                Tag::Strikethrough => text.push_str("<s>"),
                Tag::Link { dest_url, .. } => {
                    let ok = safe_uri(&dest_url);
                    link_safe.push(ok);
                    if ok {
                        text.push_str(&format!("<a href=\"{}\">", escape(&dest_url)));
                    }
                }
                Tag::Image { .. } => text.push_str("[image: "),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => flush(&mut blocks, &mut text, &mut kind),
                TagEnd::Heading(_) => flush(&mut blocks, &mut text, &mut kind),
                TagEnd::BlockQuote(_) => quote_depth = quote_depth.saturating_sub(1),
                TagEnd::CodeBlock => {
                    if let Some(block_kind) = kind.take() {
                        blocks.push(Block {
                            kind: block_kind,
                            text: std::mem::take(&mut text),
                            markup: false,
                        });
                    }
                }
                TagEnd::List(_) => {
                    lists.pop();
                }
                TagEnd::Item => flush(&mut blocks, &mut text, &mut kind),
                TagEnd::Strong => text.push_str("</b>"),
                TagEnd::Emphasis => text.push_str("</i>"),
                TagEnd::Strikethrough => text.push_str("</s>"),
                TagEnd::Link => {
                    if link_safe.pop().unwrap_or(false) {
                        text.push_str("</a>");
                    }
                }
                TagEnd::Image => text.push(']'),
                _ => {}
            },
            Event::Text(value) => {
                if kind == Some(Kind::Code) {
                    text.push_str(&value);
                } else {
                    kind.get_or_insert(Kind::Paragraph);
                    text.push_str(&escape(&value));
                }
            }
            Event::Code(value) => text.push_str(&format!("<tt>{}</tt>", escape(&value))),
            Event::Html(value) | Event::InlineHtml(value) => {
                kind.get_or_insert(Kind::Paragraph);
                text.push_str(&escape(&value));
            }
            Event::SoftBreak | Event::HardBreak => text.push('\n'),
            Event::Rule => {
                flush(&mut blocks, &mut text, &mut kind);
                blocks.push(Block {
                    kind: Kind::Rule,
                    text: "────────────────────".into(),
                    markup: false,
                });
            }
            Event::TaskListMarker(checked) => {
                if text.ends_with("• ") {
                    text.truncate(text.len() - "• ".len());
                }
                text.push_str(if checked { "☑ " } else { "☐ " });
            }
            _ => {}
        }
    }
    flush(&mut blocks, &mut text, &mut kind);
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_html_is_inert_and_code_is_preserved() {
        let blocks = parse("# <script>x</script>\n\n```rs\n<a>\n```");
        assert!(blocks[0].text.contains("&lt;script&gt;"));
        assert_eq!(blocks[1].kind, Kind::Code);
        assert_eq!(blocks[1].text, "<a>\n");
    }

    #[test]
    fn only_http_links_become_anchors() {
        let blocks = parse("[safe](https://example.com) [bad](javascript:alert(1))");
        assert!(blocks[0]
            .text
            .contains("<a href=\"https://example.com\">safe</a>"));
        assert!(blocks[0].text.contains("bad"));
        assert!(!blocks[0].text.contains("javascript:"));
    }

    #[test]
    fn lists_and_tasks_remain_readable() {
        let blocks = parse("- [x] done\n- [ ] later\n\n3. three");
        assert!(blocks.iter().any(|b| b.text.contains("☑ done")));
        assert!(blocks.iter().any(|b| b.text.contains("3. three")));
    }

    #[test]
    fn standalone_html_and_unsupported_table_content_are_not_lost() {
        let blocks = parse("<div>literal & visible</div>\n\n| A | B |\n|---|---|\n| 1 | 2 |");
        assert!(blocks.iter().any(|b| b
            .text
            .contains("&lt;div&gt;literal &amp; visible&lt;/div&gt;")));
        assert!(blocks.iter().any(|b| b.text.contains("| 1 | 2 |")));
    }
}
