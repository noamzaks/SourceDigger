use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use git2::{
    AutotagOption, DiffFile, Direction, FetchOptions, FileMode, ObjectType, Oid, Remote,
    RemoteCallbacks, Repository,
};
use rayon::prelude::*;
use regex::Regex;

use crate::diff::{Event, FileSymbol, diff};
use crate::{Symbol, c};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub name: String,
    pub tag: String,
    pub commit: Oid,
}

pub(crate) fn open(path: &Path) -> Result<Repository> {
    Repository::open_bare(path).with_context(|| format!("opening {}", path.display()))
}

pub fn fetch(
    url: &str,
    path: &Path,
    mut on_progress: impl FnMut(usize, usize),
) -> Result<Repository> {
    let repository = if path.exists() {
        Repository::open_bare(path)
    } else {
        Repository::init_bare(path)
    }
    .with_context(|| format!("opening {}", path.display()))?;
    let mut callbacks = RemoteCallbacks::new();
    callbacks.transfer_progress(|progress| {
        on_progress(
            progress.received_objects() + progress.indexed_deltas(),
            progress.total_objects() + progress.total_deltas(),
        );
        true
    });
    let mut options = FetchOptions::new();
    options
        .remote_callbacks(callbacks)
        .download_tags(AutotagOption::None)
        .prune(git2::FetchPrune::On);
    repository
        .remote_anonymous(url)?
        .fetch(&["+refs/tags/*:refs/tags/*"], Some(&mut options), None)
        .with_context(|| format!("fetching tags from {url}"))?;
    Ok(repository)
}

pub fn versions(repository: &Repository, tag_pattern: &Regex) -> Result<Vec<Version>> {
    let mut tags = Vec::new();
    for tag in repository.tag_names(None)?.iter() {
        let Some(tag) = tag? else {
            continue;
        };
        let target = repository
            .find_reference(&format!("refs/tags/{tag}"))?
            .peel(ObjectType::Any)?;
        if target.kind() == Some(ObjectType::Commit) {
            tags.push((tag.to_string(), target.id()));
        }
    }
    Ok(select_versions(tags, tag_pattern))
}

pub fn remote_versions(url: &str, tag_pattern: &Regex) -> Result<Vec<Version>> {
    let mut remote = Remote::create_detached(url)?;
    remote
        .connect(Direction::Fetch)
        .with_context(|| format!("connecting to {url}"))?;
    let mut tags = HashMap::new();
    for head in remote.list()? {
        let Some(tag) = head.name().strip_prefix("refs/tags/") else {
            continue;
        };
        match tag.strip_suffix("^{}") {
            Some(tag) => {
                tags.insert(tag.to_string(), head.oid());
            }
            None => {
                tags.entry(tag.to_string()).or_insert(head.oid());
            }
        }
    }
    Ok(select_versions(tags.into_iter().collect(), tag_pattern))
}

fn select_versions(tags: Vec<(String, Oid)>, tag_pattern: &Regex) -> Vec<Version> {
    let mut versions: Vec<Version> = tags
        .into_iter()
        .filter_map(|(tag, commit)| {
            let captures = tag_pattern.captures(&tag)?;
            let name = captures
                .get(1)
                .or_else(|| captures.get(0))?
                .as_str()
                .to_string();
            Some(Version { name, tag, commit })
        })
        .collect();
    versions.sort_by(|left, right| {
        alphanumeric_sort::compare_str(&left.name, &right.name)
            .then_with(|| left.tag.cmp(&right.tag))
    });
    versions.dedup_by(|later, earlier| later.name == earlier.name);
    versions
}

pub struct Indexer<'repository> {
    repository: &'repository Repository,
    file_pattern: Regex,
    previous: Option<Version>,
    symbols: HashMap<Oid, Vec<Symbol>>,
}

impl<'repository> Indexer<'repository> {
    pub fn new(
        repository: &'repository Repository,
        file_pattern: Regex,
        previous: Option<Version>,
    ) -> Self {
        Self {
            repository,
            file_pattern,
            previous,
            symbols: HashMap::new(),
        }
    }

    pub fn index(&mut self, version: &Version) -> Result<Vec<Event>> {
        let old_tree = match &self.previous {
            Some(previous) => Some(self.repository.find_commit(previous.commit)?.tree()?),
            None => None,
        };
        let new_tree = self.repository.find_commit(version.commit)?.tree()?;
        let changes =
            self.repository
                .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;

        let mut old_files = Vec::new();
        let mut new_files = Vec::new();
        for delta in changes.deltas() {
            old_files.extend(self.source_file(delta.old_file()));
            new_files.extend(self.source_file(delta.new_file()));
        }

        let missing: HashSet<Oid> = old_files
            .iter()
            .chain(&new_files)
            .map(|(_, id)| *id)
            .filter(|id| !self.symbols.contains_key(id))
            .collect();
        let parsed = self.parse(missing)?;
        self.symbols.extend(parsed);

        let removed = self.file_symbols(&old_files);
        let added = self.file_symbols(&new_files);
        let new_ids: HashSet<Oid> = new_files.iter().map(|(_, id)| *id).collect();
        for (_, id) in &old_files {
            if !new_ids.contains(id) {
                self.symbols.remove(id);
            }
        }
        self.previous = Some(version.clone());
        Ok(diff(removed, added))
    }

    fn source_file(&self, file: DiffFile) -> Option<(String, Oid)> {
        if !file.exists() || !matches!(file.mode(), FileMode::Blob | FileMode::BlobExecutable) {
            return None;
        }
        let path = file.path()?.to_string_lossy();
        if !self.file_pattern.is_match(&path) {
            return None;
        }
        Some((path.into_owned(), file.id()))
    }

    fn parse(&self, ids: HashSet<Oid>) -> Result<Vec<(Oid, Vec<Symbol>)>> {
        let repository_path = self.repository.path();
        ids.into_par_iter()
            .map_init(
                || (open(repository_path), c::parser()),
                |(repository, parser), id| {
                    let repository = repository.as_ref().map_err(|error| anyhow!("{error:#}"))?;
                    let blob = repository.find_blob(id)?;
                    Ok((id, c::extract_symbols(parser, blob.content())))
                },
            )
            .collect()
    }

    fn file_symbols(&self, files: &[(String, Oid)]) -> Vec<FileSymbol> {
        let mut file_symbols = Vec::new();
        for (path, id) in files {
            for symbol in &self.symbols[id] {
                file_symbols.push(FileSymbol {
                    path: path.clone(),
                    symbol: symbol.clone(),
                });
            }
        }
        file_symbols
    }
}
