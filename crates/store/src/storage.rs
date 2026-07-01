use crate::rows::{json_from_sql, optional_json, optional_value_from_sql};
use crate::{Result, Store};
use faro_core::{StorageEventRecord, StorageSnapshotRecord};
use rusqlite::params;

impl Store {
    pub fn insert_storage_event(&self, event: &StorageEventRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO storage_events
             (id, session_id, tab_id, run_id, ts, origin, storage_type, operation,
              key, old_value, new_value, stack_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                event.id,
                event.session_id,
                event.tab_id,
                event.run_id,
                event.ts,
                event.origin,
                event.storage_type,
                event.operation,
                event.key,
                event.old_value,
                event.new_value,
                optional_json(&event.stack_json)?
            ],
        )?;
        Ok(())
    }

    pub fn insert_storage_snapshot(&self, snapshot: &StorageSnapshotRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO storage_snapshots
             (id, session_id, tab_id, run_id, ts, origin, storage_type, data_json, sha256)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                snapshot.id,
                snapshot.session_id,
                snapshot.tab_id,
                snapshot.run_id,
                snapshot.ts,
                snapshot.origin,
                snapshot.storage_type,
                serde_json::to_string(&snapshot.entries)?,
                snapshot.sha256
            ],
        )?;
        Ok(())
    }
    pub fn storage_events_for_session(&self, session_id: &str) -> Result<Vec<StorageEventRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, origin, storage_type, operation,
                    key, old_value, new_value, stack_json
             FROM storage_events
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let events = stmt
            .query_map(params![session_id], |row| {
                let stack_json: Option<String> = row.get(11)?;
                Ok(StorageEventRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    ts: row.get(4)?,
                    origin: row.get(5)?,
                    storage_type: row.get(6)?,
                    operation: row.get(7)?,
                    key: row.get(8)?,
                    old_value: row.get(9)?,
                    new_value: row.get(10)?,
                    stack_json: optional_value_from_sql(stack_json, 11)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(events)
    }

    pub fn storage_snapshots_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<StorageSnapshotRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, origin, storage_type, data_json, sha256
             FROM storage_snapshots
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let snapshots = stmt
            .query_map(params![session_id], |row| {
                let data_json: String = row.get(7)?;
                Ok(StorageSnapshotRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    ts: row.get(4)?,
                    origin: row.get(5)?,
                    storage_type: row.get(6)?,
                    entries: json_from_sql(&data_json, 7)?,
                    sha256: row.get(8)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(snapshots)
    }
}
