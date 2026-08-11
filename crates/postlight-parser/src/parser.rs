//! The public parser entry point, ported from upstream `src/mercury.js`.

use url::Url;

use crate::dom::Doc;
use crate::extractors::root::{run_extraction, select_extended_types, ExtractContext};
use crate::resource::create_doc;
use crate::types::{Article, ParseOptions, ParserError};
use crate::utils::parse_and_validate_url;

/// The parser. Mirrors upstream `Parser.parse(url, opts)`.
pub struct Parser;

impl Parser {
    /// Fetch `url` and extract the article.
    pub async fn parse(url: &str, opts: &ParseOptions) -> Result<Article, ParserError> {
        let parsed_url = parse_and_validate_url(url)?;
        let doc = create_doc(&parsed_url, opts.html.clone(), &opts.headers).await?;
        Self::extract(&doc, url, &parsed_url, opts)
    }

    /// Parse pre-fetched HTML (upstream `parse(url, { html })`).
    pub fn parse_html(url: &str, html: &str, opts: &ParseOptions) -> Result<Article, ParserError> {
        let parsed_url = parse_and_validate_url(url)?;
        let doc = crate::resource::generate_doc(html.as_bytes().to_vec(), "text/html", true)?;
        Self::extract(&doc, url, &parsed_url, opts)
    }

    fn extract(
        doc: &Doc,
        url: &str,
        parsed_url: &Url,
        opts: &ParseOptions,
    ) -> Result<Article, ParserError> {
        let html = doc.serialize();
        let meta_cache = doc.meta_names();

        let ctx = ExtractContext {
            doc,
            url,
            html: &html,
            meta_cache,
            parsed_url,
            fallback: opts.fallback,
            content_type: opts.content_type,
            previous_urls: vec![url.to_string()],
        };

        let extractor = opts.custom_extractor.as_ref();
        let mut article = run_extraction(&ctx, extractor);

        // Merge caller-supplied `extend` fields.
        let extended = select_extended_types(&ctx, &opts.extend);
        for (key, value) in extended {
            article.extend.entry(key).or_insert(value);
        }

        // Pagination metadata (multi-page collection lands with
        // `collectAllPages` in a later phase).
        article.total_pages = Some(1);
        article.rendered_pages = Some(1);

        // Content type conversion (markdown/text) lands with the content
        // type module.
        Ok(article)
    }
}
