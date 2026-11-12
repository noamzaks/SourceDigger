use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use regex::Regex;
use sourcedigger_index::diff::Action;
use sourcedigger_index::git::{Indexer, fetch, versions};

use crate::db::Database;
use crate::digs::{Dig, DigScope};
use crate::symbols::count;

pub struct Digger {
    pub database_path: PathBuf,
    pub repositories: PathBuf,
}

impl Digger {
    const SCHEDULE_INTERVAL: Duration = Duration::from_secs(60);
    const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);

    pub fn mirror_path(repositories: &Path, source_id: i64) -> PathBuf {
        repositories.join(format!("{source_id}.git"))
    }

    pub fn run(&self, wake: Receiver<()>) -> Result<()> {
        let database = Database::open(&self.database_path)?;
        database.fail_interrupted_digs()?;
        loop {
            if let Err(error) = self.dig_queued(&database) {
                tracing::error!("digging stopped until the next round: {error:#}");
            }
            match wake.recv_timeout(Self::SCHEDULE_INTERVAL) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }

    fn dig_queued(&self, database: &Database) -> Result<()> {
        database.queue_due_digs()?;
        while let Some(dig) = database.next_queued_dig()? {
            database.start_dig(dig.id)?;
            tracing::info!(dig = dig.id, source = dig.source_id, "digging");
            match self.dig(database, &dig) {
                Ok(()) => database.succeed_dig(dig.id)?,
                Err(error) => {
                    tracing::warn!(dig = dig.id, "dig failed: {error:#}");
                    database.fail_dig(dig.id, &format!("{error:#}"))?;
                }
            }
        }
        Ok(())
    }

    fn dig(&self, database: &Database, dig: &Dig) -> Result<()> {
        let source = database.source_by_id(dig.source_id)?;
        let settings = &source.settings;
        let mut last_report = Instant::now();
        let mut report_error = None;
        let fetched = fetch(
            &settings.url,
            &Self::mirror_path(&self.repositories, source.id),
            |received, total| {
                if last_report.elapsed() < Self::PROGRESS_INTERVAL || report_error.is_some() {
                    return;
                }
                last_report = Instant::now();
                if let Err(error) = database.update_dig_fetch(dig.id, received, total) {
                    report_error = Some(error);
                }
            },
        );
        if let Some(error) = report_error {
            return Err(error);
        }
        let repository = fetched?;
        let versions = versions(&repository, &Regex::new(&settings.tag_pattern)?)?;
        if versions.is_empty() {
            bail!("no tag matches the tag pattern {}", settings.tag_pattern);
        }

        if dig.from == DigScope::Scratch {
            database.truncate_versions(source.id, 0)?;
        }
        let indexed = database.versions(source.id)?;
        let kept = indexed
            .iter()
            .zip(&versions)
            .take_while(|(indexed, version)| indexed.is(version))
            .count();
        database.truncate_versions(source.id, kept)?;
        database.update_dig_progress(
            dig.id,
            kept,
            versions.len(),
            &format!(
                "{} versions found, {kept} already dug, {} dropped",
                versions.len(),
                indexed.len() - kept
            ),
        )?;

        let previous = kept
            .checked_sub(1)
            .map(|position| versions[position].clone());
        let mut indexer = Indexer::new(&repository, Regex::new(&settings.file_pattern)?, previous);
        for (position, version) in versions.iter().enumerate().skip(kept) {
            let start = Instant::now();
            let events = indexer.index(version)?;
            database.insert_version(source.id, position, version, &events)?;
            let line = format!(
                "{}: {} added, {} removed, {} modified in {:.1?}",
                version.name,
                count(&events, Action::Added),
                count(&events, Action::Removed),
                count(&events, Action::Modified),
                start.elapsed()
            );
            database.update_dig_progress(dig.id, position + 1, versions.len(), &line)?;
        }
        Ok(())
    }
}
