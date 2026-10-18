use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::Connection;

use crate::{AnnotationEdit, AnnotationKind, Edited, Format, Library, ReadStatus, Stats};

const LEGACY: &str = include_str!("../../../fixtures/schema-legacy.sql");
const MIGRATIONS: &str = include_str!("../../../fixtures/schema-migrations.sql");
const SAMPLE: &str = include_str!("../../../fixtures/sample-data.sql");

fn temp_db(name: &str, sql: &[&str]) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("relio-{name}-{}-{n}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let conn = Connection::open(&path).unwrap();
    for batch in sql {
        conn.execute_batch(batch).unwrap();
    }
    path
}

fn sample_library() -> Library {
    Library::new(temp_db("sample", &[LEGACY, MIGRATIONS, SAMPLE]))
}

#[test]
fn lists_downloaded_books_most_recent_first() {
    let books = sample_library().books().unwrap();
    assert_eq!(books.len(), 7);
    assert_eq!(books[0].title, "Les Misérables — Fantine");
    assert_eq!(books[0].status, ReadStatus::Reading);
    assert_eq!(books[0].percent_read, 63);
    assert_eq!(books[0].annotation_count, 2);
}

#[test]
fn falls_back_to_file_name_for_untitled_books() {
    let books = sample_library().books().unwrap();
    let pdf = books.iter().find(|b| b.format == Format::Pdf).unwrap();
    assert_eq!(pdf.title, "Rust embarque");
}

#[test]
fn skips_hidden_annotations() {
    let library = sample_library();
    let all = library.annotations(None).unwrap();
    assert_eq!(all.len(), 6);
    assert!(all.iter().all(|a| a.id != "a7"));

    let candide = library
        .annotations(Some("file:///mnt/onboard/Livres/Candide - Voltaire.kepub.epub"))
        .unwrap();
    assert_eq!(candide.len(), 2);
    assert_eq!(candide[1].kind, AnnotationKind::Note);
    assert_eq!(candide[1].book_title.as_deref(), Some("Candide, ou l'Optimisme"));
}

#[test]
fn ignores_deleted_shelves_and_entries() {
    let shelves = sample_library().shelves().unwrap();
    let names: Vec<_> = shelves.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Classiques", "Poésie"]);
    assert_eq!(shelves[0].book_ids.len(), 2);
}

#[test]
fn reads_legacy_schema_without_migrations() {
    let library = Library::new(temp_db(
        "legacy",
        &[
            LEGACY,
            "INSERT INTO content (ContentID, ContentType, MimeType, Title, ReadStatus, ___UserID)
             VALUES ('file:///mnt/onboard/a.epub', 6, 'application/epub+zip', 'A', 1, 'u');
             INSERT INTO Bookmark VALUES ('b', 'file:///mnt/onboard/a.epub', 'c', '', 0, 0, '', 0, 0,
                                          'quote', 'thought', NULL, '2012-01-01');",
        ],
    ));
    let books = library.books().unwrap();
    assert_eq!(books.len(), 1);
    assert_eq!(books[0].percent_read, 0);
    assert_eq!(books[0].annotation_count, 1);
    let annotations = library.annotations(None).unwrap();
    assert_eq!(annotations[0].kind, AnnotationKind::Note);
    assert_eq!(Stats::of(&library.books().unwrap()).reading, 1);
}

#[test]
fn accepts_integer_booleans_from_older_rows() {
    let library = Library::new(temp_db(
        "integers",
        &[
            LEGACY,
            MIGRATIONS,
            "INSERT INTO content (ContentID, ContentType, MimeType, Title, ReadStatus, ___UserID, IsDownloaded)
             VALUES ('file:///mnt/onboard/a.epub', 6, 'application/epub+zip', 'A', 0, 'u', 1),
                    ('store-b', 6, 'application/epub+zip', 'B', 0, 'u', 0);
             INSERT INTO Bookmark (BookmarkID, VolumeID, ContentID, StartContainerPath, StartContainerChildIndex,
                                   StartOffset, EndContainerPath, EndContainerChildIndex, EndOffset, Text, Hidden)
             VALUES ('shown', 'file:///mnt/onboard/a.epub', 'c', '', 0, 0, '', 0, 0, 'x', 0),
                    ('hidden', 'file:///mnt/onboard/a.epub', 'c', '', 0, 0, '', 0, 0, 'y', 1);",
        ],
    ));
    assert_eq!(library.books().unwrap().len(), 1);
    let annotations = library.annotations(None).unwrap();
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].id, "shown");
}

#[test]
fn books_on_external_storage_have_files() {
    let library = Library::new(temp_db(
        "sd",
        &[
            LEGACY,
            MIGRATIONS,
            "INSERT INTO content (ContentID, ContentType, MimeType, Title, ReadStatus, ___UserID, IsDownloaded)
             VALUES ('file:///mnt/sd/kobocloud/a.epub', 6, 'application/epub+zip', 'A', 0, 'u', 'true');",
        ],
    ));
    let book = &library.books().unwrap()[0];
    assert!(book.sideloaded);
    assert!(book.has_file);
    assert_eq!(
        library.book_file(&book.id, Path::new("/mnt/onboard")),
        Some(PathBuf::from("/mnt/sd/kobocloud/a.epub"))
    );
    assert_eq!(library.book_file("file:///mnt/sd/../onboard/.kobo/KoboReader.sqlite", Path::new("/")), None);
}

#[test]
fn resolves_only_safe_sideloaded_paths() {
    let library = Library::new("unused");
    let root = Path::new("/mnt/onboard");
    assert_eq!(
        library.book_file("file:///mnt/onboard/Livres/a.epub", root),
        Some(root.join("Livres/a.epub"))
    );
    assert_eq!(library.book_file("file:///mnt/onboard/../etc/passwd", root), None);
    assert_eq!(
        library.book_file("7d059b72-eb0d-467e-a890-d197133cc2a5", root),
        Some(root.join(".kobo/kepub/7d059b72-eb0d-467e-a890-d197133cc2a5"))
    );
    assert_eq!(library.book_file("../../etc/passwd", root), None);
}

#[test]
fn edits_notes_and_colours() {
    let library = sample_library();
    let edit = AnnotationEdit { note: Some("Optimisme à toute épreuve".into()), color: Some(3) };
    assert_eq!(library.edit_annotation("a1", &edit).unwrap(), Edited::Updated);
    let a1 = library.annotations(None).unwrap().into_iter().find(|a| a.id == "a1").unwrap();
    assert_eq!(a1.kind, AnnotationKind::Note);
    assert_eq!(a1.note.as_deref(), Some("Optimisme à toute épreuve"));
    assert_eq!(a1.color, 3);
    assert_ne!(a1.modified_at.as_deref(), Some("2026-09-03T21:10:00Z"));

    let cleared = AnnotationEdit { note: Some(String::new()), color: None };
    library.edit_annotation("a1", &cleared).unwrap();
    let a1 = library.annotations(None).unwrap().into_iter().find(|a| a.id == "a1").unwrap();
    assert_eq!(a1.kind, AnnotationKind::Highlight);
    assert_eq!(library.edit_annotation("missing", &edit).unwrap(), Edited::NotFound);
}

#[test]
fn deletes_local_annotations_and_hides_synced_ones() {
    let library = sample_library();
    Connection::open(library.db_path())
        .unwrap()
        .execute("UPDATE Bookmark SET SyncTime = '2026-10-01T00:00:00Z' WHERE BookmarkID = 'a3'", [])
        .unwrap();
    assert_eq!(library.delete_annotation("a1").unwrap(), Edited::Deleted);
    assert_eq!(library.delete_annotation("a3").unwrap(), Edited::Hidden);
    assert_eq!(library.delete_annotation("a1").unwrap(), Edited::NotFound);
    let ids: Vec<_> = library.annotations(None).unwrap().into_iter().map(|a| a.id).collect();
    assert!(!ids.contains(&"a1".to_owned()) && !ids.contains(&"a3".to_owned()));
}

#[test]
fn hides_sideloaded_books_whose_file_is_gone() {
    let library = sample_library();
    let onboard = std::env::temp_dir().join(format!("relio-onboard-{}", std::process::id()));
    std::fs::create_dir_all(onboard.join("Livres")).unwrap();
    std::fs::write(onboard.join("Livres/Candide - Voltaire.kepub.epub"), b"epub").unwrap();
    let titles: Vec<_> = library.books_on_device(&onboard).unwrap().into_iter().map(|b| b.title).collect();
    assert_eq!(titles, ["Candide, ou l'Optimisme"]);
}
