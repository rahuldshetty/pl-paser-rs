//! The public parser entry point, ported from upstream `src/mercury.js`.

use url::Url;

use crate::dom::Doc;
use crate::extractors::generic;
use crate::extractors::root::{run_extraction, select_extended_types, ExtractContext};
use crate::resource::create_doc;
use crate::types::{Article, CustomExtractor, ParseOptions, ParserError};
use crate::utils::{parse_and_validate_url, remove_anchor};

/// The parser. Mirrors upstream `Parser.parse(url, opts)`.
pub struct Parser;

/// Maximum pages fetched when following `next_page_url` chains (upstream
/// hard-caps at 26).
const MAX_PAGES: u32 = 26;

impl Parser {
    /// Fetch `url` and extract the article.
    pub async fn parse(url: &str, opts: &ParseOptions) -> Result<Article, ParserError> {
        let parsed_url = parse_and_validate_url(url)?;
        let doc = create_doc(&parsed_url, opts.html.clone(), &opts.headers).await?;
        Self::extract(&doc, url, &parsed_url, opts).await
    }

    /// Parse pre-fetched HTML (upstream `parse(url, { html })`). Async because
    /// `fetch_all_pages` may still follow `next_page_url` chains over the
    /// network, exactly like upstream.
    pub async fn parse_html(
        url: &str,
        html: &str,
        opts: &ParseOptions,
    ) -> Result<Article, ParserError> {
        let parsed_url = parse_and_validate_url(url)?;
        let doc = crate::resource::generate_doc(html.as_bytes().to_vec(), "text/html", true)?;
        Self::extract(&doc, url, &parsed_url, opts).await
    }

    async fn extract(
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

        // Extractor selection (upstream `getExtractor`): a caller-supplied
        // custom extractor is registered like upstream's `addCustomExtractor`,
        // then runtime + built-in extractors are matched by hostname, with an
        // HTML-based detector as the last resort before the generic path.
        let extractor = {
            if let Some(ex) = &opts.custom_extractor {
                let _ = crate::extractors::custom::add_extractor(ex.clone());
            }
            let host = parsed_url.host_str().unwrap_or("");
            crate::extractors::custom::get_extractor(host)
                .or_else(|| crate::extractors::custom::detect_by_html(doc))
        };

        let mut article = run_extraction(&ctx, extractor.as_ref());

        // Merge caller-supplied `extend` fields.
        let extended = select_extended_types(&ctx, &opts.extend);
        for (key, value) in extended {
            article.extend.entry(key).or_insert(value);
        }

        // Follow next-page chains when requested (upstream `collectAllPages`).
        if opts.fetch_all_pages {
            if let Some(next_page_url) = article.next_page_url.clone() {
                article =
                    Self::collect_all_pages(url, next_page_url, extractor.as_ref(), opts, article)
                        .await?;
            } else {
                article.total_pages = Some(1);
                article.rendered_pages = Some(1);
            }
        } else {
            article.total_pages = Some(1);
            article.rendered_pages = Some(1);
        }

        // Content type conversion (html -> markdown/text).
        if let Some(content) = article.content.take() {
            article.content = Some(crate::content_type::convert(&content, opts.content_type));
        }

        Ok(article)
    }

    /// Fetch the remaining pages of a multi-page article and merge their
    /// content (upstream `collectAllPages`).
    async fn collect_all_pages(
        url: &str,
        mut next_page_url: String,
        extractor: Option<&CustomExtractor>,
        opts: &ParseOptions,
        mut article: Article,
    ) -> Result<Article, ParserError> {
        let mut pages = 1u32;
        let mut previous_urls = vec![remove_anchor(url)];

        while !next_page_url.is_empty() && pages < MAX_PAGES {
            pages += 1;

            let parsed = parse_and_validate_url(&next_page_url)?;
            let doc = create_doc(&parsed, None, &opts.headers).await?;
            let html = doc.serialize();
            let meta_cache = doc.meta_names();

            let page_ctx = ExtractContext {
                doc: &doc,
                url: &next_page_url,
                html: &html,
                meta_cache,
                parsed_url: &parsed,
                fallback: opts.fallback,
                content_type: opts.content_type,
                previous_urls: previous_urls.clone(),
            };
            let page_result = run_extraction(&page_ctx, extractor);
            previous_urls.push(next_page_url.clone());

            if let Some(page_content) = page_result.content {
                let merged = format!(
                    "{}<hr><h4>Page {pages}</h4>{page_content}",
                    article.content.as_deref().unwrap_or("")
                );
                article.content = Some(merged);
            }

            next_page_url = page_result.next_page_url.unwrap_or_default();
        }

        // Recompute the word count over the merged content.
        article.word_count = generic::extract_word_count(article.content.as_deref());
        article.total_pages = Some(pages);
        article.rendered_pages = Some(pages);

        Ok(article)
    }
}
