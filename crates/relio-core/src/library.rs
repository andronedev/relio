use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, Row, params};
use serde::Serialize;

use crate::schema::Schema;

const BOOK_CONTENT_TYPE: i64 = 6;
const FILE_PREFIX: &str = "file://";
const ONBOARD_PREFIX: &str = "file:///mnt/onboard/";
const EXTERNAL_PREFIX: &str = "file:///mnt/sd/";
const EXTERNAL_ROOT: &str = "/mnt/sd";
/// Nickel stores booleans as the text `true`/`false`; older rows may hold integers.
const STORE_DOWNLOADS: &str = ".kobo/kepub";
const NOT_HIDDEN: &str = "AND COALESCE(b.Hidden, 'false') NOT IN (1, 'true')";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadStatus {
    Unread,
    Reading,
    Finished,
}

impl ReadStatus {
    fn from_db(value: Option<i64>) -> Self {
        match value {
            Some(1) => Self::Reading,
            Some(2) => Self::Finished,
            _ => Self::Unread,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Book {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub series: Option<String>,
    pub series_number: Option<String>,
    pub language: Option<String>,
    pub isbn: Option<String>,
    pub format: Format,
    pub status: ReadStatus,
    pub percent_read: u8,
    pub last_read_at: Option<String>,
    pub added_at: Option<String>,
    pub seconds_spent_reading: u64,
    pub seconds_remaining: Option<u64>,
    pub file_size: Option<u64>,
    pub sideloaded: bool,
    /// Whether the book has a file Relio can download or delete.
    pub has_file: bool,
    pub annotation_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Kepub,
    Epub,
    Pdf,
    Cbz,
    Other,
}

impl Format {
    fn from_mime(mime: &str) -> Self {
        match mime {
            "application/x-kobo-epub+zip" => Self::Kepub,
            "application/epub+zip" => Self::Epub,
            "application/pdf" => Self::Pdf,
            "application/x-cbz" => Self::Cbz,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    Highlight,
    Note,
    Bookmark,
    Markup,
}

impl AnnotationKind {
    fn from_db(kind: Option<&str>, annotation: Option<&str>) -> Self {
        match kind {
            Some("note") => Self::Note,
            Some("dogear") => Self::Bookmark,
            Some("markup") => Self::Markup,
            Some(_) => Self::Highlight,
            None if annotation.is_some_and(|a| !a.is_empty()) => Self::Note,
            None => Self::Highlight,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Annotation {
    pub id: String,
    pub book_id: String,
    pub book_title: Option<String>,
    pub kind: AnnotationKind,
    pub text: Option<String>,
    pub note: Option<String>,
    pub color: u8,
    pub chapter_progress: Option<f64>,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Shelf {
    pub id: String,
    pub name: String,
    pub book_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Stats {
    pub books: u32,
    pub reading: u32,
    pub finished: u32,
    pub annotations: u32,
    pub seconds_spent_reading: u64,
}

impl Stats {
    pub fn of(books: &[Book]) -> Self {
        Self {
            books: books.len() as u32,
            reading: books.iter().filter(|b| b.status == ReadStatus::Reading).count() as u32,
            finished: books.iter().filter(|b| b.status == ReadStatus::Finished).count() as u32,
            annotations: books.iter().map(|b| b.annotation_count).sum(),
            seconds_spent_reading: books.iter().map(|b| b.seconds_spent_reading).sum(),
        }
    }
}

/// Read-only view of `KoboReader.sqlite`. A connection is opened per call so the
/// database is never held while Nickel writes or checkpoints it.
#[derive(Debug, Clone)]
pub struct Library {
    db_path: PathBuf,
}

impl Library {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    fn open(&self) -> rusqlite::Result<(Connection, Schema)> {
        let conn = Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_secs(3))?;
        let schema = Schema::load(&conn)?;
        Ok((conn, schema))
    }

    pub fn books(&self) -> rusqlite::Result<Vec<Book>> {
        let (conn, s) = self.open()?;
        let c = |name| s.col("content", "c", name);
        let downloaded = if s.has("content", "IsDownloaded") {
            "AND c.IsDownloaded IN (1, 'true')"
        } else {
            ""
        };
        let annotation_count = if s.has_table("Bookmark") {
            let hidden = if s.has("Bookmark", "Hidden") { NOT_HIDDEN } else { "" };
            format!("(SELECT COUNT(*) FROM Bookmark b WHERE b.VolumeID = c.ContentID {hidden})")
        } else {
            "0".to_owned()
        };
        let sql = format!(
            "SELECT c.ContentID, c.Title, c.Attribution, c.Publisher, {series}, {series_number},
                    {language}, {isbn}, c.MimeType, c.ReadStatus, {percent}, c.DateLastRead,
                    {added}, {spent}, {remaining}, {size}, {annotation_count}
             FROM content c
             WHERE c.ContentType = ?1 {downloaded}
             ORDER BY c.DateLastRead IS NULL, c.DateLastRead DESC, c.Title",
            series = c("Series"),
            series_number = c("SeriesNumber"),
            language = c("Language"),
            isbn = c("ISBN"),
            percent = c("___PercentRead"),
            added = c("DateAdded"),
            spent = c("TimeSpentReading"),
            remaining = c("RestOfBookEstimate"),
            size = c("___FileSize"),
        );
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(params![BOOK_CONTENT_TYPE], book_from_row)?
            .collect()
    }

    pub fn annotations(&self, book_id: Option<&str>) -> rusqlite::Result<Vec<Annotation>> {
        let (conn, s) = self.open()?;
        if !s.has_table("Bookmark") {
            return Ok(Vec::new());
        }
        let b = |name| s.col("Bookmark", "b", name);
        let hidden = if s.has("Bookmark", "Hidden") { NOT_HIDDEN } else { "" };
        let sql = format!(
            "SELECT b.BookmarkID, b.VolumeID, v.Title, {kind}, b.Text, b.Annotation, {color},
                    {progress}, b.DateCreated, {modified}
             FROM Bookmark b
             LEFT JOIN content v ON v.ContentID = b.VolumeID AND v.ContentType = {BOOK_CONTENT_TYPE}
             WHERE (?1 IS NULL OR b.VolumeID = ?1) {hidden}
             ORDER BY b.VolumeID, {progress}, b.DateCreated",
            kind = b("Type"),
            color = b("Color"),
            progress = b("ChapterProgress"),
            modified = b("DateModified"),
        );
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(params![book_id], annotation_from_row)?
            .collect()
    }

    pub fn shelves(&self) -> rusqlite::Result<Vec<Shelf>> {
        let (conn, s) = self.open()?;
        if !s.has_table("Shelf") || !s.has_table("ShelfContent") {
            return Ok(Vec::new());
        }
        let shelf_id = if s.has("Shelf", "Id") { "COALESCE(s.Id, s.InternalName)" } else { "s.InternalName" };
        let sql = format!(
            "SELECT {shelf_id}, s.Name, sc.ContentId
             FROM Shelf s
             LEFT JOIN ShelfContent sc
                ON sc.ShelfName = s.InternalName AND COALESCE(sc._IsDeleted, 'false') NOT IN (1, 'true')
             WHERE COALESCE(s._IsDeleted, 'false') NOT IN (1, 'true')
             ORDER BY s.Name COLLATE NOCASE"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        let mut shelves: Vec<Shelf> = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let name: String = row.get::<_, Option<String>>(1)?.unwrap_or_else(|| id.clone());
            let book_id: Option<String> = row.get(2)?;
            if shelves.last().is_none_or(|last| last.id != id) {
                shelves.push(Shelf { id, name, book_ids: Vec::new() });
            }
            if let (Some(shelf), Some(book_id)) = (shelves.last_mut(), book_id) {
                shelf.book_ids.push(book_id);
            }
        }
        Ok(shelves)
    }

    /// Books Nickel can actually open: sideloaded entries whose file is gone stay in
    /// the database but Nickel hides them, so they are left out here too.
    pub fn books_on_device(&self, onboard_root: &Path) -> rusqlite::Result<Vec<Book>> {
        let mut books = self.books()?;
        books.retain(|book| {
            !book.sideloaded || self.book_file(&book.id, onboard_root).is_some_and(|path| path.exists())
        });
        Ok(books)
    }

    pub fn image_id(&self, book_id: &str) -> rusqlite::Result<Option<String>> {
        let (conn, _) = self.open()?;
        conn.query_row("SELECT ImageId FROM content WHERE ContentID = ?1", params![book_id], |row| row.get(0))
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                err => Err(err),
            })
    }

    /// The book's file on the device: its own path for books copied onto the user
    /// partition or the external storage, Nickel's download cache for store purchases.
    pub fn book_file(&self, book_id: &str, onboard_root: &Path) -> Option<PathBuf> {
        if let Some(relative) = book_id.strip_prefix(ONBOARD_PREFIX) {
            return safe_join(onboard_root, relative);
        }
        if let Some(relative) = book_id.strip_prefix(EXTERNAL_PREFIX) {
            return safe_join(Path::new(EXTERNAL_ROOT), relative);
        }
        is_store_id(book_id).then(|| onboard_root.join(STORE_DOWNLOADS).join(book_id))
    }
}

fn has_file(id: &str) -> bool {
    id.starts_with(ONBOARD_PREFIX) || id.starts_with(EXTERNAL_PREFIX) || is_store_id(id)
}

fn safe_join(base: &Path, relative: &str) -> Option<PathBuf> {
    let relative = Path::new(relative);
    let is_safe = relative
        .components()
        .all(|part| matches!(part, std::path::Component::Normal(_)));
    is_safe.then(|| base.join(relative))
}

fn is_store_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn non_negative(value: Option<i64>) -> Option<u64> {
    value.and_then(|v| u64::try_from(v).ok())
}

fn book_from_row(row: &Row) -> rusqlite::Result<Book> {
    let id: String = row.get(0)?;
    let mime: String = row.get(8)?;
    Ok(Book {
        sideloaded: id.starts_with(FILE_PREFIX),
        has_file: has_file(&id),
        title: row.get::<_, Option<String>>(1)?.unwrap_or_else(|| title_from_path(&id)),
        author: row.get(2)?,
        publisher: row.get(3)?,
        series: row.get(4)?,
        series_number: row.get(5)?,
        language: row.get(6)?,
        isbn: row.get(7)?,
        format: Format::from_mime(&mime),
        status: ReadStatus::from_db(row.get(9)?),
        percent_read: row.get::<_, Option<i64>>(10)?.unwrap_or(0).clamp(0, 100) as u8,
        last_read_at: row.get(11)?,
        added_at: row.get(12)?,
        seconds_spent_reading: non_negative(row.get(13)?).unwrap_or(0),
        seconds_remaining: non_negative(row.get(14)?),
        file_size: non_negative(row.get(15)?),
        annotation_count: row.get(16)?,
        id,
    })
}

fn annotation_from_row(row: &Row) -> rusqlite::Result<Annotation> {
    let kind: Option<String> = row.get(3)?;
    let note: Option<String> = row.get::<_, Option<String>>(5)?.filter(|n| !n.is_empty());
    Ok(Annotation {
        id: row.get(0)?,
        book_id: row.get(1)?,
        book_title: row.get(2)?,
        kind: AnnotationKind::from_db(kind.as_deref(), note.as_deref()),
        text: row.get(4)?,
        note,
        color: row.get::<_, Option<i64>>(6)?.unwrap_or(0).clamp(0, 255) as u8,
        chapter_progress: row.get(7)?,
        created_at: row.get(8)?,
        modified_at: row.get(9)?,
    })
}

fn title_from_path(content_id: &str) -> String {
    content_id
        .rsplit('/')
        .next()
        .and_then(|name| name.split('.').next())
        .unwrap_or(content_id)
        .to_owned()
}
