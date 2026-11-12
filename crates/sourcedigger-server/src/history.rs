use std::collections::BTreeMap;

use anyhow::Result;
use git2::Oid;
use git2::Repository;
use serde::Serialize;
use sourcedigger_index::code::{FileAt, read_definitions};
use sourcedigger_index::diff::Action;

use crate::sources::Source;
use crate::symbols::{EventRecord, VersionRecord};

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Definition {
    pub kind: String,
    pub path: String,
    pub line: u32,
    pub end_line: u32,
    pub url: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct Revision {
    pub version: String,
    pub position: i64,
    pub definitions: Vec<Definition>,
}

type Places = BTreeMap<(String, String), u32>;

pub struct History<'repository> {
    repository: &'repository Repository,
    source: &'repository Source,
    name: String,
    places: Vec<(i64, Places)>,
}

impl<'repository> History<'repository> {
    pub fn new(
        repository: &'repository Repository,
        source: &'repository Source,
        name: &str,
        events: &[EventRecord],
        path: Option<&str>,
    ) -> Self {
        let mut places = Vec::new();
        let mut current = Places::new();
        let mut ordered: Vec<&EventRecord> = events.iter().collect();
        ordered.sort_by_key(|event| (event.position, event.id));
        for event in ordered {
            let key = (event.kind.clone(), event.definition.path.clone());
            let previous_key = event
                .previous
                .as_ref()
                .map(|previous| (event.kind.clone(), previous.path.clone()));
            if event.action == Action::Removed.as_str() {
                remove(&mut current, key);
            } else {
                if let Some(previous_key) = previous_key {
                    remove(&mut current, previous_key);
                }
                *current.entry(key).or_default() += 1;
            }
            let mut kept = current.clone();
            if let Some(path) = path {
                kept.retain(|(_, place_path), _| place_path == path);
            }
            if places.is_empty() && kept.is_empty() {
                continue;
            }
            match places.last_mut() {
                Some((position, last)) if *position == event.position => *last = kept,
                _ => places.push((event.position, kept)),
            }
        }
        Self {
            repository,
            source,
            name: name.to_string(),
            places,
        }
    }

    pub fn at(&self, version: &VersionRecord) -> Result<Revision> {
        let mut revisions = self.revisions_in(&[version])?;
        Ok(revisions.remove(0))
    }

    pub fn revisions(&self, versions: &[VersionRecord]) -> Result<Vec<Revision>> {
        let Some((first, _)) = self.places.first() else {
            return Ok(Vec::new());
        };
        let wanted: Vec<&VersionRecord> = versions
            .iter()
            .filter(|version| version.position >= *first)
            .collect();
        let mut revisions: Vec<Revision> = Vec::new();
        for revision in self.revisions_in(&wanted)? {
            let changed = match revisions.last() {
                Some(last) => !same_code(&last.definitions, &revision.definitions),
                None => true,
            };
            if changed {
                revisions.push(revision);
            }
        }
        Ok(revisions)
    }

    fn revisions_in(&self, versions: &[&VersionRecord]) -> Result<Vec<Revision>> {
        let mut files = Vec::new();
        let mut owners = Vec::new();
        for (index, version) in versions.iter().enumerate() {
            let commit = Oid::from_str(&version.commit)?;
            for (kind, path) in self.places_at(version.position) {
                files.push(FileAt {
                    commit,
                    path: path.clone(),
                });
                owners.push((index, kind));
            }
        }
        let codes = read_definitions(self.repository, &self.name, &files)?;

        let mut revisions: Vec<Revision> = versions
            .iter()
            .map(|version| Revision {
                version: version.name.clone(),
                position: version.position,
                definitions: Vec::new(),
            })
            .collect();
        for ((index, kind), codes) in owners.into_iter().zip(codes) {
            for code in codes {
                if code.kind.as_str() != kind {
                    continue;
                }
                revisions[index].definitions.push(Definition {
                    kind: kind.clone(),
                    url: self
                        .source
                        .viewer_url(&versions[index].tag, &code.path, code.line),
                    path: code.path,
                    line: code.line,
                    end_line: code.end_line,
                    code: code.text,
                });
            }
        }
        Ok(revisions)
    }

    fn places_at(&self, position: i64) -> impl Iterator<Item = &(String, String)> {
        self.places
            .iter()
            .rev()
            .find(|(start, _)| *start <= position)
            .into_iter()
            .flat_map(|(_, places)| places.keys())
    }
}

fn remove(places: &mut Places, key: (String, String)) {
    let count = places
        .get_mut(&key)
        .expect("a definition is removed only after it was added");
    *count -= 1;
    if *count == 0 {
        places.remove(&key);
    }
}

fn same_code(left: &[Definition], right: &[Definition]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.path == right.path && left.code == right.code)
}
