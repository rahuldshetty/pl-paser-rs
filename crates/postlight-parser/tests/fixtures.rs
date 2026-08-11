//! Fixture-driven end-to-end tests using upstream `fixtures/*.html` (MIT).
//!
//! These exercise the full generic extraction pipeline on real saved pages.

use postlight_parser::{Parser, ParseOptions};

fn parse_fixture(name: &str, url: &str) -> postlight_parser::Article {
    let html = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    Parser::parse_html(url, &html, &ParseOptions::default()).expect("parse ok")
}

#[test]
fn nytimes_fixture_metadata() {
    // Expectations from upstream src/extractors/custom/www.nytimes.com/index.test.js
    let article = parse_fixture(
        "www.nytimes.com.html",
        "http://www.nytimes.com/2016/09/20/nyregion/ahmad-khan-rahami-is-arrested-in-manhattan-and-new-jersey-bombings.html",
    );
    // Upstream's exact title comes from the nytimes CUSTOM extractor
    // (Phase 4); with the generic extractor og:title includes a suffix.
    let title = article.title.expect("title");
    assert!(
        title.starts_with("Ahmad Khan Rahami Is Arrested in Manhattan and New Jersey Bombings"),
        "title: {title}"
    );
    assert_eq!(
        article.date_published.as_deref(),
        Some("2016-09-19T11:46:01.000Z")
    );
    let content = article.content.expect("content");
    assert!(content.len() > 1000, "content too short: {}", content.len());
    assert!(
        content.contains("Rahami"),
        "content should contain the article subject"
    );
    // NOTE: upstream's exact first-13-words expectation requires the nytimes
    // custom extractor (Phase 4); the generic extractor picks a valid
    // subsection of the article body.
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
fn fortune_fixture_extracts() {
    let article = parse_fixture(
        "fortune.com.html",
        "http://fortune.com/2016/09/19/ios-10-iphone-7-review/",
    );
    let content = article.content.expect("content");
    assert!(content.len() > 200, "content too short: {}", content.len());
}
