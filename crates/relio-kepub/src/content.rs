//! Content document transform: a port of kepubify's TransformContent.

use crate::dom::{self, DOCUMENT, Data, Dom, NodeId};
use crate::sentences::split_sentences;
use crate::smartypants::smarten;

pub(crate) const KOBO_STYLE_CLASS: &str = "kobostylehacks";
pub(crate) const KOBO_STYLE_CSS: &str = "div#book-inner { margin-top: 0; margin-bottom: 0;}";

const PARAGRAPHS: &[&str] = &["p", "ol", "ul", "table", "h1", "h2", "h3", "h4", "h5", "h6"];
const NO_SPANS: &[&str] = &["script", "style", "pre", "audio", "video", "svg", "math"];
const NO_SMART: &[&str] = &["pre", "code", "style", "script"];
const ADEPT_META: &[&str] = &["Adept.expected.resource", "Adept.resource"];

pub(crate) struct Style {
    pub class: String,
    pub css: String,
}

pub(crate) struct Config<'a> {
    /// Kobo style hacks first, then the extra CSS options in order.
    pub styles: &'a [Style],
    pub smart_punctuation: bool,
    pub cjk_sentences: bool,
}

pub(crate) struct Converted {
    pub html: String,
    pub spans: usize,
}

pub(crate) fn transform(html: &str, config: &Config) -> Converted {
    let mut dom = dom::parse(html);
    let head = dom.find(DOCUMENT, "head");
    let body = dom.find(DOCUMENT, "body");

    if let Some(head) = head {
        charset_utf8(&mut dom, head);
    }
    let mut styles = config.styles.iter();
    if let (Some(head), Some(style)) = (head, styles.next()) {
        add_style(&mut dom, head, style);
    }
    let mut spans = 0;
    if let Some(body) = body {
        kobo_divs(&mut dom, body);
        spans = kobo_spans(&mut dom, body, config.cjk_sentences);
    }
    if let Some(head) = head {
        for style in styles {
            add_style(&mut dom, head, style);
        }
    }
    if let (Some(body), true) = (body, config.smart_punctuation) {
        punctuation(&mut dom, body);
    }
    clean(&mut dom);

    Converted {
        html: dom::render(&dom),
        spans,
    }
}

fn is_space(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

fn has_class(dom: &Dom, id: NodeId, class: &str) -> bool {
    dom.attr(id, "class").is_some_and(|value| {
        value
            .split([' ', '\t', '\r', '\n', '\x0c'])
            .any(|c| c == class)
    })
}

fn descendants(dom: &Dom, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    let mut stack = dom.children(id);
    stack.reverse();
    std::iter::from_fn(move || {
        let next = stack.pop()?;
        stack.extend(dom.children(next).into_iter().rev());
        Some(next)
    })
}

/// Rewrites charset declarations in the head to UTF-8, since the output always is.
fn charset_utf8(dom: &mut Dom, head: NodeId) {
    let metas: Vec<NodeId> = std::iter::once(head)
        .chain(descendants(dom, head))
        .filter(|&id| dom.name(id) == Some("meta"))
        .collect();
    for meta in metas {
        let Data::Element { attrs, .. } = dom.data_mut(meta) else {
            continue;
        };
        for i in 0..attrs.len() {
            if attrs[i].key == "charset" && attrs[i].value.to_uppercase() != "UTF-8" {
                attrs[i].value = "UTF-8".to_owned();
                break;
            }
            if attrs[i].key == "http-equiv" && attrs[i].value.to_lowercase() == "content-type" {
                if let Some(content) = attrs.iter_mut().find(|a| a.key == "content")
                    && let Some(value) = utf8_media_type(&content.value)
                {
                    content.value = value;
                }
                break;
            }
        }
    }
}

/// Go's mime.ParseMediaType + FormatMediaType after forcing `charset=utf-8`,
/// or `None` when nothing changes.
fn utf8_media_type(content: &str) -> Option<String> {
    let mut parts = content.split(';');
    let media_type = parts.next()?.trim().to_lowercase();
    let (kind, subtype) = media_type.split_once('/')?;
    if kind.is_empty() || subtype.is_empty() {
        return None;
    }
    let mut params = Vec::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) = part.split_once('=')?;
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value);
        params.push((key.trim().to_lowercase(), value.to_owned()));
    }
    let charset = params.iter_mut().find(|(key, _)| key == "charset")?;
    if charset.1.to_lowercase() == "utf-8" {
        return None;
    }
    charset.1 = "utf-8".to_owned();
    params.sort();
    Some(params.iter().fold(media_type, |acc, (key, value)| {
        format!("{acc}; {key}={}", quote_token(value))
    }))
}

fn quote_token(value: &str) -> String {
    let is_token = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii() && c > ' ' && c != '\x7f' && !"()<>@,;:\\\"/[]?=".contains(c));
    if is_token {
        return value.to_owned();
    }
    let escaped: String = value
        .chars()
        .flat_map(|c| {
            if c == '"' || c == '\\' {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect();
    format!("\"{escaped}\"")
}

fn add_style(dom: &mut Dom, head: NodeId, style: &Style) {
    let element = dom.new_element("style", &[("type", "text/css"), ("class", &style.class)]);
    let text = dom.new_text(&style.css);
    dom.append(element, text);
    dom.append(head, element);
}

/// Wraps the body contents in `div#book-columns > div#book-inner`.
fn kobo_divs(dom: &mut Dom, body: NodeId) {
    let wrapped = dom.children(body).into_iter().any(|c| {
        dom.attr(c, "id") == Some("book-columns")
            && dom
                .children(c)
                .into_iter()
                .any(|ci| dom.attr(ci, "id") == Some("book-inner"))
    });
    if wrapped {
        return;
    }
    for layer in ["book-inner", "book-columns"] {
        let div = dom.new_element("div", &[("id", layer)]);
        for child in dom.children(body) {
            dom.append(div, child);
        }
        dom.append(body, div);
    }
}

fn kobo_span(dom: &mut Dom, para: usize, seg: usize) -> NodeId {
    dom.new_element(
        "span",
        &[("class", "koboSpan"), ("id", &format!("kobo.{para}.{seg}"))],
    )
}

/// Adds `koboSpan`s around sentences and images, numbered per paragraph.
fn kobo_spans(dom: &mut Dom, body: NodeId, cjk: bool) -> usize {
    if descendants(dom, body).any(|id| has_class(dom, id, "koboSpan")) {
        return 0;
    }
    let (mut para, mut seg, mut spans) = (0, 0, 0);
    let mut next_para = false;
    let mut stack = vec![body];
    while let Some(cur) = stack.pop() {
        match &dom.node(cur).data {
            Data::Text(text) => {
                let text = text.clone();
                let parent = dom.node(cur).parent.expect("text in body has a parent");
                let under_p = dom.name(parent) == Some("p");
                for sentence in split_sentences(&text, cjk) {
                    let node = dom.new_text(sentence);
                    if is_space(sentence) && !under_p {
                        dom.insert_before(cur, node);
                        continue;
                    }
                    if next_para {
                        para += 1;
                        seg = 0;
                        next_para = false;
                    }
                    seg += 1;
                    spans += 1;
                    let span = kobo_span(dom, para, seg);
                    dom.append(span, node);
                    dom.insert_before(cur, span);
                }
                dom.detach(cur);
            }
            Data::Element { name, .. } => {
                let name = name.local.to_string();
                match name.as_str() {
                    "img" => {
                        para += 1;
                        seg = 1;
                        next_para = false;
                        spans += 1;
                        let span = kobo_span(dom, para, seg);
                        let image = dom.duplicate(cur);
                        dom.append(span, image);
                        dom.insert_before(cur, span);
                        dom.detach(cur);
                        continue;
                    }
                    n if NO_SPANS.contains(&n) => continue,
                    n if PARAGRAPHS.contains(&n) => next_para = true,
                    _ => {}
                }
                stack.extend(dom.children(cur).into_iter().rev());
            }
            _ => {}
        }
    }
    spans
}

fn punctuation(dom: &mut Dom, body: NodeId) {
    let mut stack = vec![body];
    while let Some(cur) = stack.pop() {
        match dom.data_mut(cur) {
            Data::Text(text) if !is_space(text) => *text = smarten(text),
            Data::Element { name, .. } if !NO_SMART.contains(&&*name.local) => {
                stack.extend(dom.children(cur).into_iter().rev());
            }
            _ => {}
        }
    }
}

/// Drops Adept meta tags, empty MS Office tags and replacement characters.
fn clean(dom: &mut Dom) {
    let mut stack = vec![DOCUMENT];
    while let Some(cur) = stack.pop() {
        match dom.data_mut(cur) {
            Data::Text(text) => {
                if text.contains('\u{fffd}') {
                    *text = text.replace('\u{fffd}', "");
                }
                continue;
            }
            Data::Element { .. } => {
                if is_removable(dom, cur) {
                    dom.detach(cur);
                    continue;
                }
            }
            Data::Document => {}
            _ => continue,
        }
        stack.extend(dom.children(cur).into_iter().rev());
    }
}

fn is_removable(dom: &Dom, id: NodeId) -> bool {
    let name = dom.name(id).unwrap_or_default();
    if name == "meta" {
        return dom
            .attr(id, "name")
            .is_some_and(|n| ADEPT_META.contains(&n));
    }
    (name == "o:p" || name.starts_with("st1:"))
        && dom
            .children(id)
            .into_iter()
            .all(|c| match &dom.node(c).data {
                Data::Comment(_) => true,
                Data::Text(text) => is_space(text),
                _ => false,
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(body: &str) -> String {
        let styles = [Style {
            class: KOBO_STYLE_CLASS.into(),
            css: KOBO_STYLE_CSS.into(),
        }];
        let config = Config {
            styles: &styles,
            smart_punctuation: false,
            cjk_sentences: false,
        };
        let html = transform(
            &format!("<html><head></head><body>{body}</body></html>"),
            &config,
        )
        .html;
        let open = r#"<div id="book-columns"><div id="book-inner">"#;
        let start = html.find(open).unwrap() + open.len();
        let end = html.rfind("</div></div></body>").unwrap();
        html[start..end].to_owned()
    }

    #[test]
    fn numbers_spans_like_kepubify() {
        assert_eq!(
            body("Loose<p>A. B</p><div>C</div><p><img src='x'/> D</p>"),
            concat!(
                r#"<span class="koboSpan" id="kobo.0.1">Loose</span>"#,
                r#"<p><span class="koboSpan" id="kobo.1.1">A. </span><span class="koboSpan" id="kobo.1.2">B</span></p>"#,
                r#"<div><span class="koboSpan" id="kobo.1.3">C</span></div>"#,
                r#"<p><span class="koboSpan" id="kobo.2.1"><img src="x"/></span><span class="koboSpan" id="kobo.2.2"> D</span></p>"#,
            )
        );
    }

    #[test]
    fn wraps_whitespace_only_directly_under_p() {
        assert_eq!(
            body("<p><b>a</b> <i>b</i></p>\n<div> </div>"),
            concat!(
                r#"<p><b><span class="koboSpan" id="kobo.1.1">a</span></b><span class="koboSpan" id="kobo.1.2"> </span>"#,
                r#"<i><span class="koboSpan" id="kobo.1.3">b</span></i></p>"#,
                "\n<div> </div>",
            )
        );
    }

    #[test]
    fn cleans_like_kepubify() {
        assert_eq!(
            body("<o:p> <!-- x --> </o:p><st1:place>Paris</st1:place>\u{fffd}"),
            concat!(
                r#"<st1:place><span class="koboSpan" id="kobo.0.1">Paris</span></st1:place>"#,
                r#"<span class="koboSpan" id="kobo.0.2"></span>"#,
            )
        );
    }

    #[test]
    fn rewrites_charset_declarations() {
        assert_eq!(
            utf8_media_type("Text/HTML; charset=ISO-8859-1").as_deref(),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(utf8_media_type("text/html; charset=utf-8"), None);
    }
}
