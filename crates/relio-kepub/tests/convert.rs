use std::io::{Cursor, Read, Write};

use relio_kepub::{Options, Stats, convert, convert_with};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles>
<rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
</rootfiles></container>"#;

const IMAGE: &[u8] = &[0x89, b'P', b'N', b'G', 0, 1, 2, 3, 255];

fn opf(documents: &[&str]) -> String {
    let items: String = documents
        .iter()
        .enumerate()
        .map(|(i, href)| {
            format!(r#"<item id="c{i}" href="{href}" media-type="application/xhtml+xml"/>"#)
        })
        .collect();
    let spine: String = (0..documents.len())
        .map(|i| format!(r#"<itemref idref="c{i}"/>"#))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0"><metadata><meta name="cover" content="img"/></metadata>
<manifest><item id="img" href="cover.png" media-type="image/png"/>{items}</manifest><spine>{spine}</spine></package>"#
    )
}

fn chapter(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title></head><body>{body}</body></html>"
    )
}

fn epub(documents: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer.start_file("mimetype", stored).unwrap();
    writer.write_all(b"application/epub+zip").unwrap();
    let hrefs: Vec<&str> = documents.iter().map(|(name, _)| *name).collect();
    let package = opf(&hrefs);
    let mut entries = vec![
        ("META-INF/container.xml".to_owned(), CONTAINER.as_bytes()),
        ("OEBPS/content.opf".to_owned(), package.as_bytes()),
        ("OEBPS/cover.png".to_owned(), IMAGE),
    ];
    entries.extend(
        documents
            .iter()
            .map(|(name, bytes)| (format!("OEBPS/{name}"), *bytes)),
    );
    for (name, bytes) in entries {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn run(input: &[u8], options: &Options) -> (Vec<u8>, Stats) {
    let mut output = Cursor::new(Vec::new());
    let stats = convert_with(Cursor::new(input), &mut output, options).unwrap();
    (output.into_inner(), stats)
}

fn no_titlepage() -> Options {
    Options {
        dummy_titlepage: Some(false),
        ..Options::default()
    }
}

fn entry(archive: &[u8], name: &str) -> Vec<u8> {
    let mut zip = ZipArchive::new(Cursor::new(archive)).unwrap();
    let mut bytes = Vec::new();
    zip.by_name(name).unwrap().read_to_end(&mut bytes).unwrap();
    bytes
}

fn text(archive: &[u8], name: &str) -> String {
    String::from_utf8(entry(archive, name)).unwrap()
}

#[test]
fn adds_spans_wrappers_and_style() {
    let body = "<h1>Title</h1><p>Hello world. How are you? <em>Fine</em>, thanks!</p><p>Second <img src=\"cover.png\"/> one</p>";
    let (out, stats) = run(
        &epub(&[("ch1.xhtml", chapter(body).as_bytes())]),
        &no_titlepage(),
    );

    let html = text(&out, "OEBPS/ch1.xhtml");
    assert!(html.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?><!DOCTYPE html>"));
    assert!(html.contains(
        r#"<title>T</title><style type="text/css" class="kobostylehacks">div#book-inner { margin-top: 0; margin-bottom: 0;}</style></head>"#
    ));
    assert!(html.contains(r#"<body><div id="book-columns"><div id="book-inner"><h1>"#));
    assert!(html.ends_with("</p></div></div></body></html>"));
    assert!(html.contains(r#"<h1><span class="koboSpan" id="kobo.1.1">Title</span></h1>"#));
    assert!(html.contains(concat!(
        r#"<p><span class="koboSpan" id="kobo.2.1">Hello world. </span>"#,
        r#"<span class="koboSpan" id="kobo.2.2">How are you? </span>"#,
        r#"<em><span class="koboSpan" id="kobo.2.3">Fine</span></em>"#,
        r#"<span class="koboSpan" id="kobo.2.4">, thanks!</span></p>"#,
    )));
    assert!(html.contains(concat!(
        r#"<p><span class="koboSpan" id="kobo.3.1">Second </span>"#,
        r#"<span class="koboSpan" id="kobo.4.1"><img src="cover.png"/></span>"#,
        r#"<span class="koboSpan" id="kobo.4.2"> one</span></p>"#,
    )));
    assert_eq!(
        stats,
        Stats {
            documents: 1,
            spans: 8,
            removed: 0,
            titlepage: false
        }
    );
}

#[test]
fn marks_cover_and_keeps_resources() {
    let (out, _) = run(
        &epub(&[("ch1.xhtml", chapter("<p>Text.</p>").as_bytes())]),
        &no_titlepage(),
    );

    let mut zip = ZipArchive::new(Cursor::new(&out)).unwrap();
    let mimetype = zip.by_index(0).unwrap();
    assert_eq!(mimetype.name(), "mimetype");
    assert_eq!(mimetype.compression(), CompressionMethod::Stored);
    drop(mimetype);

    assert!(text(&out, "OEBPS/content.opf").contains(
        r#"<item id="img" href="cover.png" media-type="image/png" properties="cover-image"/>"#
    ));
    assert_eq!(entry(&out, "OEBPS/cover.png"), IMAGE);
    assert_eq!(entry(&out, "META-INF/container.xml"), CONTAINER.as_bytes());
}

#[test]
fn adds_dummy_titlepage_before_a_short_first_page() {
    let (out, stats) = run(
        &epub(&[("ch1.xhtml", chapter("<p>Short.</p>").as_bytes())]),
        &Options::default(),
    );

    assert!(stats.titlepage);
    assert!(text(&out, "OEBPS/content.opf").contains(
        "<itemref idref=\"kepubify-titlepage-dummy\"/>\n        <itemref idref=\"c0\"/>"
    ));
    assert!(
        text(&out, "OEBPS/kepubify-titlepage-dummy.xhtml")
            .contains("Page intentionally left blank")
    );
}

#[test]
fn reconversion_keeps_spans_and_wrappers() {
    let source = chapter("<p>One. Two.</p><p>Three</p>");
    let (once, first) = run(&epub(&[("ch1.xhtml", source.as_bytes())]), &no_titlepage());
    let (twice, second) = run(&once, &no_titlepage());

    let html = text(&twice, "OEBPS/ch1.xhtml");
    assert_eq!(first.spans, 3);
    assert_eq!(second.spans, 0);
    assert_eq!(html.matches("koboSpan").count(), 3);
    assert_eq!(html.matches("book-columns").count(), 1);
    assert_eq!(
        text(&once, "OEBPS/content.opf"),
        text(&twice, "OEBPS/content.opf")
    );
}

#[test]
fn preserves_non_ascii_text() {
    let body = "<p>Ça va ? Très bien… Merci « Zoé ». 日本語のテキスト。 Ελληνικά! 🙂 Fin</p>";
    let (out, _) = run(
        &epub(&[("ch.xhtml", chapter(body).as_bytes())]),
        &no_titlepage(),
    );

    let html = text(&out, "OEBPS/ch.xhtml");
    assert!(html.contains(r#"<span class="koboSpan" id="kobo.1.1">Ça va ? </span>"#));
    assert!(
        html.contains(r#"<span class="koboSpan" id="kobo.1.2">Très bien… Merci « Zoé ». </span>"#)
    );
    assert!(html.contains(r#"<span class="koboSpan" id="kobo.1.4">🙂 Fin</span>"#));

    let cjk = Options {
        cjk_sentences: true,
        ..no_titlepage()
    };
    let (out, _) = run(
        &epub(&[("ch.xhtml", chapter("<p>一つ目。二つ目！</p>").as_bytes())]),
        &cjk,
    );
    assert!(
        text(&out, "OEBPS/ch.xhtml")
            .contains(r#"<span class="koboSpan" id="kobo.1.2">二つ目！</span>"#)
    );
}

#[test]
fn repairs_malformed_documents() {
    let broken = "<html><head><title/></head><body><p>Unclosed <b>bold. 1 < 2 & true<p>Next<a id=\"x\"/> after";
    let latin1: &[u8] = b"<html><body><p>Caf\xe9</p></body></html>";
    let (out, stats) = run(
        &epub(&[
            ("a.html", broken.as_bytes()),
            ("b.htm", latin1),
            ("c.xhtml", b""),
        ]),
        &no_titlepage(),
    );

    let html = text(&out, "OEBPS/a.html");
    assert!(html.contains("<title></title>"));
    assert!(
        html.contains(r#"<span class="koboSpan" id="kobo.1.3">1 &lt; 2 &amp; true</span></b></p>"#)
    );
    assert!(html.contains(r#"<a id="x"></a><span class="koboSpan" id="kobo.2.2"> after</span>"#));
    assert!(html.ends_with("</div></div></body></html>"));
    assert_eq!(
        entry(&out, "OEBPS/b.htm")
            .windows(4)
            .filter(|w| w == b"Caf\xe9")
            .count(),
        1
    );
    assert!(
        text(&out, "OEBPS/c.xhtml")
            .contains(r#"<div id="book-columns"><div id="book-inner"></div></div>"#)
    );
    assert_eq!(stats.documents, 3);
}

#[test]
fn rejects_non_zip_input() {
    let mut output = Cursor::new(Vec::new());
    assert!(convert(Cursor::new(b"not an epub".to_vec()), &mut output).is_err());
}
