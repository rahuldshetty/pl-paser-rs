//! Text and URL helpers ported from upstream `src/utils/` and
//! `src/utils/text/`.

use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

use crate::types::ParserError;

// --- regex constants (upstream `src/utils/text/constants.js`) ---

/// Matches page-number markers in a URL: `page=1`, `pg=1`, `p=1`,
/// `paging=12`, `pag=7`, `pagination/1`, `p/11`, … but not `pg=102` or
/// `page:2`. Capture group 6 is the page digit(s).
pub static PAGE_IN_HREF_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(page|paging|(p(a|g|ag)?(e|enum|ewanted|ing|ination)))?(=|/)([0-9]{1,3})")
        .expect("valid PAGE_IN_HREF_RE")
});

static HAS_ALPHA_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[a-z]").expect("valid alpha"));
static IS_ALPHA_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-z]+$").expect("valid alpha-only"));
static IS_DIGIT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[0-9]+$").expect("valid digit-only"));
static ENCODING_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"charset=([\w-]+)\b").expect("valid encoding re"));

/// Default charset used when none is declared (upstream `DEFAULT_ENCODING`).
pub const DEFAULT_ENCODING: &str = "utf-8";

/// Collapse runs of whitespace outside `<pre>/<code>/<textarea>` and trim.
/// Port of upstream `normalize-spaces.js`.
pub fn normalize_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_ws = false;
    let mut chars = text.chars().peekable();
    // Track whether we are inside one of the whitespace-preserving tags.
    let mut in_preserve = 0u32;

    while let Some(c) = chars.next() {
        // Detect <pre / <code / <textarea open tags (case-insensitive).
        if c == '<' {
            // Flush any pending whitespace before the tag.
            if pending_ws {
                out.push(' ');
                pending_ws = false;
            }
            // Find the end of the tag to determine what it is.
            let mut rest = String::new();
            for nc in chars.by_ref() {
                if nc == '>' {
                    break;
                }
                rest.push(nc);
            }
            let tag = rest.trim().to_ascii_lowercase();
            let closing = tag.starts_with('/');
            let name = tag
                .trim_start_matches('/')
                .split(|ch: char| ch.is_whitespace() || ch == '/')
                .next()
                .unwrap_or("");
            if !closing && matches!(name, "pre" | "code" | "textarea") {
                in_preserve += 1;
            } else if closing && matches!(name, "pre" | "code" | "textarea") {
                in_preserve = in_preserve.saturating_sub(1);
            }
            out.push('<');
            out.push_str(&rest);
            out.push('>');
            continue;
        }

        if in_preserve == 0 && c.is_whitespace() {
            pending_ws = true;
            continue;
        }

        if pending_ws {
            out.push(' ');
            pending_ws = false;
        }
        out.push(c);
    }

    out.trim().to_string()
}

/// Extract the page number from a URL, if any (upstream `page-num-from-url.js`).
pub fn page_num_from_url(url: &str) -> Option<u32> {
    let caps = PAGE_IN_HREF_RE.captures(url)?;
    let page_num: u32 = caps.get(6)?.as_str().parse().ok()?;
    // Return pageNum < 100, otherwise null.
    (page_num < 100).then_some(page_num)
}

/// Strip anchor (`#...`) and trailing slash (upstream `remove-anchor.js`).
pub fn remove_anchor(url: &str) -> String {
    url.split('#')
        .next()
        .unwrap_or("")
        .trim_end_matches('/')
        .to_string()
}

/// True if the text appears to contain a sentence ending (upstream
/// `has-sentence-end.js`): any period followed by a space or end of string.
pub fn has_sentence_end(text: &str) -> bool {
    text.contains(". ") || text.trim_end().ends_with('.')
}

/// First `words` whitespace-separated words (upstream `excerpt-content.js`).
pub fn excerpt_content(content: &str, words: usize) -> String {
    content
        .split_whitespace()
        .take(words)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Find the first regex in `regex_list` that matches `url` and return its
/// capture group 1 (upstream `extract-from-url.js`).
pub fn extract_from_url(url: &str, regex_list: &[Regex]) -> Option<String> {
    regex_list
        .iter()
        .find(|re| re.is_match(url))
        .and_then(|re| re.captures(url))
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

/// Detect the charset declared in a `content-type` / `<meta charset>` string
/// (upstream `get-encoding.js`). Returns the declared label verbatim when it
/// is a known encoding, otherwise `utf-8`.
pub fn get_encoding(str: &str) -> String {
    let declared = ENCODING_RE
        .captures(str)
        .and_then(|caps| caps.get(1).map(|m| m.as_str()));
    match declared {
        Some(label) if encoding_rs::Encoding::for_label(label.as_bytes()).is_some() => {
            label.to_string()
        }
        _ => DEFAULT_ENCODING.to_string(),
    }
}

// ---------------------------------------------------------------------------
// URL validation (upstream `validate-url.js`)
// ---------------------------------------------------------------------------

/// Extremely simple first-step URL validation: require a non-empty hostname.
pub fn validate_url(url: &Url) -> bool {
    url.host_str().map(|h| !h.is_empty()).unwrap_or(false)
}

/// Parse a URL string and validate it, returning the upstream error for
/// invalid URLs.
pub fn parse_and_validate_url(url: &str) -> Result<Url, ParserError> {
    let parsed = Url::parse(url).map_err(|_| ParserError::InvalidUrl)?;
    if validate_url(&parsed) {
        Ok(parsed)
    } else {
        Err(ParserError::InvalidUrl)
    }
}

/// Extract the base (registrable-ish) domain: last two dot-separated labels
/// of the host (upstream `baseDomain`).
pub fn base_domain(host: &str) -> String {
    let mut parts: Vec<&str> = host.split('.').collect();
    let len = parts.len();
    if len > 2 {
        parts = parts[len - 2..].to_vec();
    }
    parts.join(".")
}

// ---------------------------------------------------------------------------
// Article base URL (upstream `article-base-url.js`)
// ---------------------------------------------------------------------------

fn is_good_segment(segment: &str, index: usize, first_segment_has_letters: bool) -> bool {
    let mut good = true;
    // A short purely-numeric first/second segment is probably a page number
    // (kept here, but the short-segment rule below usually removes it).
    if index < 2 && IS_DIGIT_RE.is_match(segment) && segment.len() < 3 {
        good = true;
    }
    // First segment that is just "index" is removed.
    if index == 0 && segment.eq_ignore_ascii_case("index") {
        good = false;
    }
    // Short first/second segment with no letters in the first segment is removed.
    if index < 2 && segment.len() < 3 && !first_segment_has_letters {
        good = false;
    }
    good
}

/// Strip pagination data from a URL so it can be compared to other links
/// (upstream `article-base-url.js`).
pub fn article_base_url(url: &Url) -> String {
    let protocol = url.scheme();
    let host = url.host_str().unwrap_or("");
    let path = url.path();

    let mut first_segment_has_letters = false;
    let mut cleaned: Vec<String> = Vec::new();

    for (index, raw_segment) in path.split('/').rev().enumerate() {
        let mut segment = raw_segment.to_string();

        // Split off anything that looks like a file extension.
        if let Some(dot) = segment.rfind('.') {
            let (possible, file_ext) = segment.split_at(dot);
            let file_ext = &file_ext[1..];
            if IS_ALPHA_RE.is_match(file_ext) {
                segment = possible.to_string();
            }
        }

        // Remove page-number-like fragments in the first two segments.
        if index < 2 {
            if let Some(caps) = PAGE_IN_HREF_RE.captures(&segment) {
                if caps.get(6).is_some() {
                    segment = PAGE_IN_HREF_RE.replace(&segment, "").to_string();
                }
            }
        }

        if index == 0 {
            first_segment_has_letters = HAS_ALPHA_RE.is_match(&segment);
        }

        if is_good_segment(&segment, index, first_segment_has_letters) {
            cleaned.push(segment);
        }
    }

    cleaned.reverse();
    let path_joined = cleaned.join("/");
    format!("{protocol}://{host}{path_joined}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    #[test]
    fn normalize_spaces_collapses_runs() {
        assert_eq!(normalize_spaces("a  b   c"), "a b c");
        assert_eq!(
            normalize_spaces("  leading and trailing  "),
            "leading and trailing"
        );
    }

    #[test]
    fn normalize_spaces_preserves_pre_code_textarea() {
        assert_eq!(
            normalize_spaces("a  <pre>x  y</pre>  b"),
            "a <pre>x  y</pre> b"
        );
    }

    #[test]
    fn page_num_from_url_matches() {
        assert_eq!(page_num_from_url("http://x.com/story?page=1"), Some(1));
        assert_eq!(page_num_from_url("http://x.com/story/p/11"), Some(11));
        assert_eq!(page_num_from_url("http://x.com/story?pg=102"), None);
    }

    #[test]
    fn remove_anchor_strips_fragment_and_slash() {
        assert_eq!(remove_anchor("http://x.com/a#frag"), "http://x.com/a");
        assert_eq!(remove_anchor("http://x.com/a/"), "http://x.com/a");
    }

    #[test]
    fn has_sentence_end_detects_period() {
        assert!(has_sentence_end("Hello world."));
        assert!(has_sentence_end("Hello world. More"));
        assert!(!has_sentence_end("Hello world"));
    }

    #[test]
    fn excerpt_content_takes_words() {
        assert_eq!(excerpt_content("a b c d e", 3), "a b c");
    }

    #[test]
    fn get_encoding_parses_labels() {
        assert_eq!(get_encoding("text/html; charset=utf-8"), "utf-8");
        assert_eq!(get_encoding("text/html; charset=shift_jis"), "shift_jis");
        assert_eq!(get_encoding("text/html"), "utf-8");
    }

    #[test]
    fn validate_url_requires_host() {
        assert!(validate_url(&Url::parse("http://example.com/x").unwrap()));
        // A URL with no host is invalid.
        let no_host = Url::parse("mailto:someone@example.com").unwrap();
        assert!(!validate_url(&no_host));
        // A URL with an empty host is invalid.
        let empty_host = Url::parse("http://example.com").unwrap();
        let _ = empty_host;
        assert!(validate_url(
            &Url::parse("https://en.wikipedia.org/wiki/Thunder_(mascot)").unwrap()
        ));
    }

    #[test]
    fn base_domain_joins_last_two() {
        assert_eq!(
            base_domain("erotictrains.livejournal.com"),
            "livejournal.com"
        );
        assert_eq!(base_domain("example.com"), "example.com");
    }

    #[test]
    fn article_base_url_strips_page_segments() {
        let url =
            Url::parse("https://www.nytimes.com/2020/01/01/story.html?pagewanted=all").unwrap();
        // no pagination path segments; page query stays
        assert!(article_base_url(&url).contains("2020/01/01/story"));
        let url2 = Url::parse("https://x.com/story/2").unwrap();
        assert_eq!(article_base_url(&url2), "https://x.com/story");
    }

    #[test]
    fn article_base_url_upstream_cases() {
        // upstream article-base-url.test.js
        assert_eq!(
            article_base_url(&Url::parse("http://example.com/foo/bar/wow-cool/page=10").unwrap()),
            "http://example.com/foo/bar/wow-cool"
        );
        assert_eq!(
            article_base_url(&Url::parse("http://example.com/foo/bar/wow-cool/").unwrap()),
            "http://example.com/foo/bar/wow-cool"
        );
    }

    #[test]
    fn normalize_spaces_upstream_pre_case() {
        let input = "<div>\n        <p>What   do  you    think?</p>\n        <pre>  What     happens to        spaces?    </pre>\n      </div>";
        let expected =
            "<div> <p>What do you think?</p> <pre>  What     happens to        spaces?    </pre> </div>";
        assert_eq!(normalize_spaces(input), expected);
    }
}
