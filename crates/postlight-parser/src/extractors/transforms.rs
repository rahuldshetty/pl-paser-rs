//! Named content transforms for custom extractors (upstream transform
//! *functions*), ported from the upstream `src/extractors/custom/*/index.js`
//! files into `sites.rs`.

mod sites;

use ego_tree::NodeId;

use crate::dom::Doc;

/// Apply a named transform to the element at `id`.
///
/// `resource` is the full parsed document (some transforms read page-level
/// metadata); `doc` is the content document being cleaned. Returns the tag
/// name to convert the element into, if any.
pub fn apply_named(doc: &mut Doc, resource: &Doc, id: NodeId, name: &str) -> Option<String> {
    sites::apply(doc, resource, id, name)
}
