use crate::rows::{json_from_sql, optional_json, optional_value_from_sql};
use crate::{Result, Store};
use faro_core::{CookieEventRecord, CookieSnapshotRecord};
use rusqlite::params;

impl Store {
    pub fn insert_cookie_snapshot(&self, snapshot: &CookieSnapshotRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cookie_snapshots
             (id, session_id, tab_id, run_id, ts, url, cookies_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                snapshot.id,
                snapshot.session_id,
                snapshot.tab_id,
                snapshot.run_id,
                snapshot.ts,
                snapshot.url,
                serde_json::to_string(&snapshot.cookies)?
            ],
        )?;
        Ok(())
    }

    pub fn insert_cookie_event(&self, event: &CookieEventRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cookie_events
             (id, session_id, tab_id, run_id, ts, operation, name, domain, path, value, attributes_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                event.id,
                event.session_id,
                event.tab_id,
                event.run_id,
                event.ts,
                event.operation,
                event.name,
                event.domain,
                event.path,
                event.value,
                optional_json(&event.attributes_json)?
            ],
        )?;
        Ok(())
    }
    pub fn cookie_snapshots_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<CookieSnapshotRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, url, cookies_json
             FROM cookie_snapshots
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let snapshots = stmt
            .query_map(params![session_id], |row| {
                let cookies_json: String = row.get(6)?;
                Ok(CookieSnapshotRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    ts: row.get(4)?,
                    url: row.get(5)?,
                    cookies: json_from_sql(&cookies_json, 6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(snapshots)
    }

    pub fn cookie_events_for_session(&self, session_id: &str) -> Result<Vec<CookieEventRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, operation, name, domain, path, value, attributes_json
             FROM cookie_events
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let events = stmt
            .query_map(params![session_id], |row| {
                let attributes_json: Option<String> = row.get(10)?;
                Ok(CookieEventRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    ts: row.get(4)?,
                    operation: row.get(5)?,
                    name: row.get(6)?,
                    domain: row.get(7)?,
                    path: row.get(8)?,
                    value: row.get(9)?,
                    attributes_json: optional_value_from_sql(attributes_json, 10)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(events)
    }
}
