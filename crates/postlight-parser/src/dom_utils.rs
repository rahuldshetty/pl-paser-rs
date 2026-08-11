//! DOM manipulation helpers used by the cleaners and generic extractors,
//! ported from upstream `src/utils/dom/`.

use once_cell::sync::Lazy;
use regex::Regex;
use ego_tree::NodeId;
use url::Url;

use crate::dom::{link_density, Doc};
use crate::utils::normalize_spaces;

/// The class used to mark elements we want to keep (upstream `KEEP_CLASS`).
pub const KEEP_CLASS: &str = "mercury-parser-keep";

static SPACER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"transparent|spacer|blank").expect("spacer re"));

/// iframes we always want to keep (upstream `KEEP_SELECTORS`).
pub const KEEP_SELECTORS: [&str; 6] = [
    "iframe[src^=\"https://www.youtube.com\"]",
    "iframe[src^=\"https://www.youtube-nocookie.com\"]",
    "iframe[src^=\"http://www.youtube.com\"]",
    "iframe[src^=\"https://player.vimeo\"]",
    "iframe[src^=\"http://player.vimeo\"]",
    "iframe[src^=\"https://www.redditmedia.com\"]",
];

/// Tags to strip from the output (upstream `STRIP_OUTPUT_TAGS`).
pub const STRIP_OUTPUT_TAGS: [&str; 9] = [
    "title", "script", "noscript", "link", "style", "hr", "embed", "iframe", "object",
];

/// Attributes kept on output (upstream `WHITELIST_ATTRS`).
pub const WHITELIST_ATTRS: [&str; 11] = [
    "src", "srcset", "sizes", "type", "href", "class", "id", "alt", "xlink:href", "width",
    "height",
];

static WHITELIST_ATTRS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!("^({})$", WHITELIST_ATTRS.join("|")))
        .expect("whitelist attrs re")
});

/// Tags cleaned conditionally (upstream `CLEAN_CONDITIONALLY_TAGS`).
pub const CLEAN_CONDITIONALLY_TAGS: &str = "ul, ol, table, div, button, form";

/// Header tags cleaned by `cleanHeaders` (upstream `HEADER_TAG_LIST`).
pub const HEADER_TAG_LIST: &str = "h2, h3, h4, h5, h6";

static UNLIKELY_CANDIDATES_BLACKLIST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        "ad-break|adbox|advert|addthis|agegate|aux|blogger-labels|combx|comment|conversation|disqus|entry-unrelated|extra|foot|form|header|hidden|loader|login|menu|meta|nav|pager|pagination|predicta|presence_control_external|popup|printfriendly|related|remove|remark|rss|share|shoutbox|sidebar|sociable|sponsor|tools",
    )
    .expect("blacklist re")
});

static UNLIKELY_CANDIDATES_WHITELIST: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        "and|article|body|blogindex|column|content|entry-content-asset|format|hfeed|hentry|hatom|main|page|posts|shadow",
    )
    .expect("whitelist re")
});

static DIV_TO_P_BLOCK_TAGS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(a|blockquote|dl|div|img|p|pre|table)$").expect("div-to-p tags re")
});

static NON_TOP_CANDIDATE_TAGS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(br|b|i|label|hr|area|base|basefont|input|img|link|meta)$",
    )
    .expect("non-top-candidate re")
});

/// hNews / content-specific selectors given a big score boost.
pub const HNEWS_CONTENT_SELECTORS: [(&str, &str); 6] = [
    (".hentry", ".entry-content"),
    ("entry", ".entry-content"),
    (".entry", ".entry_content"),
    (".post", ".postbody"),
    (".post", ".post_body"),
    (".post", ".post-body"),
];

static PHOTO_HINTS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"figure|photo|image|caption").expect("photo hints re"));

static POSITIVE_SCORE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        "article|articlecontent|instapaper_body|blog|body|content|entry-content-asset|entry|hentry|main|Normal|page|pagination|permalink|post|story|text|[-_]copy|\\Bcopy",
    )
    .expect("positive score re")
});

static NEGATIVE_SCORE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        "adbox|advert|author|bio|bookmark|bottom|byline|clear|com-|combx|comment|comment\\B|contact|copy|credit|crumb|date|deck|excerpt|featured|foot|footer|footnote|graf|head|info|infotext|instapaper_ignore|jump|linebreak|link|masthead|media|meta|modal|outbrain|promo|pr_|related|respond|roundcontent|scroll|secondary|share|shopping|shoutbox|side|sidebar|sponsor|stamp|sub|summary|tags|tools|widget",
    )
    .expect("negative score re")
});

static READABILITY_ASSET: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"entry-content-asset").expect("asset re"));

static PARAGRAPH_SCORE_TAGS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(p|li|span|pre)$").expect("paragraph tags re"));
static CHILD_CONTENT_TAGS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(td|blockquote|ol|ul|dl)$").expect("child content tags re"));
static BAD_TAGS: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(address|form)$").expect("bad tags re"));

static BLOCK_LEVEL_TAGS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(article|aside|blockquote|body|br|button|canvas|caption|col|colgroup|dd|div|dl|dt|embed|fieldset|figcaption|figure|footer|form|h1|h2|h3|h4|h5|h6|header|hgroup|hr|li|map|object|ol|output|p|pre|progress|section|table|tbody|textarea|tfoot|th|thead|tr|ul|video)$",
    )
    .expect("block tags re")
});

/// Remove elements that are unlikely candidates for article content
/// (upstream `stripUnlikelyCandidates`).
pub fn strip_unlikely_candidates(doc: &mut Doc) {
    let all: Vec<NodeId> = doc.select_ids("*");
    for id in all {
        let Some(node) = doc.get(id) else { continue };
        let Some(element) = node.value().as_element() else {
            continue;
        };
        if element.name().eq_ignore_ascii_case("a") {
            continue;
        }
        let classes = element.attr("class").unwrap_or("");
        let id_attr = element.attr("id").unwrap_or("");
        if classes.is_empty() && id_attr.is_empty() {
            continue;
        }
        let class_and_id = format!("{classes} {id_attr}");
        if UNLIKELY_CANDIDATES_WHITELIST.is_match(&class_and_id) {
            continue;
        }
        if UNLIKELY_CANDIDATES_BLACKLIST.is_match(&class_and_id) {
            doc.remove(&[id]);
        }
    }
}

/// Turn a `<br>` into a `<p>` wrapping the following inline siblings
/// (upstream `paragraphize(node, $, br=true)`).
pub fn paragraphize_br(doc: &mut Doc, br_id: NodeId) {
    // Collect following siblings that are text nodes or non-block elements.
    let mut follow: Vec<NodeId> = Vec::new();
    let mut cur = doc.next_sibling(br_id);
    while let Some(sib) = cur {
        let block = doc
            .get(sib)
            .and_then(|n| n.value().as_element())
            .map(|e| BLOCK_LEVEL_TAGS_RE.is_match(e.name()))
            .unwrap_or(false);
        if block {
            break;
        }
        follow.push(sib);
        cur = doc.next_sibling(sib);
    }

    // Create a <p> right after the <br>, move the siblings into it.
    if let Some(mut br) = doc.html.tree.get_mut(br_id) {
        let p = br
            .insert_after(crate::dom::new_p())
            .id();
        for sib in follow {
            if let Some(mut p_node) = doc.html.tree.get_mut(p) {
                p_node.append_id(sib);
            }
        }
    }
    // Remove the <br> itself.
    if let Some(mut br) = doc.html.tree.get_mut(br_id) {
        br.detach();
    }
}

/// Convert consecutive `<br>` runs into `<p>` tags (upstream `brsToPs`).
pub fn brs_to_ps(doc: &mut Doc) {
    let br_ids: Vec<NodeId> = doc.select_ids("br");
    let mut collapsing = false;
    for id in br_ids {
        let next_is_br = doc
            .next_sibling(id)
            .and_then(|s| doc.get(s))
            .and_then(|n| n.value().as_element())
            .map(|e| e.name().eq_ignore_ascii_case("br"))
            .unwrap_or(false);
        if next_is_br {
            collapsing = true;
            doc.remove(&[id]);
        } else if collapsing {
            collapsing = false;
            paragraphize_br(doc, id);
        }
    }
}

/// Convert `<div>`s without block children and unparented `<span>`s to `<p>`
/// (upstream `convertToParagraphs`).
pub fn convert_to_paragraphs(doc: &mut Doc) {
    brs_to_ps(doc);

    let div_ids: Vec<NodeId> = doc.select_ids("div");
    for id in div_ids {
        let children = doc.children_ids(id);
        let has_block = children.iter().any(|c| {
            doc.get(*c)
                .and_then(|n| n.value().as_element())
                .map(|e| DIV_TO_P_BLOCK_TAGS.is_match(e.name()))
                .unwrap_or(false)
        });
        if !has_block {
            doc.convert_node_to(id, "p");
        }
    }

    let span_ids: Vec<NodeId> = doc.select_ids("span");
    for id in span_ids {
        let has_content_parent = doc.ancestors_of(id).iter().any(|a| {
            doc.get(*a)
                .and_then(|n| n.value().as_element())
                .map(|e| matches!(e.name(), "p" | "div" | "li" | "figcaption"))
                .unwrap_or(false)
        });
        if !has_content_parent {
            doc.convert_node_to(id, "p");
        }
    }
}

/// Mark keep-worthy elements (YouTube/Vimeo/Reddit/same-host iframes) with
/// `mercury-parser-keep` (upstream `markToKeep`).
pub fn mark_to_keep(doc: &mut Doc, article_id: NodeId, url: &str) {
    let mut tags: Vec<String> = KEEP_SELECTORS.iter().map(|s| s.to_string()).collect();
    if let Ok(parsed) = Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            tags.push(format!("iframe[src^=\"{}://{host}\"]", parsed.scheme()));
        }
    }
    let selector = tags.join(", ");
    let ids = doc.select_ids_in(article_id, &selector);
    for id in ids {
        add_class(doc, id, KEEP_CLASS);
    }
}

/// Remove junk tags not marked to keep (upstream `stripJunkTags`).
pub fn strip_junk_tags(doc: &mut Doc, article_id: NodeId) {
    let selector = STRIP_OUTPUT_TAGS.join(", ");
    let ids = doc.select_ids_in(article_id, &selector);
    for id in ids {
        if !doc.has_class(id, KEEP_CLASS) {
            doc.remove(&[id]);
        }
    }
}

/// Remove or downgrade `h1` tags (upstream `cleanHOnes`).
pub fn clean_h_ones(doc: &mut Doc, article_id: NodeId) {
    let h_ones: Vec<NodeId> = doc.select_ids_in(article_id, "h1");
    if h_ones.len() < 3 {
        doc.remove(&h_ones);
    } else {
        doc.convert_nodes_to(&h_ones, "h2");
    }
}

/// Rename top-level `html`/`body` nodes to `div` (upstream
/// `rewriteTopLevel`).
pub fn rewrite_top_level(doc: &mut Doc) {
    let top: Vec<NodeId> = doc
        .html
        .tree
        .root()
        .children()
        .filter(|c| c.value().is_element())
        .map(|c| c.id())
        .collect();
    for id in top {
        if let Some(name) = doc.element_name_of(id) {
            if name.eq_ignore_ascii_case("html") || name.eq_ignore_ascii_case("body") {
                doc.convert_node_to(id, "div");
            }
        }
    }
}

/// Drop small/spacer images and `height` attributes (upstream
/// `cleanImages`).
pub fn clean_images(doc: &mut Doc, article_id: NodeId) {
    let img_ids: Vec<NodeId> = doc.select_ids_in(article_id, "img");
    for id in img_ids {
        let height = doc
            .attr_of(id, "height")
            .and_then(|h| h.parse::<i64>().ok());
        let width = doc
            .attr_of(id, "width")
            .and_then(|w| w.parse::<i64>().ok());
        let height_or_20 = height.filter(|v| *v != 0).unwrap_or(20);
        let width_or_20 = width.filter(|v| *v != 0).unwrap_or(20);

        // Remove images that explicitly have very small heights or widths.
        if height_or_20 < 10 || width_or_20 < 10 {
            doc.remove(&[id]);
            continue;
        }
        // Never specify a height, so we can scale with respect to width.
        if height.is_some() {
            doc.remove_attr(id, "height");
        }

        // Remove transparent/spacer images.
        if let Some(src) = doc.attr_of(id, "src") {
            if scoring_re::is_spacer(&src) {
                doc.remove(&[id]);
            }
        }
    }
}

/// Remove header tags that precede all paragraphs, match the title, or have a
/// negative weight (upstream `cleanHeaders`).
pub fn clean_headers(doc: &mut Doc, article_id: NodeId, title: &str) {
    let headers: Vec<NodeId> = doc.select_ids_in(article_id, HEADER_TAG_LIST);
    for id in headers {
        let has_prev_p = doc
            .get(id)
            .map(|n| {
                n.prev_siblings().any(|s| {
                    s.value()
                        .as_element()
                        .map(|e| e.name().eq_ignore_ascii_case("p"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        let text = doc.text_of(id).unwrap_or_default();
        if !has_prev_p {
            doc.remove(&[id]);
            continue;
        }
        if normalize_spaces(&text) == title {
            doc.remove(&[id]);
            continue;
        }
        if crate::extractors::scoring::get_weight(doc, id) < 0.0 {
            doc.remove(&[id]);
        }
    }
}

/// Conditionally remove junk container tags (upstream `cleanTags`).
pub fn clean_tags_conditionally(doc: &mut Doc, article_id: NodeId) {
    let ids: Vec<NodeId> = doc.select_ids_in(article_id, CLEAN_CONDITIONALLY_TAGS);
    for id in ids {
        // If marked to keep, or contains something marked to keep, skip it.
        if doc.has_class(id, KEEP_CLASS)
            || !doc.select_ids_in(id, &format!(".{KEEP_CLASS}")).is_empty()
        {
            continue;
        }

        let weight = crate::extractors::scoring::get_or_init_score(doc, id, true);
        if weight < 0.0 {
            doc.remove(&[id]);
        } else {
            remove_unless_content(doc, id, weight);
        }
    }
}

fn remove_unless_content(doc: &mut Doc, id: NodeId, weight: f64) {
    if doc.has_class(id, "entry-content-asset") {
        return;
    }

    let content = normalize_spaces(&doc.text_of(id).unwrap_or_default());
    let commas = content.matches(',').count();

    if commas < 10 {
        let p_count = doc.select_ids_in(id, "p").len();
        let input_count = doc.select_ids_in(id, "input").len();
        if input_count > p_count / 3 {
            doc.remove(&[id]);
            return;
        }

        let content_length = content.len();
        let img_count = doc.select_ids_in(id, "img").len();

        if content_length < 25 && img_count == 0 {
            doc.remove(&[id]);
            return;
        }

        let density = doc.get(id).map(link_density).unwrap_or(0.0);

        if weight < 25.0 && density > 0.2 && content_length > 75 {
            doc.remove(&[id]);
            return;
        }

        if weight >= 25.0 && density > 0.5 {
            let tag_name = doc.element_name_of(id).unwrap_or_default().to_lowercase();
            let is_list = tag_name == "ol" || tag_name == "ul";
            if is_list {
                if let Some(prev) = doc.prev_sibling(id) {
                    if normalize_spaces(&doc.text_of(prev).unwrap_or_default()).ends_with(':') {
                        return;
                    }
                }
            }
            doc.remove(&[id]);
            return;
        }

        let script_count = doc.select_ids_in(id, "script").len();
        if script_count > 0 && content_length < 150 {
            doc.remove(&[id]);
        }
    }
}

/// Remove empty paragraphs (upstream `removeEmpty`).
pub fn remove_empty(doc: &mut Doc, article_id: NodeId) {
    let p_ids: Vec<NodeId> = doc.select_ids_in(article_id, "p");
    for id in p_ids {
        let has_media = !doc.select_ids_in(id, "iframe, img").is_empty();
        if !has_media && doc.text_of(id).map(|t| t.trim().is_empty()).unwrap_or(true) {
            doc.remove(&[id]);
        }
    }
}

/// Remove attributes not on the whitelist, and the keep class
/// (upstream `cleanAttributes`).
pub fn clean_attributes(doc: &mut Doc) {
    let all: Vec<NodeId> = doc.select_ids("*");
    for id in all {
        let Some(node) = doc.get(id) else { continue };
        let Some(element) = node.value().as_element() else {
            continue;
        };
        let attrs: Vec<(String, String)> = element
            .attrs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (name, _) in attrs {
            if !WHITELIST_ATTRS_RE.is_match(&name) {
                doc.remove_attr(id, &name);
            }
        }
        if doc.has_class(id, KEEP_CLASS) {
            remove_class(doc, id, KEEP_CLASS);
        }
    }
}

/// Make `href`/`src`/`srcset` attributes absolute (upstream
/// `makeLinksAbsolute`).
pub fn make_links_absolute(doc: &mut Doc, url: &str) {
    let base = doc
        .attr("base", "href")
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| url.to_string());

    for attr in ["href", "src"] {
        let ids = doc.select_ids(&format!("[{attr}]"));
        for id in ids {
            if let Some(value) = doc.attr_of(id, attr) {
                if let Ok(resolved) = Url::parse(&base).and_then(|b| b.join(&value)) {
                    doc.set_attr(id, attr, resolved.as_str());
                }
            }
        }
    }

    // srcset: resolve each candidate URL and dedupe.
    let srcset_ids = doc.select_ids("[srcset]");
    for id in srcset_ids {
        let Some(value) = doc.attr_of(id, "srcset") else {
            continue;
        };
        let Ok(base_url) = Url::parse(&base) else {
            continue;
        };
        let candidates: Vec<&str> = value.split(',').map(|c| c.trim()).collect();
        let mut resolved = Vec::new();
        for candidate in candidates {
            let mut parts = candidate.split_whitespace();
            let url_part = parts.next().unwrap_or("");
            let descriptor = parts.collect::<Vec<_>>().join(" ");
            if let Ok(abs) = base_url.join(url_part) {
                let mut entry = abs.to_string();
                if !descriptor.is_empty() {
                    entry.push(' ');
                    entry.push_str(&descriptor);
                }
                if !resolved.contains(&entry) {
                    resolved.push(entry);
                }
            }
        }
        if !resolved.is_empty() {
            doc.set_attr(id, "srcset", &resolved.join(", "));
        }
    }
}

/// Extract a meta tag value by `name` (upstream `extractFromMeta`).
pub fn extract_from_meta(
    doc: &Doc,
    meta_names: &[&str],
    cached_names: &[String],
    clean_tags: bool,
) -> Option<String> {
    for name in meta_names {
        if !cached_names.iter().any(|n| n == name) {
            continue;
        }
        let values: Vec<String> = doc
            .attr_all(&format!("meta[name=\"{name}\"]"), "value")
            .into_iter()
            .filter(|v| !v.is_empty())
            .collect();
        if values.len() == 1 {
            if clean_tags {
                return Some(strip_tags(&values[0]));
            }
            return Some(values[0].clone());
        }
    }
    None
}

/// Extract a value from the first selector that matches exactly one element
/// (upstream `extractFromSelectors`).
pub fn extract_from_selectors(
    doc: &Doc,
    selectors: &[&str],
    max_children: usize,
) -> Option<String> {
    for selector in selectors {
        let nodes = doc.select(selector);
        if nodes.len() == 1 {
            let id = nodes[0].id();
            if is_good_node(doc, id, max_children) {
                let content = doc.text_of(id).unwrap_or_default();
                if !content.is_empty() {
                    return Some(content);
                }
            }
        }
    }
    None
}

fn is_good_node(doc: &Doc, id: NodeId, max_children: usize) -> bool {
    if doc.children_ids(id).len() > max_children {
        return false;
    }
    if within_comment(doc, id) {
        return false;
    }
    true
}

/// Strip all tags from a text string (upstream `stripTags`).
pub fn strip_tags(text: &str) -> String {
    let wrapped = format!("<span>{text}</span>");
    let doc = Doc::parse_fragment(&wrapped);
    let cleaned = doc.text("span").unwrap_or_default();
    if cleaned.is_empty() { text.to_string() } else { cleaned }
}

/// True if the node has an ancestor whose class/id contains "comment"
/// (upstream `withinComment`).
pub fn within_comment(doc: &Doc, id: NodeId) -> bool {
    doc.ancestors_of(id).iter().any(|ancestor| {
        let Some(node) = doc.get(*ancestor) else {
            return false;
        };
        let Some(element) = node.value().as_element() else {
            return false;
        };
        let class_and_id = format!(
            "{} {}",
            element.attr("class").unwrap_or(""),
            element.attr("id").unwrap_or("")
        );
        class_and_id.contains("comment")
    })
}

/// True if the node's text is at least 100 chars (upstream `nodeIsSufficient`).
pub fn node_is_sufficient(doc: &Doc, id: NodeId) -> bool {
    doc.text_of(id)
        .map(|t| t.trim().len() >= 100)
        .unwrap_or(false)
}

/// True if the doc is a WordPress site (upstream `isWordpress`).
pub fn is_wordpress(doc: &Doc) -> bool {
    // meta[name=generator][value^=WordPress]
    doc.select("meta[name=\"generator\"]")
        .iter()
        .any(|e| e.value().attr("value").map(|v| v.starts_with("WordPress")).unwrap_or(false))
}

/// Add a class to an element (cheerio `addClass`).
pub fn add_class(doc: &mut Doc, id: NodeId, class: &str) {
    let current = doc.attr_of(id, "class").unwrap_or_default();
    let mut classes: Vec<&str> = current.split_whitespace().collect();
    if !classes.contains(&class) {
        classes.push(class);
        doc.set_attr(id, "class", &classes.join(" "));
    }
}

/// Remove a class from an element (cheerio `removeClass`).
pub fn remove_class(doc: &mut Doc, id: NodeId, class: &str) {
    let current = doc.attr_of(id, "class").unwrap_or_default();
    let classes: Vec<&str> = current.split_whitespace().filter(|c| *c != class).collect();
    if classes.is_empty() {
        doc.remove_attr(id, "class");
    } else {
        doc.set_attr(id, "class", &classes.join(" "));
    }
}

/// The tag-name score helpers used by scoring (exported for `scoring`).
pub(crate) mod scoring_re {
    use super::*;

    pub fn is_paragraph_tag(name: &str) -> bool {
        PARAGRAPH_SCORE_TAGS.is_match(name)
    }
    pub fn is_child_content_tag(name: &str) -> bool {
        CHILD_CONTENT_TAGS.is_match(name)
    }
    pub fn is_bad_tag(name: &str) -> bool {
        BAD_TAGS.is_match(name)
    }
    pub fn is_non_top_candidate(name: &str) -> bool {
        NON_TOP_CANDIDATE_TAGS_RE.is_match(name)
    }
    pub fn is_photo_hint(text: &str) -> bool {
        PHOTO_HINTS_RE.is_match(text)
    }
    pub fn is_positive_score_hint(text: &str) -> bool {
        POSITIVE_SCORE_RE.is_match(text)
    }
    pub fn is_negative_score_hint(text: &str) -> bool {
        NEGATIVE_SCORE_RE.is_match(text)
    }
    pub fn is_readability_asset(text: &str) -> bool {
        READABILITY_ASSET.is_match(text)
    }
    pub fn hnews_selectors() -> &'static [(&'static str, &'static str)] {
        &HNEWS_CONTENT_SELECTORS
    }
    pub fn is_spacer(src: &str) -> bool {
        SPACER_RE.is_match(src)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(html: &str) -> Doc {
        Doc::parse_document(html)
    }

    #[test]
    fn strip_unlikely_removes_comment_nodes() {
        let mut d = doc("<html><body><div class=\"comment\">x</div><div class=\"content\"><p>article</p></div></body></html>");
        strip_unlikely_candidates(&mut d);
        let s = d.serialize();
        assert!(!s.contains("comment"));
        assert!(s.contains("article"));
    }

    #[test]
    fn strip_unlikely_keeps_whitelisted() {
        let mut d = doc("<html><body><div class=\"rss-content entry-content\">x</div></body></html>");
        strip_unlikely_candidates(&mut d);
        assert!(d.serialize().contains("entry-content"));
    }

    #[test]
    fn brs_to_ps_wraps_inline_siblings() {
        let mut d = doc("<html><body><p>a<br><br>b<br>c</p></body></html>");
        brs_to_ps(&mut d);
        let s = d.serialize();
        // Serialized literally (nested <p>), matching upstream cheerio output.
        assert!(s.contains("<p>a<p>b</p><br>c</p>"), "got: {s}");
    }

    #[test]
    fn convert_divs_to_ps() {
        let mut d = doc("<html><body><div>plain text</div><div><p>nested</p></div></body></html>");
        convert_to_paragraphs(&mut d);
        let s = d.serialize();
        assert!(s.contains("<p>plain text</p>"), "got: {s}");
        assert!(s.contains("<div><p>nested</p></div>"), "got: {s}");
    }

    #[test]
    fn strip_junk_keeps_marked() {
        let mut d = doc("<html><body><iframe src=\"https://www.youtube.com/x\"></iframe><script>1</script></body></html>");
        let body = d.select_ids("body")[0];
        mark_to_keep(&mut d, body, "https://example.com");
        strip_junk_tags(&mut d, body);
        let s = d.serialize();
        assert!(s.contains("youtube.com"), "got: {s}");
        assert!(!s.contains("<script"), "got: {s}");
    }

    #[test]
    fn clean_h_ones_removes_few() {
        let mut d = doc("<html><body><h1>One</h1><p>text</p></body></html>");
        let body = d.select_ids("body")[0];
        clean_h_ones(&mut d, body);
        assert!(!d.serialize().contains("<h1"));
    }

    #[test]
    fn remove_empty_paragraphs() {
        let mut d = doc("<html><body><p>   </p><p>real</p><p><img src=\"x\"></p></body></html>");
        let body = d.select_ids("body")[0];
        remove_empty(&mut d, body);
        let s = d.serialize();
        assert!(!s.contains("<p>   </p>"), "got: {s}");
        assert!(s.contains("real"));
        assert!(s.contains("<img"));
    }

    #[test]
    fn clean_attributes_whitelists() {
        let mut d = doc("<html><body><p style=\"x\" align=\"y\" class=\"z\" data-x=\"1\">t</p></body></html>");
        clean_attributes(&mut d);
        let s = d.serialize();
        assert!(!s.contains("style"));
        assert!(!s.contains("align"));
        assert!(!s.contains("data-x"));
        assert!(s.contains("class=\"z\""));
    }

    #[test]
    fn make_links_absolute_resolves() {
        let mut d = doc(r#"<html><head></head><body><a href="/rel">x</a><img src="//cdn.example.com/a.png"><img srcset="a.png 1x, b.png 2x"></body></html>"#);
        make_links_absolute(&mut d, "https://example.com/page");
        let s = d.serialize();
        assert!(s.contains(r#"href="https://example.com/rel""#), "got: {s}");
        assert!(s.contains(r#"src="https://cdn.example.com/a.png""#), "got: {s}");
        assert!(s.contains("https://example.com/a.png 1x"), "got: {s}");
    }

    #[test]
    fn extract_from_meta_single_value() {
        let d = doc(r#"<html><head><meta name="og:title" value="T"><meta name="og:title" value="T2"></head></html>"#);
        let cache = d.meta_names();
        // conflict: two values -> None
        assert_eq!(extract_from_meta(&d, &["og:title"], &cache, true), None);
    }

    #[test]
    fn extract_from_selectors_exact_match() {
        let d = doc("<html><body><div class=\"author\">Jane</div><div class=\"author\">x</div></body></html>");
        assert_eq!(extract_from_selectors(&d, &[".author"], 1), None);
        let d2 = doc("<html><body><div class=\"author\">Jane</div></body></html>");
        assert_eq!(extract_from_selectors(&d2, &[".author"], 1).as_deref(), Some("Jane"));
    }

    #[test]
    fn within_comment_detects() {
        let d = doc("<html><body><div class=\"comment\"><p>x</p></div></body></html>");
        let p = d.select_ids("p")[0];
        assert!(within_comment(&d, p));
    }

    #[test]
    fn strip_tags_removes_markup() {
        assert_eq!(strip_tags("<b>hello</b> <i>world</i>"), "hello world");
        assert_eq!(strip_tags("plain"), "plain");
    }
}
