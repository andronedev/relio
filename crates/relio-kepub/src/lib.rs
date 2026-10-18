//! Native EPUB to Kobo KEPUB conversion, porting kepubify v4.
//!
//! Content documents get `koboSpan` segments, the `book-columns`/`book-inner`
//! wrappers and the Kobo style hacks; the OPF gets the cover-image property;
//! every other entry is copied verbatim.

mod charset;
mod content;
mod dom;
mod opf;
mod sentences;
mod smartypants;
mod xml;

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};

use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub use charset::Charset;
use content::{KOBO_STYLE_CLASS, KOBO_STYLE_CSS, Style};

const MIMETYPE: &[u8] = b"application/epub+zip";
const CONTAINER: &str = "META-INF/container.xml";
const FILTERED_NAMES: &[&str] = &[
    "calibre_bookmarks.txt",
    "iTunesMetadata.plist",
    "iTunesArtwork.plist",
    ".DS_STORE",
    "thumbs.db",
    "Thumbs.db",
];

const CSS_HYPHENATE: &str = "* {
    -webkit-hyphens: auto;
    -moz-hyphens: auto;
    hyphens: auto;

    -webkit-hyphenate-limit-after: 3;
    -webkit-hyphenate-limit-before: 3;
    -webkit-hyphenate-limit-lines: 2;
}

h1, h2, h3, h4, h5, h6, td {
    -moz-hyphens: none !important;
    -webkit-hyphens: none !important;
    hyphens: none !important;
}";

const CSS_NO_HYPHENATE: &str = "* {
    -moz-hyphens: none !important;
    -webkit-hyphens: none !important;
    hyphens: none !important;
}";

const CSS_FULLSCREEN_FIXES: &str = "body {
    margin: 0 !important;
    padding: 0 !important;
}

body>div {
    padding-left: 0.2em !important;
    padding-right: 0.2em !important;
}";

/// Conversion options, mirroring kepubify's command-line flags.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// `--smarten-punctuation`
    pub smart_punctuation: bool,
    /// `--css`, in order.
    pub extra_css: Vec<String>,
    /// `--hyphenate` (`Some(true)`) or `--no-hyphenate` (`Some(false)`).
    pub hyphenate: Option<bool>,
    /// `--fullscreen-reading-fixes`
    pub fullscreen_fixes: bool,
    /// `--add-dummy-titlepage` / `--no-add-dummy-titlepage`; `None` uses the heuristic.
    pub dummy_titlepage: Option<bool>,
    /// `--replace find|replace`, applied in order to the generated HTML.
    pub replacements: Vec<(String, String)>,
    /// `--charset`
    pub charset: Charset,
    /// Also end sentences on full-width CJK punctuation (not in kepubify).
    pub cjk_sentences: bool,
}

impl Options {
    fn styles(&self) -> Vec<Style> {
        let style = |class: &str, css: &str| Style {
            class: class.to_owned(),
            css: css.to_owned(),
        };
        let mut styles = vec![style(KOBO_STYLE_CLASS, KOBO_STYLE_CSS)];
        styles.extend(
            self.extra_css
                .iter()
                .map(|css| style("kepubify-extracss", css)),
        );
        match self.hyphenate {
            Some(true) => styles.push(style("kepubify-hyphenate", CSS_HYPHENATE)),
            Some(false) => styles.push(style("kepubify-nohyphenate", CSS_NO_HYPHENATE)),
            None => {}
        }
        if self.fullscreen_fixes {
            styles.push(style("kepubify-fullscreenfixes", CSS_FULLSCREEN_FIXES));
        }
        styles
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    /// Content documents rewritten.
    pub documents: usize,
    /// `koboSpan` elements added across all documents.
    pub spans: usize,
    /// Junk entries dropped (Calibre bookmarks, OS metadata).
    pub removed: usize,
    /// Whether the dummy title page was added.
    pub titlepage: bool,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Zip(ZipError),
    UnknownCharset(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "i/o error: {e}"),
            Self::Zip(e) => write!(f, "invalid epub archive: {e}"),
            Self::UnknownCharset(label) => write!(f, "unknown charset {label:?}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Zip(e) => Some(e),
            Self::UnknownCharset(_) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<ZipError> for Error {
    fn from(e: ZipError) -> Self {
        Self::Zip(e)
    }
}

pub fn convert(input: impl Read + Seek, output: impl Write + Seek) -> Result<Stats, Error> {
    convert_with(input, output, &Options::default())
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Skip,
    Copy,
    Package,
    Content,
}

pub fn convert_with(
    input: impl Read + Seek,
    output: impl Write + Seek,
    options: &Options,
) -> Result<Stats, Error> {
    let mut archive = ZipArchive::new(input)?;
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index_raw(i).map(|f| f.name().to_owned()))
        .collect::<Result<_, _>>()?;
    let mut stats = Stats::default();

    let opf_path = read_text(&mut archive, CONTAINER)?.and_then(|c| opf::rootfile(&c));
    let opf_text = match &opf_path {
        Some(path) => read_text(&mut archive, path)?,
        None => None,
    };

    let mut actions = vec![Action::Copy; names.len()];
    let package = opf_text.as_deref().and_then(opf::package);
    match (&opf_path, &package) {
        (Some(path), Some(package)) => {
            let index: HashMap<&str, usize> = names
                .iter()
                .enumerate()
                .map(|(i, n)| (n.as_str(), i))
                .collect();
            if let Some(&i) = index.get(path.as_str()) {
                actions[i] = Action::Package;
            }
            for doc in opf::content_documents(package, path) {
                let found = index.get(doc.as_str()).copied().or_else(|| {
                    let lower = doc.to_lowercase();
                    names.iter().position(|n| n.to_lowercase() == lower)
                });
                if let Some(i) = found {
                    actions[i] = Action::Content;
                }
            }
        }
        _ => {
            for (i, name) in names.iter().enumerate() {
                if opf::has_html_extension(name) {
                    actions[i] = Action::Content;
                }
            }
        }
    }
    for (i, name) in names.iter().enumerate() {
        if name.ends_with('/') || name == "mimetype" {
            actions[i] = Action::Skip;
        } else if is_filtered(name) {
            actions[i] = Action::Skip;
            stats.removed += 1;
        }
    }

    if let (Some(path), Some(package)) = (&opf_path, &package) {
        stats.titlepage = match options.dummy_titlepage {
            Some(add) => add,
            None => match opf::titlepage_candidate(package, path) {
                Some(doc) => read_bytes(&mut archive, &doc)?
                    .map(|bytes| opf::needs_titlepage(&charset::decode_utf8(&bytes).text))
                    .unwrap_or(false),
                None => false,
            },
        };
    }

    let mut writer = ZipWriter::new(output);
    writer.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )?;
    writer.write_all(MIMETYPE)?;

    for (i, _) in actions
        .iter()
        .enumerate()
        .filter(|(_, a)| **a == Action::Copy)
    {
        writer.raw_copy_file(archive.by_index_raw(i)?)?;
    }

    let styles = options.styles();
    let config = content::Config {
        styles: &styles,
        smart_punctuation: options.smart_punctuation,
        cjk_sentences: options.cjk_sentences,
    };
    for (i, action) in actions.iter().enumerate() {
        match action {
            Action::Package => {
                let opf_path = opf_path.as_deref().unwrap_or_default();
                let text = opf_text.as_deref().unwrap_or_default();
                let (name, options) = replaced_entry(&mut archive, i)?;
                match opf::transform(text, stats.titlepage) {
                    Some(opf) => {
                        writer.start_file(name, options)?;
                        writer.write_all(opf.as_bytes())?;
                    }
                    None => writer.raw_copy_file(archive.by_index_raw(i)?)?,
                }
                if stats.titlepage {
                    let deflated = SimpleFileOptions::default()
                        .compression_method(CompressionMethod::Deflated)
                        .unix_permissions(0o666);
                    writer
                        .start_file(opf::join(opf::dir(opf_path), opf::TITLEPAGE_HREF), deflated)?;
                    writer.write_all(opf::TITLEPAGE_HTML.as_bytes())?;
                }
            }
            Action::Content => {
                let bytes = read_index(&mut archive, i)?;
                let decoded =
                    charset::decode(&bytes, &options.charset).map_err(Error::UnknownCharset)?;
                let converted = content::transform(&decoded.text, &config);
                let mut html = charset::encode(&converted.html, decoded.lossless_bytes);
                for (find, replace) in &options.replacements {
                    html = replace_bytes(&html, find.as_bytes(), replace.as_bytes());
                }
                stats.documents += 1;
                stats.spans += converted.spans;
                let (name, options) = replaced_entry(&mut archive, i)?;
                writer.start_file(name, options)?;
                writer.write_all(&html)?;
            }
            Action::Skip | Action::Copy => {}
        }
    }

    writer.finish()?.flush()?;
    Ok(stats)
}

pub fn convert_file(input: &Path, output: &Path, options: &Options) -> Result<Stats, Error> {
    let reader = BufReader::new(File::open(input)?);
    let writer = BufWriter::new(File::create(output)?);
    convert_with(reader, writer, options)
}

/// `book.epub` becomes `book.kepub.epub`, next to the original.
pub fn output_path(input: &Path) -> PathBuf {
    let name = input.file_name().and_then(|n| n.to_str()).unwrap_or("book");
    let lower = name.to_ascii_lowercase();
    let stem = [".kepub.epub", ".epub"]
        .iter()
        .find(|ext| lower.ends_with(*ext))
        .map_or(name, |ext| &name[..name.len() - ext.len()]);
    input.with_file_name(format!("{stem}.kepub.epub"))
}

fn is_filtered(name: &str) -> bool {
    let base = name.rsplit('/').next().unwrap_or(name);
    FILTERED_NAMES.contains(&base) || opf::dir(name) == "__MACOSX"
}

/// Name and header options for an entry whose content is replaced: same
/// metadata, always deflated (as kepubify's zipReplace).
fn replaced_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
) -> Result<(String, SimpleFileOptions), Error> {
    let file = archive.by_index_raw(index)?;
    let mut options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    if let Some(modified) = file.last_modified() {
        options = options.last_modified_time(modified);
    }
    if let Some(mode) = file.unix_mode() {
        options = options.unix_permissions(mode);
    }
    Ok((file.name().to_owned(), options))
}

fn read_index<R: Read + Seek>(archive: &mut ZipArchive<R>, index: usize) -> Result<Vec<u8>, Error> {
    let mut file = archive.by_index(index)?;
    let mut bytes = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn read_bytes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<Option<Vec<u8>>, Error> {
    match archive.index_for_name(name) {
        Some(index) => read_index(archive, index).map(Some),
        None => Ok(None),
    }
}

fn read_text<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<Option<String>, Error> {
    Ok(read_bytes(archive, name)?.and_then(|bytes| String::from_utf8(bytes).ok()))
}

fn replace_bytes(haystack: &[u8], find: &[u8], replace: &[u8]) -> Vec<u8> {
    if find.is_empty() {
        return haystack.to_vec();
    }
    let mut out = Vec::with_capacity(haystack.len());
    let mut rest = haystack;
    while let Some(pos) = rest.windows(find.len()).position(|w| w == find) {
        out.extend_from_slice(&rest[..pos]);
        out.extend_from_slice(replace);
        rest = &rest[pos + find.len()..];
    }
    out.extend_from_slice(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_path_replaces_extension() {
        assert_eq!(
            output_path(Path::new("/b/Dune.epub")),
            Path::new("/b/Dune.kepub.epub")
        );
        assert_eq!(
            output_path(Path::new("Dune.KEPUB.EPUB")),
            Path::new("Dune.kepub.epub")
        );
        assert_eq!(
            output_path(Path::new("notes")),
            Path::new("notes.kepub.epub")
        );
    }

    #[test]
    fn filters_junk_like_kepubify() {
        assert!(is_filtered("META-INF/calibre_bookmarks.txt"));
        assert!(is_filtered("__MACOSX/._x"));
        assert!(!is_filtered("__MACOSX/a/b"));
        assert!(!is_filtered("OEBPS/text.xhtml"));
    }

    #[test]
    fn replaces_bytes_in_order() {
        assert_eq!(replace_bytes(b"aXbXc", b"X", b"--"), b"a--b--c");
    }
}
