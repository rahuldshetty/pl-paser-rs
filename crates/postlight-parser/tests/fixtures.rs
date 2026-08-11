//! Fixture-driven end-to-end tests using upstream `fixtures/*.html` (MIT).
//!
//! These exercise the full generic extraction pipeline on real saved pages.

use postlight_parser::{ParseOptions, Parser};

fn run_parse(url: &str, html: &str, opts: &ParseOptions) -> postlight_parser::Article {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { Parser::parse_html(url, html, opts).await })
        .expect("parse ok")
}

fn parse_fixture(name: &str, url: &str) -> postlight_parser::Article {
    let html = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    // fetch_all_pages is disabled so fixture tests stay offline (pagination is
    // covered by the live-network ignored test in resource.rs).
    let opts = ParseOptions {
        fetch_all_pages: false,
        ..ParseOptions::default()
    };
    run_parse(url, &html, &opts)
}

fn parse_fixture_no_fallback(name: &str, url: &str) -> postlight_parser::Article {
    let html = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let opts = ParseOptions {
        fallback: false,
        ..ParseOptions::default()
    };
    run_parse(url, &html, &opts)
}

#[test]
fn nytimes_fixture_metadata() {
    // Expectations from upstream src/extractors/custom/www.nytimes.com/index.test.js
    let article = parse_fixture(
        "www.nytimes.com.html",
        "http://www.nytimes.com/2016/09/20/nyregion/ahmad-khan-rahami-is-arrested-in-manhattan-and-new-jersey-bombings.html",
    );
    assert_eq!(
        article.title.as_deref(),
        Some("Ahmad Khan Rahami Is Arrested in Manhattan and New Jersey Bombings")
    );
    assert_eq!(
        article.author.as_deref(),
        Some("Marc Santora, William K. Rashbaum, Al Baker and Adam Goldman")
    );
    assert_eq!(
        article.date_published.as_deref(),
        Some("2016-09-19T11:46:01.000Z")
    );
    assert!(
        article.lead_image_url.as_deref().unwrap_or("").starts_with(
            "https://static01.nyt.com/images/2016/09/20/nyregion/Manhunt/Manhunt-facebookJumbo"
        ),
        "lead image: {:?}",
        article.lead_image_url
    );

    let content = article.content.expect("content");
    // upstream: excerptContent($('*').first().text(), 13)
    let content_doc = postlight_parser::dom::Doc::parse_fragment(&content);
    let first_text = content_doc
        .select("*")
        .first()
        .map(|el| postlight_parser::dom::element_text(*el))
        .unwrap_or_default();
    let first13 = first_text
        .split_whitespace()
        .take(13)
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        first13,
        "The man who the police said sowed terror across two states, setting off"
    );
}

#[test]
fn nytimes_fixture_content_length() {
    let article = parse_fixture(
        "www.nytimes.com.html",
        "http://www.nytimes.com/2016/09/20/nyregion/ahmad-khan-rahami-is-arrested-in-manhattan-and-new-jersey-bombings.html",
    );
    let content = article.content.expect("content");
    assert!(content.len() > 1000, "content too short: {}", content.len());
    assert!(article.word_count.unwrap_or(0) > 100);
}

#[test]
fn arstechnica_fixture_extracts() {
    let article = parse_fixture(
        "arstechnica.com.html",
        "http://arstechnica.com/gadgets/2016/09/the-connected-renter-how-to-make-your-apartment-smarter/",
    );
    let title = article.title.expect("title");
    assert!(title.contains("connected renter"), "title: {title}");
    let content = article.content.expect("content");
    assert!(content.len() > 500, "content too short");
    assert!(article.word_count.unwrap_or(0) > 50);
}

#[test]
fn guardian_fixture_extracts() {
    let article = parse_fixture(
        "www.theguardian.com.html",
        "https://www.theguardian.com/us-news/2016/sep/06/standing-rock-protests-dakota-access-pipeline",
    );
    let title = article.title.expect("title");
    assert!(title.contains("Standing Rock"), "title: {title}");
    let content = article.content.expect("content");
    assert!(content.len() > 500, "content too short");
}

#[test]
fn vulture_fixture_generic_extracts() {
    // Upstream src/extractors/generic/content/extractor.test.js uses this
    // fixture with the generic extractor.
    let article = parse_fixture(
        "www.vulture.com.html",
        "http://www.vulture.com/2016/08/dc-comics-greg-berlanti-c-v-r.html",
    );
    let content = article.content.expect("content");
    assert!(!content.is_empty());
    assert!(article.word_count.unwrap_or(0) > 20);
}

#[test]
fn theverge_fixture_custom_extractor() {
    // Expectations from upstream src/extractors/custom/www.theverge.com/index.test.js
    // (which parses with fallback: false).
    let article = parse_fixture_no_fallback(
        "www.theverge.com.html",
        "http://www.theverge.com/2016/11/29/13774648/fcc-att-zero-rating-directv-net-neutrality-vs-tmobile",
    );
    assert_eq!(
        article.title.as_deref(),
        Some("AT&T just declared war on an open internet (and us)")
    );
    assert_eq!(article.author.as_deref(), Some("T.C. Sottek"));
    assert_eq!(
        article.date_published.as_deref(),
        Some("2016-11-29T15:00:19.000Z")
    );
    assert_eq!(
        article.dek.as_deref(),
        Some("‘Mobilizing Your World’ sounds like a threat now")
    );
    let content = article.content.expect("content");
    assert!(content.len() > 500, "content too short: {}", content.len());
}

#[test]
fn fortune_fixture_extracts() {
    // NOTE: the fortune.com custom extractor targets the current page layout;
    // this 2016 fixture predates it, so only assert the parse succeeds.
    let article = parse_fixture(
        "fortune.com.html",
        "http://fortune.com/2016/09/19/ios-10-iphone-7-review/",
    );
    assert!(article.title.is_some() || article.content.is_some());
}
