//! Named content transforms for custom extractors (upstream transform
//! *functions*). Each site-specific transform is registered under a
//! `domain::selector` key and implemented here in Rust, faithfully porting
//! the upstream JS.

use ego_tree::NodeId;
use scraper::ElementRef;

use crate::dom::Doc;

// --- helpers shared by the site transforms --------------------------------

/// Replace a node with a parsed HTML fragment.
fn replace_with_html(doc: &mut Doc, id: NodeId, html: &str) {
    doc.insert_before(id, html);
    doc.remove(&[id]);
}

/// First ancestor matching a selector.
fn first_ancestor_matching(doc: &Doc, id: NodeId, selector: &str) -> Option<NodeId> {
    doc.ancestors_of(id)
        .into_iter()
        .find(|a| doc.matches(*a, selector))
}

/// Percent-decode (JS `decodeURIComponent`).
fn decode_uri_component(s: &str) -> String {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 1 && i + 2 <= bytes.len() - 1 + 1 {
            let hex = &s[i + 1..(i + 3).min(s.len())];
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn text_of(doc: &Doc, id: NodeId) -> String {
    doc.text_of(id).unwrap_or_default()
}

pub fn apply(doc: &mut Doc, resource: &Doc, id: NodeId, name: &str) -> Option<String> {
    match name {
        // arstechnica.com: insert an empty paragraph before significant h2s.
        "arstechnica.com::h2" => {
            doc.insert_before(id, "<p></p>");
            None
        }

        // deadline.com: unwrap .embed-twitter embeds.
        "deadline.com::.embed-twitter" => {
            if let Some(html) = doc.inner_html_of(id) {
                replace_with_html(doc, id, &html);
            }
            None
        }

        // deadspin.com: lazy youtube iframes.
        "deadspin.com::iframe.lazyload[data-recommend-id^=\"youtube://\"]" => {
            let attr_id = doc.attr_of(id, "id").unwrap_or_default();
            if let Some(idx) = attr_id.find("youtube-") {
                let youtube_id = &attr_id[idx + "youtube-".len()..];
                doc.set_attr(
                    id,
                    "src",
                    &format!("https://www.youtube.com/embed/{youtube_id}"),
                );
            }
            None
        }

        // ma.ttias.be: demote h2s, drop ids; keep h1s by adding a paragraph.
        "ma.ttias.be::h2" => {
            doc.remove_attr(id, "id");
            Some("h3".to_string())
        }
        "ma.ttias.be::h1" => {
            doc.remove_attr(id, "id");
            doc.insert_after(id, "<p></p>");
            None
        }
        "ma.ttias.be::ul" => {
            doc.set_attr(id, "class", "entry-content-asset");
            None
        }

        // medium.com: drop cap, lazy youtube iframes, figure cleanup, small images.
        "medium.com::section span:first-of-type" => {
            let html = doc.inner_html_of(id).unwrap_or_default();
            if html.chars().count() == 1 {
                let first = html.chars().next().unwrap_or_default();
                if first.is_ascii_alphabetic() || first == '(' || first == ')' {
                    replace_with_html(doc, id, &html);
                }
            }
            None
        }
        "medium.com::iframe" => {
            let thumb = doc.attr_of(id, "data-thumbnail").unwrap_or_default();
            let thumb = decode_uri_component(&thumb);
            let yt_re =
                regex::Regex::new(r"https://i\.embed\.ly/.+url=https://i\.ytimg\.com/vi/(\w+)/")
                    .expect("medium yt re");
            let figure = first_ancestor_matching(doc, id, "figure");
            if let Some(caps) = yt_re.captures(&thumb) {
                let youtube_id = &caps[1];
                doc.set_attr(
                    id,
                    "src",
                    &format!("https://www.youtube.com/embed/{youtube_id}"),
                );
                if let Some(figure) = figure {
                    // keep the figure, iframe, and figcaption; drop the rest.
                    let caption_ids = doc.select_ids_in(figure, "figcaption");
                    let children = doc.children_ids(figure);
                    for child in children {
                        if child != id && !caption_ids.contains(&child) {
                            doc.remove(&[child]);
                        }
                    }
                }
            } else if let Some(figure) = figure {
                doc.remove(&[figure]);
            }
            None
        }
        "medium.com::figure" => {
            let has_iframe = !doc.select_ids_in(id, "iframe").is_empty();
            if has_iframe {
                return None;
            }
            let img = doc.select_ids_in(id, "img").into_iter().last();
            let captions = doc.select_ids_in(id, "figcaption");
            if let Some(img) = img {
                let children = doc.children_ids(id);
                for child in children {
                    if child != img && !captions.contains(&child) {
                        doc.remove(&[child]);
                    }
                }
            }
            None
        }
        "medium.com::img" => {
            let width = doc.attr_of(id, "width").and_then(|w| w.parse::<i64>().ok());
            if width.is_some_and(|w| w < 100) {
                doc.remove(&[id]);
            }
            None
        }

        // news.mynavi.jp: lift data-original into src.
        "news.mynavi.jp::img" => {
            if let Some(src) = doc.attr_of(id, "data-original") {
                if !src.is_empty() {
                    doc.set_attr(id, "src", &src);
                }
            }
            None
        }

        // wikipedia.org: move the first infobox image to the front.
        "wikipedia.org::.infobox img" => {
            if let Some(infobox) = first_ancestor_matching(doc, id, ".infobox") {
                let has_img_child = !doc.select_ids_in(infobox, "img").is_empty();
                if !has_img_child {
                    // prepend the image into the infobox (moves it)
                    if let Some(mut infobox_mut) = doc.html.tree.get_mut(infobox) {
                        infobox_mut.prepend_id(id);
                    }
                }
            }
            None
        }

        // wired.jp: resolve data-original against src.
        "wired.jp::img[data-original]" => {
            if let (Some(src), Some(data_original)) =
                (doc.attr_of(id, "src"), doc.attr_of(id, "data-original"))
            {
                if let (Ok(base), Ok(resolved)) = (url::Url::parse(&src), url::Url::parse(&src)) {
                    if let Ok(url) = base.join(&data_original) {
                        let _ = resolved;
                        doc.set_attr(id, "src", url.as_str());
                    }
                }
            }
            None
        }

        // www.abendblatt.de: deobfuscate text (char-code shift).
        "www.abendblatt.de::p" | "www.abendblatt.de::div" => {
            if !doc.has_class(id, "obfuscated") {
                return None;
            }
            let text = text_of(doc, id);
            let mut out = String::new();
            for ch in text.chars() {
                let r = ch as u32;
                match r {
                    177 => out.push('%'),
                    178 => out.push('!'),
                    180 => out.push(';'),
                    181 => out.push('='),
                    32 => out.push(' '),
                    10 => out.push('\n'),
                    r if r > 33 => {
                        if let Some(c) = char::from_u32(r - 1) {
                            out.push(c);
                        }
                    }
                    _ => {}
                }
            }
            doc.replace_inner_html(id, &out);
            doc.remove_attr(id, "class");
            doc.set_attr(id, "class", "deobfuscated");
            None
        }

        // www.apartmenttherapy.com: JSON data-props lazy image.
        "www.apartmenttherapy.com::div[data-render-react-id=\"images/LazyPicture\"]" => {
            let data_props = doc.attr_of(id, "data-props").unwrap_or_default();
            let parsed: serde_json::Value = serde_json::from_str(&data_props).ok()?;
            let src = parsed
                .get("sources")?
                .get(0)?
                .get("src")?
                .as_str()?
                .to_string();
            replace_with_html(doc, id, &format!("<img src=\"{src}\"/>"));
            None
        }

        // www.buzzfeed.com: custom header media.
        "www.buzzfeed.com::div.longform_custom_header_media" => {
            let has_img = !doc.select_ids_in(id, "img").is_empty();
            let has_source = !doc
                .select_ids_in(id, ".longform_header_image_source")
                .is_empty();
            if has_img && has_source {
                return Some("figure".to_string());
            }
            None
        }

        // www.cnet.com: normalize figure images.
        "www.cnet.com::figure.image" => {
            let imgs: Vec<NodeId> = doc.select_ids_in(id, "img");
            for img in &imgs {
                doc.set_attr(*img, "width", "100%");
                doc.set_attr(*img, "height", "100%");
                let class = doc.attr_of(*img, "class").unwrap_or_default();
                let mut classes: Vec<&str> = class.split_whitespace().collect();
                if !classes.contains(&"__image-lead__") {
                    classes.push("__image-lead__");
                    doc.set_attr(*img, "class", &classes.join(" "));
                }
            }
            doc.remove_selector(".imgContainer");
            if let Some(img) = imgs.last().copied() {
                if let Some(mut fig) = doc.html.tree.get_mut(id) {
                    fig.prepend_id(img);
                }
            }
            None
        }

        // www.cnn.com: paragraph cleanup.
        "www.cnn.com::.zn-body__paragraph, .el__leafmedia--sourced-paragraph" => {
            let has_text = !doc.inner_html_of(id).unwrap_or_default().is_empty();
            if has_text {
                return Some("p".to_string());
            }
            None
        }
        "www.cnn.com::.zn-body__paragraph" => {
            let has_a = !doc.select_ids_in(id, "a").is_empty();
            if has_a {
                let node_text = text_of(doc, id);
                let a_text = doc
                    .select_ids_in(id, "a")
                    .first()
                    .and_then(|a| doc.text_of(*a))
                    .unwrap_or_default();
                if node_text.trim() == a_text.trim() {
                    doc.remove(&[id]);
                }
            }
            None
        }

        // www.elecom.co.jp: table width.
        "www.elecom.co.jp::table" => {
            doc.set_attr(id, "width", "auto");
            None
        }

        // www.fool.com: caption images to figures.
        "www.fool.com::.caption img" => {
            let src = doc.attr_of(id, "src").unwrap_or_default();
            if let Some(parent) = doc.parent_of(id) {
                replace_with_html(
                    doc,
                    parent,
                    &format!("<figure><img src=\"{src}\"/></figure>"),
                );
            }
            None
        }

        // www.fortinet.com / www.si.com: noscript images to figures.
        "www.fortinet.com::noscript" | "www.si.com::noscript" => {
            let children = doc.children_ids(id);
            let is_img = children.len() == 1
                && doc
                    .element_name_of(children[0])
                    .map(|n| n.eq_ignore_ascii_case("img"))
                    .unwrap_or(false);
            if is_img {
                return Some("figure".to_string());
            }
            None
        }

        // www.gizmodo.jp / www.lifehacker.jp: strip %27 wrappers.
        "www.gizmodo.jp::img.p-post-thumbnailImage" | "www.lifehacker.jp::img.lazyload" => {
            let src = doc.attr_of(id, "src").unwrap_or_default();
            let cleaned = src
                .split("%27")
                .last()
                .unwrap_or(&src)
                .trim_end_matches("%27;");
            doc.set_attr(id, "src", cleaned);
            None
        }

        // www.latimes.com: replace container with its figure.
        "www.latimes.com::.trb_ar_la" => {
            if let Some(figure) = doc.select_ids_in(id, "figure").into_iter().next() {
                if let Some(html) = doc.html_of(figure) {
                    replace_with_html(doc, id, &html);
                }
            }
            None
        }

        // www.msnbc.com: prepend the page's og:image.
        "www.msnbc.com::.pane-node-body" => {
            let src = resource.attr("meta[name=\"og:image\"]", "value");
            if let Some(src) = src {
                doc.prepend_child(id, &format!("<img src=\"{src}\" />"));
            }
            None
        }

        // www.nationalgeographic.com: lead images from data attributes.
        "www.nationalgeographic.com::.parsys.content"
        | "news.nationalgeographic.com::.parsys.content" => {
            let first_child = doc.children_ids(id).into_iter().next();
            let is_image_group = first_child
                .map(|c| doc.has_class(c, "imageGroup"))
                .unwrap_or(false);
            if is_image_group {
                let container = first_child.and_then(|c| {
                    doc.select_ids_in(c, ".media--medium__container")
                        .into_iter()
                        .next()
                });
                let data_container = container.and_then(|c| doc.children_ids(c).into_iter().next());
                let img1 = data_container.and_then(|c| doc.attr_of(c, "data-platform-image1-path"));
                let img2 = data_container.and_then(|c| doc.attr_of(c, "data-platform-image2-path"));
                if let (Some(img1), Some(img2)) = (img1, img2) {
                    doc.prepend_child(
                            id,
                            &format!(
                                "<div class=\"__image-lead__\"><img src=\"{img1}\"/><img src=\"{img2}\"/></div>"
                            ),
                        );
                }
            } else {
                let img_src = doc
                    .select_ids_in(id, ".image.parbase.section .picturefill")
                    .into_iter()
                    .next()
                    .and_then(|c| doc.attr_of(c, "data-platform-src"));
                if let Some(img_src) = img_src {
                    doc.prepend_child(
                        id,
                        &format!("<img class=\"__image-lead__\" src=\"{img_src}\"/>"),
                    );
                }
            }
            None
        }

        // www.ndtv.com: move the dateline into the following paragraph.
        "www.ndtv.com::.place_cont" => {
            if !doc.ancestors_of(id).iter().any(|a| {
                doc.element_name_of(*a)
                    .map(|n| n.eq_ignore_ascii_case("p"))
                    .unwrap_or(false)
            }) {
                if let Some(next_p) = doc.next_sibling(id) {
                    if doc
                        .element_name_of(next_p)
                        .map(|n| n.eq_ignore_ascii_case("p"))
                        .unwrap_or(false)
                    {
                        doc.remove(&[id]);
                        if let Some(mut p) = doc.html.tree.get_mut(next_p) {
                            p.prepend_id(id);
                        }
                    }
                }
            }
            None
        }

        // www.nytimes.com: replace the {{size}} placeholder.
        "www.nytimes.com::img.g-lazy" => {
            let src = doc.attr_of(id, "src").unwrap_or_default();
            let src = src.replace("{{size}}", "640");
            doc.set_attr(id, "src", &src);
            None
        }

        // www.reddit.com: extract the image from a CSS background.
        "www.reddit.com::div[role=\"img\"]" => {
            let imgs = doc.select_ids_in(id, "img");
            let bg = doc.attr_of(id, "style").unwrap_or_default();
            if imgs.len() == 1 && !bg.is_empty() {
                let re = regex::Regex::new(r"\((.*?)\)").expect("bg re");
                if let Some(caps) = re.captures(&bg) {
                    let cleaned = caps[1].trim_matches(|c| c == '\'' || c == '"').to_string();
                    doc.set_attr(imgs[0], "src", &cleaned);
                    return Some("img".to_string());
                }
            }
            None
        }

        // www.refinery29.com: unwrap loading noscripts.
        "www.refinery29.com::div.loading noscript" => {
            let img_html = doc.inner_html_of(id).unwrap_or_default();
            if let Some(loading) = first_ancestor_matching(doc, id, ".loading") {
                replace_with_html(doc, loading, &img_html);
            }
            None
        }

        // www.theverge.com: noscript images to spans.
        "www.theverge.com::noscript" => {
            let children = doc.children_ids(id);
            let is_img = children.len() == 1
                && doc
                    .element_name_of(children[0])
                    .map(|n| n.eq_ignore_ascii_case("img"))
                    .unwrap_or(false);
            if is_img {
                return Some("span".to_string());
            }
            None
        }

        // www.vox.com: replace the dynamic image with the noscript content.
        "www.vox.com::figure .e-image__image noscript" => {
            let img_html = doc.inner_html_of(id).unwrap_or_default();
            if let Some(image) = first_ancestor_matching(doc, id, ".e-image__image") {
                let dynamic = doc.select_ids_in(image, ".c-dynamic-image");
                for d in dynamic {
                    replace_with_html(doc, d, &img_html);
                }
            }
            None
        }

        // www.washingtonpost.com: inline content to figure or remove.
        "www.washingtonpost.com::div.inline-content" => {
            let has_media = !doc.select_ids_in(id, "img, iframe, video").is_empty();
            if has_media {
                return Some("figure".to_string());
            }
            doc.remove(&[id]);
            None
        }

        // www.youtube.com: replace players with embeds.
        "www.youtube.com::#player-api" => {
            let video_id = resource.attr("meta[itemProp=\"videoId\"]", "value");
            if let Some(video_id) = video_id {
                doc.replace_inner_html(
                        id,
                        &format!(
                            "<iframe src=\"https://www.youtube.com/embed/{video_id}\" frameborder=\"0\" allowfullscreen></iframe>"
                        ),
                    );
            }
            None
        }
        "www.youtube.com::#player-container-outer" => {
            let video_id = resource.attr("meta[itemProp=\"videoId\"]", "value");
            let description = resource.attr("meta[itemProp=\"description\"]", "value");
            if let (Some(video_id), Some(description)) = (video_id, description) {
                doc.replace_inner_html(
                        id,
                        &format!(
                            "<iframe src=\"https://www.youtube.com/embed/{video_id}\" frameborder=\"0\" allowfullscreen></iframe><div><span>{description}</span></div>"
                        ),
                    );
            }
            None
        }

        _ => None,
    }
}

#[allow(dead_code)]
fn _keep(_: &Doc, _: ElementRef<'_>) {}
