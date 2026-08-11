//! Root extractor: orchestrates per-field extraction for custom extractors
//! with fallback to the generic extractor, ported from upstream
//! `src/extractors/root-extractor.js`.

use std::collections::HashMap;

use ego_tree::NodeId;
use url::Url;

use crate::cleaners::{clean_author, clean_date_published, clean_dek, clean_lead_image_url, clean_title};
use crate::dom::{append_cloned, Doc};
use crate::dom_utils;
use crate::extractors::content::{clean_content, ContentOptions};
use crate::extractors::generic;
use crate::extractors::transforms;
use crate::types::{
    Article, ContentType, ContentField, CustomExtractor, DateField, ExtendField, Field, FieldValue,
    Selector, Transform,
};

/// Everything the extraction pipeline needs, mirroring upstream `opts`.
pub struct ExtractContext<'a> {
    pub doc: &'a Doc,
    pub url: &'a str,
    pub html: &'a str,
    pub meta_cache: Vec<String>,
    pub parsed_url: &'a Url,
    pub fallback: bool,
    pub content_type: ContentType,
    /// URLs already fetched (for next-page chaining).
    pub previous_urls: Vec<String>,
}

/// The extraction options for one field of a custom extractor.
pub enum FieldOpts {
    Value(String),
    Field(Field),
    Date(DateField),
    Content(ContentField),
}

/// The result of a `select` call: a single value or an array.
#[derive(Debug, Clone)]
pub enum SelectResult {
    Value(String),
    Multiple(Vec<String>),
}

impl SelectResult {
    pub fn into_value(self) -> Option<String> {
        match self {
            SelectResult::Value(v) => Some(v),
            SelectResult::Multiple(m) => m.into_iter().next(),
        }
    }

    /// True when the result carries no usable value (upstream falsy check).
    pub fn is_empty(&self) -> bool {
        match self {
            SelectResult::Value(v) => v.is_empty(),
            SelectResult::Multiple(m) => m.is_empty() || m.iter().all(|v| v.is_empty()),
        }
    }
}

/// Run the full extraction (upstream `RootExtractor.extract`), with or
/// without a custom extractor.
pub fn run_extraction(ctx: &ExtractContext<'_>, extractor: Option<&CustomExtractor>) -> Article {
    match extractor {
        Some(ex) if ex.domain != "*" => extract_custom(ctx, ex),
        _ => extract_generic(ctx),
    }
}

/// Extract every field with the generic extractor (upstream
/// `GenericExtractor.extract`).
pub fn extract_generic(ctx: &ExtractContext<'_>) -> Article {
    let title = generic::extract_title(ctx.doc, ctx.url, &ctx.meta_cache);
    let content = generic::extract_content(ctx.doc, ctx.url);
    let lead_image_url = generic::extract_lead_image_url(ctx.doc, content.as_deref(), &ctx.meta_cache);
    let excerpt = generic::extract_excerpt(ctx.doc, content.as_deref(), &ctx.meta_cache);
    let word_count = generic::extract_word_count(content.as_deref());
    let direction = generic::extract_direction(&title);
    let (url, domain) = generic::extract_url_and_domain(ctx.doc, ctx.url, &ctx.meta_cache);

    Article {
        title: Some(title),
        content,
        author: generic::extract_author(ctx.doc, &ctx.meta_cache),
        date_published: generic::extract_date_published(ctx.doc, ctx.url, &ctx.meta_cache),
        lead_image_url,
        dek: generic::extract_dek(),
        next_page_url: generic::extract_next_page_url(
            ctx.doc,
            ctx.url,
            ctx.parsed_url,
            &ctx.previous_urls,
        ),
        url,
        domain,
        excerpt,
        word_count,
        direction,
        total_pages: None,
        rendered_pages: None,
        extend: std::collections::HashMap::new(),
    }
}

/// Extract every field with a custom extractor, falling back to generic per
/// field (upstream `RootExtractor.extract` for non-`*` domains).
fn extract_custom(ctx: &ExtractContext<'_>, extractor: &CustomExtractor) -> Article {
    let title = extract_result(ctx, extractor, "title", None)
        .into_value()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| generic::extract_title(ctx.doc, ctx.url, &ctx.meta_cache));
    let date_published = non_empty(extract_result(ctx, extractor, "date_published", None));
    let author = non_empty(extract_result(ctx, extractor, "author", None));
    let next_page_url = non_empty(extract_result(ctx, extractor, "next_page_url", None));
    let content = non_empty(extract_result(ctx, extractor, "content", Some(&title)));
    let lead_image_url = non_empty(extract_result(ctx, extractor, "lead_image_url", None));
    let excerpt = non_empty(extract_result(ctx, extractor, "excerpt", None));
    let dek = non_empty(extract_result(ctx, extractor, "dek", excerpt.as_deref()));

    let word_count = generic::extract_word_count(content.as_deref());
    let direction = generic::extract_direction(&title);
    let (url, domain) = generic::extract_url_and_domain(ctx.doc, ctx.url, &ctx.meta_cache);

    let mut article = Article {
        title: Some(title),
        content,
        author,
        date_published,
        lead_image_url,
        dek,
        next_page_url,
        url,
        domain,
        excerpt,
        word_count,
        direction,
        total_pages: None,
        rendered_pages: None,
        extend: std::collections::HashMap::new(),
    };

    // Merge the extractor's own `extend` fields (upstream `selectExtendedTypes`).
    let extended = select_extended_types(ctx, &extractor.extend);
    for (key, value) in extended {
        article.extend.entry(key).or_insert(value);
    }

    article
}

/// Map a select result to `None` when it carries no usable value.
fn non_empty(result: SelectResult) -> Option<String> {
    result.into_value().filter(|v| !v.is_empty())
}

/// The `extractResult` function: try the custom selector, fall back to the
/// generic extractor for the field.
fn extract_result(
    ctx: &ExtractContext<'_>,
    extractor: &CustomExtractor,
    field: &str,
    context_value: Option<&str>,
) -> SelectResult {
    let extraction_opts = field_opts(extractor, field);

    if let Some(result) = select_for_field(ctx, field, extraction_opts, context_value) {
        // Empty values are falsy in upstream JS and fall through.
        if !result.is_empty() {
            return result;
        }
    }

    if ctx.fallback {
        return match field {
            "title" => SelectResult::Value(generic::extract_title(ctx.doc, ctx.url, &ctx.meta_cache)),
            "content" => SelectResult::Value(
                generic::extract_content(ctx.doc, ctx.url).unwrap_or_default(),
            ),
            "author" => SelectResult::Value(
                generic::extract_author(ctx.doc, &ctx.meta_cache).unwrap_or_default(),
            ),
            "date_published" => SelectResult::Value(
                generic::extract_date_published(ctx.doc, ctx.url, &ctx.meta_cache)
                    .unwrap_or_default(),
            ),
            "dek" => SelectResult::Value(generic::extract_dek().unwrap_or_default()),
            "lead_image_url" => SelectResult::Value(
                generic::extract_lead_image_url(ctx.doc, None, &ctx.meta_cache).unwrap_or_default(),
            ),
            "excerpt" => SelectResult::Value(
                generic::extract_excerpt(ctx.doc, None, &ctx.meta_cache).unwrap_or_default(),
            ),
            "next_page_url" => SelectResult::Value(
                generic::extract_next_page_url(ctx.doc, ctx.url, ctx.parsed_url, &ctx.previous_urls)
                    .unwrap_or_default(),
            ),
            _ => SelectResult::Value(String::new()),
        };
    }

    SelectResult::Value(String::new())
}

fn field_opts(extractor: &CustomExtractor, field: &str) -> Option<FieldOpts> {
    match field {
        "title" => extractor.title.as_ref().map(|f| FieldOpts::Field(f.clone())),
        "author" => match &extractor.author {
            Some(FieldValue::Value(v)) => Some(FieldOpts::Value(v.clone())),
            Some(FieldValue::Selectors(f)) => Some(FieldOpts::Field(f.clone())),
            None => None,
        },
        "date_published" => extractor
            .date_published
            .as_ref()
            .map(|f| FieldOpts::Date(f.clone())),
        "lead_image_url" => extractor
            .lead_image_url
            .as_ref()
            .map(|f| FieldOpts::Field(f.clone())),
        "dek" => extractor.dek.as_ref().map(|f| FieldOpts::Field(f.clone())),
        "excerpt" => extractor.excerpt.as_ref().map(|f| FieldOpts::Field(f.clone())),
        "next_page_url" => extractor
            .next_page_url
            .as_ref()
            .map(|f| FieldOpts::Field(f.clone())),
        "content" => extractor.content.as_ref().map(|f| FieldOpts::Content(f.clone())),
        _ => None,
    }
}

fn select_for_field(
    ctx: &ExtractContext<'_>,
    field: &str,
    extraction_opts: Option<FieldOpts>,
    context_value: Option<&str>,
) -> Option<SelectResult> {
    let extraction_opts = extraction_opts?;

    // A hardcoded string (e.g. Wikipedia contributors).
    if let FieldOpts::Value(v) = &extraction_opts {
        return Some(SelectResult::Value(v.clone()));
    }

    let override_allow_multiple = field == "lead_image_url";

    match extraction_opts {
        FieldOpts::Field(f) => {
            let matching = find_matching_selector(ctx.doc, &f.selectors, false, override_allow_multiple)?;
            select_text_or_attr(ctx, field, matching, override_allow_multiple, None, context_value)
        }
        FieldOpts::Date(d) => {
            let matching = find_matching_selector(ctx.doc, &d.selectors, false, override_allow_multiple)?;
            select_text_or_attr(ctx, field, matching, override_allow_multiple, Some(&d), context_value)
        }
        FieldOpts::Content(c) => {
            let matching = find_matching_selector(ctx.doc, &c.selectors, true, false)?;
            select_html(ctx, matching, context_value, &c)
        }
        FieldOpts::Value(_) => unreachable!(),
    }
}

/// Port of upstream `findMatchingSelector`.
fn find_matching_selector(
    doc: &Doc,
    selectors: &[Selector],
    extract_html: bool,
    allow_multiple: bool,
) -> Option<Selector> {
    for selector in selectors {
        match selector {
            Selector::Multi(inner) => {
                if extract_html && inner.iter().all(|s| doc.count(s) > 0) {
                    return Some(selector.clone());
                }
            }
            Selector::Attr { selector: sel, attr, .. } => {
                let count = doc.count(sel);
                if allow_multiple || count == 1 {
                    if let Some(v) = doc.attr(sel, attr) {
                        if !v.trim().is_empty() {
                            return Some(selector.clone());
                        }
                    }
                }
            }
            Selector::Css(css) => {
                let count = doc.count(css);
                if allow_multiple || count == 1 {
                    if let Some(t) = doc.text(css) {
                        if !t.trim().is_empty() {
                            return Some(selector.clone());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Port of upstream `selectHtml` (content extraction with cleaning).
fn select_html(
    ctx: &ExtractContext<'_>,
    matching: Selector,
    context_value: Option<&str>,
    content_field: &ContentField,
) -> Option<SelectResult> {
    let mut content = Doc::new_fragment();
    let article_id: NodeId = match matching {
        Selector::Multi(selectors) => {
            // All must match; collect every match into a wrapper div.
            let mut wrapper = Doc::parse_fragment("<div></div>");
            let wrapper_id = wrapper.select_ids("div")[0];
            for s in selectors {
                for id in ctx.doc.select_ids(&s) {
                    if let Some(node) = ctx.doc.get(id) {
                        if let Some(mut w) = wrapper.html.tree.get_mut(wrapper_id) {
                            append_cloned(&mut w, &node);
                        }
                    }
                }
            }
            let wrapper_id = wrapper.html.tree.root().last_child()?.id();
            let wrapper_node = wrapper.html.tree.get(wrapper_id)?;
            let mut content_root = content.html.tree.root_mut();
            append_cloned(&mut content_root, &wrapper_node);
            content_root.last_child()?.id()
        }
        Selector::Css(css) => {
            let id = ctx.doc.select_ids(&css).into_iter().next()?;
            let node = ctx.doc.get(id)?;
            let mut root = content.html.tree.root_mut();
            append_cloned(&mut root, &node);
            root.last_child()?.id()
        }
        Selector::Attr { .. } => return None,
    };

    // Transform and clean (upstream `transformAndClean`).
    dom_utils::make_links_absolute(&mut content, ctx.url);

    for clean_selector in &content_field.clean {
        content.remove_selector(clean_selector);
    }
    for (selector, transform) in &content_field.transforms {
        let ids = content.select_ids(selector);
        for id in ids {
            match transform {
                Transform::ToTag(tag) => content.convert_node_to(id, tag),
                Transform::Named(name) => {
                    if let Some(tag) = transforms::apply_named(&mut content, ctx.doc, id, name) {
                        content.convert_node_to(id, &tag);
                    }
                }
            }
        }
    }

    clean_content(
        &mut content,
        article_id,
        context_value.unwrap_or(""),
        ctx.url,
        content_field.default_cleaner,
    );

    let html = content.serialize();
    if html.is_empty() {
        return None;
    }
    Some(SelectResult::Value(html))
}

/// Port of upstream `select` for non-HTML fields (text or attribute).
#[allow(clippy::too_many_arguments)]
fn select_text_or_attr(
    ctx: &ExtractContext<'_>,
    field: &str,
    matching: Selector,
    allow_multiple: bool,
    date: Option<&DateField>,
    context_value: Option<&str>,
) -> Option<SelectResult> {
    let values: Vec<String> = match &matching {
        Selector::Attr { selector, attr, .. } => ctx.doc.attr_all(selector, attr),
        Selector::Css(css) => ctx.doc.text_all(css),
        _ => return None,
    };

    // Apply the field cleaner, if there is one.
    let clean_one = |v: String| match field {
        "title" => clean_title(&v, ctx.url, ctx.doc),
        "author" => clean_author(&v),
        "date_published" => {
            let timezone = date.and_then(|d| d.timezone.as_deref());
            let format = date.and_then(|d| d.format.as_deref());
            clean_date_published(&v, timezone, format).unwrap_or_default()
        }
        "dek" => clean_dek(&v, context_value).unwrap_or_default(),
        "lead_image_url" => clean_lead_image_url(&v).unwrap_or_default(),
        _ => v,
    };

    if allow_multiple {
        Some(SelectResult::Multiple(values.into_iter().map(clean_one).collect()))
    } else {
        let v = values.into_iter().next().unwrap_or_default();
        Some(SelectResult::Value(clean_one(v)))
    }
}

/// Select `extend` fields (upstream `selectExtendedTypes`).
pub fn select_extended_types(
    ctx: &ExtractContext<'_>,
    extend: &HashMap<String, ExtendField>,
) -> HashMap<String, serde_json::Value> {
    let mut results = HashMap::new();
    for (name, opts) in extend {
        let selectors: Vec<Selector> = opts
            .selectors
            .iter()
            .map(|s| Selector::Css(s.clone()))
            .collect();
        let field = Field { selectors };
        let matching = find_matching_selector(ctx.doc, &field.selectors, false, opts.allow_multiple);
        let value =
            matching.and_then(|m| select_text_or_attr(ctx, name, m, opts.allow_multiple, None, None));
        match value {
            Some(SelectResult::Value(v)) => {
                results.insert(name.clone(), serde_json::Value::String(v));
            }
            Some(SelectResult::Multiple(values)) => {
                results.insert(
                    name.clone(),
                    serde_json::Value::Array(
                        values.into_iter().map(serde_json::Value::String).collect(),
                    ),
                );
            }
            None => {
                results.insert(name.clone(), serde_json::Value::Null);
            }
        }
    }
    results
}

/// Content extraction used by the generic path.
pub fn generic_content(ctx: &ExtractContext<'_>) -> Option<String> {
    crate::extractors::content::extract_content_with(
        ctx.doc,
        "",
        ctx.url,
        ContentOptions::default(),
        true,
    )
}
