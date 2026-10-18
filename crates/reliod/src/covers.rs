use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Nickel renders covers into `.kobo-images/<a>/<b>/<ImageId> - <variant>.parsed` (JPEG).
/// The two hashed directory levels are not reproduced here: the folder is indexed instead,
/// and re-indexed whenever a cover is missing.
pub struct Covers {
    root: PathBuf,
    index: Mutex<HashMap<String, PathBuf>>,
}

#[derive(Debug, Clone, Copy)]
pub enum CoverSize {
    Grid,
    Full,
}

impl CoverSize {
    fn variant(self) -> &'static str {
        match self {
            Self::Grid => "N3_LIBRARY_GRID",
            Self::Full => "N3_LIBRARY_FULL",
        }
    }
}

impl Covers {
    pub fn new(root: PathBuf) -> Self {
        Self { root, index: Mutex::new(HashMap::new()) }
    }

    pub fn find(&self, image_id: &str, size: CoverSize) -> Option<PathBuf> {
        let name = format!("{image_id} - {}.parsed", size.variant());
        let mut index = self.index.lock().unwrap();
        if let Some(path) = index.get(&name).filter(|path| path.exists()) {
            return Some(path.clone());
        }
        *index = scan(&self.root);
        index.get(&name).cloned()
    }
}

fn scan(root: &Path) -> HashMap<String, PathBuf> {
    let mut found = HashMap::new();
    let entries = |dir: &Path| std::fs::read_dir(dir).into_iter().flatten().flatten();
    for first in entries(root).filter(|e| e.path().is_dir()) {
        for second in entries(&first.path()).filter(|e| e.path().is_dir()) {
            for file in entries(&second.path()) {
                if let Ok(name) = file.file_name().into_string() {
                    found.insert(name, file.path());
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_covers_in_hashed_directories_and_notices_new_ones() {
        let root = std::env::temp_dir().join(format!("reliod-covers-{}", crate::token::random_token()));
        let dir = root.join("2/84");
        std::fs::create_dir_all(&dir).unwrap();
        let covers = Covers::new(root.clone());
        assert_eq!(covers.find("abc", CoverSize::Grid), None);

        std::fs::write(dir.join("abc - N3_LIBRARY_GRID.parsed"), b"\xff\xd8").unwrap();
        assert_eq!(covers.find("abc", CoverSize::Grid), Some(dir.join("abc - N3_LIBRARY_GRID.parsed")));
        assert_eq!(covers.find("abc", CoverSize::Full), None);
    }
}
