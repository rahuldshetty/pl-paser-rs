//! Custom extractor registry: built-in extractors (ported from upstream
//! `src/extractors/custom/`) plus runtime registration, ported from upstream
//! `get-extractor.js` / `add-extractor.js` / `all.js`.

use once_cell::sync::Lazy;
use std::sync::RwLock;

use crate::dom::Doc;
use crate::extractors::custom_data::{all_extractors, extra_extractors};
use crate::types::CustomExtractor;
use crate::utils::base_domain;

/// Built-in extractors, keyed by domain (upstream `Extractors`).
static BUILTIN: Lazy<Vec<CustomExtractor>> = Lazy::new(|| {
    let mut all = all_extractors();
    all.extend(extra_extractors());
    all
});

/// Extractors added at runtime (upstream `apiExtractors`).
static RUNTIME: Lazy<RwLock<Vec<CustomExtractor>>> = Lazy::new(|| RwLock::new(Vec::new()));

/// Find the extractor for a hostname (upstream `getExtractor`): runtime
/// extractors first, then built-ins, matched by full hostname or base domain.
pub fn get_extractor(host: &str) -> Option<CustomExtractor> {
    let base = base_domain(host);

    if let Ok(registry) = RUNTIME.read() {
        if let Some(ex) = find_in(&registry, host, &base) {
            return Some(ex.clone());
        }
    }

    find_in(&BUILTIN, host, &base).cloned()
}

fn find_in<'a>(
    extractors: &'a [CustomExtractor],
    host: &str,
    base: &str,
) -> Option<&'a CustomExtractor> {
    extractors
        .iter()
        .find(|e| e.domain == host || e.supported_domains.iter().any(|d| d == host))
        .or_else(|| {
            extractors
                .iter()
                .find(|e| e.domain == base || e.supported_domains.iter().any(|d| d == base))
        })
}

/// Detect an extractor from page HTML (upstream `detectByHtml`).
pub fn detect_by_html(doc: &Doc) -> Option<CustomExtractor> {
    if !doc
        .select("meta[name=\"al:ios:app_name\"][value=\"Medium\"]")
        .is_empty()
    {
        return get_extractor("medium.com");
    }
    if !doc
        .select("meta[name=\"generator\"][value=\"blogger\"]")
        .is_empty()
    {
        return get_extractor("blogspot.com");
    }
    None
}

/// Register a custom extractor at runtime (upstream `addExtractor`).
pub fn add_extractor(extractor: CustomExtractor) -> Result<(), String> {
    if extractor.domain.is_empty() {
        return Err("Unable to add custom extractor. Invalid parameters.".to_string());
    }
    if let Ok(mut registry) = RUNTIME.write() {
        registry.push(extractor);
    }
    Ok(())
}

/// True if a custom extractor is registered for the host.
pub fn has_extractor(host: &str) -> bool {
    get_extractor(host).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_by_hostname_and_base_domain() {
        let nytimes = get_extractor("www.nytimes.com").expect("nytimes");
        assert_eq!(nytimes.domain, "www.nytimes.com");
        // subdomain falls back to the base domain extractor
        let medium = get_extractor("blog.medium.com").expect("medium via base domain");
        assert_eq!(medium.domain, "medium.com");
    }

    #[test]
    fn supported_domains_are_aliases() {
        // theverge.com supports www.polygon.com
        let polygon = get_extractor("www.polygon.com").expect("polygon");
        assert_eq!(polygon.domain, "www.theverge.com");
    }

    #[test]
    fn runtime_extractor_takes_priority() {
        let ex = CustomExtractor {
            domain: "example.org".to_string(),
            ..CustomExtractor::default()
        };
        add_extractor(ex).unwrap();
        let found = get_extractor("example.org").expect("runtime extractor");
        assert_eq!(found.domain, "example.org");
    }

    #[test]
    fn add_extractor_validates() {
        assert!(add_extractor(CustomExtractor::default()).is_err());
    }

    #[test]
    fn detects_by_html() {
        let doc = Doc::parse_document(
            r#"<html><head><meta name="generator" value="blogger"></head><body></body></html>"#,
        );
        let ex = detect_by_html(&doc).expect("blogspot");
        assert_eq!(ex.domain, "blogspot.com");
    }
}
