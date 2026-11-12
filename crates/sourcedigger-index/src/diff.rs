use std::collections::BTreeMap;

use crate::{Kind, Symbol};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Added,
    Removed,
    Modified,
}

impl Action {
    pub const ALL: [Action; 3] = [Action::Added, Action::Removed, Action::Modified];

    pub fn as_str(self) -> &'static str {
        match self {
            Action::Added => "added",
            Action::Removed => "removed",
            Action::Modified => "modified",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub path: String,
    pub line: u32,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub action: Action,
    pub name: String,
    pub kind: Kind,
    pub definition: Definition,
    pub previous: Option<Definition>,
}

#[derive(Clone, Debug)]
pub(crate) struct FileSymbol {
    pub path: String,
    pub symbol: Symbol,
}

pub(crate) fn diff(removed: Vec<FileSymbol>, added: Vec<FileSymbol>) -> Vec<Event> {
    let mut groups: BTreeMap<(String, Kind), (Vec<Definition>, Vec<Definition>)> = BTreeMap::new();
    for file_symbol in removed {
        let (key, definition) = split(file_symbol);
        groups.entry(key).or_default().0.push(definition);
    }
    for file_symbol in added {
        let (key, definition) = split(file_symbol);
        groups.entry(key).or_default().1.push(definition);
    }

    let mut events = Vec::new();
    for ((name, kind), (old, new)) in groups {
        for (action, definition, previous) in pair(old, new) {
            events.push(Event {
                action,
                name: name.clone(),
                kind,
                definition,
                previous,
            });
        }
    }
    events
}

fn split(file_symbol: FileSymbol) -> ((String, Kind), Definition) {
    let FileSymbol { path, symbol } = file_symbol;
    let definition = Definition {
        path,
        line: symbol.line,
        signature: symbol.signature,
    };
    ((symbol.name, symbol.kind), definition)
}

fn pair(
    mut old: Vec<Definition>,
    mut new: Vec<Definition>,
) -> Vec<(Action, Definition, Option<Definition>)> {
    new.retain(|definition| {
        let unchanged = old.iter().position(|candidate| {
            candidate.path == definition.path && candidate.signature == definition.signature
        });
        match unchanged {
            Some(index) => {
                old.remove(index);
                false
            }
            None => true,
        }
    });
    old.sort_by(|left, right| left.path.cmp(&right.path));
    new.sort_by(|left, right| left.path.cmp(&right.path));

    let mut pairs = Vec::new();
    new.retain(|definition| {
        match old
            .iter()
            .position(|candidate| candidate.path == definition.path)
        {
            Some(index) => {
                pairs.push((
                    Action::Modified,
                    definition.clone(),
                    Some(old.remove(index)),
                ));
                false
            }
            None => true,
        }
    });

    if let ([previous], [definition]) = (old.as_slice(), new.as_slice()) {
        pairs.push((Action::Modified, definition.clone(), Some(previous.clone())));
        return pairs;
    }
    pairs.extend(
        new.into_iter()
            .map(|definition| (Action::Added, definition, None)),
    );
    pairs.extend(
        old.into_iter()
            .map(|previous| (Action::Removed, previous, None)),
    );
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn located(path: &str, name: &str, line: u32, signature: &str) -> FileSymbol {
        FileSymbol {
            path: path.to_string(),
            symbol: Symbol {
                name: name.to_string(),
                kind: Kind::Function,
                line,
                span: 0..0,
                signature: signature.to_string(),
            },
        }
    }

    fn summary(events: Vec<Event>) -> Vec<(Action, String, String, Option<String>)> {
        events
            .into_iter()
            .map(|event| {
                (
                    event.action,
                    event.name,
                    format!(
                        "{}:{} {}",
                        event.definition.path, event.definition.line, event.definition.signature
                    ),
                    event.previous.map(|previous| {
                        format!("{}:{} {}", previous.path, previous.line, previous.signature)
                    }),
                )
            })
            .collect()
    }

    #[test]
    fn moved_lines_are_not_events() {
        let events = diff(
            vec![located("a.c", "f", 1, "int f()")],
            vec![located("a.c", "f", 9, "int f()")],
        );
        assert_eq!(events, vec![]);
    }

    #[test]
    fn changes() {
        let events = diff(
            vec![
                located("a.c", "changed", 1, "int changed()"),
                located("a.c", "moved", 2, "int moved()"),
                located("a.c", "gone", 3, "int gone()"),
                located("a.c", "twice", 4, "int twice()"),
            ],
            vec![
                located("a.c", "changed", 1, "long changed()"),
                located("b.c", "moved", 2, "int moved()"),
                located("a.c", "new", 3, "int new()"),
                located("a.c", "twice", 4, "int twice()"),
                located("b.c", "twice", 4, "int twice()"),
            ],
        );
        assert_eq!(
            summary(events),
            vec![
                (
                    Action::Modified,
                    "changed".to_string(),
                    "a.c:1 long changed()".to_string(),
                    Some("a.c:1 int changed()".to_string())
                ),
                (
                    Action::Removed,
                    "gone".to_string(),
                    "a.c:3 int gone()".to_string(),
                    None
                ),
                (
                    Action::Modified,
                    "moved".to_string(),
                    "b.c:2 int moved()".to_string(),
                    Some("a.c:2 int moved()".to_string())
                ),
                (
                    Action::Added,
                    "new".to_string(),
                    "a.c:3 int new()".to_string(),
                    None
                ),
                (
                    Action::Added,
                    "twice".to_string(),
                    "b.c:4 int twice()".to_string(),
                    None
                ),
            ]
        );
    }

    #[test]
    fn only_a_single_leftover_pair_is_a_move() {
        let events = diff(
            vec![
                located("a.c", "f", 1, "int f(a)"),
                located("b.c", "f", 1, "int f(b)"),
            ],
            vec![
                located("c.c", "f", 1, "int f(c)"),
                located("d.c", "f", 1, "int f(d)"),
            ],
        );
        let actions: Vec<_> = events.iter().map(|event| event.action).collect();
        assert_eq!(
            actions,
            vec![
                Action::Added,
                Action::Added,
                Action::Removed,
                Action::Removed
            ]
        );
    }

    #[test]
    fn same_path_pairs_first() {
        let events = diff(
            vec![
                located("a.c", "f", 1, "int f(a)"),
                located("b.c", "f", 1, "int f(b)"),
            ],
            vec![located("b.c", "f", 1, "int f(B)")],
        );
        assert_eq!(
            summary(events),
            vec![
                (
                    Action::Modified,
                    "f".to_string(),
                    "b.c:1 int f(B)".to_string(),
                    Some("b.c:1 int f(b)".to_string())
                ),
                (
                    Action::Removed,
                    "f".to_string(),
                    "a.c:1 int f(a)".to_string(),
                    None
                ),
            ]
        );
    }
}
