# A place for the next idea

A quiet Markdown preview, with **clear structure**, *emphasis*, and ~~finished thoughts~~.

## This week

- [x] Keep notes local
- [ ] Make the preview useful
  1. Use an existing parser
  2. Let an existing engine handle layout

> Хороший инструмент не должен отвлекать от мысли.
>
> Український текст теж має читатися природно.

### What is supported?

| Element | Example | Status |
| :--- | :--- | ---: |
| Inline code | `cargo test --locked` | Ready |
| Links | [GNOME](https://www.gnome.org/) | External browser |
| Task lists | Checked and unchecked | Read-only |

```rust
fn next_idea() -> &'static str {
    "Keep the application small; borrow the difficult parts."
}
```

#### A smaller heading

Paragraphs, nested lists and quotes retain their structure.

##### One more level

Long URLs and code should not make the entire document wider than the window.

###### The final heading level

---

No scripts, no background network requests, no embedded raw HTML.
