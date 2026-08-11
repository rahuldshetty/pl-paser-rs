//! Resource fetching and document preparation, ported from upstream
//! `src/resource/`.
//!
//! Pipeline: fetch (or accept pre-fetched HTML) → validate → decode the
//! charset → parse → normalize meta tags (`content`→`value`,
//! `property`→`name`) → lift lazy-loaded images → remove junk tags and
//! comments.

use std::time::Duration;

use reqwest::header::CONTENT_TYPE;
use reqwest::StatusCode;
use serde_json::Value;

use crate::dom::Doc;
use crate::types::ParserError;
use crate::utils::{get_encoding, validate_url};

/// Desktop user agent used for requests (upstream `REQUEST_HEADERS`).
pub const REQUEST_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 6.1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/41.0.2228.0 Safari/537.36";

/// Request timeout in milliseconds (upstream `FETCH_TIMEOUT`).
pub const FETCH_TIMEOUT: Duration = Duration::from_millis(10_000);

/// Maximum acceptable content length (upstream `MAX_CONTENT_LENGTH`, 5 MB).
pub const MAX_CONTENT_LENGTH: u64 = 5_242_880;

/// Content types we refuse to extract from (upstream `BAD_CONTENT_TYPES`).
pub const BAD_CONTENT_TYPES: [&str; 4] = ["audio/mpeg", "image/gif", "image/jpeg", "image/jpg"];

fn is_link_re() -> &'static regex::Regex {
    static RE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| regex::Regex::new(r"https?://").expect("valid link re"));
    &RE
}

fn is_image_re() -> &'static regex::Regex {
    static RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r"\.(png|gif|jpe?g)").expect("valid image re")
    });
    &RE
}

fn is_srcset_re() -> &'static regex::Regex {
    static RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r"\.(png|gif|jpe?g)(\?\S+)?(\s*[\d.]+[wx])").expect("valid srcset re")
    });
    &RE
}

/// The bytes and metadata of a fetched resource.
pub struct FetchedResource {
    pub body: Vec<u8>,
    pub content_type: String,
    /// Content-Length header, when present.
    pub content_length: Option<u64>,
}

/// Fetch a URL with upstream's behavior: desktop UA, cookie jar, gzip,
/// redirect following, 10 s timeout.
pub async fn fetch_resource(
    url: &url::Url,
    headers: &[(String, String)],
) -> Result<FetchedResource, ParserError> {
    let client = reqwest::Client::builder()
        .user_agent(REQUEST_USER_AGENT)
        .timeout(FETCH_TIMEOUT)
        .cookie_store(true)
        .build()
        .map_err(|e| ParserError::Http(e.to_string()))?;

    let mut request = client.get(url.clone());
    for (name, value) in headers {
        request = request.header(name.as_str(), value.as_str());
    }

    let response = request
        .send()
        .await
        .map_err(|e| ParserError::Http(e.to_string()))?;

    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    validate_response(status, &content_type, content_length)?;

    let body = response
        .bytes()
        .await
        .map_err(|e| ParserError::Http(e.to_string()))?
        .to_vec();

    Ok(FetchedResource {
        body,
        content_type,
        content_length,
    })
}

/// Validate a response (upstream `validateResponse`).
pub fn validate_response(
    status: StatusCode,
    content_type: &str,
    content_length: Option<u64>,
) -> Result<(), ParserError> {
    if status != StatusCode::OK {
        return Err(ParserError::Non200(status.as_u16()));
    }

    let trimmed = content_type.trim().to_ascii_lowercase();
    if BAD_CONTENT_TYPES.contains(&trimmed.as_str()) {
        return Err(ParserError::BadContentType(content_type.to_string()));
    }

    if let Some(len) = content_length {
        if len > MAX_CONTENT_LENGTH {
            return Err(ParserError::ContentTooLarge);
        }
    }

    Ok(())
}

/// Create a parsed, prepared document from a URL (fetching it) or from
/// pre-fetched HTML (upstream `Resource.create`).
pub async fn create_doc(
    url: &url::Url,
    html: Option<String>,
    headers: &[(String, String)],
) -> Result<Doc, ParserError> {
    if !validate_url(url) {
        return Err(ParserError::InvalidUrl);
    }

    let (body, content_type, already_decoded) = match html {
        Some(prepared) => (prepared.into_bytes(), "text/html".to_string(), true),
        None => {
            let fetched = fetch_resource(url, headers).await?;
            (fetched.body, fetched.content_type, false)
        }
    };

    generate_doc(body, &content_type, already_decoded)
}

/// Parse and prepare a document from raw bytes (upstream `Resource.generateDoc`).
pub fn generate_doc(
    body: Vec<u8>,
    content_type: &str,
    already_decoded: bool,
) -> Result<Doc, ParserError> {
    // TODO(upstream): implement `is_text` from readability's utils/text.py.
    if !content_type.contains("html") && !content_type.contains("text") {
        return Err(ParserError::NotText);
    }

    let mut doc = encode_doc(body, content_type, already_decoded)?;

    if doc.root_children_count() == 0 {
        return Err(ParserError::NoChildren);
    }

    normalize_meta_tags(&mut doc);
    convert_lazy_loaded_images(&mut doc);
    clean(&mut doc);

    Ok(doc)
}

/// Decode and parse raw bytes (upstream `Resource.encodeDoc`): decode using
/// the charset declared in the HTTP `content-type`, then re-decode with the
/// `<meta>` charset if the body declares a different one.
pub fn encode_doc(
    content: Vec<u8>,
    content_type: &str,
    already_decoded: bool,
) -> Result<Doc, ParserError> {
    if already_decoded {
        let text = String::from_utf8_lossy(&content).into_owned();
        return Ok(Doc::parse_document(&text));
    }

    let encoding = get_encoding(content_type);
    let mut doc = Doc::parse_document(&decode(&content, &encoding));

    // After the first parse, check whether the body declares a charset.
    let meta_content_type = doc
        .attr("meta[http-equiv=content-type i]", "content")
        .or_else(|| doc.attr("meta[charset]", "charset"));

    if let Some(meta) = meta_content_type {
        let proper_encoding = get_encoding(&meta);
        // If the encodings in the header and body differ, use the body's.
        if proper_encoding != encoding {
            doc = Doc::parse_document(&decode(&content, &proper_encoding));
        }
    }

    Ok(doc)
}

/// Decode bytes with a charset label, falling back to UTF-8.
pub fn decode(bytes: &[u8], encoding: &str) -> String {
    let enc = encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::UTF_8);
    let (decoded, _, _) = enc.decode(bytes);
    decoded.into_owned()
}

/// Replace `content` with `value` and `property` with `name` on every meta
/// tag (upstream `normalizeMetaTags`).
pub fn normalize_meta_tags(doc: &mut Doc) {
    let meta_ids = doc.select_ids("meta");
    for id in &meta_ids {
        if let Some(value) = doc.attr_of(*id, "content") {
            doc.set_attr(*id, "value", &value);
            doc.remove_attr(*id, "content");
        }
        if let Some(property) = doc.attr_of(*id, "property") {
            doc.set_attr(*id, "name", &property);
            doc.remove_attr(*id, "property");
        }
    }
}

/// Try to read `src` out of a JSON-encoded attribute value
/// (e.g. `data-image="{&quot;src&quot;:&quot;...&quot;}"`).
fn extract_src_from_json(value: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(value).ok()?;
    match parsed.get("src") {
        Some(Value::String(src)) => Some(src.clone()),
        _ => None,
    }
}

/// Lift lazy-loaded images: copy URLs from `data-*` / JSON attributes into
/// `src`/`srcset` (upstream `convertLazyLoadedImages`).
pub fn convert_lazy_loaded_images(doc: &mut Doc) {
    let img_ids = doc.select_ids("img");
    for id in &img_ids {
        // Snapshot attrs to avoid aliasing while mutating.
        let attrs: Vec<(String, String)> = doc
            .get(*id)
            .and_then(|node| node.value().as_element())
            .map(|el| {
                el.attrs()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        for (attr, value) in attrs {
            if attr != "srcset" && is_link_re().is_match(&value) && is_srcset_re().is_match(&value)
            {
                doc.set_attr(*id, "srcset", &value);
            } else if attr != "src"
                && attr != "srcset"
                && is_link_re().is_match(&value)
                && is_image_re().is_match(&value)
            {
                // Is the value a JSON object? If so, extract the image src.
                match extract_src_from_json(&value) {
                    Some(src) => doc.set_attr(*id, "src", &src),
                    None => doc.set_attr(*id, "src", &value),
                }
            }
        }
    }
}

/// Remove junk tags (`script`, `style`, `form`) and all comments
/// (upstream resource-level `clean`).
pub fn clean(doc: &mut Doc) {
    doc.remove_selector("script, style, form");
    doc.remove_comments();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_response_checks_status() {
        assert!(validate_response(StatusCode::OK, "text/html", None).is_ok());
        assert!(matches!(
            validate_response(StatusCode::NOT_FOUND, "text/html", None),
            Err(ParserError::Non200(404))
        ));
    }

    #[test]
    fn validate_response_rejects_bad_content_types() {
        assert!(matches!(
            validate_response(StatusCode::OK, "image/jpeg", None),
            Err(ParserError::BadContentType(_))
        ));
        assert!(validate_response(StatusCode::OK, "text/html; charset=utf-8", None).is_ok());
    }

    #[test]
    fn validate_response_rejects_too_large() {
        assert!(matches!(
            validate_response(StatusCode::OK, "text/html", Some(MAX_CONTENT_LENGTH + 1)),
            Err(ParserError::ContentTooLarge)
        ));
        assert!(validate_response(StatusCode::OK, "text/html", Some(100)).is_ok());
    }

    #[test]
    fn generate_doc_rejects_non_text() {
        assert!(matches!(
            generate_doc(b"x".to_vec(), "application/pdf", false),
            Err(ParserError::NotText)
        ));
    }

    #[test]
    fn encode_doc_decodes_utf8() {
        let html = "<html><body><p>héllo</p></body></html>";
        let doc = encode_doc(html.as_bytes().to_vec(), "text/html; charset=utf-8", false).unwrap();
        assert_eq!(doc.text("p").as_deref(), Some("héllo"));
    }

    #[test]
    fn generate_doc_normalizes_meta_and_cleans() {
        let html = r#"<html><head>
            <meta property="og:title" content="The Title">
            <script>bad()</script>
        </head><body><!-- comment --><form><input></form><p>text</p></body></html>"#;
        let doc = generate_doc(html.as_bytes().to_vec(), "text/html", false).unwrap();
        // property -> name, content -> value
        assert_eq!(
            doc.attr("meta[name=\"og:title\"]", "value").as_deref(),
            Some("The Title")
        );
        let serialized = doc.serialize();
        assert!(!serialized.contains("<script"));
        assert!(!serialized.contains("<!-- comment"));
        assert!(!serialized.contains("<form"));
        assert!(serialized.contains("<p>text</p>"));
    }

    #[test]
    fn convert_lazy_loaded_images_lifts_srcs() {
        let html = r#"<html><body>
            <img data-src="https://x.com/a.png">
            <img data-srcset="https://x.com/a.png 1x">
            <img src="placeholder.gif" data-original="https://x.com/b.jpg">
        </body></html>"#;
        let doc = generate_doc(html.as_bytes().to_vec(), "text/html", false).unwrap();
        let serialized = doc.serialize();
        assert!(
            serialized.contains(r#"src="https://x.com/a.png""#),
            "{serialized}"
        );
        assert!(serialized.contains("srcset"), "{serialized}");
    }

    #[test]
    fn decode_supports_shift_jis() {
        // 'こんにちは' in Shift_JIS
        let bytes = [0x82u8, 0xb1, 0x82, 0xf1, 0x82, 0xc9, 0x82, 0xbf, 0x82, 0xcd];
        let decoded = decode(&bytes, "shift_jis");
        assert_eq!(decoded, "こんにちは");
    }

    #[tokio::test]
    #[ignore = "live network test"]
    async fn fetch_resource_live() {
        let url = url::Url::parse("https://example.com/").unwrap();
        let fetched = fetch_resource(&url, &[]).await.expect("fetch ok");
        assert!(!fetched.body.is_empty());
        assert!(fetched.content_type.contains("html"));
    }
}
