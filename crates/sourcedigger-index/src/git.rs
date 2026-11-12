use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use git2::{DiffFile, FileMode, ObjectType, Oid, Repository};
use rayon::prelude::*;
use regex::Regex;
use tree_sitter::Parser;

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

pub(crate) fn in_parallel<T: Send, R: Send>(
    repository: &Repository,
    items: Vec<T>,
    work: impl Fn(&Repository, &mut Parser, T) -> Result<R> + Sync + Send,
) -> Result<Vec<R>> {
    const PARALLEL_FROM: usize = 16;
    if items.len() < PARALLEL_FROM {
        let mut parser = c::parser();
        return items
            .into_iter()
            .map(|item| work(repository, &mut parser, item))
            .collect();
    }
    let path = repository.path();
    items
        .into_par_iter()
        .map_init(
            || (open(path), c::parser()),
            |(opened, parser), item| {
                let opened = opened.as_ref().map_err(|error| anyhow!("{error:#}"))?;
                work(opened, parser, item)
            },
        )
        .collect()
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
    let branch = default_branch(url)?;
    let mut child = git_command()
        .arg("--git-dir")
        .arg(path)
        .args([
            "fetch",
            "--progress",
            "--prune",
            "--no-tags",
            "--no-auto-maintenance",
            "--",
            url,
            "+refs/tags/*:refs/tags/*",
            &format!("+{branch}:{branch}"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("running git fetch")?;
    let stderr = child.stderr.take().expect("stderr is piped");
    let messages = read_fetch_output(stderr, &mut on_progress);
    let status = child.wait().context("waiting for git fetch")?;
    let messages = messages.context("reading the output of git fetch")?;
    if !status.success() {
        bail!(
            "fetching tags and {branch} from {url} failed with {status}:\n{}",
            messages.join("\n")
        );
    }
    repository.set_head(&branch)?;
    Ok(repository)
}

fn git_command() -> Command {
    let mut command = Command::new("git");
    command.env("GIT_TERMINAL_PROMPT", "0").stdin(Stdio::null());
    command
}

fn ls_remote(arguments: &[&str]) -> Result<String> {
    let output = git_command()
        .arg("ls-remote")
        .args(arguments)
        .output()
        .context("running git ls-remote")?;
    if !output.status.success() {
        bail!(
            "git ls-remote {} failed with {}:\n{}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("git ls-remote printed something that is not UTF-8")
}

fn default_branch(url: &str) -> Result<String> {
    let references = ls_remote(&["--symref", "--", url, "HEAD"])?;
    for line in references.lines() {
        let Some(symbolic) = line.strip_prefix("ref: ") else {
            continue;
        };
        let (branch, _) = symbolic
            .split_once('\t')
            .with_context(|| format!("unexpected line from git ls-remote: {line}"))?;
        return Ok(branch.to_string());
    }
    bail!("{url} has no default branch:\n{references}")
}

fn read_fetch_output(
    stderr: impl Read,
    on_progress: &mut impl FnMut(usize, usize),
) -> Result<Vec<String>> {
    let mut messages = Vec::new();
    let mut objects = (0, 0);
    let mut deltas = (0, 0);
    for update in BufReader::new(stderr).split(b'\r') {
        let update = update?;
        for line in String::from_utf8_lossy(&update).lines() {
            match parse_progress(line) {
                Some(("Receiving objects" | "Unpacking objects", counts)) => objects = counts,
                Some(("Resolving deltas", counts)) => deltas = counts,
                Some(_) => continue,
                None => {
                    if !line.trim().is_empty() {
                        messages.push(line.trim().to_string());
                    }
                    continue;
                }
            }
            on_progress(objects.0 + deltas.0, objects.1 + deltas.1);
        }
    }
    Ok(messages)
}

fn parse_progress(line: &str) -> Option<(&str, (usize, usize))> {
    let (before, after) = line.split_once("% (")?;
    let (label, _) = before.rsplit_once(':')?;
    let (counts, _) = after.split_once(')')?;
    let (done, total) = counts.split_once('/')?;
    Some((label.trim(), (done.parse().ok()?, total.parse().ok()?)))
}

pub const UNRELEASED: &str = "unreleased";

pub fn unreleased(repository: &Repository, latest: &Version) -> Result<Option<Version>> {
    let head = repository.head()?;
    let commit = head.peel_to_commit()?.id();
    if commit == latest.commit || !repository.graph_descendant_of(commit, latest.commit)? {
        return Ok(None);
    }
    Ok(Some(Version {
        name: UNRELEASED.to_string(),
        tag: head.shorthand().unwrap_or(UNRELEASED).to_string(),
        commit,
    }))
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
    let references = ls_remote(&["--tags", "--", url])?;
    let mut tags = HashMap::new();
    for line in references.lines() {
        let (id, name) = line
            .split_once('\t')
            .with_context(|| format!("unexpected line from git ls-remote: {line}"))?;
        let Some(tag) = name.strip_prefix("refs/tags/") else {
            continue;
        };
        let id = Oid::from_str(id)?;
        match tag.strip_suffix("^{}") {
            Some(tag) => {
                tags.insert(tag.to_string(), id);
            }
            None => {
                tags.entry(tag.to_string()).or_insert(id);
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub commit: Oid,
    pub summary: String,
    pub author: String,
    pub time: i64,
}

pub fn steps(
    repository: &Repository,
    previous: Option<&Version>,
    version: &Version,
) -> Result<Vec<Step>> {
    let Some(previous) = previous else {
        return Ok(vec![step(repository, version.commit)?]);
    };
    let mut walk = repository.revwalk()?;
    walk.simplify_first_parent()?;
    walk.push(version.commit)?;
    walk.hide(previous.commit)?;
    let mut steps = walk
        .map(|commit| step(repository, commit?))
        .collect::<Result<Vec<_>>>()?;
    steps.reverse();
    Ok(steps)
}

fn step(repository: &Repository, commit: Oid) -> Result<Step> {
    let commit = repository.find_commit(commit)?;
    Ok(Step {
        commit: commit.id(),
        summary: String::from_utf8_lossy(commit.summary_bytes().unwrap_or_default()).into_owned(),
        author: String::from_utf8_lossy(commit.author().name_bytes()).into_owned(),
        time: commit.time().seconds(),
    })
}

pub struct Indexer<'repository> {
    repository: &'repository Repository,
    file_pattern: Regex,
    previous: Option<Oid>,
    symbols: HashMap<Oid, Vec<Symbol>>,
}

impl<'repository> Indexer<'repository> {
    pub fn new(
        repository: &'repository Repository,
        file_pattern: Regex,
        previous: Option<Oid>,
    ) -> Self {
        Self {
            repository,
            file_pattern,
            previous,
            symbols: HashMap::new(),
        }
    }

    pub fn index(&mut self, commit: Oid) -> Result<Vec<Event>> {
        let old_tree = match self.previous {
            Some(previous) => Some(self.repository.find_commit(previous)?.tree()?),
            None => None,
        };
        let new_tree = self.repository.find_commit(commit)?.tree()?;
        let changes =
            self.repository
                .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;

        let mut old_files = Vec::new();
        let mut new_files = Vec::new();
        for delta in changes.deltas() {
            old_files.extend(self.source_file(delta.old_file()));
            new_files.extend(self.source_file(delta.new_file()));
        }

        let mut missing: Vec<Oid> = old_files
            .iter()
            .chain(&new_files)
            .map(|(_, id)| *id)
            .filter(|id| !self.symbols.contains_key(id))
            .collect();
        missing.sort_unstable();
        missing.dedup();
        let parsed = in_parallel(self.repository, missing, |repository, parser, id| {
            let blob = repository.find_blob(id)?;
            Ok((id, c::extract_symbols(parser, blob.content())))
        })?;
        self.symbols.extend(parsed);

        let removed = self.file_symbols(&old_files);
        let added = self.file_symbols(&new_files);
        let new_ids: HashSet<Oid> = new_files.iter().map(|(_, id)| *id).collect();
        for (_, id) in &old_files {
            if !new_ids.contains(id) {
                self.symbols.remove(id);
            }
        }
        self.previous = Some(commit);
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
