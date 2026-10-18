use std::io;
use std::path::{Path, PathBuf};

use crate::token::random_token;
use serde::Serialize;

/// Formats Nickel opens, by lowercase file-name suffix.
const SUPPORTED_SUFFIXES: &[&str] = &[".kepub.epub", ".epub", ".pdf", ".cbz", ".cbr", ".mobi", ".txt", ".html", ".rtf"];
const MAX_NAME_CHARS: usize = 120;

#[derive(Debug, Serialize)]
pub struct Added {
    pub name: String,
    pub converted: bool,
}

#[derive(Debug)]
pub enum FileError {
    UnsupportedFormat,
    InvalidName,
    Conversion(String),
    Io(io::Error),
}

impl From<io::Error> for FileError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Books added from the dashboard land in one folder of the user partition
/// (its root on the device); Nickel imports them when the plugin asks it to rescan.
#[derive(Debug, Clone)]
pub struct BookFiles {
    books_dir: PathBuf,
    rescan_trigger: PathBuf,
}

impl BookFiles {
    pub fn new(books_dir: PathBuf, rescan_trigger: PathBuf) -> Self {
        Self { books_dir, rescan_trigger }
    }

    /// Validates the name and returns a fresh temporary path to stream the upload into.
    pub fn staging_path(&self, name: &str) -> Result<(String, PathBuf), FileError> {
        let name = clean_name(name).ok_or(FileError::InvalidName)?;
        if suffix(&name).is_none() {
            return Err(FileError::UnsupportedFormat);
        }
        std::fs::create_dir_all(&self.books_dir)?;
        let staging = self.books_dir.join(format!(".relio-upload-{}", &random_token()[..12]));
        Ok((name, staging))
    }

    /// Moves a fully received upload into place, converting EPUB to KEPUB when asked.
    pub fn finish(&self, staging: &Path, name: &str, to_kepub: bool) -> Result<Added, FileError> {
        let result = self.place(staging, name, to_kepub);
        let _ = std::fs::remove_file(staging);
        if result.is_ok() {
            self.request_rescan();
        }
        result
    }

    fn place(&self, staging: &Path, name: &str, to_kepub: bool) -> Result<Added, FileError> {
        let convert = to_kepub && suffix(name) == Some(".epub");
        if !convert {
            let target = unique_path(&self.books_dir, name);
            std::fs::rename(staging, &target)?;
            return Ok(Added { name: file_name(&target), converted: false });
        }

        let kepub_name = file_name(&relio_kepub::output_path(Path::new(name)));
        let target = unique_path(&self.books_dir, &kepub_name);
        let partial = staging.with_extension("kepub");
        let converted = relio_kepub::convert_file(staging, &partial, &relio_kepub::Options::default())
            .map_err(|err| FileError::Conversion(err.to_string()))
            .and_then(|_| std::fs::rename(&partial, &target).map_err(FileError::Io));
        let _ = std::fs::remove_file(&partial);
        converted.map(|()| Added { name: file_name(&target), converted: true })
    }

    /// A file that is already gone still gets a rescan, which drops it from Nickel's library.
    pub fn delete(&self, path: &Path) -> io::Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
        self.request_rescan();
        Ok(())
    }

    fn request_rescan(&self) {
        if let Err(err) = std::fs::write(&self.rescan_trigger, b"") {
            eprintln!("cannot request a library rescan: {err}");
        }
    }
}

fn suffix(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    SUPPORTED_SUFFIXES.iter().copied().find(|s| lower.ends_with(s))
}

fn clean_name(name: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next()?;
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.');
    if cleaned.is_empty() || cleaned.chars().count() > MAX_NAME_CHARS {
        return None;
    }
    Some(cleaned.to_owned())
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let ext = suffix(name).unwrap_or("");
    let stem = &name[..name.len() - ext.len()];
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|path| !path.exists())
        .expect("an unused name exists")
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> (BookFiles, PathBuf) {
        let root = std::env::temp_dir().join(format!("reliod-files-{}", random_token()));
        std::fs::create_dir_all(&root).unwrap();
        (BookFiles::new(root.join("onboard"), root.join("rescan")), root)
    }

    #[test]
    fn keeps_only_the_base_name_and_rejects_hidden_or_unknown_files() {
        assert_eq!(clean_name("../../etc/Le Rouge: et le Noir.epub").as_deref(), Some("Le Rouge et le Noir.epub"));
        assert_eq!(clean_name("..").as_deref(), None);
        let (files, _) = files();
        assert!(matches!(files.staging_path("notes.exe"), Err(FileError::UnsupportedFormat)));
        assert!(matches!(files.staging_path("..."), Err(FileError::InvalidName)));
    }

    #[test]
    fn never_overwrites_an_existing_book() {
        let (files, root) = files();
        for _ in 0..2 {
            let (name, staging) = files.staging_path("Candide.pdf").unwrap();
            std::fs::write(&staging, b"%PDF").unwrap();
            files.finish(&staging, &name, true).unwrap();
        }
        assert!(root.join("onboard/Candide.pdf").exists());
        assert!(root.join("onboard/Candide (2).pdf").exists());
        assert!(root.join("rescan").exists());
    }

    fn tiny_epub() -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let entries = [
            ("mimetype", "application/epub+zip"),
            ("META-INF/container.xml", r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#),
            ("content.opf", r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Candide</dc:title><dc:identifier id="id">x</dc:identifier></metadata><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c"/></spine></package>"#),
            ("c.xhtml", r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c</title></head><body><p>Il faut cultiver notre jardin.</p></body></html>"#),
        ];
        for (name, body) in entries {
            zip.start_file(name, stored).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn converts_epub_uploads_to_kepub() {
        let (files, root) = files();
        let (name, staging) = files.staging_path("Candide.epub").unwrap();
        std::fs::write(&staging, tiny_epub()).unwrap();
        let added = files.finish(&staging, &name, true).unwrap();
        assert_eq!(added.name, "Candide.kepub.epub");
        assert!(added.converted);
        let kepub = std::fs::File::open(root.join("onboard/Candide.kepub.epub")).unwrap();
        let mut chapter = String::new();
        std::io::Read::read_to_string(&mut zip::ZipArchive::new(kepub).unwrap().by_name("c.xhtml").unwrap(), &mut chapter).unwrap();
        assert!(chapter.contains("koboSpan"));
        assert_eq!(std::fs::read_dir(root.join("onboard")).unwrap().count(), 1);
    }

    #[test]
    fn reports_broken_epubs_without_leaving_files_behind() {
        let (files, root) = files();
        let (name, staging) = files.staging_path("broken.epub").unwrap();
        std::fs::write(&staging, b"not a zip").unwrap();
        assert!(matches!(files.finish(&staging, &name, true), Err(FileError::Conversion(_))));
        assert_eq!(std::fs::read_dir(root.join("onboard")).unwrap().count(), 0);
    }
}
