//! OCF container and OPF package handling: kepubify's TransformOPF and the
//! dummy title page fix.

use crate::dom::{self, Data};
use crate::xml::{self, Element, Item};

pub(crate) const TITLEPAGE_ID: &str = "kepubify-titlepage-dummy";
pub(crate) const TITLEPAGE_HREF: &str = "kepubify-titlepage-dummy.xhtml";
pub(crate) const TITLEPAGE_HTML: &str = r#"<!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml" lang="en"><head><title></title></head><body><p style="text-align: center; margin: 4em 0; font-size: .7em; font-style: italic;">Page intentionally left blank by kepubify.</p></body></html>"#;

const HTML_TYPES: &[&str] = &["application/xhtml+xml", "text/html"];
const HTML_EXTENSIONS: &[&str] = &["htm", "html", "xhtml"];

pub(crate) struct ManifestItem {
    pub id: String,
    pub href: String,
    pub media_type: String,
}

pub(crate) struct Package {
    pub items: Vec<ManifestItem>,
    pub spine: Vec<(String, bool)>,
}

/// Path of the first OPF package listed in `META-INF/container.xml`.
pub(crate) fn rootfile(container: &str) -> Option<String> {
    let doc = xml::parse(container).ok()?;
    doc.descendants()
        .into_iter()
        .find(|e| {
            e.tag == "rootfile" && e.attr("media-type") == Some("application/oebps-package+xml")
        })
        .and_then(|e| e.attr("full-path"))
        .map(str::to_owned)
}

pub(crate) fn package(opf: &str) -> Option<Package> {
    let doc = xml::parse(opf).ok()?;
    let root = doc.child_elements().find(|e| e.tag == "package")?;
    let section = |tag: &str, child: &str| -> Vec<&Element> {
        root.child_elements()
            .filter(|e| e.tag == tag)
            .flat_map(|e| e.child_elements())
            .filter(|e| e.tag == child)
            .collect()
    };
    let attr = |e: &Element, key: &str| e.attr(key).unwrap_or_default().to_owned();
    Some(Package {
        items: section("manifest", "item")
            .into_iter()
            .map(|e| ManifestItem {
                id: attr(e, "id"),
                href: attr(e, "href"),
                media_type: attr(e, "media-type"),
            })
            .collect(),
        spine: section("spine", "itemref")
            .into_iter()
            .map(|e| (attr(e, "idref"), e.attr("linear") != Some("no")))
            .collect(),
    })
}

pub(crate) fn content_documents(package: &Package, opf_path: &str) -> Vec<String> {
    package
        .items
        .iter()
        .filter(|item| {
            HTML_TYPES.contains(&item.media_type.as_str()) || has_html_extension(&item.href)
        })
        .map(|item| join(dir(opf_path), &item.href))
        .collect()
}

/// The first linear spine document, when it may need a title page in front.
pub(crate) fn titlepage_candidate(package: &Package, opf_path: &str) -> Option<String> {
    let idref = package
        .spine
        .iter()
        .find(|(_, linear)| *linear)
        .map(|(idref, _)| idref)?;
    let mut href = None;
    for item in package.items.iter().filter(|item| &item.id == idref) {
        if !has_html_extension(&item.href) {
            return None;
        }
        href = Some(&item.href);
    }
    let href = href?;
    let name = base(href).to_lowercase();
    if name.contains("cover") || name.contains("title") {
        return None;
    }
    Some(join(dir(opf_path), href))
}

/// kepubify's heuristic: the first document is not a bare title page.
pub(crate) fn needs_titlepage(html: &str) -> bool {
    const SKIPPED: &[&str] = &["script", "style", "pre", "audio", "video", "math"];
    let dom = dom::parse(html);
    let Some(body) = dom.find(dom::DOCUMENT, "body") else {
        return false;
    };
    let (mut paragraphs, mut words, mut images) = (0, 0, 0);
    let mut stack = vec![body];
    while let Some(cur) = stack.pop() {
        match &dom.node(cur).data {
            Data::Element { name, .. } => match &*name.local {
                "p" => {
                    paragraphs += 1;
                    if paragraphs > 4 {
                        return true;
                    }
                }
                "img" | "svg" => {
                    images += 1;
                    continue;
                }
                n if SKIPPED.contains(&n) => continue,
                _ => stack.extend(dom.children(cur).into_iter().rev()),
            },
            Data::Text(text) => {
                words += text.split_whitespace().filter(|w| w.len() > 3).count();
                if words > 20 {
                    return true;
                }
            }
            _ => {}
        }
    }
    (images == 0 && words < 5) || images > 4
}

/// Adds `cover-image` to the cover item, drops Calibre noise and optionally
/// registers the dummy title page first in the spine. `None` when the
/// package is not well-formed XML.
pub(crate) fn transform(opf: &str, titlepage: bool) -> Option<String> {
    let mut doc = xml::parse(opf).ok()?;

    let descendants = doc.descendants();
    let cover_id = descendants
        .iter()
        .find(|e| e.tag == "meta" && e.attr("name") == Some("cover"))
        .map_or("cover", |e| e.attr("content").unwrap_or("cover"))
        .to_owned();
    let cover = descendants
        .iter()
        .position(|e| e.attr("id") == Some(cover_id.as_str()));
    if let Some(element) = cover.and_then(|i| doc.descendant_mut(i)) {
        element.set_attr("", "properties", "cover-image");
    }
    doc.remove_where(&|e| {
        (e.tag == "meta" && e.attr("name") == Some("calibre:timestamp"))
            || (e.tag == "contributor" && e.attr("role") == Some("bkp"))
    });
    doc.indent(4);

    if titlepage && let Some(package) = doc.child_mut("package") {
        if let Some(manifest) = package.child_mut("manifest") {
            let mut item = Element::new(&manifest.space.clone(), "item");
            item.set_attr("", "id", TITLEPAGE_ID);
            item.set_attr("", "href", TITLEPAGE_HREF);
            item.set_attr("", "media-type", "application/xhtml+xml");
            manifest.children.push(Item::Element(item));
        }
        if let Some(spine) = package.child_mut("spine") {
            let mut itemref = Element::new(&spine.space.clone(), "itemref");
            itemref.set_attr("", "idref", TITLEPAGE_ID);
            spine.children.insert(0, Item::Element(itemref));
        }
        doc.indent(4);
    }

    Some(doc.write_document())
}

pub(crate) fn has_html_extension(href: &str) -> bool {
    base(href)
        .rsplit_once('.')
        .is_some_and(|(_, ext)| HTML_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

fn base(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

pub(crate) fn dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Go's `path.Join`: joins and lexically cleans a slash-separated path.
pub(crate) fn join(dir: &str, href: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in dir.split('/').chain(href.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|p| *p != "..") {
                    parts.pop();
                } else {
                    parts.push("..");
                }
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
  <meta name="cover" content="img"/>
  <dc:contributor opf:role="bkp">calibre</dc:contributor>
  <meta name="calibre:timestamp" content="x"/>
</metadata><manifest>
  <item id="img" href="c.jpg" media-type="image/jpeg"/>
  <item id="t" href="Text/one.xhtml" media-type="application/xhtml+xml"/>
</manifest><spine>
  <itemref idref="t"/>
</spine></package>"#;

    #[test]
    fn transforms_package() {
        let out = transform(OPF, true).unwrap();
        assert_eq!(
            out,
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
    <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
        <meta name="cover" content="img"/>
    </metadata>
    <manifest>
        <item id="img" href="c.jpg" media-type="image/jpeg" properties="cover-image"/>
        <item id="t" href="Text/one.xhtml" media-type="application/xhtml+xml"/>
        <item id="kepubify-titlepage-dummy" href="kepubify-titlepage-dummy.xhtml" media-type="application/xhtml+xml"/>
    </manifest>
    <spine>
        <itemref idref="kepubify-titlepage-dummy"/>
        <itemref idref="t"/>
    </spine>
</package>
"#
        );
    }

    #[test]
    fn finds_documents() {
        let package = package(OPF).unwrap();
        assert_eq!(
            content_documents(&package, "OEBPS/content.opf"),
            ["OEBPS/Text/one.xhtml"]
        );
        assert_eq!(
            titlepage_candidate(&package, "OEBPS/content.opf").as_deref(),
            Some("OEBPS/Text/one.xhtml")
        );
        let container = r#"<container version="1.0"><rootfiles><rootfile full-path="a/b.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        assert_eq!(rootfile(container).as_deref(), Some("a/b.opf"));
    }

    #[test]
    fn joins_like_go() {
        assert_eq!(join("", "a.xhtml"), "a.xhtml");
        assert_eq!(
            join("OEBPS/Text", "../Images/./x.png"),
            "OEBPS/Images/x.png"
        );
    }

    #[test]
    fn titlepage_heuristic() {
        assert!(needs_titlepage("<body><p>Short</p></body>"));
        assert!(!needs_titlepage(
            "<body><h1>The Long Title</h1><img src='a'/><p>by Somebody Important</p></body>"
        ));
        assert!(needs_titlepage(&format!(
            "<body>{}</body>",
            "<p>x</p>".repeat(5)
        )));
    }
}
