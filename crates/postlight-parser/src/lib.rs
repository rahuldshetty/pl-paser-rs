//! postlight-parser — a Rust port of the Postlight Mercury Parser.
//!
//! Fetches a web page (or parses provided HTML) and extracts the article:
//! `title`, `content`, `author`, `date_published`, `lead_image_url`, `dek`,
//! `next_page_url`, `url`, `domain`, `excerpt`, `word_count`, `direction`,
//! `total_pages`, and `rendered_pages`.
//!
//! The extraction pipeline mirrors upstream `@postlight/parser` v2.2.3:
//!
//! 1. **resource** — URL validation, HTTP fetch (redirects, mobile-UA retry,
//!    charset decoding), lazy-loaded attribute lifting.
//! 2. **extractor selection** — a per-domain custom extractor ("connector")
//!    when one is registered, otherwise the generic extractor.
//! 3. **root-extractor** — runs the field extractors (`title`, `author`,
//!    `date_published`, …) and the content cleaner/scorer.
//! 4. **collect-all-pages** — follows `next_page_url` chains when requested.
//! 5. **content type conversion** — `html`, `markdown`, or `text`.
//!
//! Async (tokio-based) so it drops straight into a Tauri command.

pub mod cleaners;
pub mod dom;
pub mod dom_utils;
pub mod extractors;
pub mod parser;
pub mod resource;
pub mod types;
pub mod utils;

// Public API re-exports.
pub use crate::parser::Parser;
pub use crate::types::{Article, ContentType, ParseOptions, ParserError};
