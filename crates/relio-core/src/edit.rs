use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, TransactionBehavior, params_from_iter, types::Value};

use crate::library::Library;
use crate::schema::Schema;

/// Nickel's `HighlightColor` values: four colours and gray (the Clara Colour shows 0 as cyan).
pub const HIGHLIGHT_COLORS: u8 = 5;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AnnotationEdit {
    pub note: Option<String>,
    pub color: Option<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Edited {
    Updated,
    Deleted,
    /// Already synced to the Kobo cloud: hidden so the deletion syncs too.
    Hidden,
    NotFound,
}

impl Library {
    fn open_writable(&self) -> rusqlite::Result<(Connection, Schema)> {
        let conn = Connection::open_with_flags(self.db_path(), OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let schema = Schema::load(&conn)?;
        Ok((conn, schema))
    }

    pub fn edit_annotation(&self, id: &str, edit: &AnnotationEdit) -> rusqlite::Result<Edited> {
        let (mut conn, schema) = self.open_writable()?;
        let mut sets = Vec::new();
        let mut values: Vec<Value> = Vec::new();
        if let Some(note) = &edit.note {
            sets.push("Annotation = ?".to_owned());
            values.push(note.clone().into());
            if schema.has("Bookmark", "Type") {
                sets.push("Type = CASE WHEN Type IN ('highlight', 'note') THEN CASE WHEN ? = '' THEN 'highlight' ELSE 'note' END ELSE Type END".to_owned());
                values.push(note.clone().into());
            }
        }
        if let (Some(color), true) = (edit.color, schema.has("Bookmark", "Color")) {
            sets.push("Color = ?".to_owned());
            values.push(i64::from(color).into());
        }
        if sets.is_empty() {
            return Ok(Edited::NotFound);
        }
        if schema.has("Bookmark", "DateModified") {
            sets.push("DateModified = ?".to_owned());
            values.push(nickel_timestamp(SystemTime::now()).into());
        }
        values.push(id.to_owned().into());

        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            &format!("UPDATE Bookmark SET {} WHERE BookmarkID = ?", sets.join(", ")),
            params_from_iter(values),
        )?;
        tx.commit()?;
        Ok(if changed == 0 { Edited::NotFound } else { Edited::Updated })
    }

    pub fn delete_annotation(&self, id: &str) -> rusqlite::Result<Edited> {
        let (mut conn, schema) = self.open_writable()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let synced = schema.has("Bookmark", "SyncTime") && schema.has("Bookmark", "Hidden");
        let outcome = if synced
            && tx.query_row("SELECT SyncTime IS NOT NULL FROM Bookmark WHERE BookmarkID = ?1", [id], |row| row.get::<_, bool>(0))
                .unwrap_or(false)
        {
            let modified = if schema.has("Bookmark", "DateModified") { ", DateModified = ?2" } else { "" };
            let sql = format!("UPDATE Bookmark SET Hidden = 'true'{modified} WHERE BookmarkID = ?1");
            if modified.is_empty() {
                tx.execute(&sql, [id])?;
            } else {
                tx.execute(&sql, [id, &nickel_timestamp(SystemTime::now())])?;
            }
            Edited::Hidden
        } else if tx.execute("DELETE FROM Bookmark WHERE BookmarkID = ?1", [id])? > 0 {
            Edited::Deleted
        } else {
            Edited::NotFound
        };
        tx.commit()?;
        Ok(outcome)
    }
}

/// `yyyy-MM-ddThh:mm:ss.zzz` in UTC, as Nickel writes it.
pub fn nickel_timestamp(time: SystemTime) -> String {
    let since_epoch = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since_epoch.as_secs();
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        since_epoch.subsec_millis()
    )
}

/// Howard Hinnant's days-to-civil conversion.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_timestamps_like_nickel() {
        let time = UNIX_EPOCH + Duration::from_millis(1_792_281_730_042);
        assert_eq!(nickel_timestamp(time), "2026-10-18T00:02:10.042");
        assert_eq!(nickel_timestamp(UNIX_EPOCH), "1970-01-01T00:00:00.000");
    }
}
