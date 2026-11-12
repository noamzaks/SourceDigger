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
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Function, Kind::Define, Kind::Variable];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Function => "function",
            Kind::Define => "define",
            Kind::Variable => "variable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Symbol {
    pub name: String,
    pub kind: Kind,
    pub line: u32,
    pub signature: String,
    pub span: Range<usize>,
}
