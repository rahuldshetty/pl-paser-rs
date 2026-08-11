//! Content type conversion for the `content` field, mirroring upstream's
//! `contentType` option: `html` (passthrough), `text`, and `markdown`
//! (a Turndown-style serializer for the subset of HTML article content uses).

use ego_tree::NodeRef;
use scraper::node::Node;

use crate::dom::Doc;
use crate::types::ContentType;

/// Convert extracted content HTML to the requested output format.
pub fn convert(content: &str, content_type: ContentType) -> String {
    match content_type {
        ContentType::Html => content.to_string(),
        ContentType::Text => to_text(content),
        ContentType::Markdown => to_markdown(content),
    }
}

/// Extract the plain text of the content (upstream `$.text()`).
pub fn to_text(html: &str) -> String {
    let doc = Doc::parse_fragment(html);
    let root = doc.html.tree.root();
    let mut out = String::new();
    collect_text(root, &mut out);
    collapse_whitespace(&out).trim().to_string()
}

fn collect_text(node: NodeRef<'_, Node>, out: &mut String) {
    match node.value() {
        Node::Text(t) => out.push_str(t),
        Node::Element(_) | Node::Fragment | Node::Document | Node::Doctype(_) => {
            for child in node.children() {
                collect_text(child, out);
            }
        }
        _ => {}
    }
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Serialize content HTML to GitHub-flavored markdown (a small Turndown-style
/// converter covering the tags article content typically contains).
pub fn to_markdown(html: &str) -> String {
    let doc = Doc::parse_fragment(html);
    let root = doc.html.tree.root();
    let mut out = String::new();
    for child in root.children() {
        render(child, 0, &mut out);
    }
    out.trim().to_string() + "\n"
}

fn render(node: NodeRef<'_, Node>, depth: usize, out: &mut String) {
    match node.value() {
        Node::Text(t) => out.push_str(t),
        Node::Element(el) => {
            let name = el.name().to_ascii_lowercase();
            match name.as_str() {
                "p" | "div" | "section" | "article" | "figure" | "figcaption" | "blockquote"
                | "header" | "footer" => {
                    let mut inner = String::new();
                    for c in node.children() {
                        render(c, depth, &mut inner);
                    }
                    let inner = inner.trim();
                    if !inner.is_empty() {
                        if name == "blockquote" {
                            for line in inner.lines() {
                                out.push_str("> ");
                                out.push_str(line);
                                out.push('\n');
                            }
                        } else {
                            out.push_str(inner);
                            out.push_str("\n\n");
                        }
                    }
                }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    let level = name[1..].parse::<usize>().unwrap_or(1);
                    let mut inner = String::new();
                    for c in node.children() {
                        render(c, depth, &mut inner);
                    }
                    out.push_str(&"#".repeat(level));
                    out.push(' ');
                    out.push_str(inner.trim());
                    out.push_str("\n\n");
                }
                "a" => {
                    let mut inner = String::new();
                    for c in node.children() {
                        render(c, depth, &mut inner);
                    }
                    let href = el.attr("href").unwrap_or("");
                    out.push('[');
                    out.push_str(&inner);
                    out.push_str("](");
                    out.push_str(href);
                    out.push(')');
                }
                "img" => {
                    let src = el.attr("src").unwrap_or("");
                    let alt = el.attr("alt").unwrap_or("");
                    out.push_str(&format!("![{alt}]({src})"));
                }
                "strong" | "b" => {
                    let mut inner = String::new();
                    for c in node.children() {
                        render(c, depth, &mut inner);
                    }
                    out.push_str(&format!("**{}**", inner.trim()));
                }
                "em" | "i" => {
                    let mut inner = String::new();
                    for c in node.children() {
                        render(c, depth, &mut inner);
                    }
                    out.push_str(&format!("*{}*", inner.trim()));
                }
                "code" => {
                    let mut inner = String::new();
                    for c in node.children() {
                        render(c, depth, &mut inner);
                    }
                    out.push_str(&format!("`{}`", inner));
                }
                "pre" => {
                    let mut inner = String::new();
                    for c in node.children() {
                        render(c, depth, &mut inner);
                    }
                    out.push_str("```\n");
                    out.push_str(inner.trim());
                    out.push_str("\n```\n\n");
                }
                "br" => out.push_str("  \n"),
                "hr" => out.push_str("---\n\n"),
                "ul" | "ol" => {
                    let ordered = name == "ol";
                    let mut index = 1usize;
                    for c in node.children() {
                        if c.value().is_element() {
                            let mut inner = String::new();
                            render(c, depth + 1, &mut inner);
                            let prefix = if ordered {
                                let p = format!("{index}. ");
                                index += 1;
                                p
                            } else {
                                "- ".to_string()
                            };
                            out.push_str(&"  ".repeat(depth));
                            out.push_str(&prefix);
                            out.push_str(inner.trim());
                            out.push('\n');
                        }
                    }
                    out.push('\n');
                }
                "li" => {
                    // handled by ul/ol
                    for c in node.children() {
                        render(c, depth, out);
                    }
                }
                _ => {
                    // unknown/inline tags: render children inline
                    for c in node.children() {
                        render(c, depth, out);
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_extraction() {
        let html = "<div><p>Hello <b>world</b></p><p>Second para</p></div>";
        // cheerio .text() concatenates block children without a separator.
        assert_eq!(to_text(html), "Hello worldSecond para");
    }

    #[test]
    fn markdown_basics() {
        let html = "<div><h2>Title</h2><p>Hello <strong>bold</strong> and <a href=\"https://x.com\">link</a></p><ul><li>one</li><li>two</li></ul></div>";
        let md = to_markdown(html);
        assert!(md.contains("## Title"), "{md}");
        assert!(md.contains("**bold**"), "{md}");
        assert!(md.contains("[link](https://x.com)"), "{md}");
        assert!(md.contains("- one"), "{md}");
    }

    #[test]
    fn convert_passthrough_html() {
        assert_eq!(convert("<p>x</p>", ContentType::Html), "<p>x</p>");
    }
}
