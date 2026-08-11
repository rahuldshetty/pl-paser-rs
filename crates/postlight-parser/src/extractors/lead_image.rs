//! Lead image URL scoring, ported from upstream
//! `src/extractors/generic/lead-image-url/`.

use once_cell::sync::Lazy;
use regex::Regex;

static POSITIVE_LEAD_IMAGE_URL_HINTS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new("upload|wp-content|large|photo|wp-image").expect("positive hints re"));
static NEGATIVE_LEAD_IMAGE_URL_HINTS_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        "spacer|sprite|blank|throbber|gradient|tile|bg|background|icon|social|header|hdr|advert|spinner|loader|loading|default|rating|share|facebook|twitter|theme|promo|ads|wp-includes",
    )
    .expect("negative hints re")
});
static GIF_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\.gif(\?.*)?$").expect("gif re"));
static JPG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\.jpe?g(\?.*)?$").expect("jpg re"));

/// Score an image URL by its string hints (upstream `scoreImageUrl`).
pub fn score_image_url(url: &str) -> f64 {
    let url = url.trim();
    let mut score = 0.0;

    if POSITIVE_LEAD_IMAGE_URL_HINTS_RE.is_match(url) {
        score += 20.0;
    }
    if NEGATIVE_LEAD_IMAGE_URL_HINTS_RE.is_match(url) {
        score -= 20.0;
    }
    if GIF_RE.is_match(url) {
        score -= 10.0;
    }
    if JPG_RE.is_match(url) {
        score += 10.0;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_url_hints() {
        assert!(score_image_url("https://x.com/wp-content/uploads/big-photo.jpg") > 0.0);
        assert!(score_image_url("https://x.com/img/spacer.gif") < 0.0);
    }
}
