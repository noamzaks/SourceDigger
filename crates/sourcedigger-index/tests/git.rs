mod common;

use regex::Regex;
use sourcedigger_index::Kind;
use sourcedigger_index::code::{Code, FileAt, read_definitions};
use sourcedigger_index::diff::Action;
use sourcedigger_index::git::{Indexer, fetch, remote_versions, steps, unreleased, versions};

#[test]
fn versions_and_events() {
    let source = tempfile::tempdir().expect("creating temporary directory");
    common::create_repository(
        source.path(),
        &[
            (
                "v1",
                &[
                    ("a.c", "int f(void) { return 0; }\n"),
                    ("notes.txt", "int ignored;\n"),
                ],
            ),
            (
                "v2",
                &[
                    ("a.c", "long f(void) { return 0; }\n"),
                    ("lib/b.h", "#define B 1\n"),
                ],
            ),
            (
                "v10",
                &[("lib/b.h", "#define B 1\nint g(int x) { return x; }\n")],
            ),
            ("other", &[]),
            ("v2-again", &[]),
        ],
    );

    let mirror = tempfile::tempdir().expect("creating temporary directory");
    let repository = fetch(
        source.path().to_str().expect("UTF-8 path"),
        &mirror.path().join("mirror.git"),
        |_, _| {},
    )
    .expect("fetching");
    let found = versions(&repository, &Regex::new(r"^(v\d+)").expect("valid regex"))
        .expect("listing versions");
    let names: Vec<_> = found
        .iter()
        .map(|version| (version.name.as_str(), version.tag.as_str()))
        .collect();
    assert_eq!(names, vec![("v1", "v1"), ("v2", "v2"), ("v10", "v10")]);
    let remote = remote_versions(
        source.path().to_str().expect("UTF-8 path"),
        &Regex::new(r"^(v\d+)").expect("valid regex"),
    )
    .expect("listing remote versions");
    assert_eq!(remote, found);

    let file_pattern = Regex::new(r"\.[ch]$").expect("valid regex");
    let mut indexer = Indexer::new(&repository, file_pattern, None);
    let mut summarize = |version: &sourcedigger_index::git::Version| {
        let events = indexer.index(version.commit).expect("indexing");
        events
            .into_iter()
            .map(|event| {
                (
                    event.action,
                    event.name,
                    event.definition.path,
                    event.definition.signature,
                )
            })
            .collect::<Vec<_>>()
    };
    let entry = |action, name: &str, path: &str, signature: &str| {
        (
            action,
            name.to_string(),
            path.to_string(),
            signature.to_string(),
        )
    };

    assert_eq!(
        summarize(&found[0]),
        vec![entry(Action::Added, "f", "a.c", "int f(void)")]
    );
    assert_eq!(
        summarize(&found[1]),
        vec![
            entry(Action::Added, "B", "lib/b.h", "#define B 1"),
            entry(Action::Signature, "f", "a.c", "long f(void)"),
        ]
    );
    assert_eq!(
        summarize(&found[2]),
        vec![
            entry(Action::Removed, "f", "a.c", "long f(void)"),
            entry(Action::Added, "g", "lib/b.h", "int g(int x)"),
        ]
    );

    let file = |path: &str| FileAt {
        commit: found[2].commit,
        path: path.to_string(),
    };
    let code = read_definitions(&repository, "g", &[file("lib/b.h"), file("lib/b.h")])
        .expect("reading code");
    let expected = vec![Code {
        kind: Kind::Function,
        path: "lib/b.h".to_string(),
        line: 2,
        end_line: 2,
        text: "int g(int x) { return x; }".to_string(),
    }];
    assert_eq!(code, vec![expected.clone(), expected]);
    assert!(read_definitions(&repository, "g", &[file("a.c")]).is_err());
}

#[test]
fn steps_follow_the_first_parent() {
    let source = tempfile::tempdir().expect("creating temporary directory");
    let path = source.path();
    common::create_repository(path, &[("v1", &[("a.c", "int a;\n")])]);
    common::git(path, &["checkout", "--quiet", "-b", "topic"]);
    std::fs::write(path.join("b.c"), "int b;\n").expect("writing file");
    common::git(path, &["add", "--all"]);
    common::git(path, &["commit", "--quiet", "--message", "Add b"]);
    common::git(path, &["checkout", "--quiet", "main"]);
    std::fs::write(path.join("c.c"), "int c;\n").expect("writing file");
    common::git(path, &["add", "--all"]);
    common::git(path, &["commit", "--quiet", "--message", "Add c"]);
    common::git(
        path,
        &[
            "merge",
            "--quiet",
            "--no-ff",
            "--message",
            "Merge topic",
            "topic",
        ],
    );
    common::git(path, &["tag", "v2"]);

    let mirror = tempfile::tempdir().expect("creating temporary directory");
    let repository = fetch(
        path.to_str().expect("UTF-8 path"),
        &mirror.path().join("mirror.git"),
        |_, _| {},
    )
    .expect("fetching");
    let found = versions(&repository, &Regex::new(r"^v\d+$").expect("valid regex"))
        .expect("listing versions");

    assert_eq!(
        unreleased(&repository, &found[1]).expect("finding unreleased commits"),
        None
    );
    std::fs::write(path.join("d.c"), "int d;\n").expect("writing file");
    common::git(path, &["add", "--all"]);
    common::git(path, &["commit", "--quiet", "--message", "Add d"]);
    let repository = fetch(
        path.to_str().expect("UTF-8 path"),
        &mirror.path().join("mirror.git"),
        |_, _| {},
    )
    .expect("fetching again");
    let head = unreleased(&repository, &found[1])
        .expect("finding unreleased commits")
        .expect("main is ahead of v2");
    assert_eq!(
        (head.name.as_str(), head.tag.as_str()),
        ("unreleased", "main")
    );
    let after = steps(&repository, Some(&found[1]), &head).expect("listing steps");
    assert_eq!(
        after
            .iter()
            .map(|step| step.summary.as_str())
            .collect::<Vec<_>>(),
        vec!["Add d"]
    );

    let first = steps(&repository, None, &found[0]).expect("listing steps");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].commit, found[0].commit);

    let second = steps(&repository, Some(&found[0]), &found[1]).expect("listing steps");
    let summaries: Vec<_> = second.iter().map(|step| step.summary.as_str()).collect();
    assert_eq!(summaries, vec!["Add c", "Merge topic"]);
    assert_eq!(second[0].author, "test");

    let mut indexer = Indexer::new(&repository, Regex::new(r"\.c$").expect("valid regex"), None);
    indexer.index(found[0].commit).expect("indexing");
    let names = |events: Vec<sourcedigger_index::diff::Event>| -> Vec<String> {
        events.into_iter().map(|event| event.name).collect()
    };
    assert_eq!(
        names(indexer.index(second[0].commit).expect("indexing")),
        vec!["c"]
    );
    assert_eq!(
        names(indexer.index(second[1].commit).expect("indexing")),
        vec!["b"]
    );
}

#[test]
fn fetching_reports_progress() {
    const FILE_COUNT: usize = 200;
    let source = tempfile::tempdir().expect("creating temporary directory");
    let contents: Vec<(String, String)> = (0..FILE_COUNT)
        .map(|index| (format!("{index}.c"), format!("int f{index};\n")))
        .collect();
    let files: Vec<(&str, &str)> = contents
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();
    common::create_repository(source.path(), &[("v1", &files)]);

    let mirror = tempfile::tempdir().expect("creating temporary directory");
    let mut progress = (0, 0);
    fetch(
        source.path().to_str().expect("UTF-8 path"),
        &mirror.path().join("mirror.git"),
        |received, total| progress = (received, total),
    )
    .expect("fetching");
    assert!(
        progress.0 > FILE_COUNT && progress.0 == progress.1,
        "{progress:?}"
    );
}

#[test]
fn failures_show_what_git_said() {
    let missing = tempfile::tempdir().expect("creating temporary directory");
    let url = missing.path().join("missing.git");
    let url = url.to_str().expect("UTF-8 path");
    let pattern = Regex::new(".*").expect("valid regex");

    let error = remote_versions(url, &pattern).expect_err("the repository is missing");
    assert!(
        format!("{error:#}").contains("does not appear to be a git repository"),
        "{error:#}"
    );

    let mirror = missing.path().join("mirror.git");
    let Err(error) = fetch(url, &mirror, |_, _| {}) else {
        panic!("fetched a missing repository");
    };
    assert!(
        format!("{error:#}").contains("does not appear to be a git repository"),
        "{error:#}"
    );

    let error = remote_versions("--upload-pack=touch", &pattern).expect_err("not a URL");
    assert!(format!("{error:#}").contains("blocked"), "{error:#}");
}
