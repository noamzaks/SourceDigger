use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use git2::{Oid, Repository};

use crate::git::in_parallel;
use crate::{Kind, c};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Code {
    pub kind: Kind,
    pub path: String,
    pub line: u32,
    pub end_line: u32,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileAt {
    pub commit: Oid,
    pub path: String,
}

pub fn read_definitions(
    repository: &Repository,
    name: &str,
    files: &[FileAt],
) -> Result<Vec<Vec<Code>>> {
    let blobs = in_parallel(repository, files.iter().collect(), |repository, _, file| {
        blob_at(repository, file)
    })?;

    let unique: HashMap<Oid, &str> = blobs
        .iter()
        .copied()
        .zip(files.iter().map(|file| file.path.as_str()))
        .collect();
    let parsed: HashMap<Oid, Vec<Code>> = in_parallel(
        repository,
        unique.into_iter().collect(),
        |repository, parser, (blob, path)| {
            let content = repository.find_blob(blob)?;
            Ok((blob, definitions(parser, name, path, content.content())))
        },
    )?
    .into_iter()
    .collect();

    Ok(blobs
        .iter()
        .zip(files)
        .map(|(blob, file)| {
            parsed[blob]
                .iter()
                .map(|code| Code {
                    path: file.path.clone(),
                    ..code.clone()
                })
                .collect()
        })
        .collect())
}

fn blob_at(repository: &Repository, file: &FileAt) -> Result<Oid> {
    let tree = repository.find_commit(file.commit)?.tree()?;
    let entry = tree
        .get_path(Path::new(&file.path))
        .with_context(|| format!("{} in {}", file.path, file.commit))?;
    Ok(entry.id())
}

fn definitions(
    parser: &mut tree_sitter::Parser,
    name: &str,
    path: &str,
    content: &[u8],
) -> Vec<Code> {
    let mut definitions = Vec::new();
    for symbol in c::extract_symbols(parser, content) {
        if symbol.name != name {
            continue;
        }
        let text = String::from_utf8_lossy(&content[symbol.span.clone()])
            .trim_end()
            .to_string();
        let line = 1 + count_lines(&content[..symbol.span.start]);
        definitions.push(Code {
            kind: symbol.kind,
            path: path.to_string(),
            line,
            end_line: line + count_lines(text.as_bytes()),
            text,
        });
    }
    definitions
}

fn count_lines(bytes: &[u8]) -> u32 {
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count();
    u32::try_from(newlines).expect("a source file has fewer than 2^32 lines")
}
