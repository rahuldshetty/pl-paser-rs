//! Field cleaners, ported from upstream `src/cleaners/`: they take a raw
//! extracted value and return the cleaned final value.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::dom::Doc;
use crate::dom_utils::strip_tags;
use crate::utils::{excerpt_content, normalize_spaces};

// --- author ---

static CLEAN_AUTHOR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^\s*(posted |written )?by\s*:?\s*(.*)").expect("author re"));

/// Clean an author string: strip leading "By"/"Posted by" (upstream
/// `cleanAuthor`).
pub fn clean_author(author: &str) -> String {
    let replaced = CLEAN_AUTHOR_RE.replace(author, "$2");
    normalize_spaces(replaced.trim())
}

// --- dek ---

static TEXT_LINK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)http(s)?://").expect("link re"));

/// Clean a dek fragment (upstream `cleanDek`).
pub fn clean_dek(dek: &str, excerpt: Option<&str>) -> Option<String> {
    if dek.len() > 1000 || dek.len() < 5 {
        return None;
    }

    if let Some(excerpt) = excerpt {
        if excerpt_content(excerpt, 10) == excerpt_content(dek, 10) {
            return None;
        }
    }

    let dek_text = strip_tags(dek);
    if TEXT_LINK_RE.is_match(&dek_text) {
        return None;
    }

    Some(normalize_spaces(dek_text.trim()))
}

// --- title ---

static TITLE_SPLITTERS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r": | - | \| ").expect("title splitters re"));

static DOMAIN_ENDINGS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\.com$|\.net$|\.org$|\.co\.uk$").expect("domain endings re"));

/// Clean a title: resolve breadcrumbed/split titles and cap the length
/// (upstream `cleanTitle`).
pub fn clean_title(title: &str, url: &str, doc: &Doc) -> String {
    let mut title = title.to_string();

    if TITLE_SPLITTERS_RE.is_match(&title) {
        title = resolve_split_title(&title, url);
    }

    if title.len() > 150 {
        let h1 = doc.select("h1");
        if h1.len() == 1 {
            title = crate::dom::element_text(h1[0]);
        }
    }

    normalize_spaces(strip_tags(&title).trim())
}

/// Resolve whether any segments of a split title should be removed
/// (upstream `resolveSplitTitle`).
pub fn resolve_split_title(title: &str, url: &str) -> String {
    let split_title = split_preserving(title, &TITLE_SPLITTERS_RE);
    if split_title.len() == 1 {
        return title.to_string();
    }

    if let Some(t) = extract_breadcrumb_title(&split_title, title) {
        return t;
    }

    if let Some(t) = clean_domain_from_title(&split_title, url) {
        return t;
    }

    title.to_string()
}

/// Split a string keeping the matched separators (JS `String.split` with a
/// capturing regex).
fn split_preserving(text: &str, re: &Regex) -> Vec<String> {
    let mut parts = Vec::new();
    let mut last = 0;
    for cap in re.captures_iter(text) {
        let m = cap.get(0).unwrap();
        if m.start() > last {
            parts.push(text[last..m.start()].to_string());
        }
        parts.push(m.as_str().to_string());
        last = m.end();
    }
    if last < text.len() {
        parts.push(text[last..].to_string());
    }
    parts
}

fn extract_breadcrumb_title(split_title: &[String], text: &str) -> Option<String> {
    if split_title.len() < 6 {
        return None;
    }

    // Count how often each term appears in the original split.
    let mut term_counts: Vec<(&str, usize)> = Vec::new();
    for term in split_title {
        if let Some(entry) = term_counts.iter_mut().find(|(t, _)| *t == term) {
            entry.1 += 1;
        } else {
            term_counts.push((term.as_str(), 1));
        }
    }

    let (max_term, term_count) = term_counts.iter().fold(("", 0usize), |acc, (term, count)| {
        if *count > acc.1 {
            (*term, *count)
        } else {
            acc
        }
    });

    // A splitter used more than once is probably the breadcrumber; re-split
    // on that literal (JS `text.split(maxTerm)` drops the separators).
    let segments: Vec<String> = if term_count >= 2 && max_term.len() <= 4 {
        text.split(max_term).map(|s| s.to_string()).collect()
    } else {
        split_title.to_vec()
    };

    let first = segments.first().map(String::as_str).unwrap_or("");
    let last = segments.last().map(String::as_str).unwrap_or("");
    let longest = if first.len() > last.len() {
        first
    } else {
        last
    };
    if longest.len() > 10 {
        return Some(longest.to_string());
    }

    Some(text.to_string())
}

fn clean_domain_from_title(split_title: &[String], url: &str) -> Option<String> {
    let host = url::Url::parse(url).ok()?.host_str()?.to_string();
    let naked_domain = DOMAIN_ENDINGS_RE.replace(&host, "").to_string();

    let start_slug = split_title[0].to_lowercase().replace(' ', "");
    let start_ratio = strsim::normalized_levenshtein(&start_slug, &naked_domain);
    if start_ratio > 0.4 && start_slug.len() > 5 {
        return Some(split_title[2..].join(""));
    }

    let end_slug = split_title
        .last()
        .map(|s| s.to_lowercase().replace(' ', ""))
        .unwrap_or_default();
    let end_ratio = strsim::normalized_levenshtein(&end_slug, &naked_domain);
    if end_ratio > 0.4 && end_slug.len() >= 5 {
        let end = split_title.len().saturating_sub(2);
        return Some(split_title[..end].join(""));
    }

    None
}

// --- excerpt ---

/// Truncate text at a word boundary with an ellipsis (port of the
/// `ellipsize` package used upstream).
pub fn ellipsize(text: &str, max_length: usize, ellipse: &str) -> String {
    if text.len() <= max_length {
        return text.to_string();
    }
    // `max_length` is a byte budget; step back to a char boundary so we
    // never slice mid-UTF-8-char.
    let mut bound = max_length.min(text.len());
    while bound > 0 && !text.is_char_boundary(bound) {
        bound -= 1;
    }
    match text[..bound].rfind(' ') {
        Some(i) => {
            let mut out = text[..i].trim_end().to_string();
            out.push_str(ellipse);
            out
        }
        None => {
            let mut out = text[..bound].to_string();
            out.push_str(ellipse);
            out
        }
    }
}

/// Clean excerpt text (upstream `clean` in excerpt extractor).
pub fn clean_excerpt(content: &str, max_length: usize) -> String {
    let content = content.replace(['\n', '\r', '\t'], " ");
    let content = content.split_whitespace().collect::<Vec<_>>().join(" ");
    ellipsize(&content, max_length, "&hellip;")
}

// --- lead image url ---

static VALID_WEB_URL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(https?|ftp)://").expect("web url re"));

/// Validate a lead image URL (upstream `cleanImage` via `valid-url`).
pub fn clean_lead_image_url(url: &str) -> Option<String> {
    let url = url.trim();
    if VALID_WEB_URL_RE.is_match(url) {
        return Some(url.to_string());
    }
    None
}

// ---------------------------------------------------------------------------
// date_published (upstream `cleaners/date-published.js` + moment-parseformat)
// ---------------------------------------------------------------------------

static MS_DATE_STRING: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{13}$").expect("ms re"));
static SEC_DATE_STRING: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{10}$").expect("sec re"));
static CLEAN_DATE_STRING_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*published\s*:?\s*(.*)").expect("clean date re"));
static TIME_MERIDIAN_SPACE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(.*\d)(am|pm)(.*)").expect("meridian re"));
static TIME_MERIDIAN_DOTS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\.m\.").expect("dots re"));
static TIME_NOW_STRING: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(just|right)?\s*now\s*").expect("now re"));
static TIME_AGO_STRING: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(\d+)\s+(seconds?|minutes?|hours?|days?|weeks?|months?|years?)\s+ago")
        .expect("ago re")
});
static SPLIT_DATE_STRING: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)([0-9]{1,2}:[0-9]{2,2}( ?[ap]\.?m\.?)?)|([0-9]{1,2}[/-][0-9]{1,2}[/-][0-9]{2,4})|(-[0-9]{3,4}$)|([0-9]{1,4})|(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)",
    )
    .expect("split date re")
});
static TIME_WITH_OFFSET_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"-\d{3,4}$").expect("offset re"));

/// The date string split/joined before re-parsing (upstream `cleanDateString`).
pub fn clean_date_string(date_string: &str) -> String {
    let mut cleaned: Vec<&str> = SPLIT_DATE_STRING
        .find_iter(date_string)
        .map(|m| m.as_str())
        .collect();
    let mut joined = cleaned.join(" ");
    joined = TIME_MERIDIAN_DOTS_RE.replace_all(&joined, "m").to_string();
    joined = TIME_MERIDIAN_SPACE_RE
        .replace_all(&joined, "$1 $2 $3")
        .to_string();
    joined = CLEAN_DATE_STRING_RE.replace(&joined, "$1").to_string();
    let _ = &mut cleaned;
    joined.trim().to_string()
}

/// Subtract `n` units from a timestamp (moment `subtract`).
fn subtract_units(
    now: chrono::DateTime<chrono::Utc>,
    n: i64,
    unit: &str,
) -> chrono::DateTime<chrono::Utc> {
    use chrono::{Duration, Months};
    match unit {
        u if u.starts_with("second") => now - Duration::seconds(n),
        u if u.starts_with("minute") => now - Duration::minutes(n),
        u if u.starts_with("hour") => now - Duration::hours(n),
        u if u.starts_with("day") => now - Duration::days(n),
        u if u.starts_with("week") => now - Duration::weeks(n),
        u if u.starts_with("month") => now - Months::new(n as u32),
        u if u.starts_with("year") => now - Months::new((n * 12) as u32),
        _ => now,
    }
}

/// Map a moment.js format string to a chrono format string.
pub fn moment_to_chrono(format: &str) -> String {
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '[' {
            // literal text until ]
            i += 1;
            while i < chars.len() && chars[i] != ']' {
                out.push(chars[i]);
                i += 1;
            }
            i += 1; // skip ]
            continue;
        }
        let rest: String = chars[i..].iter().collect();
        let mut matched = false;
        for (token, fmt) in [
            ("YYYY", "%Y"),
            ("MMMM", "%B"),
            ("dddd", "%A"),
            ("HHmm", "%H%M"),
            ("YY", "%y"),
            ("MMM", "%b"),
            ("ddd", "%a"),
            ("HH", "%H"),
            ("hh", "%I"),
            ("mm", "%M"),
            ("ss", "%S"),
            ("DD", "%d"),
            ("MM", "%m"),
            ("ZZ", "%z"),
            ("SSS", "%.3f"),
            ("M", "%-m"),
            ("D", "%-d"),
            ("H", "%-H"),
            ("h", "%-I"),
            ("m", "%-M"),
            ("s", "%-S"),
            ("A", "%p"),
            ("a", "%P"),
            ("Z", "%:z"),
            ("z", ""),
        ] {
            if rest.starts_with(token) {
                out.push_str(fmt);
                i += token.len();
                matched = true;
                break;
            }
        }
        if !matched {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn parse_with_offset(date_string: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_string) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%:z",
        "%Y-%m-%dT%H:%M%:z",
        "%Y-%m-%d %H:%M:%S%:z",
        "%Y-%m-%d %H:%M%:z",
        "%Y-%m-%d%:z",
    ] {
        if let Ok(dt) = chrono::DateTime::parse_from_str(date_string, fmt) {
            return Some(dt.with_timezone(&chrono::Utc));
        }
    }
    dateparser::parse(date_string)
        .ok()
        .map(|dt| zero_time_if_date_only(date_string, dt.with_timezone(&chrono::Utc)))
}

/// `dateparser` fills unspecified time components with the current wall-clock
/// time; date-only strings should parse to midnight instead (matching
/// moment's behavior).
fn zero_time_if_date_only(
    original: &str,
    dt: chrono::DateTime<chrono::Utc>,
) -> chrono::DateTime<chrono::Utc> {
    if original.contains(':') || original.contains("am") || original.contains("pm") {
        dt
    } else {
        dt.date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|d| d.and_utc())
            .unwrap_or(dt)
    }
}

fn parse_generic(date_string: &str, format: Option<&str>) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Some(fmt) = format {
        let chrono_fmt = moment_to_chrono(fmt);
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(date_string, &chrono_fmt) {
            return Some(naive.and_utc());
        }
        if let Ok(naive) = chrono::NaiveDate::parse_from_str(date_string, &chrono_fmt) {
            return Some(naive.and_hms_opt(0, 0, 0)?.and_utc());
        }
        return None;
    }
    // Prefer explicit formats (so date-only strings parse to midnight), with
    // `dateparser` as the last resort.
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%Y %m %d",
        "%B %-d, %Y",
        "%b %-d, %Y",
        "%b %-d, %Y %H:%M",
        "%B %-d, %Y %-I:%M %P",
        "%a, %d %b %Y %H:%M:%S",
    ] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(date_string, fmt) {
            return Some(naive.and_utc());
        }
        if let Ok(naive) = chrono::NaiveDate::parse_from_str(date_string, fmt) {
            if let Some(dt) = naive.and_hms_opt(0, 0, 0) {
                return Some(dt.and_utc());
            }
        }
    }
    parse_with_offset(date_string)
}

fn parse_in_timezone(
    date_string: &str,
    format: Option<&str>,
    tz: chrono_tz::Tz,
) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Some(fmt) = format {
        let chrono_fmt = moment_to_chrono(fmt);
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(date_string, &chrono_fmt) {
            return naive
                .and_local_timezone(tz)
                .single()
                .map(|dt| dt.with_timezone(&chrono::Utc));
        }
        if let Ok(naive) = chrono::NaiveDate::parse_from_str(date_string, &chrono_fmt) {
            if let Some(dt) = naive.and_hms_opt(0, 0, 0) {
                return dt
                    .and_local_timezone(tz)
                    .single()
                    .map(|dt| dt.with_timezone(&chrono::Utc));
            }
        }
        return None;
    }
    // No format: prefer naive-in-tz parses so date-only strings resolve in
    // the target timezone, then fall back to offset-carrying parses.
    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%Y %m %d",
        "%B %-d, %Y",
        "%b %-d, %Y",
        "%b %-d, %Y %H:%M",
    ] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(date_string, fmt) {
            if let Some(dt) = naive.and_local_timezone(tz).single() {
                return Some(dt.with_timezone(&chrono::Utc));
            }
        }
        if let Ok(naive) = chrono::NaiveDate::parse_from_str(date_string, fmt) {
            if let Some(dt) = naive.and_hms_opt(0, 0, 0) {
                if let Some(dt) = dt.and_local_timezone(tz).single() {
                    return Some(dt.with_timezone(&chrono::Utc));
                }
            }
        }
    }
    parse_with_offset(date_string)
}

fn create_date(
    date_string: &str,
    timezone: Option<&str>,
    format: Option<&str>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    if TIME_WITH_OFFSET_RE.is_match(date_string) {
        return parse_with_offset(date_string);
    }

    if let Some(caps) = TIME_AGO_STRING.captures(date_string) {
        let n: i64 = caps.get(1)?.as_str().parse().ok()?;
        let unit = caps.get(2)?.as_str().to_ascii_lowercase();
        return Some(subtract_units(chrono::Utc::now(), n, &unit));
    }

    if TIME_NOW_STRING.is_match(date_string) {
        return Some(chrono::Utc::now());
    }

    match timezone {
        Some(tz_name) => {
            let tz: chrono_tz::Tz = tz_name.parse().ok()?;
            parse_in_timezone(date_string, format, tz)
        }
        None => parse_generic(date_string, format),
    }
}

/// Convert a datetime to the ISO-8601 string `Date#toISOString` produces.
pub fn to_iso_string(dt: &chrono::DateTime<chrono::Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Clean a date-published string (upstream `cleanDatePublished`).
pub fn clean_date_published(
    date_string: &str,
    timezone: Option<&str>,
    format: Option<&str>,
) -> Option<String> {
    use chrono::TimeZone;

    // Milliseconds / seconds timestamps.
    if MS_DATE_STRING.is_match(date_string) {
        let ms: i64 = date_string.parse().ok()?;
        return Some(to_iso_string(
            &chrono::Utc.timestamp_millis_opt(ms).single()?,
        ));
    }
    if SEC_DATE_STRING.is_match(date_string) {
        let secs: i64 = date_string.parse().ok()?;
        return Some(to_iso_string(&chrono::Utc.timestamp_opt(secs, 0).single()?));
    }

    let mut date = create_date(date_string, timezone, format);

    if date.is_none() {
        let cleaned = clean_date_string(date_string);
        date = create_date(&cleaned, timezone, format);
    }

    date.map(|dt| to_iso_string(&dt))
}

#[cfg(test)]
mod date_tests {
    use super::*;

    #[test]
    fn timestamp_strings() {
        assert_eq!(
            clean_date_published("1500000000000", None, None).as_deref(),
            Some("2017-07-14T02:40:00.000Z")
        );
        assert_eq!(
            clean_date_published("1500000000", None, None).as_deref(),
            Some("2017-07-14T02:40:00.000Z")
        );
    }

    #[test]
    fn iso_with_offset() {
        let d = clean_date_published("2016-09-02T07:30:01-04:00", None, None).unwrap();
        assert_eq!(d, "2016-09-02T11:30:01.000Z");
    }

    #[test]
    fn ago_strings() {
        let d = clean_date_published("2 days ago", None, None).unwrap();
        let parsed = chrono::DateTime::parse_from_rfc3339(&d)
            .unwrap()
            .with_timezone(&chrono::Utc);
        let two_days = chrono::Duration::days(2);
        assert!((chrono::Utc::now() - two_days - parsed).num_minutes().abs() < 5);
    }

    #[test]
    fn just_now() {
        assert!(clean_date_published("just now", None, None).is_some());
    }

    #[test]
    fn moment_format_mapping() {
        assert_eq!(
            moment_to_chrono("MMMM D, YYYY h:mm a"),
            "%B %-d, %Y %-I:%M %P"
        );
        assert_eq!(
            moment_to_chrono("YYYY年MM月DD日 HH時mm分"),
            "%Y年%m月%d日 %H時%M分"
        );
        assert_eq!(moment_to_chrono("YYYY-MM-DD|HH[h]mm"), "%Y-%m-%d|%Hh%M");
    }

    #[test]
    fn format_based_parse() {
        assert_eq!(
            clean_date_published("September 2, 2016", None, Some("MMMM D, YYYY")).as_deref(),
            Some("2016-09-02T00:00:00.000Z")
        );
    }

    #[test]
    fn timezone_parse() {
        assert_eq!(
            clean_date_published("2016-09-02", Some("Asia/Tokyo"), None).as_deref(),
            Some("2016-09-01T15:00:00.000Z")
        );
    }

    #[test]
    fn clean_date_string_joins_fragments() {
        assert_eq!(
            clean_date_string("published September 2, 2016"),
            "Sep 2 2016"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn author_strips_by_prefix() {
        assert_eq!(clean_author("By David Smith"), "David Smith");
        assert_eq!(clean_author("Posted by Jane Doe "), "Jane Doe");
        assert_eq!(clean_author("Written by John"), "John");
        assert_eq!(clean_author("Solo Author"), "Solo Author");
    }

    #[test]
    fn dek_validation() {
        assert_eq!(
            clean_dek("A short dek.", None).as_deref(),
            Some("A short dek.")
        );
        assert_eq!(clean_dek("abc", None), None);
        assert_eq!(clean_dek("short", None).as_deref(), Some("short"));
        assert_eq!(clean_dek("Visit http://example.com now", None), None);
    }

    #[test]
    fn title_splitter_resolution() {
        // Breadcrumbed title: keep the longest end.
        let resolved = resolve_split_title(
            "The Best Gadgets on Earth : Bits : Blogs : NYTimes.com",
            "http://www.nytimes.com/",
        );
        assert!(
            resolved.contains("The Best Gadgets on Earth"),
            "got: {resolved}"
        );
    }

    #[test]
    fn ellipsize_truncates_at_word() {
        assert_eq!(ellipsize("short", 100, "&hellip;"), "short");
        assert_eq!(
            ellipsize("one two three four", 10, "&hellip;"),
            "one two&hellip;"
        );
    }

    #[test]
    fn ellipsize_handles_multibyte_boundaries() {
        // byte budget landing mid-char must not panic
        let text = "héllo wörld éverybody";
        let out = ellipsize(text, 7, "&hellip;");
        assert!(out.ends_with("&hellip;"));
        assert!(out.is_char_boundary(out.len()));
    }

    #[test]
    fn clean_excerpt_collapses_whitespace() {
        assert_eq!(clean_excerpt("a\n b   c", 100), "a b c");
    }

    #[test]
    fn lead_image_url_validation() {
        assert_eq!(
            clean_lead_image_url("https://x.com/a.png").as_deref(),
            Some("https://x.com/a.png")
        );
        assert_eq!(clean_lead_image_url("/relative/a.png"), None);
    }
}
