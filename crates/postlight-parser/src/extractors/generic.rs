//! Generic per-field extractors, ported from upstream
//! `src/extractors/generic/<field>/`.

use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

use crate::cleaners::{
    clean_author, clean_date_published, clean_excerpt, clean_lead_image_url, clean_title,
};
use crate::dom::{element_text, node_text, Doc};
use crate::dom_utils::{extract_from_meta, extract_from_selectors, strip_tags};
use crate::extractors::lead_image::score_image_url;
use crate::utils::{extract_from_url, normalize_spaces};

// --- title ---

pub const STRONG_TITLE_META_TAGS: [&str; 5] = [
    "tweetmeme-title",
    "dc.title",
    "rbtitle",
    "headline",
    "title",
];
pub const WEAK_TITLE_META_TAGS: [&str; 1] = ["og:title"];
pub const STRONG_TITLE_SELECTORS: [&str; 6] = [
    ".hentry .entry-title",
    "h1#articleHeader",
    "h1.articleHeader",
    "h1.article",
    ".instapaper_title",
    "#meebo-title",
];
pub const WEAK_TITLE_SELECTORS: [&str; 15] = [
    "article h1",
    "#entry-title",
    ".entry-title",
    "#entryTitle",
    "#entrytitle",
    ".entryTitle",
    ".entrytitle",
    "#articleTitle",
    ".articleTitle",
    "post post-title",
    "h1.title",
    "h2.article",
    "h1",
    "html head title",
    "title",
];

/// Extract the title (upstream `GenericTitleExtractor.extract`).
pub fn extract_title(doc: &Doc, url: &str, meta_cache: &[String]) -> String {
    let mut title = extract_from_meta(doc, &STRONG_TITLE_META_TAGS, meta_cache, true);
    if let Some(t) = title {
        return clean_title(&t, url, doc);
    }

    title = extract_from_selectors(doc, &STRONG_TITLE_SELECTORS, 1);
    if let Some(t) = title {
        return clean_title(&t, url, doc);
    }

    title = extract_from_meta(doc, &WEAK_TITLE_META_TAGS, meta_cache, true);
    if let Some(t) = title {
        return clean_title(&t, url, doc);
    }

    title = extract_from_selectors(doc, &WEAK_TITLE_SELECTORS, 1);
    if let Some(t) = title {
        return clean_title(&t, url, doc);
    }

    String::new()
}

// --- author ---

pub const AUTHOR_META_TAGS: [&str; 7] = [
    "byl",
    "clmst",
    "dc.author",
    "dcsext.author",
    "dc.creator",
    "rbauthors",
    "authors",
];
pub const AUTHOR_MAX_LENGTH: usize = 300;
pub const AUTHOR_SELECTORS: [&str; 22] = [
    ".entry .entry-author",
    ".author.vcard .fn",
    ".author .vcard .fn",
    ".byline.vcard .fn",
    ".byline .vcard .fn",
    ".byline .by .author",
    ".byline .by",
    ".byline .author",
    ".post-author.vcard",
    ".post-author .vcard",
    "a[rel=author]",
    "#by_author",
    ".by_author",
    "#entryAuthor",
    ".entryAuthor",
    ".byline a[href*=author]",
    "#author .authorname",
    ".author .authorname",
    "#author",
    ".author",
    ".articleauthor",
    ".ArticleAuthor",
];

static BYLINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\n\s]*By").expect("byline re"));
const BYLINE_SELECTORS_RE: [(&str, usize); 2] = [("#byline", 0), (".byline", 0)];

/// Extract the author (upstream `GenericAuthorExtractor.extract`).
pub fn extract_author(doc: &Doc, meta_cache: &[String]) -> Option<String> {
    let author = extract_from_meta(doc, &AUTHOR_META_TAGS, meta_cache, true);
    if let Some(a) = author {
        if a.len() < AUTHOR_MAX_LENGTH {
            return Some(clean_author(&a));
        }
    }

    let author = extract_from_selectors(doc, &AUTHOR_SELECTORS, 2);
    if let Some(a) = author {
        if a.len() < AUTHOR_MAX_LENGTH {
            return Some(clean_author(&a));
        }
    }

    for (selector, _) in BYLINE_SELECTORS_RE {
        let nodes = doc.select(selector);
        if nodes.len() == 1 {
            let text = element_text(nodes[0]);
            if BYLINE_RE.is_match(&text) {
                return Some(clean_author(&text));
            }
        }
    }

    None
}

// --- date_published ---

pub const DATE_PUBLISHED_META_TAGS: [&str; 15] = [
    "article:published_time",
    "displaydate",
    "dc.date",
    "dc.date.issued",
    "rbpubdate",
    "publish_date",
    "pub_date",
    "pagedate",
    "pubdate",
    "revision_date",
    "doc_date",
    "date_created",
    "content_create_date",
    "lastmodified",
    "created",
];
pub const DATE_PUBLISHED_SELECTORS: [&str; 17] = [
    ".hentry .dtstamp.published",
    ".hentry .published",
    ".hentry .dtstamp.updated",
    ".hentry .updated",
    ".single .published",
    ".meta .published",
    ".meta .postDate",
    ".entry-date",
    ".byline .date",
    ".postmetadata .date",
    ".article_datetime",
    ".date-header",
    ".story-date",
    ".dateStamp",
    "#story .datetime",
    ".dateline",
    ".pubdate",
];

pub static DATE_PUBLISHED_URL_RES: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)/(20\d{2}/\d{2}/\d{2})/").expect("url date re 1"),
        Regex::new(r"(?i)(20\d{2}-[01]\d-[0-3]\d)").expect("url date re 2"),
        Regex::new(r"(?i)/(20\d{2}/(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)/[0-3]\d)/")
            .expect("url date re 3"),
    ]
});

/// Extract the publish date (upstream `GenericDatePublishedExtractor.extract`).
pub fn extract_date_published(doc: &Doc, url: &str, meta_cache: &[String]) -> Option<String> {
    let date = extract_from_meta(doc, &DATE_PUBLISHED_META_TAGS, meta_cache, false);
    if let Some(d) = date {
        return clean_date_published(&d, None, None);
    }

    let date = extract_from_selectors(doc, &DATE_PUBLISHED_SELECTORS, 1);
    if let Some(d) = date {
        return clean_date_published(&d, None, None);
    }

    let date = extract_from_url(url, &DATE_PUBLISHED_URL_RES);
    if let Some(d) = date {
        return clean_date_published(&d, None, None);
    }

    None
}

// --- dek ---

/// The generic dek extractor always returns null (upstream behavior).
pub fn extract_dek() -> Option<String> {
    None
}

// --- lead_image_url ---

pub const LEAD_IMAGE_URL_META_TAGS: [&str; 3] = ["og:image", "twitter:image", "image_src"];
pub const LEAD_IMAGE_URL_SELECTORS: [&str; 1] = ["link[rel=image_src]"];

/// Extract the lead image URL (upstream
/// `GenericLeadImageUrlExtractor.extract`).
pub fn extract_lead_image_url(
    doc: &Doc,
    content: Option<&str>,
    meta_cache: &[String],
) -> Option<String> {
    let image_url = extract_from_meta(doc, &LEAD_IMAGE_URL_META_TAGS, meta_cache, false);
    if let Some(url) = image_url {
        if let Some(clean) = clean_lead_image_url(&url) {
            return Some(clean);
        }
    }

    // Score the images inside the extracted content.
    if let Some(content) = content {
        let content_doc = Doc::parse_fragment(content);
        let img_elements: Vec<scraper::ElementRef<'_>> = content_doc.select("img");
        let total = img_elements.len();
        let mut img_scores: Vec<(String, f64)> = Vec::new();
        for (index, el) in img_elements.iter().enumerate() {
            let Some(src) = el.value().attr("src") else {
                continue;
            };
            let mut score = score_image_url(src);
            score += score_attr(el);
            score += score_by_parents(&content_doc, el.id());
            score += score_by_sibling(&content_doc, el.id());
            score += score_by_dimensions(el);
            score += score_by_position(total, index);
            img_scores.push((src.to_string(), score));
        }

        let top = img_scores.iter().max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((top_url, top_score)) = top {
            if *top_score > 0.0 {
                if let Some(clean) = clean_lead_image_url(top_url) {
                    return Some(clean);
                }
            }
        }
    }

    // Fall back to <link rel="image_src">.
    for selector in LEAD_IMAGE_URL_SELECTORS {
        if let Some(node) = doc.select_first(selector) {
            for attr in ["src", "href", "value"] {
                if let Some(v) = node.value().attr(attr) {
                    if let Some(clean) = clean_lead_image_url(v) {
                        return Some(clean);
                    }
                }
            }
        }
    }

    None
}

fn score_attr(el: &scraper::ElementRef<'_>) -> f64 {
    if el.value().attr("alt").is_some() {
        5.0
    } else {
        0.0
    }
}

fn score_by_parents(doc: &Doc, id: ego_tree::NodeId) -> f64 {
    let mut score = 0.0;
    let ancestors = doc.ancestors_of(id);
    let mut iter = ancestors.iter();
    let parent = iter.next().and_then(|p| doc.get(*p)).map(|n| n.id());
    let grandparent = iter.next().copied();
    let parent_is_figure = parent
        .and_then(|p| doc.get(p))
        .and_then(|n| n.value().as_element())
        .map(|e| e.name().eq_ignore_ascii_case("figure"))
        .unwrap_or(false);
    if parent_is_figure {
        score += 25.0;
    }
    for id in [parent, grandparent].into_iter().flatten() {
        if let Some(node) = doc.get(id) {
            if let Some(element) = node.value().as_element() {
                let sig = format!(
                    "{} {}",
                    element.attr("class").unwrap_or(""),
                    element.attr("id").unwrap_or("")
                );
                if crate::dom_utils::scoring_re::is_photo_hint(&sig) {
                    score += 15.0;
                }
            }
        }
    }
    score
}

fn score_by_sibling(doc: &Doc, id: ego_tree::NodeId) -> f64 {
    let mut score = 0.0;
    if let Some(sibling) = doc.next_sibling(id) {
        if let Some(node) = doc.get(sibling) {
            if let Some(element) = node.value().as_element() {
                if element.name().eq_ignore_ascii_case("figcaption") {
                    score += 25.0;
                }
                let sig = format!(
                    "{} {}",
                    element.attr("class").unwrap_or(""),
                    element.attr("id").unwrap_or("")
                );
                if crate::dom_utils::scoring_re::is_photo_hint(&sig) {
                    score += 15.0;
                }
            }
        }
    }
    score
}

fn score_by_dimensions(el: &scraper::ElementRef<'_>) -> f64 {
    let mut score = 0.0;
    let width = el.value().attr("width").and_then(|w| w.parse::<f64>().ok());
    let height = el
        .value()
        .attr("height")
        .and_then(|h| h.parse::<f64>().ok());
    let src = el.value().attr("src").unwrap_or("");

    if let Some(w) = width {
        if w <= 50.0 {
            score -= 50.0;
        }
    }
    if let Some(h) = height {
        if h <= 50.0 {
            score -= 50.0;
        }
    }

    if let (Some(w), Some(h)) = (width, height) {
        if !src.contains("sprite") {
            let area = w * h;
            if area < 5000.0 {
                score -= 100.0;
            } else {
                score += (area / 1000.0).round();
            }
        }
    }

    score
}

fn score_by_position(total: usize, index: usize) -> f64 {
    total as f64 / 2.0 - index as f64
}

// --- excerpt ---

pub const EXCERPT_META_SELECTORS: [&str; 2] = ["og:description", "twitter:description"];

/// Extract the excerpt (upstream `GenericExcerptExtractor.extract`).
pub fn extract_excerpt(doc: &Doc, content: Option<&str>, meta_cache: &[String]) -> Option<String> {
    let excerpt = extract_from_meta(doc, &EXCERPT_META_SELECTORS, meta_cache, true);
    if let Some(e) = excerpt {
        return Some(clean_excerpt(&strip_tags(&e), 200));
    }

    let max_length = 200;
    let short_content = content.map(|c| {
        let chars: Vec<char> = c.chars().take(max_length * 5).collect();
        chars.into_iter().collect::<String>()
    })?;
    let text = {
        let frag = Doc::parse_fragment(&short_content);
        let root = frag.html.tree.root();
        node_text(root)
    };
    Some(clean_excerpt(&text, max_length))
}

// --- word_count ---

/// Count words in content (upstream `getWordCount`).
pub fn extract_word_count(content: Option<&str>) -> Option<u32> {
    let content = content?;
    let doc = Doc::parse_fragment(content);
    let first_div = doc.select_first("div");
    let text = match first_div {
        Some(el) => element_text(el),
        None => node_text(doc.html.tree.root()),
    };
    let normalized = normalize_spaces(&text);
    let mut count = normalized.split_whitespace().count() as u32;
    if count == 1 {
        let stripped = strip_tags(content);
        let collapsed = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
        count = collapsed.split(' ').count() as u32;
    }
    Some(count)
}

// --- direction ---

/// Determine text direction (upstream `stringDirection.getDirection`).
pub fn extract_direction(title: &str) -> Option<String> {
    let mut ltr = 0usize;
    let mut rtl = 0usize;
    for ch in title.chars() {
        match unicode_bidi::bidi_class(ch) {
            unicode_bidi::BidiClass::L => ltr += 1,
            unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL => rtl += 1,
            _ => {}
        }
    }
    if rtl > ltr {
        Some("rtl".to_string())
    } else if ltr > rtl {
        Some("ltr".to_string())
    } else {
        None
    }
}

// --- url_and_domain ---

pub const CANONICAL_META_SELECTORS: [&str; 1] = ["og:url"];

/// Extract the canonical URL + domain (upstream `GenericUrlExtractor.extract`).
pub fn extract_url_and_domain(
    doc: &Doc,
    url: &str,
    meta_cache: &[String],
) -> (Option<String>, Option<String>) {
    let canonical = doc.select_first("link[rel=canonical]");
    if let Some(node) = canonical {
        if let Some(href) = node.value().attr("href") {
            let domain = Url::parse(href)
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()));
            return (Some(href.to_string()), domain);
        }
    }

    let meta_url = extract_from_meta(doc, &CANONICAL_META_SELECTORS, meta_cache, false);
    if let Some(u) = meta_url {
        let domain = Url::parse(&u)
            .ok()
            .and_then(|p| p.host_str().map(|h| h.to_string()));
        return (Some(u), domain);
    }

    let domain = Url::parse(url)
        .ok()
        .and_then(|p| p.host_str().map(|h| h.to_string()));
    (Some(url.to_string()), domain)
}

// --- next_page_url ---

pub use crate::extractors::next_page::extract_next_page_url;

/// Extract article content using the generic content extractor.
pub fn extract_content(doc: &Doc, url: &str) -> Option<String> {
    crate::extractors::content::extract_content(doc, "", url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_extraction_order() {
        let d = Doc::parse_document(
            r#"<html><head><meta name="title" value="Meta Title"></head><body><h1>H1 Title</h1></body></html>"#,
        );
        let cache = d.meta_names();
        assert_eq!(extract_title(&d, "http://x.com", &cache), "Meta Title");
    }

    #[test]
    fn title_falls_back_to_h1() {
        let d = Doc::parse_document("<html><body><h1>Just a Heading</h1></body></html>");
        let cache = d.meta_names();
        assert_eq!(extract_title(&d, "http://x.com", &cache), "Just a Heading");
    }

    #[test]
    fn author_meta_and_byline() {
        let d = Doc::parse_document(
            r#"<html><head><meta name="authors" value="Jane Doe"></head><body></body></html>"#,
        );
        let cache = d.meta_names();
        assert_eq!(extract_author(&d, &cache).as_deref(), Some("Jane Doe"));
    }

    #[test]
    fn author_byline_regex() {
        let d =
            Doc::parse_document("<html><body><div id=\"byline\">By John Smith</div></body></html>");
        let cache = d.meta_names();
        assert_eq!(extract_author(&d, &cache).as_deref(), Some("John Smith"));
    }

    #[test]
    fn date_from_meta() {
        let d = Doc::parse_document(
            r#"<html><head><meta name="article:published_time" value="2016-09-02T07:30:00Z"></head><body></body></html>"#,
        );
        let cache = d.meta_names();
        assert_eq!(
            extract_date_published(&d, "http://x.com", &cache).as_deref(),
            Some("2016-09-02T07:30:00.000Z")
        );
    }

    #[test]
    fn date_from_url() {
        let d = Doc::parse_document("<html><body></body></html>");
        let cache = d.meta_names();
        let url = "https://example.com/2019/02/03/story/";
        assert_eq!(
            extract_date_published(&d, url, &cache).as_deref(),
            Some("2019-02-03T00:00:00.000Z")
        );
    }

    #[test]
    fn lead_image_from_meta() {
        let d = Doc::parse_document(
            r#"<html><head><meta name="og:image" value="https://x.com/img.jpg"></head><body></body></html>"#,
        );
        let cache = d.meta_names();
        assert_eq!(
            extract_lead_image_url(&d, None, &cache).as_deref(),
            Some("https://x.com/img.jpg")
        );
    }

    #[test]
    fn word_count_counts() {
        let content = "<div><p>one two three</p><p>four five</p></div>";
        // cheerio .text() concatenates block children without a separator.
        assert_eq!(extract_word_count(Some(content)), Some(4));
    }

    #[test]
    fn direction_ltr_and_rtl() {
        assert_eq!(extract_direction("Hello world").as_deref(), Some("ltr"));
        assert_eq!(extract_direction("שלום עולם").as_deref(), Some("rtl"));
        assert_eq!(extract_direction("12345"), None);
    }

    #[test]
    fn canonical_url() {
        let d = Doc::parse_document(
            r#"<html><head><link rel="canonical" href="https://example.com/canonical"></head><body></body></html>"#,
        );
        let cache = d.meta_names();
        let (url, domain) = extract_url_and_domain(&d, "https://example.com/x", &cache);
        assert_eq!(url.as_deref(), Some("https://example.com/canonical"));
        assert_eq!(domain.as_deref(), Some("example.com"));
    }
}
