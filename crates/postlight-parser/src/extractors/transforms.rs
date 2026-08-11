//! Named content transforms for custom extractors (upstream transform
//! *functions*). Each site-specific transform is registered under a
//! `domain.selector` key and implemented in Rust.
//!
//! The full set of ~43 upstream transform functions is ported in the custom
//! extractor phase (`extractors/transforms/sites.rs`); until then, unknown
//! names are a no-op.

use ego_tree::NodeId;

use crate::dom::Doc;

/// Apply a named transform to the element at `id`.
///
/// Returns the tag name to convert the element into, if any
/// (a transform may also mutate the DOM directly).
pub fn apply_named(doc: &mut Doc, id: NodeId, name: &str) -> Option<String> {
    sites::apply(doc, id, name)
}

mod sites {
    use super::*;

    pub fn apply(doc: &mut Doc, id: NodeId, _name: &str) -> Option<String> {
        let _ = (doc, id);
        // Site transforms are registered here in the custom extractor phase.
        None
    }
}
