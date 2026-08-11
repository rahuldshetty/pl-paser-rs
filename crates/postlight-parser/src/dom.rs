//! DOM wrapper and helpers around `scraper`, mirroring upstream
//! `src/utils/dom/` and `src/resource/utils/dom/`.
//!
//! `Doc` owns a `scraper::Html` tree and exposes the query/mutation surface
//! the port needs: CSS selection, text/attr extraction, and tree edits
//! (removal, tag renaming, attribute setting, fragment grafting) via
//! `ego_tree` node handles (`NodeId`).

use ego_tree::{NodeId, NodeMut, NodeRef};
use html5ever::interface::QualName;
use html5ever::ns;
use scraper::node::{Element, Node};
use scraper::{ElementRef, Html, Selector, StrTendril};

/// A parsed HTML document with query + mutation helpers.
pub struct Doc {
    pub html: Html,
}

impl Doc {
    pub fn parse_document(input: &str) -> Doc {
        Doc { html: Html::parse_document(input) }
    }

    pub fn parse_fragment(input: &str) -> Doc {
        Doc { html: Html::parse_fragment(input) }
    }

    /// Serialize the whole document.
    pub fn serialize(&self) -> String {
        self.html.html()
    }

    // -----------------------------------------------------------------------
    // Querying
    // -----------------------------------------------------------------------

    /// All elements matching a CSS selector.
    pub fn select(&self, selector: &str) -> Vec<ElementRef<'_>> {
        let Ok(sel) = Selector::parse(selector) else {
            return Vec::new();
        };
        self.html.select(&sel).collect()
    }

    /// Node ids of all elements matching a CSS selector.
    pub fn select_ids(&self, selector: &str) -> Vec<NodeId> {
        let Ok(sel) = Selector::parse(selector) else {
            return Vec::new();
        };
        self.html.select(&sel).map(|e| e.id()).collect()
    }

    /// First element matching, if any.
    pub fn select_first(&self, selector: &str) -> Option<ElementRef<'_>> {
        self.select(selector).into_iter().next()
    }

    /// Number of matches for a selector.
    pub fn count(&self, selector: &str) -> usize {
        self.select(selector).len()
    }

    /// Trimmed text of the first match.
    pub fn text(&self, selector: &str) -> Option<String> {
        self.select_first(selector).map(element_text)
    }

    /// Trimmed text of every match.
    pub fn text_all(&self, selector: &str) -> Vec<String> {
        self.select(selector).iter().map(|e| element_text(*e)).collect()
    }

    /// Trimmed attribute value of the first match.
    pub fn attr(&self, selector: &str, attr: &str) -> Option<String> {
        self.select_first(selector)
            .and_then(|e| e.value().attr(attr))
            .map(|s| s.trim().to_string())
    }

    /// Attribute values of every match.
    pub fn attr_all(&self, selector: &str, attr: &str) -> Vec<String> {
        self.select(selector)
            .iter()
            .filter_map(|e| e.value().attr(attr))
            .map(|s| s.trim().to_string())
            .collect()
    }

    /// Node value lookup.
    pub fn get(&self, id: NodeId) -> Option<NodeRef<'_, Node>> {
        self.html.tree.get(id)
    }

    /// Outer HTML of the element at `id`.
    pub fn html_of(&self, id: NodeId) -> Option<String> {
        let node = self.get(id)?;
        let el = ElementRef::wrap(node)?;
        Some(el.html())
    }

    /// Inner HTML of the element at `id`.
    pub fn inner_html_of(&self, id: NodeId) -> Option<String> {
        let node = self.get(id)?;
        let el = ElementRef::wrap(node)?;
        Some(el.inner_html())
    }

    /// Descendant text of the element at `id` (all text nodes, concatenated).
    pub fn text_of(&self, id: NodeId) -> Option<String> {
        let node = self.get(id)?;
        Some(node_text(node))
    }

    /// Attribute of the element at `id`.
    pub fn attr_of(&self, id: NodeId, attr: &str) -> Option<String> {
        let node = self.get(id)?;
        let element = node.value().as_element()?;
        element.attr(attr).map(|s| s.trim().to_string())
    }

    /// Element tag name at `id`.
    pub fn element_name_of(&self, id: NodeId) -> Option<String> {
        let node = self.get(id)?;
        node.value().as_element().map(|e| e.name().to_string())
    }

    /// True if the element at `id` matches `selector`.
    pub fn matches(&self, id: NodeId, selector: &str) -> bool {
        let node = match self.get(id) {
            Some(n) => n,
            None => return false,
        };
        let Some(el) = ElementRef::wrap(node) else {
            return false;
        };
        let Ok(sel) = Selector::parse(selector) else {
            return false;
        };
        sel.matches(&el)
    }

    /// All ancestor ids of `id`, nearest first.
    pub fn ancestors_of(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = self.get(id);
        while let Some(node) = cur {
            match node.parent() {
                Some(p) => {
                    out.push(p.id());
                    cur = Some(p);
                }
                None => break,
            }
        }
        out
    }

    /// Direct children ids of the element at `id`.
    pub fn children_ids(&self, id: NodeId) -> Vec<NodeId> {
        let node = match self.get(id) {
            Some(n) => n,
            None => return Vec::new(),
        };
        node.children().map(|c| c.id()).collect()
    }

    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        self.get(id).and_then(|n| n.parent().map(|p| p.id()))
    }

    /// `id` is a descendant of `ancestor_id`.
    pub fn is_descendant_of(&self, id: NodeId, ancestor_id: NodeId) -> bool {
        self.ancestors_of(id).contains(&ancestor_id)
    }

    /// Cached list of every `meta` element's `name` attribute.
    pub fn meta_names(&self) -> Vec<String> {
        self.attr_all("meta", "name")
    }

    /// Number of top-level children in the parsed tree (used to detect a
    /// completely failed parse, upstream `$.root().children().length`).
    pub fn root_children_count(&self) -> usize {
        self.html.tree.root().children().count()
    }

    // -----------------------------------------------------------------------
    // Mutation
    // -----------------------------------------------------------------------

    /// Remove nodes (and their subtrees) by id.
    pub fn remove(&mut self, ids: &[NodeId]) {
        for id in ids {
            if let Some(mut node) = self.html.tree.get_mut(*id) {
                node.detach();
            }
        }
    }

    /// Remove every node matching a selector; returns the number removed.
    pub fn remove_selector(&mut self, selector: &str) -> usize {
        let ids = self.select_ids(selector);
        let n = ids.len();
        self.remove(&ids);
        n
    }

    /// Remove all comment nodes.
    pub fn remove_comments(&mut self) {
        let ids: Vec<NodeId> = self
            .html
            .tree
            .root()
            .descendants()
            .filter(|n| n.value().is_comment())
            .map(|n| n.id())
            .collect();
        self.remove(&ids);
    }

    /// Set an attribute on the element at `id`.
    pub fn set_attr(&mut self, id: NodeId, name: &str, value: &str) {
        if let Some(mut node) = self.html.tree.get_mut(id) {
            set_element_attr(node.value(), name, value);
        }
    }

    /// Remove an attribute from the element at `id`.
    pub fn remove_attr(&mut self, id: NodeId, name: &str) {
        if let Some(mut node) = self.html.tree.get_mut(id) {
            if let Node::Element(element) = node.value() {
                let qn = attr_qualname(name);
                if let Ok(i) = element.attrs.binary_search_by(|(n, _)| n.cmp(&qn)) {
                    element.attrs.remove(i);
                }
            }
        }
    }

    /// Set an attribute on every element in `ids`.
    pub fn set_attrs(&mut self, ids: &[NodeId], name: &str, value: &str) {
        for id in ids {
            self.set_attr(*id, name, value);
        }
    }

    /// Rename the element at `id` (upstream `convertNodeTo`).
    pub fn convert_node_to(&mut self, id: NodeId, tag: &str) {
        if let Some(mut node) = self.html.tree.get_mut(id) {
            if let Node::Element(element) = node.value() {
                element.name = element_qualname(tag);
            }
        }
    }

    /// Rename every element in `ids`.
    pub fn convert_nodes_to(&mut self, ids: &[NodeId], tag: &str) {
        for id in ids {
            self.convert_node_to(*id, tag);
        }
    }

    /// Replace the inner HTML of the element at `id` with the parsed fragment
    /// (upstream `$node.html(...)` used by content transforms).
    pub fn replace_inner_html(&mut self, id: NodeId, html: &str) {
        let fragment = Html::parse_fragment(html);
        let sources = fragment_sources(&fragment);
        let Some(mut node) = self.html.tree.get_mut(id) else {
            return;
        };
        // Remove existing children.
        while let Some(mut child) = node.first_child() {
            child.detach();
        }
        for source in &sources {
            append_cloned(&mut node, source);
        }
    }

    /// Wrap the element at `id` in a new `<tag>` element; returns the wrapper
    /// id (upstream `$node.wrap($('<div></div>'))`).
    pub fn wrap(&mut self, id: NodeId, tag: &str) -> Option<NodeId> {
        let wrapper_id = {
            let mut node = self.html.tree.get_mut(id)?;
            node.insert_before(Node::Element(Element::new(element_qualname(tag), Vec::new())))
                .id()
        };
        // Move the original under the wrapper.
        {
            let mut wrapper = self.html.tree.get_mut(wrapper_id)?;
            wrapper.append_id(id);
        }
        Some(wrapper_id)
    }

    /// Parse `html` and prepend the resulting nodes to the element at `id`.
    pub fn prepend_child(&mut self, id: NodeId, html: &str) {
        let fragment = Html::parse_fragment(html);
        let sources = fragment_sources(&fragment);
        let Some(mut node) = self.html.tree.get_mut(id) else {
            return;
        };
        for source in sources.iter().rev() {
            let mut first = node.prepend(source.value().clone());
            for gc in source.children() {
                append_cloned(&mut first, &gc);
            }
        }
    }

    /// Parse `html` and append the resulting nodes to the element at `id`.
    pub fn append_child(&mut self, id: NodeId, html: &str) {
        let fragment = Html::parse_fragment(html);
        let sources = fragment_sources(&fragment);
        let Some(mut node) = self.html.tree.get_mut(id) else {
            return;
        };
        for source in &sources {
            append_cloned(&mut node, source);
        }
    }

    /// Parse `html` and insert the resulting nodes before the element at `id`.
    pub fn insert_before(&mut self, id: NodeId, html: &str) {
        let fragment = Html::parse_fragment(html);
        let sources = fragment_sources(&fragment);
        let Some(mut node) = self.html.tree.get_mut(id) else {
            return;
        };
        for source in &sources {
            let mut sib = node.insert_before(source.value().clone());
            for gc in source.children() {
                append_cloned(&mut sib, &gc);
            }
        }
    }

    /// Parse `html` and insert the resulting nodes after the element at `id`.
    pub fn insert_after(&mut self, id: NodeId, html: &str) {
        let fragment = Html::parse_fragment(html);
        let sources = fragment_sources(&fragment);
        let Some(mut node) = self.html.tree.get_mut(id) else {
            return;
        };
        for source in &sources {
            let mut sib = node.insert_after(source.value().clone());
            for gc in source.children() {
                append_cloned(&mut sib, &gc);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// Element (html-namespace) qualified name for a tag.
fn element_qualname(tag: &str) -> QualName {
    QualName::new(None, ns!(html), tag.into())
}

/// Attribute qualified name (empty namespace, matching scraper storage).
fn attr_qualname(name: &str) -> QualName {
    QualName::new(None, ns!(), name.into())
}

/// Set an attribute on a node value (maintaining the sorted attrs vec).
fn set_element_attr(node: &mut Node, name: &str, value: &str) {
    if let Node::Element(element) = node {
        let qn = attr_qualname(name);
        match element.attrs.binary_search_by(|(n, _)| n.cmp(&qn)) {
            Ok(i) => element.attrs[i].1 = StrTendril::from(value),
            Err(i) => element.attrs.insert(i, (qn, StrTendril::from(value))),
        }
    }
}

/// Concatenated descendant text of a node.
pub fn node_text(node: NodeRef<'_, Node>) -> String {
    let mut out = String::new();
    for desc in node.descendants() {
        if let Node::Text(t) = desc.value() {
            out.push_str(t);
        }
    }
    out
}

/// Concatenated descendant text of an element ref.
pub fn element_text(el: ElementRef<'_>) -> String {
    let mut out = String::new();
    for t in el.text() {
        out.push_str(t);
    }
    out.trim().to_string()
}

/// Deep-clone `source` (a node in another tree) as a child of `parent`.
fn append_cloned(parent: &mut NodeMut<'_, Node>, source: &NodeRef<'_, Node>) {
    let mut child = parent.append(source.value().clone());
    for gc in source.children() {
        append_cloned(&mut child, &gc);
    }
}

/// The content nodes of a parsed fragment. `Html::parse_fragment` wraps the
/// content in a synthetic `<html>` (and possibly `<head>`/`<body>`); this
/// unwraps to the actual fragment content.
fn fragment_sources(fragment: &Html) -> Vec<NodeRef<'_, Node>> {
    let root = fragment.tree.root();
    let top: Vec<NodeRef<'_, Node>> = root.children().collect();
    if top.len() == 1 {
        if let Some(el) = top[0].value().as_element() {
            if el.name().eq_ignore_ascii_case("html") {
                return top[0]
                    .children()
                    .filter(|n| {
                        !n.value()
                            .as_element()
                            .map(|e| e.name().eq_ignore_ascii_case("head"))
                            .unwrap_or(false)
                    })
                    .collect();
            }
        }
    }
    top
}

/// All attributes of an element as `(name, value)` (upstream `getAttrs`).
pub fn get_attrs(el: ElementRef<'_>) -> Vec<(String, String)> {
    el.value()
        .attrs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Total length of descendant text of a node (upstream `textLength`).
pub fn text_length(node: NodeRef<'_, Node>) -> usize {
    node_text(node).len()
}

/// Length of text inside descendant `<a>` elements (upstream
/// `linkTextLength`).
pub fn link_text_length(node: NodeRef<'_, Node>) -> usize {
    let mut link_text = 0usize;
    for desc in node.descendants() {
        let Some(element) = desc.value().as_element() else {
            continue;
        };
        if element.name().eq_ignore_ascii_case("a") {
            link_text += node_text(desc).len();
        }
    }
    link_text
}

/// Link density: text inside descendant `<a>` elements over total text
/// (upstream `linkDensity`).
pub fn link_density(node: NodeRef<'_, Node>) -> f64 {
    let total = text_length(node);
    if total == 0 {
        return 0.0;
    }
    link_text_length(node) as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_and_text() {
        let doc = Doc::parse_document(
            "<html><body><h1>Title</h1><p>Hello <b>world</b></p></body></html>",
        );
        assert_eq!(doc.text("h1").as_deref(), Some("Title"));
        assert_eq!(doc.text("p").as_deref(), Some("Hello world"));
        assert_eq!(doc.count("p"), 1);
        assert_eq!(doc.meta_names(), Vec::<String>::new());
    }

    #[test]
    fn attr_extraction() {
        let doc = Doc::parse_document(r#"<meta name="og:title" content="X"><img src="a.png">"#);
        assert_eq!(
            doc.attr("meta[name=\"og:title\"]", "content").as_deref(),
            Some("X")
        );
        assert_eq!(doc.attr("img", "src").as_deref(), Some("a.png"));
    }

    #[test]
    fn remove_selector_removes_subtree() {
        let mut doc = Doc::parse_document(
            "<html><body><script>bad()</script><p>good <b>text</b></p><form><input></form></body></html>",
        );
        assert_eq!(doc.remove_selector("script, form"), 2);
        let html = doc.serialize();
        assert!(!html.contains("bad()"));
        assert!(html.contains("good"));
        assert!(!html.contains("<form"));
    }

    #[test]
    fn remove_comments() {
        let mut doc = Doc::parse_document("<!-- c1 --><p>a<!-- c2 --></p>");
        doc.remove_comments();
        assert!(!doc.serialize().contains("c1"));
        assert!(!doc.serialize().contains("c2"));
    }

    #[test]
    fn convert_node_to_renames() {
        let mut doc = Doc::parse_document("<h1>Hello</h1>");
        let id = doc.select_ids("h1")[0];
        doc.convert_node_to(id, "h2");
        assert!(doc.serialize().contains("<h2>Hello</h2>"));
    }

    #[test]
    fn set_attr_and_remove_attr() {
        let mut doc = Doc::parse_document("<img src=\"placeholder.png\">");
        let id = doc.select_ids("img")[0];
        doc.set_attr(id, "src", "real.jpg");
        assert!(doc.serialize().contains("real.jpg"));
        assert_eq!(doc.attr_of(id, "src").as_deref(), Some("real.jpg"));
        doc.remove_attr(id, "src");
        assert!(!doc.serialize().contains("placeholder"));
        assert_eq!(doc.attr_of(id, "src"), None);
    }

    #[test]
    fn replace_inner_html_grafts_fragment() {
        let mut doc = Doc::parse_document("<div id=\"d\"><span>old</span></div>");
        let id = doc.select_ids("#d")[0];
        doc.replace_inner_html(id, "<iframe src=\"x\"></iframe><div><span>new</span></div>");
        let html = doc.serialize();
        assert!(html.contains("<iframe src=\"x\"></iframe>"));
        assert!(html.contains("<span>new</span>"));
        assert!(!html.contains("old"));
    }

    #[test]
    fn wrap_moves_node_under_wrapper() {
        let mut doc = Doc::parse_document("<p class=\"c\">Hello</p>");
        let id = doc.select_ids("p")[0];
        let wrapper = doc.wrap(id, "div").expect("wrapper id");
        let html = doc.serialize();
        assert!(html.contains("<div><p class=\"c\">Hello</p></div>"), "got: {html}");
        assert_eq!(doc.element_name_of(wrapper).as_deref(), Some("div"));
    }

    #[test]
    fn prepend_and_append_child() {
        let mut doc = Doc::parse_document("<div id=\"d\"></div>");
        let id = doc.select_ids("#d")[0];
        doc.prepend_child(id, "<span>first</span>");
        doc.append_child(id, "<span>last</span>");
        let html = doc.serialize();
        assert!(html.contains("<span>first</span><span>last</span>"), "got: {html}");
    }

    #[test]
    fn insert_before_after() {
        let mut doc = Doc::parse_document("<p id=\"x\">X</p>");
        let id = doc.select_ids("#x")[0];
        doc.insert_before(id, "<i>before</i>");
        doc.insert_after(id, "<i>after</i>");
        let html = doc.serialize();
        assert!(
            html.contains("<i>before</i><p id=\"x\">X</p><i>after</i>"),
            "got: {html}"
        );
    }

    #[test]
    fn matches_selector() {
        let doc = Doc::parse_document("<div class=\"a\"><p>t</p></div>");
        let p = doc.select_ids("p")[0];
        let div = doc.select_ids("div")[0];
        assert!(doc.matches(p, "p"));
        assert!(doc.matches(div, "div.a"));
        assert!(!doc.matches(p, "div"));
    }

    #[test]
    fn get_attrs_and_html_of() {
        let doc = Doc::parse_document("<img src=\"a.png\" data-x=\"1\"><p>text</p>");
        let el = doc.select_first("img").unwrap();
        let attrs = get_attrs(el);
        assert!(attrs.contains(&("src".to_string(), "a.png".to_string())));
        assert!(attrs.contains(&("data-x".to_string(), "1".to_string())));
        let p = doc.select_ids("p")[0];
        assert_eq!(doc.html_of(p).as_deref(), Some("<p>text</p>"));
        assert_eq!(doc.inner_html_of(p).as_deref(), Some("text"));
    }

    #[test]
    fn link_density_counts_a_text() {
        let doc = Doc::parse_document("<div><a>link</a> plain text</div>");
        let div = doc.select_first("div").unwrap();
        let node = doc.html.tree.get(div.id()).unwrap();
        let d = link_density(node);
        assert!(d > 0.2 && d < 0.4, "density = {d}");
    }
}
