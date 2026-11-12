mod common;

use regex::Regex;
use sourcedigger_index::Kind;
use sourcedigger_index::code::{Code, FileAt, read_definitions};
use sourcedigger_index::diff::Action;
use sourcedigger_index::git::{Indexer, fetch, remote_versions, versions};

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
    let mut summarize = |version| {
        let events = indexer.index(version).expect("indexing");
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
            entry(Action::Modified, "f", "a.c", "long f(void)"),
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
