//! Generic content extraction, ported from upstream
//! `src/extractors/generic/content/`.
//!
//! Pipeline: strip unlikely candidates → convert divs/spans/brs to
//! paragraphs → score nodes → find the top candidate (merging well-scored
//! siblings) → clean the extracted content. On failure, retry with
//! progressively laxer options against a fresh parse of the source HTML.

use ego_tree::NodeId;

use crate::dom::{append_cloned, node_text, Doc};
use crate::dom_utils::{
    clean_attributes, clean_h_ones, clean_headers, clean_images, clean_tags_conditionally,
    convert_to_paragraphs, make_links_absolute, mark_to_keep, remove_empty, rewrite_top_level,
    strip_junk_tags, strip_unlikely_candidates,
};
use crate::extractors::scoring::{find_top_candidate_merged, MergedContent};
use crate::utils::normalize_spaces;

/// The content extraction options (upstream `defaultOpts`).
#[derive(Debug, Clone, Copy)]
pub struct ContentOptions {
    pub strip_unlikely_candidates: bool,
    pub weight_nodes: bool,
    pub clean_conditionally: bool,
}

impl Default for ContentOptions {
    fn default() -> Self {
        Self {
            strip_unlikely_candidates: true,
            weight_nodes: true,
            clean_conditionally: true,
        }
    }
}

/// Extract the article content as an HTML string (upstream
/// `GenericContentExtractor.extract`).
pub fn extract_content(doc: &Doc, title: &str, url: &str) -> Option<String> {
    extract_content_with(doc, title, url, ContentOptions::default(), true)
}

/// Extract content with custom options and default-cleaner setting.
pub fn extract_content_with(
    doc: &Doc,
    title: &str,
    url: &str,
    mut opts: ContentOptions,
    default_cleaner: bool,
) -> Option<String> {
    let html = doc.serialize();

    let mut node = get_content_node(&html, title, url, opts, default_cleaner);
    if node.as_ref().is_some_and(node_is_sufficient) {
        return clean_and_return_node(node);
    }

    // Disable extraction options one by one and retry.
    let mut keys: Vec<&str> = Vec::new();
    if opts.strip_unlikely_candidates {
        keys.push("strip_unlikely_candidates");
    }
    if opts.weight_nodes {
        keys.push("weight_nodes");
    }
    if opts.clean_conditionally {
        keys.push("clean_conditionally");
    }

    for key in keys {
        match key {
            "strip_unlikely_candidates" => opts.strip_unlikely_candidates = false,
            "weight_nodes" => opts.weight_nodes = false,
            "clean_conditionally" => opts.clean_conditionally = false,
            _ => {}
        }
        node = get_content_node(&html, title, url, opts, default_cleaner);
        if node.as_ref().is_some_and(node_is_sufficient) {
            break;
        }
    }

    clean_and_return_node(node).or_else(|| readability_fallback(&html, url))
}

/// Optional last-resort content extraction via the `readability` crate.
fn readability_fallback(html: &str, url: &str) -> Option<String> {
    #[cfg(feature = "fallback")]
    {
        if let Some(content) =
            crate::extractors::readability_fallback::readability_content(html, url)
        {
            return Some(content);
        }
    }
    let _ = (html, url);
    None
}

fn get_content_node(
    html: &str,
    title: &str,
    url: &str,
    opts: ContentOptions,
    default_cleaner: bool,
) -> Option<Doc> {
    let mut source = Doc::parse_document(html);
    let (mut content, article_id) = extract_best_node(&mut source, opts)?;
    clean_content(&mut content, article_id, title, url, default_cleaner);
    Some(content)
}

fn node_is_sufficient(content: &Doc) -> bool {
    content
        .html
        .tree
        .root()
        .children()
        .next()
        .map(|n| node_text(n).trim().len() >= 100)
        .unwrap_or(false)
}

fn clean_and_return_node(content: Option<Doc>) -> Option<String> {
    let content = content?;
    let html = content.serialize();
    if html.is_empty() {
        return None;
    }
    Some(normalize_spaces(&html))
}

/// Find the top candidate and build a fresh document containing (clones of)
/// the merged content nodes (upstream `extractBestNode`).
fn extract_best_node(source: &mut Doc, opts: ContentOptions) -> Option<(Doc, NodeId)> {
    if opts.strip_unlikely_candidates {
        strip_unlikely_candidates(source);
    }

    convert_to_paragraphs(source);
    crate::extractors::scoring::score_content(source, opts.weight_nodes);

    let merged = find_top_candidate_merged(source)?;

    let mut content = Doc::new_fragment();
    let article_id = match merged {
        MergedContent::Single(id) => {
            let node = source.get(id)?;
            let mut root = content.html.tree.root_mut();
            append_cloned(&mut root, &node);
            let child = root.last_child()?;
            child.id()
        }
        MergedContent::Merged(ids) => {
            let mut wrapper_doc = Doc::parse_fragment("<div></div>");
            let wrapper = wrapper_doc.select_ids("div")[0];
            for id in &ids {
                if let Some(node) = source.get(*id) {
                    let root = wrapper_doc.html.tree.get_mut(wrapper);
                    if let Some(mut root) = root {
                        append_cloned(&mut root, &node);
                    }
                }
            }
            // Move the wrapper doc into `content` and return its id.
            let wrapper_id = wrapper_doc.html.tree.root().last_child()?.id();
            let wrapper_node = wrapper_doc.html.tree.get(wrapper_id)?;
            let mut content_root = content.html.tree.root_mut();
            append_cloned(&mut content_root, &wrapper_node);
            let child = content_root.last_child()?;
            child.id()
        }
    };

    Some((content, article_id))
}

/// Clean the extracted content node (upstream `cleanContent`).
pub fn clean_content(
    content: &mut Doc,
    article_id: NodeId,
    title: &str,
    url: &str,
    default_cleaner: bool,
) {
    rewrite_top_level(content);

    // Drop small images and spacer images (can be too aggressive, so only
    // when the default cleaner is enabled).
    if default_cleaner {
        clean_images(content, article_id);
    }

    make_links_absolute(content, url);

    mark_to_keep(content, article_id, url);

    strip_junk_tags(content, article_id);

    clean_h_ones(content, article_id);

    clean_headers(content, article_id, title);

    if default_cleaner {
        clean_tags_conditionally(content, article_id);
    }

    remove_empty(content, article_id);

    clean_attributes(content);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_article_content() {
        let html = r#"<html><body>
            <div class="nav">Home About Contact</div>
            <div class="content">
                <h1>My Great Article</h1>
                <p>This is the first paragraph of the article. It contains enough text to be considered real content by the scoring algorithm, which requires at least twenty-five characters per paragraph and one hundred characters overall.</p>
                <p>A second paragraph with even more useful information about the topic at hand, continuing the article body nicely.</p>
                <div class="comment"><p>spam comment</p></div>
            </div>
        </body></html>"#;
        let doc = Doc::parse_document(html);
        let content = extract_content(&doc, "My Great Article", "http://example.com").unwrap();
        assert!(content.contains("first paragraph"), "got: {content}");
        assert!(content.contains("second paragraph"), "got: {content}");
        assert!(!content.contains("spam comment"), "got: {content}");
    }

    #[test]
    fn falls_back_to_laxer_options() {
        // Little structure: divs only; first strict pass may fail, laxer pass succeeds.
        let html = r#"<html><body>
            <div>Some plain text that goes on for quite a while and should eventually be long enough to count as content when we fall back to laxer options during extraction of this particular page.</div>
        </body></html>"#;
        let doc = Doc::parse_document(html);
        let content = extract_content(&doc, "", "http://example.com");
        assert!(content.is_some());
    }
}
