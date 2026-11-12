use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, Row, ToSql, Transaction, params};
use serde::Serialize;
use sourcedigger_index::diff::{Action, Event};
use sourcedigger_index::git::{Step, Version};

use crate::db::Database;
use crate::query::{NamePattern, Search, Term};
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
    pub signature: u32,
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
    pub search: Search,
    pub cursor: Option<Cursor>,
    pub limit: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub commit: i64,
    pub id: i64,
}

impl Display for Cursor {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "{}.{}", self.commit, self.id)
    }
}

impl FromStr for Cursor {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let (commit, id) = text
            .split_once('.')
            .with_context(|| format!("invalid cursor {text}"))?;
        Ok(Cursor {
            commit: commit.parse()?,
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
pub struct CommitRecord {
    #[serde(skip)]
    pub id: i64,
    pub hash: String,
    pub summary: String,
    pub author: String,
    pub time: i64,
}

#[derive(Debug, Serialize)]
pub struct EventRecord {
    pub id: i64,
    pub version: String,
    pub position: i64,
    pub commit: CommitRecord,
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
            "SELECT position, name, tag, commit_id, added, removed, modified, signature
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
                    signature: row.get(7)?,
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
        if removed.is_empty() {
            return Ok(());
        }
        for version_id in removed {
            self.connection
                .execute("DELETE FROM versions WHERE id = ?1", [version_id])?;
        }
        const SOURCE_EVENTS: &str =
            "events JOIN commits ON commits.id = events.commit_id WHERE commits.source_id = ?1";
        self.connection.execute(
            &format!(
                "DELETE FROM paths WHERE source_id = ?1 AND id NOT IN
                   (SELECT path_id FROM {SOURCE_EVENTS}
                    UNION SELECT previous_path_id FROM {SOURCE_EVENTS} AND previous_path_id IS NOT NULL)"
            ),
            [source_id],
        )?;
        self.connection.execute(
            &format!(
                "DELETE FROM signatures WHERE source_id = ?1 AND id NOT IN
                   (SELECT signature_id FROM {SOURCE_EVENTS}
                    UNION SELECT previous_signature_id FROM {SOURCE_EVENTS}
                      AND previous_signature_id IS NOT NULL)"
            ),
            [source_id],
        )?;

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
        steps: &[(Step, Vec<Event>)],
    ) -> Result<VersionRecord> {
        let count = |action| {
            let events = steps.iter().flat_map(|(_, events)| events);
            let count = events.filter(|event| event.action == action).count();
            u32::try_from(count).expect("a version has fewer than 2^32 events")
        };
        let record = VersionRecord {
            position: i64::try_from(position).expect("positions fit in i64"),
            name: version.name.clone(),
            tag: version.tag.clone(),
            commit: version.commit.to_string(),
            added: count(Action::Added),
            removed: count(Action::Removed),
            modified: count(Action::Modified),
            signature: count(Action::Signature),
        };
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO versions
               (source_id, position, name, tag, commit_id, added, removed, modified, signature)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                source_id,
                record.position,
                record.name,
                record.tag,
                record.commit,
                record.added,
                record.removed,
                record.modified,
                record.signature
            ],
        )?;
        let version_id = transaction.last_insert_rowid();

        let mut insert_commit = transaction.prepare_cached(
            "INSERT INTO commits (source_id, version_id, hash, summary, author, time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut names = Interned::new(&transaction, "names", "name")?;
        let mut paths = Interned::new(&transaction, "paths", "text")?;
        let mut signatures = Interned::new(&transaction, "signatures", "text")?;
        let mut insert_event = transaction.prepare_cached(
            "INSERT INTO events (commit_id, name_id, kind, action, path_id, line, signature_id,
                previous_path_id, previous_line, previous_signature_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for (step, events) in steps.iter().filter(|(_, events)| !events.is_empty()) {
            insert_commit.execute(params![
                source_id,
                version_id,
                step.commit.to_string(),
                step.summary,
                step.author,
                step.time
            ])?;
            let commit_id = transaction.last_insert_rowid();
            for event in events {
                let (previous_path, previous_line, previous_signature) = match &event.previous {
                    Some(previous) => (
                        Some(paths.id(source_id, &previous.path)?),
                        Some(previous.line),
                        Some(signatures.id(source_id, &previous.signature)?),
                    ),
                    None => (None, None, None),
                };
                insert_event.execute(params![
                    commit_id,
                    names.id(source_id, &event.name)?,
                    event.kind.as_str(),
                    event.action.as_str(),
                    paths.id(source_id, &event.definition.path)?,
                    event.definition.line,
                    signatures.id(source_id, &event.definition.signature)?,
                    previous_path,
                    previous_line,
                    previous_signature,
                ])?;
            }
        }
        drop((insert_commit, names, paths, signatures, insert_event));
        transaction.commit()?;
        Ok(record)
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
            previous_version.tag, names.name, events.kind, events.action, paths.text, events.line,
            signatures.text, previous_paths.text, events.previous_line, previous_signatures.text,
            commits.id, commits.hash, commits.summary, commits.author, commits.time";
        const TEXTS: &str = "CROSS JOIN paths ON paths.id = events.path_id
            CROSS JOIN signatures ON signatures.id = events.signature_id
            LEFT JOIN paths AS previous_paths ON previous_paths.id = events.previous_path_id
            LEFT JOIN signatures AS previous_signatures
              ON previous_signatures.id = events.previous_signature_id";
        const PREVIOUS_VERSION: &str = "LEFT JOIN versions AS previous_version
            ON previous_version.source_id = versions.source_id
           AND previous_version.position = versions.position - 1";
        const ORDER: &str = "ORDER BY commits.id DESC, events.id LIMIT :limit";

        let mut condition = Condition::default();
        let search = condition.sql(&filter.search);
        let conditions = format!(
            "commits.source_id = :source
             AND (commits.id < :cursor_commit OR (commits.id = :cursor_commit AND events.id > :cursor_event))
             AND {search}"
        );
        let sql = match &filter.names {
            None => format!(
                "SELECT {COLUMNS} FROM commits
                 JOIN events ON events.commit_id = commits.id
                 JOIN versions ON versions.id = commits.version_id
                 JOIN names ON names.id = events.name_id
                 {TEXTS}
                 {PREVIOUS_VERSION}
                 WHERE {conditions} {ORDER}"
            ),
            Some(_) => format!(
                "SELECT {COLUMNS} FROM names
                 CROSS JOIN events ON events.name_id = names.id
                 CROSS JOIN commits ON commits.id = events.commit_id
                 CROSS JOIN versions ON versions.id = commits.version_id
                 {TEXTS}
                 {PREVIOUS_VERSION}
                 WHERE {conditions} AND names.source_id = :source
                   AND names.name >= :prefix COLLATE NOCASE AND names.name < :prefix_end COLLATE NOCASE
                   AND names.name REGEXP :names
                 {ORDER}"
            ),
        };
        let mut statement = self.connection.prepare_cached(&sql)?;
        let cursor = filter.cursor.unwrap_or(Cursor {
            commit: i64::MAX,
            id: 0,
        });
        let limit = filter.limit + 1;
        let mut parameters: Vec<(&str, &dyn ToSql)> = vec![
            (":source", &source.id),
            (":cursor_commit", &cursor.commit),
            (":cursor_event", &cursor.id),
            (":limit", &limit),
        ];
        for (name, value) in &condition.parameters {
            parameters.push((name, value));
        }
        let prefix_end = filter.names.as_ref().map(NamePattern::prefix_end);
        if let (Some(names), Some(prefix_end)) = (&filter.names, &prefix_end) {
            parameters.extend([
                (":prefix", &names.prefix as &dyn ToSql),
                (":prefix_end", prefix_end),
                (":names", &names.regex),
            ]);
        }
        let mut events = statement
            .query_map(parameters.as_slice(), |row| event_record(source, row))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut next = None;
        if events.len() > filter.limit as usize {
            events.truncate(filter.limit as usize);
            next = events.last().map(|event| {
                Cursor {
                    commit: event.commit.id,
                    id: event.id,
                }
                .to_string()
            });
        }
        Ok(EventPage { events, next })
    }
}

#[derive(Default)]
struct Condition {
    parameters: Vec<(String, String)>,
}

impl Condition {
    fn sql(&mut self, search: &Search) -> String {
        match search {
            Search::Term(term) => format!("({})", self.term(term)),
            Search::Not(inner) => format!("(NOT {})", self.sql(inner)),
            Search::And(parts) => self.joined(parts, "AND", "1"),
            Search::Or(parts) => self.joined(parts, "OR", "0"),
        }
    }

    fn joined(&mut self, parts: &[Search], operator: &str, empty: &str) -> String {
        if parts.is_empty() {
            return empty.to_string();
        }
        let conditions: Vec<String> = parts.iter().map(|part| self.sql(part)).collect();
        format!("({})", conditions.join(&format!(" {operator} ")))
    }

    fn term(&mut self, term: &Term) -> String {
        match term.field.as_str() {
            "name" => match term.name_pattern() {
                Some(pattern) => format!("names.name REGEXP {}", self.add(pattern.regex)),
                None => "1".to_string(),
            },
            "kind" => format!("events.kind IN {}", self.choices(term)),
            "action" => format!("events.action IN {}", self.choices(term)),
            "version" => format!("versions.position = {}", self.version(term)),
            "after" => format!("versions.position > {}", self.version(term)),
            "before" => format!("versions.position < {}", self.version(term)),
            "path" => {
                let regex = self.add(term.path_regex());
                format!(
                    "paths.text REGEXP {regex} OR coalesce(previous_paths.text REGEXP {regex}, 0)"
                )
            }
            "commit" => {
                let start = self.add(term.value.clone());
                format!("substr(commits.hash, 1, length({start})) = {start}")
            }
            field => unreachable!("fields are checked when parsed, not {field:?}"),
        }
    }

    fn version(&mut self, term: &Term) -> String {
        let name = self.add(term.value.clone());
        format!("(SELECT position FROM versions WHERE source_id = :source AND name = {name})")
    }

    fn choices(&mut self, term: &Term) -> String {
        let names: Vec<String> = term
            .choices()
            .into_iter()
            .map(|choice| self.add(choice.to_string()))
            .collect();
        format!("({})", names.join(", "))
    }

    fn add(&mut self, value: String) -> String {
        let name = format!(":value{}", self.parameters.len());
        self.parameters.push((name.clone(), value));
        name
    }
}

struct Interned<'transaction> {
    insert: rusqlite::CachedStatement<'transaction>,
    select: rusqlite::CachedStatement<'transaction>,
}

impl<'transaction> Interned<'transaction> {
    fn new(transaction: &'transaction Transaction, table: &str, column: &str) -> Result<Self> {
        Ok(Self {
            insert: transaction.prepare_cached(&format!(
                "INSERT INTO {table} (source_id, {column}) VALUES (?1, ?2)"
            ))?,
            select: transaction.prepare_cached(&format!(
                "SELECT id FROM {table} WHERE source_id = ?1 AND {column} = ?2"
            ))?,
        })
    }

    fn id(&mut self, source_id: i64, text: &str) -> Result<i64> {
        let found = self
            .select
            .query_row(params![source_id, text], |row| row.get(0))
            .optional()?;
        if let Some(id) = found {
            return Ok(id);
        }
        Ok(self.insert.insert(params![source_id, text])?)
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
        commit: CommitRecord {
            id: row.get(14)?,
            hash: row.get(15)?,
            summary: row.get(16)?,
            author: row.get(17)?,
            time: row.get(18)?,
        },
    })
}
