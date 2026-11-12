use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, Row, ToSql, params};
use serde::Serialize;
use sourcedigger_index::diff::{Action, Event};
use sourcedigger_index::git::Version;

use crate::db::Database;
use crate::query::NamePattern;
use crate::sources::Source;

#[derive(Clone, Debug, Serialize)]
pub struct VersionRecord {
    pub position: i64,
    pub name: String,
    pub tag: String,
    pub commit: String,
    pub added: u32,
    pub removed: u32,
    pub modified: u32,
}

impl VersionRecord {
    pub fn is(&self, version: &Version) -> bool {
        self.name == version.name
            && self.tag == version.tag
            && self.commit == version.commit.to_string()
    }
}

pub struct EventFilter {
    pub names: Option<NamePattern>,
    pub kinds: Vec<String>,
    pub actions: Vec<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub path: Option<String>,
    pub after: Option<Cursor>,
    pub limit: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub position: i64,
    pub id: i64,
}

impl Display for Cursor {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "{}.{}", self.position, self.id)
    }
}

impl FromStr for Cursor {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let (position, id) = text
            .split_once('.')
            .with_context(|| format!("invalid cursor {text}"))?;
        Ok(Cursor {
            position: position.parse()?,
            id: id.parse()?,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Place {
    pub path: String,
    pub line: u32,
    pub signature: String,
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct EventRecord {
    pub id: i64,
    pub version: String,
    pub position: i64,
    pub name: String,
    pub kind: String,
    pub action: String,
    pub definition: Place,
    pub previous: Option<Place>,
}

#[derive(Debug, Serialize)]
pub struct EventPage {
    pub events: Vec<EventRecord>,
    pub next: Option<String>,
}

impl Database {
    pub fn versions(&self, source_id: i64) -> Result<Vec<VersionRecord>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT position, name, tag, commit_id, added, removed, modified
             FROM versions WHERE source_id = ?1 ORDER BY position",
        )?;
        let versions = statement
            .query_map([source_id], |row| {
                Ok(VersionRecord {
                    position: row.get(0)?,
                    name: row.get(1)?,
                    tag: row.get(2)?,
                    commit: row.get(3)?,
                    added: row.get(4)?,
                    removed: row.get(5)?,
                    modified: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(versions)
    }

    pub fn version_position(&self, source_id: i64, name: &str) -> Result<Option<i64>> {
        let position = self
            .connection
            .query_row(
                "SELECT position FROM versions WHERE source_id = ?1 AND name = ?2",
                params![source_id, name],
                |row| row.get(0),
            )
            .optional()?;
        Ok(position)
    }

    pub fn truncate_versions(&self, source_id: i64, keep: usize) -> Result<()> {
        const NAMES_PER_TRANSACTION: i64 = 100_000;
        let mut newest_first = self.connection.prepare(
            "SELECT id FROM versions WHERE source_id = ?1 AND position >= ?2 ORDER BY position DESC",
        )?;
        let removed: Vec<i64> = newest_first
            .query_map(params![source_id, keep], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for version_id in removed {
            self.connection
                .execute("DELETE FROM versions WHERE id = ?1", [version_id])?;
        }

        let (first, last): (Option<i64>, Option<i64>) = self.connection.query_row(
            "SELECT min(id), max(id) FROM names WHERE source_id = ?1",
            [source_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let (Some(first), Some(last)) = (first, last) else {
            return Ok(());
        };
        for start in (first..=last).step_by(NAMES_PER_TRANSACTION as usize) {
            self.connection.execute(
                "DELETE FROM names WHERE source_id = ?1 AND id >= ?2 AND id < ?3
                   AND NOT EXISTS (SELECT 1 FROM events WHERE name_id = names.id)",
                params![source_id, start, start + NAMES_PER_TRANSACTION],
            )?;
        }
        Ok(())
    }

    pub fn insert_version(
        &self,
        source_id: i64,
        position: usize,
        version: &Version,
        events: &[Event],
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO versions (source_id, position, name, tag, commit_id, added, removed, modified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                source_id,
                position,
                version.name,
                version.tag,
                version.commit.to_string(),
                count(events, Action::Added),
                count(events, Action::Removed),
                count(events, Action::Modified)
            ],
        )?;
        let version_id = transaction.last_insert_rowid();

        let mut insert_name = transaction.prepare_cached(
            "INSERT INTO names (source_id, name) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
        )?;
        let mut select_name = transaction
            .prepare_cached("SELECT id FROM names WHERE source_id = ?1 AND name = ?2")?;
        let mut insert_event = transaction.prepare_cached(
            "INSERT INTO events (version_id, name_id, kind, action, path, line, signature,
                previous_path, previous_line, previous_signature)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for event in events {
            insert_name.execute(params![source_id, event.name])?;
            let name_id: i64 =
                select_name.query_row(params![source_id, event.name], |row| row.get(0))?;
            let previous = event.previous.as_ref();
            insert_event.execute(params![
                version_id,
                name_id,
                event.kind.as_str(),
                event.action.as_str(),
                event.definition.path,
                event.definition.line,
                event.definition.signature,
                previous.map(|previous| &previous.path),
                previous.map(|previous| previous.line),
                previous.map(|previous| &previous.signature),
            ])?;
        }
        drop((insert_name, select_name, insert_event));
        transaction.commit()?;
        Ok(())
    }

    pub fn names(&self, source_id: i64, pattern: &NamePattern, limit: u32) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT name FROM names
             WHERE source_id = ?1 AND name >= ?2 COLLATE NOCASE AND name < ?3 COLLATE NOCASE
               AND name REGEXP ?4
             ORDER BY length(name), name LIMIT ?5",
        )?;
        let names = statement
            .query_map(
                params![
                    source_id,
                    pattern.prefix,
                    pattern.prefix_end(),
                    pattern.regex,
                    limit
                ],
                |row| row.get(0),
            )?
            .collect::<rusqlite::Result<_>>()?;
        Ok(names)
    }

    pub fn events(&self, source: &Source, filter: &EventFilter) -> Result<EventPage> {
        const COLUMNS: &str = "events.id, versions.position, versions.name, versions.tag,
            previous_version.tag, names.name, events.kind, events.action, events.path, events.line,
            events.signature, events.previous_path, events.previous_line, events.previous_signature";
        const CONDITIONS: &str = "versions.source_id = ?1
            AND events.kind IN (SELECT value FROM json_each(?2))
            AND events.action IN (SELECT value FROM json_each(?3))
            AND (versions.position < ?4 OR (versions.position = ?4 AND events.id > ?5))
            AND (?6 IS NULL OR versions.position > ?6)
            AND (?7 IS NULL OR versions.position <= ?7)
            AND (?9 IS NULL OR substr(events.path, 1, length(?9)) = ?9
                 OR substr(events.previous_path, 1, length(?9)) = ?9)";
        const PREVIOUS_VERSION: &str = "LEFT JOIN versions AS previous_version
            ON previous_version.source_id = versions.source_id
           AND previous_version.position = versions.position - 1";
        const ORDER: &str = "ORDER BY versions.position DESC, events.id LIMIT ?8";

        let sql = match &filter.names {
            None => format!(
                "SELECT {COLUMNS} FROM versions
                 JOIN events ON events.version_id = versions.id
                 JOIN names ON names.id = events.name_id
                 {PREVIOUS_VERSION}
                 WHERE {CONDITIONS} {ORDER}"
            ),
            Some(_) => format!(
                "SELECT {COLUMNS} FROM names
                 CROSS JOIN events ON events.name_id = names.id
                 CROSS JOIN versions ON versions.id = events.version_id
                 {PREVIOUS_VERSION}
                 WHERE {CONDITIONS} AND names.source_id = ?1
                   AND names.name >= ?10 COLLATE NOCASE AND names.name < ?11 COLLATE NOCASE
                   AND names.name REGEXP ?12
                 {ORDER}"
            ),
        };
        let mut statement = self.connection.prepare_cached(&sql)?;
        let after = filter.after.unwrap_or(Cursor {
            position: i64::MAX,
            id: 0,
        });
        let kinds = serde_json::to_string(&filter.kinds)?;
        let actions = serde_json::to_string(&filter.actions)?;
        let limit = filter.limit + 1;
        let mut values: Vec<&dyn ToSql> = vec![
            &source.id,
            &kinds,
            &actions,
            &after.position,
            &after.id,
            &filter.since,
            &filter.until,
            &limit,
            &filter.path,
        ];
        let prefix_end = filter.names.as_ref().map(NamePattern::prefix_end);
        if let (Some(names), Some(prefix_end)) = (&filter.names, &prefix_end) {
            values.extend([&names.prefix as &dyn ToSql, prefix_end, &names.regex]);
        }
        let mut events = statement
            .query_map(values.as_slice(), |row| event_record(source, row))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut next = None;
        if events.len() > filter.limit as usize {
            events.truncate(filter.limit as usize);
            next = events.last().map(|event| {
                Cursor {
                    position: event.position,
                    id: event.id,
                }
                .to_string()
            });
        }
        Ok(EventPage { events, next })
    }
}

fn event_record(source: &Source, row: &Row) -> rusqlite::Result<EventRecord> {
    let tag: String = row.get(3)?;
    let previous_tag: Option<String> = row.get(4)?;
    let action: String = row.get(7)?;
    let definition_tag = if action == Action::Removed.as_str() {
        previous_tag
            .clone()
            .expect("a removed symbol existed in a previous version")
    } else {
        tag
    };
    let place = |tag: &str, path: String, line: u32, signature: String| Place {
        url: source.viewer_url(tag, &path, line),
        path,
        line,
        signature,
    };
    let previous_path: Option<String> = row.get(11)?;
    let previous = match previous_path {
        Some(path) => Some(place(
            previous_tag
                .as_deref()
                .expect("a modified symbol existed in a previous version"),
            path,
            row.get(12)?,
            row.get(13)?,
        )),
        None => None,
    };
    Ok(EventRecord {
        id: row.get(0)?,
        position: row.get(1)?,
        version: row.get(2)?,
        name: row.get(5)?,
        kind: row.get(6)?,
        action,
        definition: place(&definition_tag, row.get(8)?, row.get(9)?, row.get(10)?),
        previous,
    })
}

pub fn count(events: &[Event], action: Action) -> usize {
    events.iter().filter(|event| event.action == action).count()
}
