use anyhow::Result;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::{OptionalExtension, Row, ToSql, params};
use serde::{Deserialize, Serialize};

use crate::db::{Database, now};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DigStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl DigStatus {
    fn as_str(self) -> &'static str {
        match self {
            DigStatus::Queued => "queued",
            DigStatus::Running => "running",
            DigStatus::Succeeded => "succeeded",
            DigStatus::Failed => "failed",
        }
    }
}

impl ToSql for DigStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

impl FromSql for DigStatus {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        match value.as_str()? {
            "queued" => Ok(DigStatus::Queued),
            "running" => Ok(DigStatus::Running),
            "succeeded" => Ok(DigStatus::Succeeded),
            "failed" => Ok(DigStatus::Failed),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DigScope {
    New,
    Scratch,
}

#[derive(Debug, Serialize)]
pub struct Dig {
    pub id: i64,
    pub source_id: i64,
    pub status: DigStatus,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub versions_done: u32,
    pub versions_total: u32,
    pub objects_received: u64,
    pub objects_total: u64,
    pub from: DigScope,
    pub log: String,
    pub error: Option<String>,
}

impl Dig {
    const COLUMNS: &str = "id, source_id, status, queued_at, started_at, finished_at, versions_done, versions_total, objects_received, objects_total, from_scratch, log, error";
    const SECONDS_PER_HOUR: i64 = 3600;

    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            source_id: row.get(1)?,
            status: row.get(2)?,
            queued_at: row.get(3)?,
            started_at: row.get(4)?,
            finished_at: row.get(5)?,
            versions_done: row.get(6)?,
            versions_total: row.get(7)?,
            objects_received: row.get(8)?,
            objects_total: row.get(9)?,
            from: match row.get(10)? {
                true => DigScope::Scratch,
                false => DigScope::New,
            },
            log: row.get(11)?,
            error: row.get(12)?,
        })
    }
}

impl Database {
    pub fn dig(&self, id: i64) -> Result<Option<Dig>> {
        let dig = self
            .connection
            .query_row(
                &format!("SELECT {} FROM digs WHERE id = ?1", Dig::COLUMNS),
                [id],
                Dig::from_row,
            )
            .optional()?;
        Ok(dig)
    }

    pub fn digs(&self, source_id: i64, limit: u32) -> Result<Vec<Dig>> {
        let mut statement = self.connection.prepare_cached(&format!(
            "SELECT {} FROM digs WHERE source_id = ?1 ORDER BY id DESC LIMIT ?2",
            Dig::COLUMNS
        ))?;
        let digs = statement
            .query_map(params![source_id, limit], Dig::from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(digs)
    }

    pub fn queue_dig(&self, source_id: i64, from: DigScope) -> Result<i64> {
        let pending: Option<i64> = self
            .connection
            .query_row(
                "SELECT id FROM digs WHERE source_id = ?1 AND status IN (?2, ?3)",
                params![source_id, DigStatus::Queued, DigStatus::Running],
                |row| row.get(0),
            )
            .optional()?;
        let from_scratch = from == DigScope::Scratch;
        if let Some(id) = pending {
            self.connection.execute(
                "UPDATE digs SET from_scratch = from_scratch OR ?2 WHERE id = ?1 AND status = ?3",
                params![id, from_scratch, DigStatus::Queued],
            )?;
            return Ok(id);
        }
        self.connection.execute(
            "INSERT INTO digs (source_id, status, queued_at, from_scratch) VALUES (?1, ?2, ?3, ?4)",
            params![source_id, DigStatus::Queued, now(), from_scratch],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn queue_due_digs(&self) -> Result<()> {
        let mut statement = self.connection.prepare(
            "SELECT sources.id FROM sources
             WHERE refresh_hours IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM digs WHERE source_id = sources.id AND status IN (?1, ?2))
               AND coalesce((SELECT max(queued_at) FROM digs WHERE source_id = sources.id), 0)
                   + refresh_hours * ?3 <= ?4",
        )?;
        let due: Vec<i64> = statement
            .query_map(
                params![
                    DigStatus::Queued,
                    DigStatus::Running,
                    Dig::SECONDS_PER_HOUR,
                    now()
                ],
                |row| row.get(0),
            )?
            .collect::<rusqlite::Result<_>>()?;
        for source_id in due {
            self.queue_dig(source_id, DigScope::New)?;
        }
        Ok(())
    }

    pub fn next_queued_dig(&self) -> Result<Option<Dig>> {
        let dig = self
            .connection
            .query_row(
                &format!(
                    "SELECT {} FROM digs WHERE status = ?1 ORDER BY id LIMIT 1",
                    Dig::COLUMNS
                ),
                [DigStatus::Queued],
                Dig::from_row,
            )
            .optional()?;
        Ok(dig)
    }

    pub fn start_dig(&self, id: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE digs SET status = ?2, started_at = ?3 WHERE id = ?1",
            params![id, DigStatus::Running, now()],
        )?;
        Ok(())
    }

    pub fn update_dig_progress(
        &self,
        id: i64,
        done: usize,
        total: usize,
        line: &str,
    ) -> Result<()> {
        self.connection.execute(
            "UPDATE digs SET versions_done = ?2, versions_total = ?3, log = log || ?4 || char(10)
             WHERE id = ?1",
            params![id, done, total, line],
        )?;
        Ok(())
    }

    pub fn update_dig_fetch(&self, id: i64, received: usize, total: usize) -> Result<()> {
        self.connection.execute(
            "UPDATE digs SET objects_received = ?2, objects_total = ?3 WHERE id = ?1",
            params![id, received, total],
        )?;
        Ok(())
    }

    pub fn succeed_dig(&self, id: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE digs SET status = ?2, finished_at = ?3 WHERE id = ?1",
            params![id, DigStatus::Succeeded, now()],
        )?;
        Ok(())
    }

    pub fn fail_dig(&self, id: i64, error: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE digs SET status = ?2, finished_at = ?3, error = ?4 WHERE id = ?1",
            params![id, DigStatus::Failed, now(), error],
        )?;
        Ok(())
    }

    pub fn fail_interrupted_digs(&self) -> Result<()> {
        self.connection.execute(
            "UPDATE digs SET status = ?1, finished_at = ?2, error = 'the server stopped during the dig'
             WHERE status = ?3",
            params![DigStatus::Failed, now(), DigStatus::Running],
        )?;
        Ok(())
    }
}
