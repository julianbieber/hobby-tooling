//! Turning text from the JSON into safe HTML: escaping, and rustdoc markdown whose
//! links either point at a page in the site or become plain text.

use pulldown_cmark::{BrokenLink, CowStr, Event, Options, Parser, Tag, TagEnd, html};

/// Escapes `& < > " '`.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The first line of the docs, escaped and cut to a length that fits a list.
pub fn summary(docs: Option<&str>) -> String {
    let line = docs
        .and_then(|d| d.lines().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("");
    let cut: String = line.chars().take(140).collect();
    escape(&cut)
}

/// Renders rustdoc markdown to HTML.
///
/// `resolve` maps a link target as written in the docs to a site-relative href, or
/// `None`. `http(s)` targets are kept; every other target that `resolve` does not
/// accept becomes its text. Raw HTML is shown as text and images as their alt text.
pub fn render_docs(docs: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let mut broken = |link: BrokenLink<'_>| {
        let key = link.reference.to_string();
        resolve(&key).map(|_| (CowStr::from(key), CowStr::from("")))
    };
    let parser = Parser::new_with_broken_link_callback(docs, options, Some(&mut broken));
    let mut dropped: Vec<bool> = Vec::new();
    let events: Vec<Event> = parser
        .filter_map(|event| match event {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let href = if dest_url.starts_with("http://") || dest_url.starts_with("https://") {
                    Some(dest_url.to_string())
                } else {
                    resolve(&dest_url)
                };
                dropped.push(href.is_none());
                href.map(|h| {
                    Event::Start(Tag::Link {
                        link_type,
                        dest_url: CowStr::from(h),
                        title,
                        id,
                    })
                })
            }
            Event::End(TagEnd::Link) => {
                if dropped.pop().unwrap_or(true) {
                    None
                } else {
                    Some(Event::End(TagEnd::Link))
                }
            }
            Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => None,
            Event::Html(t) | Event::InlineHtml(t) => Some(Event::Text(t)),
            other => Some(other),
        })
        .collect();
    let mut out = String::new();
    html::push_html(&mut out, events.into_iter());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver(key: &str) -> Option<String> {
        (key == "`Score`").then(|| "../score.html".to_string())
    }

    // Pins that every character that can open markup is escaped, since all JSON strings are untrusted.
    #[test]
    fn escape_covers_the_five_characters() {
        assert_eq!(
            escape("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    // Pins that a doc comment cannot inject a script element.
    #[test]
    fn raw_html_becomes_text() {
        let out = render_docs("<script>alert(1)</script>", &resolver);
        assert!(!out.contains("<script"), "{out}");
        assert!(out.contains("&lt;script&gt;"), "{out}");
    }

    // Pins that a non-http destination is dropped but its text stays.
    #[test]
    fn javascript_links_become_text() {
        let out = render_docs("[bad](javascript:alert(1))", &resolver);
        assert!(!out.contains("href"), "{out}");
        assert!(out.contains("bad"), "{out}");
    }

    // Pins that an intra-doc reference resolves to the page it names, and an http link survives.
    #[test]
    fn resolved_and_http_links_are_kept() {
        let out = render_docs("See [`Score`] and [x](https://e.com).", &resolver);
        assert!(out.contains("href=\"../score.html\""), "{out}");
        assert!(out.contains("href=\"https://e.com\""), "{out}");
    }

    // Pins that an unresolved reference is text, not a dead anchor the link check would reject.
    #[test]
    fn unresolved_reference_is_text() {
        let out = render_docs("See [`Missing`].", &resolver);
        assert!(!out.contains("href"), "{out}");
    }
}
