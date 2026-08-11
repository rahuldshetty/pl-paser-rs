//! Optional readability-based content fallback.
//!
//! Upstream `@postlight/parser` has no readability integration (its
//! `fallback` option means "fall back to the generic extractor"); this is an
//! optional extension gated behind the `fallback` cargo feature: when the
//! Mercury-style content extractor produces nothing, try the Rust
//! `readability` crate as a last resort.

/// Extract content with the `readability` crate (feature-gated).
pub fn readability_content(html: &str, url: &str) -> Option<String> {
    let mut input = html.as_bytes();
    let parsed = url::Url::parse(url).ok()?;
    let product = readability::extractor::extract(&mut input, &parsed).ok()?;
    if product.content.is_empty() {
        None
    } else {
        Some(product.content)
    }
}
