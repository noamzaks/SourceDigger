use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use regex::Regex;
use sourcedigger_index::git::{Indexer, fetch, steps, unreleased, versions};

use crate::db::Database;
use crate::digs::{Dig, DigScope};
use crate::settings::Settings;

pub struct Digger {
    pub settings: Settings,
}

impl Digger {
    const SCHEDULE_INTERVAL: Duration = Duration::from_secs(60);
    const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);

    pub fn mirror_path(repositories: &Path, source_id: i64) -> PathBuf {
        repositories.join(format!("{source_id}.git"))
    }

    pub fn run(&self, wake: Receiver<()>) -> Result<()> {
        let database = Database::open(&self.settings.database)?;
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
            &Self::mirror_path(&self.settings.repositories, source.id),
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
        let mut versions = versions(&repository, &Regex::new(&settings.tag_pattern)?)?;
        let Some(latest) = versions.last() else {
            bail!("no tag matches the tag pattern {}", settings.tag_pattern);
        };
        if let Some(unreleased) = unreleased(&repository, latest)? {
            versions.push(unreleased);
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

        let previous = kept.checked_sub(1).map(|position| &versions[position]);
        let mut indexer = Indexer::new(
            &repository,
            Regex::new(&settings.file_pattern)?,
            previous.map(|version| version.commit),
        );
        for (position, version) in versions.iter().enumerate().skip(kept) {
            let start = Instant::now();
            let previous = position.checked_sub(1).map(|position| &versions[position]);
            let mut indexed = Vec::new();
            for step in steps(&repository, previous, version)? {
                let events = indexer.index(step.commit)?;
                indexed.push((step, events));
            }
            let record = database.insert_version(source.id, position, version, &indexed)?;
            let line = format!(
                "{}: {} commits, {} changing symbols; {} added, {} removed, {} modified, {} signature in {:.1?}",
                version.name,
                indexed.len(),
                indexed
                    .iter()
                    .filter(|(_, events)| !events.is_empty())
                    .count(),
                record.added,
                record.removed,
                record.modified,
                record.signature,
                start.elapsed()
            );
            database.update_dig_progress(dig.id, position + 1, versions.len(), &line)?;
        }
        Ok(())
    }
}
