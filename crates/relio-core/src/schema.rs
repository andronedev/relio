use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

const TABLES: &[&str] = &["content", "Bookmark", "Shelf", "ShelfContent"];

/// Nickel grows its schema through firmware migrations, so columns are resolved
/// at runtime and missing ones read as `NULL`.
#[derive(Debug, Default)]
pub struct Schema {
    columns: HashMap<String, HashSet<String>>,
}

impl Schema {
    pub fn load(conn: &Connection) -> rusqlite::Result<Self> {
        let mut columns = HashMap::new();
        for table in TABLES {
            let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
            let names = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .map(|name| name.map(|n| n.to_ascii_lowercase()))
                .collect::<rusqlite::Result<HashSet<_>>>()?;
            columns.insert(table.to_ascii_lowercase(), names);
        }
        Ok(Self { columns })
    }

    pub fn has_table(&self, table: &str) -> bool {
        self.columns
            .get(&table.to_ascii_lowercase())
            .is_some_and(|cols| !cols.is_empty())
    }

    pub fn has(&self, table: &str, column: &str) -> bool {
        self.columns
            .get(&table.to_ascii_lowercase())
            .is_some_and(|cols| cols.contains(&column.to_ascii_lowercase()))
    }

    pub fn col(&self, table: &str, alias: &str, column: &str) -> String {
        if self.has(table, column) {
            format!("{alias}.{column}")
        } else {
            "NULL".to_owned()
        }
    }
}
