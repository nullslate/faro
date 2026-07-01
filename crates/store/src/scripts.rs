use crate::{Result, ScriptRecord, Store, StoreError};
use rusqlite::{OptionalExtension, params};

impl Store {
    pub fn scripts(&self) -> Result<Vec<ScriptRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, body, created_at, updated_at, last_run_at
             FROM scripts
             ORDER BY updated_at DESC, name ASC",
        )?;
        let scripts = stmt
            .query_map([], |row| {
                Ok(ScriptRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    body: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    last_run_at: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(scripts)
    }

    pub fn script(&self, id: &str) -> Result<Option<ScriptRecord>> {
        self.conn
            .query_row(
                "SELECT id, name, body, created_at, updated_at, last_run_at
                 FROM scripts
                 WHERE id = ?1",
                params![id],
                |row| {
                    Ok(ScriptRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        body: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        last_run_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn save_script(&self, script: &ScriptRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO scripts (id, name, body, created_at, updated_at, last_run_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                body = excluded.body,
                updated_at = excluded.updated_at,
                last_run_at = excluded.last_run_at",
            params![
                script.id,
                script.name,
                script.body,
                script.created_at,
                script.updated_at,
                script.last_run_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_script(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM scripts WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn mark_script_run(&self, id: &str, ts: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE scripts
             SET last_run_at = ?2, updated_at = updated_at
             WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }
}
