use crate::models::*;
use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 unix 毫秒。
/// 本地时区的 `HH:MM:SS.mmm`（事件流时间列用）。
/// 走 libc 的 localtime_r：跨时区/夏令时都对，且不引时区数据库。
pub fn local_hms_millis(epoch_ms: i64) -> String {
    let secs = epoch_ms.div_euclid(1000) as libc::time_t;
    let ms = epoch_ms.rem_euclid(1000);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r 只写 tm，指针在本函数内一直有效
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        tm.tm_hour, tm.tm_min, tm.tm_sec, ms
    )
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    /// unix 毫秒
    pub sent_at: i64,
    pub status: Option<u16>,
    pub duration_ms: u64,
    pub error: Option<String>,
    /// 请求完整快照（可恢复）
    pub request: RequestItem,
}

pub struct HistoryStore {
    conn: Connection,
}

impl HistoryStore {
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                sent_at INTEGER NOT NULL,
                status INTEGER,
                duration_ms INTEGER NOT NULL,
                error TEXT,
                method TEXT NOT NULL,
                url TEXT NOT NULL,
                request_json TEXT NOT NULL
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn insert(&self, entry: &HistoryEntry) -> Result<i64> {
        let request_json = serde_json::to_string(&entry.request)?;
        self.conn.execute(
            "INSERT INTO history (sent_at, status, duration_ms, error, method, url, request_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.sent_at,
                entry.status.map(|s| s as i64),
                entry.duration_ms as i64,
                entry.error,
                entry.request.method,
                entry.request.url,
                request_json,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 最新的 limit 条（id 倒序，全部请求）。
    pub fn list(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        self.query(
            "SELECT id, sent_at, status, duration_ms, error, request_json
             FROM history ORDER BY id DESC LIMIT ?1",
            rusqlite::params![limit as i64],
        )
    }

    /// 指定请求的最新 limit 条（id 倒序）。
    /// 用 request_json 里的 id 过滤，省掉一次表结构迁移。
    pub fn list_for_request(&self, request_id: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
        self.query(
            "SELECT id, sent_at, status, duration_ms, error, request_json
             FROM history
             WHERE json_extract(request_json, '$.id') = ?1
             ORDER BY id DESC LIMIT ?2",
            rusqlite::params![request_id, limit as i64],
        )
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(sql)?;
        type Row = (i64, i64, Option<i64>, i64, Option<String>, String);
        let rows: Vec<Row> = stmt
            .query_map(params, |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        rows.into_iter()
            .map(|(id, sent_at, status, duration_ms, error, request_json)| {
                Ok(HistoryEntry {
                    id,
                    sent_at,
                    status: status.map(|s| s as u16),
                    duration_ms: duration_ms as u64,
                    error,
                    request: serde_json::from_str(&request_json)?,
                })
            })
            .collect()
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM history WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_hms_keeps_the_milliseconds() {
        // 时区里的偏移都是整分钟，所以「毫秒三位」和形状与时区无关，可以硬断言
        for ms in [0_i64, 1, 999, 1_500, 1_700_000_000_123] {
            let s = local_hms_millis(ms);
            assert_eq!(s.len(), 12, "{s}");
            let (hms, frac) = s.split_once('.').unwrap();
            assert_eq!(hms.split(':').count(), 3, "{s}");
            assert!(hms.split(':').all(|p| p.len() == 2), "{s}");
            assert_eq!(frac, format!("{:03}", ms.rem_euclid(1000)), "{s}");
        }
        // 同一时刻的毫秒不同 → 字符串必须不同（别把 ms 丢了）
        assert_ne!(local_hms_millis(1_700_000_000_000), local_hms_millis(1_700_000_000_400));
    }
}
