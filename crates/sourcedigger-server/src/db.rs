use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use regex::Regex;
use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;
use rusqlite::types::ValueRef;

use crate::digs::DigScope;

pub struct Database {
    pub connection: Connection,
}

impl Database {
    const PRAGMAS: &str = "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        PRAGMA synchronous = NORMAL;
    ";
    const SCHEMA: &str = "
        CREATE TABLE IF NOT EXISTS sources (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            url TEXT NOT NULL,
            tag_pattern TEXT NOT NULL,
            file_pattern TEXT NOT NULL,
            viewer_url TEXT NOT NULL,
            refresh_hours INTEGER
        );
        CREATE TABLE IF NOT EXISTS digs (
            id INTEGER PRIMARY KEY,
            source_id INTEGER NOT NULL REFERENCES sources (id) ON DELETE CASCADE,
            status TEXT NOT NULL,
            queued_at INTEGER NOT NULL,
            started_at INTEGER,
            finished_at INTEGER,
            versions_done INTEGER NOT NULL DEFAULT 0,
            versions_total INTEGER NOT NULL DEFAULT 0,
            objects_received INTEGER NOT NULL DEFAULT 0,
            objects_total INTEGER NOT NULL DEFAULT 0,
            from_scratch INTEGER NOT NULL DEFAULT 0,
            log TEXT NOT NULL DEFAULT '',
            error TEXT
        );
        CREATE INDEX IF NOT EXISTS digs_source ON digs (source_id, id);
        CREATE TABLE IF NOT EXISTS versions (
            id INTEGER PRIMARY KEY,
            source_id INTEGER NOT NULL REFERENCES sources (id) ON DELETE CASCADE,
            position INTEGER NOT NULL,
            name TEXT NOT NULL,
            tag TEXT NOT NULL,
            commit_id TEXT NOT NULL,
            added INTEGER NOT NULL,
            removed INTEGER NOT NULL,
            modified INTEGER NOT NULL,
            signature INTEGER NOT NULL,
            UNIQUE (source_id, position)
        );
        CREATE TABLE IF NOT EXISTS commits (
            id INTEGER PRIMARY KEY,
            source_id INTEGER NOT NULL REFERENCES sources (id) ON DELETE CASCADE,
            version_id INTEGER NOT NULL REFERENCES versions (id) ON DELETE CASCADE,
            hash TEXT NOT NULL,
            summary TEXT NOT NULL,
            author TEXT NOT NULL,
            time INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS commits_source ON commits (source_id, id);
        CREATE INDEX IF NOT EXISTS commits_version ON commits (version_id);
        CREATE TABLE IF NOT EXISTS names (
            id INTEGER PRIMARY KEY,
            source_id INTEGER NOT NULL REFERENCES sources (id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            UNIQUE (source_id, name)
        );
        CREATE TABLE IF NOT EXISTS paths (
            id INTEGER PRIMARY KEY,
            source_id INTEGER NOT NULL REFERENCES sources (id) ON DELETE CASCADE,
            text TEXT NOT NULL,
            UNIQUE (source_id, text)
        );
        CREATE TABLE IF NOT EXISTS signatures (
            id INTEGER PRIMARY KEY,
            source_id INTEGER NOT NULL REFERENCES sources (id) ON DELETE CASCADE,
            text TEXT NOT NULL,
            UNIQUE (source_id, text)
        );
        CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY,
            commit_id INTEGER NOT NULL REFERENCES commits (id) ON DELETE CASCADE,
            name_id INTEGER NOT NULL REFERENCES names (id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            action TEXT NOT NULL,
            path_id INTEGER NOT NULL,
            line INTEGER NOT NULL,
            signature_id INTEGER NOT NULL,
            previous_path_id INTEGER,
            previous_line INTEGER,
            previous_signature_id INTEGER
        );
        CREATE INDEX IF NOT EXISTS names_folded ON names (source_id, name COLLATE NOCASE);
        CREATE INDEX IF NOT EXISTS events_name ON events (name_id);
        CREATE INDEX IF NOT EXISTS events_commit ON events (commit_id);
    ";
    const SCHEMA_VERSION: i64 = 3;
    const OUTDATED_TABLES: &str = "
        DROP TABLE IF EXISTS events;
        DROP TABLE IF EXISTS names;
        DROP TABLE IF EXISTS commits;
        DROP TABLE IF EXISTS versions;
        DROP TABLE IF EXISTS paths;
        DROP TABLE IF EXISTS signatures;
    ";
    const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

    pub fn create(path: &Path) -> Result<Self> {
        let database = Self::open(path)?;
        let version: i64 = database
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let tables: i64 = database.connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table'",
            [],
            |row| row.get(0),
        )?;
        let outdated = tables > 0 && version != Self::SCHEMA_VERSION;
        if outdated {
            tracing::info!(
                "upgrading the index from schema {version} to {}; every source is dug again",
                Self::SCHEMA_VERSION
            );
            database.connection.execute_batch(Self::OUTDATED_TABLES)?;
        }
        database.connection.execute_batch(Self::SCHEMA)?;
        database
            .connection
            .pragma_update(None, "user_version", Self::SCHEMA_VERSION)?;
        database.fail_interrupted_digs()?;
        if outdated {
            for source in database.sources()? {
                database.queue_dig(source.id, DigScope::Scratch)?;
            }
        }
        Ok(database)
    }

    pub fn open(path: &Path) -> Result<Self> {
        let connection =
            Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        connection.busy_timeout(Self::BUSY_TIMEOUT)?;
        connection.execute_batch(Self::PRAGMAS)?;
        connection.create_scalar_function(
            "regexp",
            2,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |context| {
                if context.get_raw(0) == ValueRef::Null {
                    return Ok(None);
                }
                let regex: Arc<Regex> = context.get_or_create_aux(
                    0,
                    |pattern| -> Result<Regex, Box<dyn std::error::Error + Send + Sync>> {
                        Ok(Regex::new(pattern.as_str()?)?)
                    },
                )?;
                let text = context.get_raw(1).as_str_or_null()?;
                Ok(text.map(|text| regex.is_match(text)))
            },
        )?;
        Ok(Self { connection })
    }
}

pub fn now() -> i64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970");
    i64::try_from(elapsed.as_secs()).expect("seconds since 1970 fit in i64")
}
