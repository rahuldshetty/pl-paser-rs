//! Content node scoring, ported from upstream
//! `src/extractors/generic/content/scoring/`.
//!
//! Scores are stored on elements as a `score` attribute, exactly like
//! upstream's `setScore`, so `getScore`/`getOrInitScore` stay compatible.

use ego_tree::NodeId;

use crate::dom::{link_density, text_length, Doc};
use crate::dom_utils::scoring_re;
use crate::utils::has_sentence_end;

/// Read a node's stored score (upstream `getScore`).
pub fn get_score(doc: &Doc, id: NodeId) -> Option<f64> {
    let raw = doc.attr_of(id, "score")?;
    let parsed: f64 = raw.parse().ok()?;
    // upstream: `parseFloat(attr) || null` — 0 is treated as unset
    if parsed == 0.0 {
        return None;
    }
    Some(parsed)
}

/// Store a node's score (upstream `setScore`).
pub fn set_score(doc: &mut Doc, id: NodeId, score: f64) {
    doc.set_attr(id, "score", &score.to_string());
}

/// Score based on className and id (upstream `getWeight`).
pub fn get_weight(doc: &Doc, id: NodeId) -> f64 {
    let Some(node) = doc.get(id) else {
        return 0.0;
    };
    let Some(element) = node.value().as_element() else {
        return 0.0;
    };
    let classes = element.attr("class").unwrap_or("").to_string();
    let id_attr = element.attr("id").unwrap_or("").to_string();
    let mut score = 0.0;

    if !id_attr.is_empty() {
        if scoring_re::is_positive_score_hint(&id_attr) {
            score += 25.0;
        }
        if scoring_re::is_negative_score_hint(&id_attr) {
            score -= 25.0;
        }
    }

    if !classes.is_empty() {
        if score == 0.0 {
            if scoring_re::is_positive_score_hint(&classes) {
                score += 25.0;
            }
            if scoring_re::is_negative_score_hint(&classes) {
                score -= 25.0;
            }
        }

        if scoring_re::is_photo_hint(&classes) {
            score += 10.0;
        }

        if scoring_re::is_readability_asset(&classes) {
            score += 25.0;
        }
    }

    score
}

/// One point per comma (upstream `scoreCommas`).
pub fn score_commas(text: &str) -> usize {
    text.matches(',').count()
}

/// Length bonus for a paragraph (upstream `scoreLength`).
pub fn score_length(text_length: f64, tag_name: &str) -> f64 {
    let chunks = text_length / 50.0;
    if chunks > 0.0 {
        let length_bonus = if matches!(tag_name.to_ascii_lowercase().as_str(), "p" | "pre") {
            chunks - 2.0
        } else {
            chunks - 1.25
        };
        return length_bonus.clamp(0.0, 3.0);
    }
    0.0
}

/// Score a paragraph node (upstream `scoreParagraph`).
pub fn score_paragraph(doc: &Doc, id: NodeId) -> f64 {
    let text = doc.text_of(id).unwrap_or_default();
    let text = text.trim();
    let text_len = text.len();
    if text_len < 25 {
        return 0.0;
    }

    let mut score = 1.0;
    score += score_commas(text) as f64;
    score += score_length(text_len as f64, "p");

    if text.ends_with(':') {
        score -= 1.0;
    }

    score
}

/// Score a single node based on tag type (upstream `scoreNode`).
pub fn score_node(doc: &Doc, id: NodeId) -> f64 {
    let Some(tag_name) = doc.element_name_of(id) else {
        return 0.0;
    };
    let tag = tag_name.to_lowercase();

    if scoring_re::is_paragraph_tag(&tag) {
        return score_paragraph(doc, id);
    }
    if tag == "div" {
        return 5.0;
    }
    if scoring_re::is_child_content_tag(&tag) {
        return 3.0;
    }
    if scoring_re::is_bad_tag(&tag) {
        return -3.0;
    }
    if tag == "th" {
        return -5.0;
    }
    0.0
}

/// Add a quarter of a child's score to its parent (upstream `addToParent`).
pub fn add_to_parent(doc: &mut Doc, id: NodeId, score: f64) {
    if let Some(parent) = doc.parent_of(id) {
        let current = get_or_init_score(doc, parent, true);
        set_score(doc, parent, current + score * 0.25);
    }
}

/// Get a node's score, initializing it if unset (upstream `getOrInitScore`).
pub fn get_or_init_score(doc: &mut Doc, id: NodeId, weight_nodes: bool) -> f64 {
    if let Some(score) = get_score(doc, id) {
        return score;
    }

    let mut score = score_node(doc, id);
    if weight_nodes {
        score += get_weight(doc, id);
    }

    add_to_parent(doc, id, score);
    score
}

/// Add an amount to a node's score (upstream `addScore`).
pub fn add_score(doc: &mut Doc, id: NodeId, amount: f64) {
    let score = get_or_init_score(doc, id, true) + amount;
    set_score(doc, id, score);
}

/// Score all content nodes (upstream `scoreContent`).
pub fn score_content(doc: &mut Doc, weight_nodes: bool) {
    // hNews selectors get a big boost to their parent.
    for (parent_selector, child_selector) in scoring_re::hnews_selectors() {
        let selector = format!("{parent_selector} {child_selector}");
        let matched: Vec<NodeId> = doc.select_ids(&selector);
        for id in matched {
            if let Some(parent) = doc.parent_of(id) {
                if doc.matches(parent, parent_selector) {
                    add_score(doc, parent, 80.0);
                }
            }
        }
    }

    score_ps(doc, weight_nodes);
    score_ps(doc, weight_nodes);
}

fn score_ps(doc: &mut Doc, weight_nodes: bool) {
    let ids: Vec<NodeId> = doc.select_ids("p, pre");
    for id in ids {
        if doc.attr_of(id, "score").is_some() {
            continue;
        }
        let score = get_or_init_score(doc, id, weight_nodes);
        set_score(doc, id, score);

        let raw_score = score_node(doc, id);
        if let Some(parent) = doc.parent_of(id) {
            add_score_to(doc, parent, raw_score);
            if let Some(grandparent) = doc.parent_of(parent) {
                add_score_to(doc, grandparent, raw_score / 2.0);
            }
        }
    }
}

fn add_score_to(doc: &mut Doc, id: NodeId, score: f64) {
    // Convert spans to divs when scoring (upstream `convertSpans`).
    if doc.element_name_of(id).map(|n| n.eq_ignore_ascii_case("span")).unwrap_or(false) {
        doc.convert_node_to(id, "div");
    }
    add_score(doc, id, score);
}

/// Find the highest-scoring candidate (upstream `findTopCandidate`).
#[derive(Debug)]
pub enum MergedContent {
    Single(NodeId),
    Merged(Vec<NodeId>),
}

pub fn merge_siblings_result(doc: &Doc, candidate: NodeId, top_score: f64) -> MergedContent {
    let Some(parent) = doc.parent_of(candidate) else {
        return MergedContent::Single(candidate);
    };
    let sibling_score_threshold = 10.0f64.max(top_score * 0.25);

    let mut merged: Vec<NodeId> = Vec::new();
    let candidate_class = doc.attr_of(candidate, "class");

    for sibling in doc.children_ids(parent) {
        let Some(tag_name) = doc.element_name_of(sibling) else {
            continue;
        };
        if scoring_re::is_non_top_candidate(&tag_name.to_lowercase()) {
            continue;
        }

        let Some(sibling_score) = get_score(doc, sibling) else {
            continue;
        };

        if sibling == candidate {
            merged.push(sibling);
            continue;
        }

        let mut content_bonus = 0.0;
        let density = doc.get(sibling).map(link_density).unwrap_or(0.0);

        if density < 0.05 {
            content_bonus += 20.0;
        }
        if density >= 0.5 {
            content_bonus -= 20.0;
        }
        if doc.attr_of(sibling, "class") == candidate_class {
            content_bonus += top_score * 0.2;
        }

        let new_score = sibling_score + content_bonus;
        if new_score >= sibling_score_threshold {
            merged.push(sibling);
            continue;
        }

        if tag_name.eq_ignore_ascii_case("p") {
            let sibling_content = doc.text_of(sibling).unwrap_or_default();
            let sibling_content_length = text_length(&sibling_content);
            if sibling_content_length > 80 && density < 0.25 {
                merged.push(sibling);
                continue;
            }
            if sibling_content_length <= 80
                && density == 0.0
                && has_sentence_end(&sibling_content)
            {
                merged.push(sibling);
            }
        }
    }

    if merged.len() == 1 && merged[0] == candidate {
        MergedContent::Single(candidate)
    } else {
        MergedContent::Merged(merged)
    }
}

/// The `findTopCandidate` entry point that returns a `MergedContent`.
pub fn find_top_candidate_merged(doc: &mut Doc) -> Option<MergedContent> {
    let scored: Vec<NodeId> = doc.select_ids("[score]");
    let mut candidate = None;
    let mut top_score = 0.0;

    for id in scored {
        let Some(name) = doc.element_name_of(id) else {
            continue;
        };
        if scoring_re::is_non_top_candidate(&name.to_lowercase()) {
            continue;
        }
        let score = get_score(doc, id).unwrap_or(0.0);
        if score > top_score {
            top_score = score;
            candidate = Some(id);
        }
    }

    let candidate = candidate.or_else(|| {
        let body = doc.select_ids("body").into_iter().next();
        if body.is_some() {
            body
        } else {
            doc.select_ids("*").into_iter().next()
        }
    })?;

    Some(merge_siblings_result(doc, candidate, top_score))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(html: &str) -> Doc {
        Doc::parse_document(html)
    }

    #[test]
    fn score_commas_counts() {
        assert_eq!(score_commas("a, b, c"), 2);
    }

    #[test]
    fn score_length_bonuses() {
        assert_eq!(score_length(150.0, "p"), 1.0);
        assert_eq!(score_length(500.0, "p"), 3.0);
        assert_eq!(score_length(50.0, "div"), 0.0); // 1 - 1.25 clamped to 0
    }

    #[test]
    fn score_paragraph_thresholds() {
        let d = doc("<html><body><p>short</p><p>This is a paragraph with more than twenty-five characters in it.</p></body></html>");
        let short = d.select_ids("p")[0];
        let long = d.select_ids("p")[1];
        assert_eq!(score_paragraph(&d, short), 0.0);
        assert!(score_paragraph(&d, long) >= 1.0);
    }

    #[test]
    fn score_node_tag_types() {
        let d = doc("<html><body><p>This is a nice long paragraph with enough text in it.</p><div>x</div><table><tr><th>y</th></tr></table><address>z</address></body></html>");
        assert!(score_node(&d, d.select_ids("p")[0]) >= 1.0);
        assert_eq!(score_node(&d, d.select_ids("div")[0]), 5.0);
        assert_eq!(score_node(&d, d.select_ids("th")[0]), -5.0);
        assert_eq!(score_node(&d, d.select_ids("address")[0]), -3.0);
    }

    #[test]
    fn get_weight_uses_hints() {
        let d = doc("<html><body><div id=\"comment\">x</div><div class=\"entry-content\">y</div></body></html>");
        let neg = d.select_ids("#comment")[0];
        let pos = d.select_ids(".entry-content")[0];
        assert_eq!(get_weight(&d, neg), -25.0);
        assert_eq!(get_weight(&d, pos), 25.0);
    }

    #[test]
    fn get_or_init_adds_parent_quarter() {
        let mut d = doc("<html><body><div><p>This is a paragraph with more than twenty-five characters in it for sure.</p></div></body></html>");
        let p = d.select_ids("p")[0];
        let div = d.select_ids("div")[0];
        let p_score = get_or_init_score(&mut d, p, true);
        assert!(p_score >= 1.0);
        let div_score = get_score(&d, div).unwrap_or(0.0);
        assert!(div_score >= p_score * 0.25, "div {div_score} vs p {p_score}");
    }
}
