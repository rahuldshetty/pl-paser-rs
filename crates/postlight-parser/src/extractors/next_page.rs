//! Next-page URL detection, ported from upstream
//! `src/extractors/generic/next-page-url/`.

use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

use crate::dom::Doc;
use crate::dom_utils::{is_wordpress, scoring_re};
use crate::utils::{article_base_url, page_num_from_url, remove_anchor};

static EXTRANEOUS_LINK_HINTS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new("(?i)print|archive|comment|discuss|e-mail|email|share|reply|all|login|sign|single|adx|entry-unrelated")
        .expect("extraneous re")
});
static NEXT_LINK_TEXT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(next|weiter|continue|>([^|]|$)|»([^|]|$))").expect("next re"));
static CAP_LINK_TEXT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(first|last|end)").expect("cap re"));
static PREV_LINK_TEXT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(prev|earl|old|new|<|«)").expect("prev re"));
static PAGE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)pag(e|ing|inat)").expect("page re"));
static DIGIT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[0-9]").expect("digit re"));

/// Extract the next page URL (upstream `GenericNextPageUrlExtractor.extract`).
pub fn extract_next_page_url(
    doc: &Doc,
    url: &str,
    parsed_url: &Url,
    previous_urls: &[String],
) -> Option<String> {
    let article_url = remove_anchor(url);
    let base_url = article_base_url(parsed_url);

    let link_ids: Vec<ego_tree::NodeId> = doc.select_ids("a[href]");

    let mut scored_pages: Vec<ScoredPage> = Vec::new();
    let is_wp = is_wordpress(doc);

    for link_id in link_ids {
        let Some(href) = doc.attr_of(link_id, "href") else {
            continue;
        };
        let href = remove_anchor(&href);
        let link_text = doc.text_of(link_id).unwrap_or_default();

        if !should_score(
            &href,
            &article_url,
            &base_url,
            parsed_url,
            &link_text,
            previous_urls,
        ) {
            continue;
        }

        // Find or create the entry for this href.
        let index = if let Some(i) = scored_pages.iter().position(|p| p.href == href) {
            scored_pages[i].link_text = format!("{}|{}", scored_pages[i].link_text, link_text);
            i
        } else {
            scored_pages.push(ScoredPage {
                score: 0.0,
                link_text: link_text.clone(),
                href: href.clone(),
            });
            scored_pages.len() - 1
        };

        let link_data = make_sig(doc, link_id, &link_text);
        let page_num = page_num_from_url(&href);

        let mut score = score_base_url(&href, &base_url);
        score += score_next_link_text(&link_data);
        score += score_cap_links(&link_data);
        score += score_prev_link(&link_data);
        score += score_by_parents(doc, link_id);
        score += score_extraneous_links(&href);
        score += score_page_in_link(page_num, is_wp);
        score += score_link_text(&link_text, page_num);
        score += score_similarity(score, &article_url, &href);

        scored_pages[index].score = score;
    }

    if scored_pages.is_empty() {
        return None;
    }

    let top = scored_pages
        .iter()
        .max_by(|a, b| a.score.total_cmp(&b.score))?;
    if top.score >= 50.0 {
        return Some(top.href.clone());
    }

    None
}

struct ScoredPage {
    score: f64,
    link_text: String,
    href: String,
}

fn make_sig(doc: &Doc, id: ego_tree::NodeId, link_text: &str) -> String {
    let class = doc.attr_of(id, "class").unwrap_or_default();
    let id_attr = doc.attr_of(id, "id").unwrap_or_default();
    format!("{link_text} {class} {id_attr}")
}

fn should_score(
    href: &str,
    article_url: &str,
    base_url: &str,
    parsed_url: &Url,
    link_text: &str,
    previous_urls: &[String],
) -> bool {
    if previous_urls.iter().any(|u| u == href) {
        return false;
    }
    if href.is_empty() || href == article_url || href == base_url {
        return false;
    }

    let link_host = Url::parse(href)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string));
    if link_host.as_deref() != parsed_url.host_str() {
        return false;
    }

    let fragment = href.replace(base_url, "");
    if !DIGIT_RE.is_match(&fragment) {
        return false;
    }

    if EXTRANEOUS_LINK_HINTS_RE.is_match(link_text) {
        return false;
    }

    if link_text.len() > 25 {
        return false;
    }

    true
}

fn score_base_url(href: &str, base_url: &str) -> f64 {
    if !href
        .to_ascii_lowercase()
        .starts_with(&base_url.to_ascii_lowercase())
    {
        return -25.0;
    }
    0.0
}

fn score_next_link_text(link_data: &str) -> f64 {
    if NEXT_LINK_TEXT_RE.is_match(link_data) {
        50.0
    } else {
        0.0
    }
}

fn score_cap_links(link_data: &str) -> f64 {
    if CAP_LINK_TEXT_RE.is_match(link_data) && NEXT_LINK_TEXT_RE.is_match(link_data) {
        -65.0
    } else {
        0.0
    }
}

fn score_prev_link(link_data: &str) -> f64 {
    if PREV_LINK_TEXT_RE.is_match(link_data) {
        -200.0
    } else {
        0.0
    }
}

fn score_by_parents(doc: &Doc, link_id: ego_tree::NodeId) -> f64 {
    let mut score = 0.0;
    let mut positive_match = false;
    let mut negative_match = false;

    let ancestors = doc.ancestors_of(link_id);
    for ancestor in ancestors.iter().take(5) {
        let Some(node) = doc.get(*ancestor) else {
            continue;
        };
        let Some(element) = node.value().as_element() else {
            continue;
        };
        let parent_data = format!(
            "{} {}",
            element.attr("class").unwrap_or(""),
            element.attr("id").unwrap_or("")
        );

        if !positive_match && PAGE_RE.is_match(&parent_data) {
            positive_match = true;
            score += 25.0;
        }

        if !negative_match
            && scoring_re::is_negative_score_hint(&parent_data)
            && EXTRANEOUS_LINK_HINTS_RE.is_match(&parent_data)
            && !scoring_re::is_positive_score_hint(&parent_data)
        {
            negative_match = true;
            score -= 25.0;
        }
    }

    score
}

fn score_extraneous_links(href: &str) -> f64 {
    if EXTRANEOUS_LINK_HINTS_RE.is_match(href) {
        -25.0
    } else {
        0.0
    }
}

fn score_page_in_link(page_num: Option<u32>, is_wp: bool) -> f64 {
    if page_num.is_some() && !is_wp {
        50.0
    } else {
        0.0
    }
}

fn score_link_text(link_text: &str, page_num: Option<u32>) -> f64 {
    let trimmed = link_text.trim();
    if !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return 0.0;
    }
    let Ok(link_text_as_num) = trimmed.parse::<i64>() else {
        return 0.0;
    };
    let mut score = if link_text_as_num < 2 {
        -30.0
    } else {
        (10.0 - link_text_as_num as f64).max(0.0)
    };
    if let Some(page_num) = page_num {
        if page_num as i64 >= link_text_as_num {
            score -= 50.0;
        }
    }
    score
}

/// Score based on difflib `SequenceMatcher.ratio` similarity (upstream
/// `scoreSimilarity`).
fn score_similarity(score: f64, article_url: &str, href: &str) -> f64 {
    if score > 0.0 {
        let similarity = sequence_matcher_ratio(article_url, href);
        let diff_percent = 1.0 - similarity;
        let diff_modifier = -(250.0 * (diff_percent - 0.2));
        score + diff_modifier
    } else {
        0.0
    }
}

/// `difflib.SequenceMatcher(None, a, b).ratio()`.
pub fn sequence_matcher_ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let b2j: std::collections::HashMap<char, Vec<usize>> = {
        let mut map: std::collections::HashMap<char, Vec<usize>> = std::collections::HashMap::new();
        for (i, c) in b.iter().enumerate() {
            map.entry(*c).or_default().push(i);
        }
        map
    };

    fn find_longest(
        a: &[char],
        _b: &[char],
        b2j: &std::collections::HashMap<char, Vec<usize>>,
        a_lo: usize,
        a_hi: usize,
        b_lo: usize,
        b_hi: usize,
    ) -> (usize, usize, usize) {
        let mut best_i = a_lo;
        let mut best_j = b_lo;
        let mut best_size = 0usize;

        let mut j2len: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        for (offset, a_i) in a[a_lo..a_hi].iter().enumerate() {
            let i = a_lo + offset;
            let mut new_j2len: std::collections::HashMap<usize, usize> =
                std::collections::HashMap::new();
            if let Some(js) = b2j.get(a_i) {
                for &j in js {
                    if j < b_lo {
                        continue;
                    }
                    if j >= b_hi {
                        break;
                    }
                    let k = j2len.get(&(j.wrapping_sub(1))).copied().unwrap_or(0) + 1;
                    new_j2len.insert(j, k);
                    if k > best_size {
                        best_i = i + 1 - k;
                        best_j = j + 1 - k;
                        best_size = k;
                    }
                }
            }
            j2len = new_j2len;
        }
        (best_i, best_j, best_size)
    }

    fn total_matching(
        a: &[char],
        b: &[char],
        b2j: &std::collections::HashMap<char, Vec<usize>>,
        a_lo: usize,
        a_hi: usize,
        b_lo: usize,
        b_hi: usize,
    ) -> usize {
        let (i, j, k) = find_longest(a, b, b2j, a_lo, a_hi, b_lo, b_hi);
        if k == 0 {
            return 0;
        }
        let mut total = k;
        if i > a_lo && j > b_lo {
            total += total_matching(a, b, b2j, a_lo, i, b_lo, j);
        }
        if i + k < a_hi && j + k < b_hi {
            total += total_matching(a, b, b2j, i + k, a_hi, j + k, b_hi);
        }
        total
    }

    let matching = total_matching(&a, &b, &b2j, 0, a.len(), 0, b.len());
    2.0 * matching as f64 / (a.len() + b.len()) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_matcher_identical() {
        assert_eq!(sequence_matcher_ratio("abc", "abc"), 1.0);
    }

    #[test]
    fn sequence_matcher_disjoint() {
        assert_eq!(sequence_matcher_ratio("abc", "xyz"), 0.0);
    }

    #[test]
    fn sequence_matcher_partial() {
        let r = sequence_matcher_ratio("abcd", "abef");
        assert!(r > 0.3 && r < 0.6, "ratio = {r}");
    }

    #[test]
    fn score_link_text_rules() {
        assert_eq!(score_link_text("2", None), 8.0);
        assert_eq!(score_link_text("5", None), 5.0);
        assert_eq!(score_link_text("1", None), -30.0);
        assert_eq!(score_link_text("4", Some(5)), -44.0);
    }

    #[test]
    fn base_url_penalty() {
        assert_eq!(
            score_base_url("http://foo.com/x", "http://example.com"),
            -25.0
        );
        assert_eq!(
            score_base_url("http://example.com/x", "http://example.com"),
            0.0
        );
    }

    #[test]
    fn next_page_url_detection() {
        let html = r#"<html><body>
            <p>Article text with plenty of words and enough content to be considered real.</p>
            <a href="http://example.com/story?page=2">Next</a>
        </body></html>"#;
        let doc = Doc::parse_document(html);
        let parsed = Url::parse("http://example.com/story").unwrap();
        let next = extract_next_page_url(&doc, "http://example.com/story", &parsed, &[]);
        assert!(next.is_some(), "expected next page");
    }

    #[test]
    fn no_next_page_when_absent() {
        let html = r#"<html><body><p>Article text with plenty of words and enough content to be considered real here.</p><a href="/about">About</a></body></html>"#;
        let doc = Doc::parse_document(html);
        let parsed = Url::parse("http://example.com/story").unwrap();
        let next = extract_next_page_url(&doc, "http://example.com/story", &parsed, &[]);
        assert!(next.is_none(), "expected no next page, got {next:?}");
    }
}
