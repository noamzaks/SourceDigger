use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

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
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct FileChange {
    pub path: String,
    pub moved_from: Option<String>,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Serialize)]
pub struct Change {
    pub version: String,
    pub previous_version: Option<String>,
    pub action: &'static str,
    pub files: Vec<FileChange>,
}

pub fn changes(revisions: &[Revision]) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut previous: Option<&Revision> = None;
    for revision in revisions {
        let before = previous.map_or_else(BTreeMap::new, |previous| previous.files.clone());
        let after = revision.files.clone();
        let action = if before.is_empty() {
            Action::Added
        } else if after.is_empty() {
            Action::Removed
        } else {
            Action::Modified
        };
        changes.push(Change {
            version: revision.version.clone(),
            previous_version: previous.map(|previous| previous.version.clone()),
            action: action.as_str(),
            files: file_changes(before, after),
        });
        previous = Some(revision);
    }
    changes.reverse();
    changes
}

fn by_file(definitions: &[Definition]) -> BTreeMap<String, String> {
    let mut files: BTreeMap<&str, Vec<&Definition>> = BTreeMap::new();
    for definition in definitions {
        files.entry(&definition.path).or_default().push(definition);
    }
    files
        .into_iter()
        .map(|(path, mut definitions)| {
            definitions.sort_by_key(|definition| {
                (
                    definition.line,
                    Reverse(definition.end_line),
                    Reverse(definition.code.len()),
                )
            });
            let mut codes: Vec<&str> = Vec::new();
            let mut covered_until = 0;
            for definition in definitions {
                if definition.line > covered_until {
                    codes.push(&definition.code);
                    covered_until = definition.end_line;
                }
            }
            (path.to_string(), codes.join("\n\n"))
        })
        .collect()
}

fn file_changes(
    mut before: BTreeMap<String, String>,
    mut after: BTreeMap<String, String>,
) -> Vec<FileChange> {
    let mut files = Vec::new();
    let gone: Vec<String> = before
        .keys()
        .filter(|path| !after.contains_key(*path))
        .cloned()
        .collect();
    let new: Vec<String> = after
        .keys()
        .filter(|path| !before.contains_key(*path))
        .cloned()
        .collect();
    if let ([from], [to]) = (gone.as_slice(), new.as_slice()) {
        files.push(FileChange {
            path: to.clone(),
            moved_from: Some(from.clone()),
            before: before.remove(from).expect("a gone file was there before"),
            after: after.remove(to).expect("a new file is there after"),
        });
    }
    let paths: BTreeSet<String> = before.keys().chain(after.keys()).cloned().collect();
    for path in paths {
        let old = before.remove(&path).unwrap_or_default();
        let new = after.remove(&path).unwrap_or_default();
        if old != new {
            files.push(FileChange {
                path,
                moved_from: None,
                before: old,
                after: new,
            });
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

type Places = BTreeMap<(String, String), u32>;

pub struct History {
    repository: Repository,
    pub source: Source,
    name: String,
    places: Vec<(i64, Places)>,
}

impl History {
    pub fn new(
        repository: Repository,
        source: Source,
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
        let codes = read_definitions(&self.repository, &self.name, &files)?;

        let mut revisions: Vec<Revision> = versions
            .iter()
            .map(|version| Revision {
                version: version.name.clone(),
                position: version.position,
                definitions: Vec::new(),
                files: BTreeMap::new(),
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
        for revision in &mut revisions {
            revision.files = by_file(&revision.definitions);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn revision(version: &str, definitions: &[(&str, u32, &str)]) -> Revision {
        let definitions: Vec<Definition> = definitions
            .iter()
            .map(|(path, line, code)| Definition {
                kind: "struct".to_string(),
                path: path.to_string(),
                line: *line,
                end_line: *line,
                url: String::new(),
                code: code.to_string(),
            })
            .collect();
        Revision {
            version: version.to_string(),
            position: 0,
            files: by_file(&definitions),
            definitions,
        }
    }

    fn file(path: &str, moved_from: Option<&str>, before: &str, after: &str) -> FileChange {
        FileChange {
            path: path.to_string(),
            moved_from: moved_from.map(str::to_string),
            before: before.to_string(),
            after: after.to_string(),
        }
    }

    #[test]
    fn changes_are_per_file() {
        let revisions = [
            revision("1.0", &[("a.c", 1, "int x;"), ("b.c", 1, "int x;")]),
            revision(
                "1.1",
                &[
                    ("a.c", 9, "struct x {}"),
                    ("a.c", 9, "struct x {} y;"),
                    ("a.c", 1, "int x;"),
                    ("b.c", 1, "int x;"),
                ],
            ),
            revision("1.2", &[("a.c", 1, "int x;"), ("c/b.c", 1, "long x;")]),
            revision("1.3", &[]),
        ];
        let changes = changes(&revisions);
        let summary: Vec<_> = changes
            .iter()
            .map(|change| {
                (
                    change.version.as_str(),
                    change.previous_version.as_deref(),
                    change.action,
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("1.3", Some("1.2"), "removed"),
                ("1.2", Some("1.1"), "modified"),
                ("1.1", Some("1.0"), "modified"),
                ("1.0", None, "added"),
            ]
        );
        assert_eq!(
            changes[2].files,
            vec![file("a.c", None, "int x;", "int x;\n\nstruct x {} y;")]
        );
        assert_eq!(
            changes[1].files,
            vec![
                file("a.c", None, "int x;\n\nstruct x {} y;", "int x;"),
                file("c/b.c", Some("b.c"), "int x;", "long x;"),
            ]
        );
        assert_eq!(changes[0].files.len(), 2);
        assert_eq!(changes[3].files.len(), 2);
    }
}
