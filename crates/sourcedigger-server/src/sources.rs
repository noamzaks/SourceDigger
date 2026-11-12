use anyhow::{Result, bail};
use regex::Regex;
use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::digs::Dig;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SourceSettings {
    pub name: String,
    pub url: String,
    pub tag_pattern: String,
    pub file_pattern: String,
    pub viewer_url: String,
    pub refresh_hours: Option<u32>,
}

impl SourceSettings {
    pub fn validate(&self) -> Result<()> {
        let valid_name = Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]*$").expect("valid regex");
        if !valid_name.is_match(&self.name) {
            bail!("the name must be letters, digits, '.', '_' or '-'");
        }
        if self.url.trim().is_empty() {
            bail!("the URL is empty");
        }
        if let Err(error) = Regex::new(&self.tag_pattern) {
            bail!("invalid tag pattern: {error}");
        }
        if let Err(error) = Regex::new(&self.file_pattern) {
            bail!("invalid file pattern: {error}");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Source {
    pub id: i64,
    #[serde(flatten)]
    pub settings: SourceSettings,
}

impl Source {
    const COLUMNS: &str = "id, name, url, tag_pattern, file_pattern, viewer_url, refresh_hours";

    pub fn viewer_url(&self, tag: &str, path: &str, line: u32) -> String {
        self.settings
            .viewer_url
            .replace("{tag}", tag)
            .replace("{path}", path)
            .replace("{line}", &line.to_string())
    }

    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            settings: SourceSettings {
                name: row.get(1)?,
                url: row.get(2)?,
                tag_pattern: row.get(3)?,
                file_pattern: row.get(4)?,
                viewer_url: row.get(5)?,
                refresh_hours: row.get(6)?,
            },
        })
    }
}

#[derive(Debug, Serialize)]
pub struct SourceSummary {
    #[serde(flatten)]
    pub source: Source,
    pub version_count: u32,
    pub first_version: Option<String>,
    pub last_version: Option<String>,
    pub last_dig: Option<Dig>,
}

impl Database {
    pub fn sources(&self) -> Result<Vec<Source>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {} FROM sources ORDER BY name",
            Source::COLUMNS
        ))?;
        let sources = statement
            .query_map([], Source::from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(sources)
    }

    pub fn source(&self, name: &str) -> Result<Option<Source>> {
        let source = self
            .connection
            .query_row(
                &format!("SELECT {} FROM sources WHERE name = ?1", Source::COLUMNS),
                [name],
                Source::from_row,
            )
            .optional()?;
        Ok(source)
    }

    pub fn source_by_id(&self, id: i64) -> Result<Source> {
        let source = self.connection.query_row(
            &format!("SELECT {} FROM sources WHERE id = ?1", Source::COLUMNS),
            [id],
            Source::from_row,
        )?;
        Ok(source)
    }

    pub fn summarize(&self, source: Source) -> Result<SourceSummary> {
        let (version_count, first_version, last_version) = self.connection.query_row(
            "SELECT count(*),
                (SELECT name FROM versions WHERE source_id = ?1 ORDER BY position LIMIT 1),
                (SELECT name FROM versions WHERE source_id = ?1 ORDER BY position DESC LIMIT 1)
             FROM versions WHERE source_id = ?1",
            [source.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let last_dig = self.digs(source.id, 1)?.into_iter().next();
        Ok(SourceSummary {
            source,
            version_count,
            first_version,
            last_version,
            last_dig,
        })
    }

    pub fn create_source(&self, settings: SourceSettings) -> Result<Source> {
        self.connection.execute(
            "INSERT INTO sources (name, url, tag_pattern, file_pattern, viewer_url, refresh_hours)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                settings.name,
                settings.url,
                settings.tag_pattern,
                settings.file_pattern,
                settings.viewer_url,
                settings.refresh_hours
            ],
        )?;
        Ok(Source {
            id: self.connection.last_insert_rowid(),
            settings,
        })
    }

    pub fn update_source(&self, id: i64, settings: SourceSettings) -> Result<Source> {
        if self.source_by_id(id)?.settings.file_pattern != settings.file_pattern {
            self.truncate_versions(id, 0)?;
        }
        self.connection.execute(
            "UPDATE sources SET name = ?2, url = ?3, tag_pattern = ?4, file_pattern = ?5,
                viewer_url = ?6, refresh_hours = ?7
             WHERE id = ?1",
            params![
                id,
                settings.name,
                settings.url,
                settings.tag_pattern,
                settings.file_pattern,
                settings.viewer_url,
                settings.refresh_hours
            ],
        )?;
        Ok(Source { id, settings })
    }

    pub fn delete_source(&self, id: i64) -> Result<()> {
        self.connection
            .execute("DELETE FROM sources WHERE id = ?1", [id])?;
        Ok(())
    }
}
