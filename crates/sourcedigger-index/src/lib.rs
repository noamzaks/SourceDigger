mod c;
pub mod code;
pub mod diff;
pub mod git;

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Function,
    Define,
    Variable,
    Struct,
    Union,
    Enum,
    Enumerator,
    Typedef,
    Invocation,
}

impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::Function,
        Kind::Define,
        Kind::Variable,
        Kind::Struct,
        Kind::Union,
        Kind::Enum,
        Kind::Enumerator,
        Kind::Typedef,
        Kind::Invocation,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Function => "function",
            Kind::Define => "define",
            Kind::Variable => "variable",
            Kind::Struct => "struct",
            Kind::Union => "union",
            Kind::Enum => "enum",
            Kind::Enumerator => "enumerator",
            Kind::Typedef => "typedef",
            Kind::Invocation => "invocation",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Symbol {
    pub name: String,
    pub kind: Kind,
    pub line: u32,
    pub signature: String,
    pub digest: u64,
    pub span: Range<usize>,
}
