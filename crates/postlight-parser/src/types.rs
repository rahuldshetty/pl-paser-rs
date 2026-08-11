//! Core types for the parser: output `Article`, `ParseOptions`, errors, and
//! the custom-extractor data model.
//!
//! Field names and JSON shape mirror upstream `@postlight/parser` v2.2.3.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Output format for the `content` field (upstream `contentType` option).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    #[default]
    Html,
    Markdown,
    Text,
}

impl ContentType {
    /// The option string used by the upstream API and CLI.
    pub fn as_str(self) -> &'static str {
        match self {
            ContentType::Html => "html",
            ContentType::Markdown => "markdown",
            ContentType::Text => "text",
        }
    }

    /// Parse an option string; upstream accepts `html`, `markdown`, `text`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "html" => Some(ContentType::Html),
            "markdown" => Some(ContentType::Markdown),
            "text" => Some(ContentType::Text),
            _ => None,
        }
    }
}

/// The extracted article. Every field mirrors the upstream JSON output; a
/// field the parser could not find is `None` (serialized as `null`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Article {
    pub title: Option<String>,
    pub content: Option<String>,
    pub author: Option<String>,
    pub date_published: Option<String>,
    pub lead_image_url: Option<String>,
    pub dek: Option<String>,
    pub next_page_url: Option<String>,
    pub url: Option<String>,
    pub domain: Option<String>,
    pub excerpt: Option<String>,
    pub word_count: Option<u32>,
    pub direction: Option<String>,
    pub total_pages: Option<u32>,
    pub rendered_pages: Option<u32>,
    /// Extra keys merged into the output via `extend`.
    #[serde(flatten)]
    pub extend: std::collections::HashMap<String, serde_json::Value>,
}

/// Options accepted by `Parser::parse`, mirroring upstream `parse(url, opts)`.
#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// Pre-fetched HTML to parse instead of fetching `url`.
    pub html: Option<String>,
    /// Follow `next_page_url` chains and merge content (upstream default: `true`).
    pub fetch_all_pages: bool,
    /// Fall back to the generic extractor (and readability) when a custom
    /// extractor selector misses (upstream default: `true`).
    pub fallback: bool,
    /// Output format for `content` (upstream default: `html`).
    pub content_type: ContentType,
    /// Extra request headers as `(name, value)` pairs.
    pub headers: Vec<(String, String)>,
    /// Extra fields to add to the result: name -> selector mapping.
    pub extend: HashMap<String, ExtendField>,
    /// A custom extractor registered at runtime (upstream `addExtractor`).
    pub custom_extractor: Option<CustomExtractor>,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            html: None,
            fetch_all_pages: true,
            fallback: true,
            content_type: ContentType::Html,
            headers: Vec::new(),
            extend: HashMap::new(),
            custom_extractor: None,
        }
    }
}

/// A custom field defined through `extend` (site or caller supplied).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtendField {
    pub selectors: Vec<String>,
    /// When true, collect *all* matching nodes and always return an array.
    pub allow_multiple: bool,
}

/// Errors produced by the parser. Messages match upstream so API/CLI output
/// stays compatible.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ParserError {
    #[error(
        "The url parameter passed does not look like a valid URL. Please check your URL and try again."
    )]
    InvalidUrl,
    #[error("Unable to fetch content. Original exception was {0}")]
    Fetch(String),
    #[error("Resource returned a response status code of {0} and resource was instructed to reject non-200 status codes.")]
    Non200(u16),
    #[error("Content-type for this resource was {0} and is not allowed.")]
    BadContentType(String),
    #[error("Content for this resource was too large. Maximum content length is 5242880.")]
    ContentTooLarge,
    #[error("Content does not appear to be text.")]
    NotText,
    #[error("No children, likely a bad parse.")]
    NoChildren,
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("Unsupported content type: {0}")]
    UnknownContentType(String),
}

impl ParserError {
    /// The `{ "error": true, "message": ... }` payload upstream returns on
    /// failure paths.
    pub fn to_error_json(&self) -> serde_json::Value {
        serde_json::json!({ "error": true, "message": self.to_string() })
    }
}

// ---------------------------------------------------------------------------
// Custom extractor ("connector") data model
//
// Mirrors the schema documented in upstream
// `src/extractors/custom/README.md` and used by `src/extractors/root-extractor.js`.
// ---------------------------------------------------------------------------

/// A per-site custom extractor.
#[derive(Debug, Clone, Default)]
pub struct CustomExtractor {
    /// Canonical domain, e.g. `www.nytimes.com`.
    pub domain: String,
    /// Additional domains served by the same extractor.
    pub supported_domains: Vec<String>,
    pub title: Option<Field>,
    /// Author may be selector-driven or a hardcoded string
    /// (e.g. Wikipedia).
    pub author: Option<FieldValue>,
    pub date_published: Option<DateField>,
    pub lead_image_url: Option<Field>,
    pub dek: Option<Field>,
    pub excerpt: Option<Field>,
    pub next_page_url: Option<Field>,
    pub content: Option<ContentField>,
    /// Extra keys merged into the result.
    pub extend: HashMap<String, ExtendField>,
}

/// A field value: selector-driven extraction or a hardcoded string.
#[derive(Debug, Clone)]
pub enum FieldValue {
    Selectors(Field),
    Value(String),
}

impl FieldValue {
    pub fn field(&self) -> Option<&Field> {
        match self {
            FieldValue::Selectors(f) => Some(f),
            FieldValue::Value(_) => None,
        }
    }
}

/// Ordered selector list for one field. The extractor stops at the first
/// matching selector (upstream `findMatchingSelector`).
#[derive(Debug, Clone, Default)]
pub struct Field {
    pub selectors: Vec<Selector>,
}

/// `date_published` extraction, plus optional `format` (moment-style) and
/// `timezone` used to parse the extracted value.
#[derive(Debug, Clone, Default)]
pub struct DateField {
    pub selectors: Vec<Selector>,
    pub format: Option<String>,
    pub timezone: Option<String>,
}

/// Content extraction: ordered selectors, `clean` selectors, `transforms`,
/// and whether the default cleaner runs afterwards (`defaultCleaner`).
#[derive(Debug, Clone, Default)]
pub struct ContentField {
    pub selectors: Vec<Selector>,
    /// Selectors of nodes to remove from the extracted content.
    pub clean: Vec<String>,
    /// Ordered `(selector, transform)` pairs applied to matching nodes.
    pub transforms: Vec<(String, Transform)>,
    /// Run the default content cleaner after extraction (default: `true`).
    pub default_cleaner: bool,
}

/// A single entry in a field's selector list.
#[derive(Debug, Clone)]
pub enum Selector {
    /// Plain CSS selector; extracted value is the trimmed text.
    Css(String),
    /// `(selector, attr)`: extract the trimmed attribute value.
    Attr {
        selector: String,
        attr: String,
        /// Optional named transform applied to the extracted attribute value
        /// (upstream third element of the pair, PR #430).
        transform: Option<String>,
    },
    /// Multiple selectors that must *all* match; all matched nodes are
    /// included (content multi-match selection).
    Multi(Vec<String>),
}

/// Content transform: convert matching nodes to another tag, or run a named
/// site-specific transformation implemented in Rust.
#[derive(Debug, Clone)]
pub enum Transform {
    ToTag(String),
    Named(String),
}
