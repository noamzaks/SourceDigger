use std::fmt::{self, Display, Formatter};
use std::iter::Peekable;
use std::str::Chars;

use regex::Regex;
use serde::Serialize;
use sourcedigger_index::Kind;
use sourcedigger_index::diff::Action;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Term {
    pub field: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Search {
    Term(Term),
    Not(Box<Search>),
    And(Vec<Search>),
    Or(Vec<Search>),
}

impl Search {
    pub const FIELDS: [(&str, &str); 8] = [
        (
            "name",
            "the name, as a regular expression over all of it; the field of a term without one",
        ),
        ("kind", "the kinds, as a comma list"),
        ("action", "the actions, as a comma list"),
        ("version", "the changes of a version"),
        ("after", "the changes after a version"),
        ("before", "the changes before a version"),
        (
            "path",
            "changes in files whose path starts with a regular expression (a file or a directory), or moved from there",
        ),
        (
            "commit",
            "the changes of a merge or commit, by hash or its start",
        ),
    ];

    pub fn everything() -> Self {
        Self::And(Vec::new())
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let tokens = Token::read(text)?;
        if tokens.is_empty() {
            return Ok(Self::everything());
        }
        let mut parser = Parser {
            tokens,
            position: 0,
        };
        let search = parser.any()?;
        if let Some(token) = parser.peek() {
            return Err(format!("Unexpected {token}"));
        }
        Ok(search)
    }

    pub fn terms(&self) -> Vec<&Term> {
        match self {
            Self::Term(term) => vec![term],
            Self::Not(inner) => inner.terms(),
            Self::And(parts) | Self::Or(parts) => parts.iter().flat_map(Self::terms).collect(),
        }
    }

    pub fn names(&self) -> Option<NamePattern> {
        let alternatives = match self {
            Self::And(parts) => parts.iter().find_map(Self::name_alternatives),
            _ => self.name_alternatives(),
        }?;
        NamePattern::exact(&alternatives.join("|")).expect("names are checked when parsed")
    }

    fn name_alternatives(&self) -> Option<Vec<&str>> {
        match self {
            Self::Term(term) if term.field == "name" => Some(vec![term.value.as_str()]),
            Self::Or(parts) => parts
                .iter()
                .map(|part| match part {
                    Self::Term(term) if term.field == "name" => Some(term.value.as_str()),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    }

    fn all(parts: Vec<Self>) -> Self {
        let mut flat = Vec::new();
        for part in parts {
            match part {
                Self::And(inner) => flat.extend(inner),
                other => flat.push(other),
            }
        }
        if flat.len() == 1 {
            return flat.remove(0);
        }
        Self::And(flat)
    }

    fn any(parts: Vec<Self>) -> Self {
        let mut flat = Vec::new();
        for part in parts {
            match part {
                Self::Or(inner) => flat.extend(inner),
                other => flat.push(other),
            }
        }
        if flat.len() == 1 {
            return flat.remove(0);
        }
        Self::Or(flat)
    }

    fn side_by_side(parts: Vec<Self>) -> Self {
        let mut merged: Vec<Self> = Vec::new();
        let mut field_positions: Vec<(String, usize)> = Vec::new();
        for part in parts {
            let Self::Term(term) = &part else {
                merged.push(part);
                continue;
            };
            if Term::RANGES.contains(&term.field.as_str()) {
                merged.push(part);
                continue;
            }
            match field_positions
                .iter()
                .find(|(field, _)| *field == term.field)
            {
                Some(&(_, position)) => {
                    let first = std::mem::replace(&mut merged[position], Self::everything());
                    merged[position] = Self::any(vec![first, part]);
                }
                None => {
                    field_positions.push((term.field.clone(), merged.len()));
                    merged.push(part);
                }
            }
        }
        Self::all(merged)
    }
}

impl Term {
    const RANGES: [&str; 2] = ["after", "before"];

    pub fn new(field: &str, value: &str) -> Self {
        Self {
            field: field.to_string(),
            value: value.to_string(),
        }
    }

    pub fn choices(&self) -> Vec<&str> {
        self.value.split(',').collect()
    }

    pub fn name_pattern(&self) -> Option<NamePattern> {
        NamePattern::exact(&self.value).expect("names are checked when parsed")
    }

    pub fn path_regex(&self) -> String {
        format!("^(?:{})", expand(&self.value))
    }

    fn read(word: &str) -> Result<Self, String> {
        let field_pattern = Regex::new(r"(?s)^([a-z]+):(.*)$").expect("valid regex");
        let term = match field_pattern.captures(word) {
            Some(captures) => Self::new(&captures[1], &unquote(&captures[2])),
            None => Self::new("name", &unquote(word)),
        };
        term.check()?;
        Ok(term)
    }

    fn check(&self) -> Result<(), String> {
        if !Search::FIELDS.iter().any(|(name, _)| *name == self.field) {
            let fields: Vec<&str> = Search::FIELDS.iter().map(|(name, _)| *name).collect();
            return Err(format!(
                "Unknown field {:?}; fields are {}",
                self.field,
                fields.join(", ")
            ));
        }
        if self.value.is_empty() {
            return Err(format!("{}: needs a value", self.field));
        }
        let invalid = |error: regex::Error| format!("{}: {error}", self.field);
        match self.field.as_str() {
            "kind" => self.check_choices(&Kind::ALL.map(Kind::as_str)),
            "action" => self.check_choices(&Action::ALL.map(Action::as_str)),
            "name" => NamePattern::exact(&self.value).map(drop).map_err(invalid),
            "path" => Regex::new(&self.path_regex()).map(drop).map_err(invalid),
            _ => Ok(()),
        }
    }

    fn check_choices(&self, all: &[&str]) -> Result<(), String> {
        match self
            .choices()
            .into_iter()
            .find(|choice| !all.contains(choice))
        {
            Some(unknown) => Err(format!(
                "{} is one of {}, not {unknown:?}",
                self.field,
                all.join(", ")
            )),
            None => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Open,
    Close,
    Minus,
    Not,
    And,
    Or,
    Term(Term),
}

impl Token {
    fn read(text: &str) -> Result<Vec<Self>, String> {
        let mut tokens = Vec::new();
        let mut characters = text.chars().peekable();
        while let Some(&character) = characters.peek() {
            if character.is_whitespace() {
                characters.next();
                continue;
            }
            let token = match character {
                '(' => Self::Open,
                ')' => Self::Close,
                '-' => Self::Minus,
                _ => {
                    tokens.push(Self::word(&mut characters)?);
                    continue;
                }
            };
            characters.next();
            tokens.push(token);
        }
        Ok(tokens)
    }

    fn word(characters: &mut Peekable<Chars>) -> Result<Self, String> {
        let mut word = String::new();
        let mut depth = 0;
        while let Some(&character) = characters.peek() {
            if character.is_whitespace() || (character == ')' && depth == 0) {
                break;
            }
            characters.next();
            word.push(character);
            match character {
                '"' => {
                    Self::quoted(characters, &mut word);
                    break;
                }
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
        }
        match word.as_str() {
            "NOT" => Ok(Self::Not),
            "AND" => Ok(Self::And),
            "OR" => Ok(Self::Or),
            _ => Term::read(&word).map(Self::Term),
        }
    }

    fn quoted(characters: &mut Peekable<Chars>, word: &mut String) {
        while let Some(character) = characters.next() {
            word.push(character);
            match character {
                '"' => return,
                '\\' => word.extend(characters.next()),
                _ => {}
            }
        }
    }
}

impl Display for Token {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match self {
            Self::Open => write!(formatter, "("),
            Self::Close => write!(formatter, ")"),
            Self::Minus => write!(formatter, "-"),
            Self::Not => write!(formatter, "NOT"),
            Self::And => write!(formatter, "AND"),
            Self::Or => write!(formatter, "OR"),
            Self::Term(term) => write!(formatter, "{}:{}", term.field, term.value),
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn previous(&self) -> Option<&Token> {
        self.tokens.get(self.position.checked_sub(1)?)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.peek().cloned();
        self.position += 1;
        token
    }

    fn take(&mut self, token: &Token) -> bool {
        if self.peek() != Some(token) {
            return false;
        }
        self.position += 1;
        true
    }

    fn starts_operand(&self) -> bool {
        matches!(
            self.peek(),
            Some(Token::Open | Token::Minus | Token::Not | Token::Term(_))
        )
    }

    fn any(&mut self) -> Result<Search, String> {
        let mut alternatives = vec![self.all()?];
        while self.take(&Token::Or) {
            alternatives.push(self.all()?);
        }
        Ok(Search::any(alternatives))
    }

    fn all(&mut self) -> Result<Search, String> {
        let mut parts = vec![self.side_by_side()?];
        while self.take(&Token::And) {
            parts.push(self.side_by_side()?);
        }
        Ok(Search::all(parts))
    }

    fn side_by_side(&mut self) -> Result<Search, String> {
        let mut parts = Vec::new();
        while self.starts_operand() {
            parts.push(self.operand()?);
        }
        if parts.is_empty() {
            return Err(self.missing_term());
        }
        Ok(Search::side_by_side(parts))
    }

    fn operand(&mut self) -> Result<Search, String> {
        match self.next() {
            Some(Token::Not | Token::Minus) => {
                if !self.starts_operand() {
                    return Err(self.missing_term());
                }
                Ok(Search::Not(Box::new(self.operand()?)))
            }
            Some(Token::Open) => {
                let inner = self.any()?;
                if !self.take(&Token::Close) {
                    return Err("( needs a matching )".to_string());
                }
                Ok(inner)
            }
            Some(Token::Term(term)) => Ok(Search::Term(term)),
            _ => unreachable!("operands start as starts_operand says"),
        }
    }

    fn missing_term(&self) -> String {
        match (self.previous(), self.peek()) {
            (Some(Token::Open), Some(Token::Close)) => "() needs a term inside".to_string(),
            (Some(Token::Open), None) => "( needs a matching )".to_string(),
            (Some(previous), _) => format!("{previous} needs a term after it"),
            (None, Some(next)) => format!("{next} needs a term before it"),
            (None, None) => unreachable!("an empty search has no terms to miss"),
        }
    }
}

fn unquote(value: &str) -> String {
    let Some(quoted) = value.strip_prefix('"') else {
        return value.to_string();
    };
    let inner = quoted.strip_suffix('"').unwrap_or(quoted);
    let escaped = Regex::new(r"\\(.)").expect("valid regex");
    escaped.replace_all(inner, "$1").into_owned()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamePattern {
    pub regex: String,
    pub prefix: String,
}

impl NamePattern {
    const MATCH_ALL: [&str; 3] = ["", "*", ".*"];

    pub fn exact(query: &str) -> Result<Option<Self>, regex::Error> {
        let query = query.trim();
        if Self::MATCH_ALL.contains(&query) {
            return Ok(None);
        }
        Self::checked(query, format!("{}^(?:{})$", case(query), expand(query))).map(Some)
    }

    pub fn starting(query: &str) -> Result<Self, regex::Error> {
        let query = query.trim();
        Self::checked(query, format!("{}^(?:{}).*$", case(query), expand(query)))
    }

    pub fn literal(name: &str) -> Self {
        Self {
            regex: format!("^{}$", regex::escape(name)),
            prefix: name.to_string(),
        }
    }

    pub fn prefix_end(&self) -> String {
        format!("{}{}", self.prefix, char::MAX)
    }

    fn checked(query: &str, regex: String) -> Result<Self, regex::Error> {
        Regex::new(&regex)?;
        Ok(Self {
            regex,
            prefix: literal_prefix(query),
        })
    }
}

fn expand(query: &str) -> String {
    let bare_repetition = Regex::new(r"(^|[^.\]\\])([*+])").expect("valid regex");
    bare_repetition.replace_all(query, "$1.$2").into_owned()
}

fn case(query: &str) -> &'static str {
    if query.chars().any(char::is_uppercase) {
        ""
    } else {
        "(?i)"
    }
}

fn literal_prefix(query: &str) -> String {
    if query.contains('|') {
        return String::new();
    }
    let length = query
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(query.len());
    let mut prefix = &query[..length];
    if query[length..].starts_with(['?', '{']) {
        prefix = &prefix[..prefix.len().saturating_sub(1)];
    }
    prefix.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact(query: &str) -> (String, String) {
        let pattern = NamePattern::exact(query).unwrap().unwrap();
        (pattern.regex, pattern.prefix)
    }

    fn owned(regex: &str, prefix: &str) -> (String, String) {
        (regex.to_string(), prefix.to_string())
    }

    fn term(field: &str, value: &str) -> Search {
        Search::Term(Term::new(field, value))
    }

    fn not(inner: Search) -> Search {
        Search::Not(Box::new(inner))
    }

    fn parse(text: &str) -> Search {
        Search::parse(text).unwrap()
    }

    #[test]
    fn searches() {
        assert_eq!(
            parse(
                r#"vm_* kind:define,function -action:removed NOT path:"mm/a b" version:6.1 commit:551acdc"#
            ),
            Search::And(vec![
                term("name", "vm_*"),
                term("kind", "define,function"),
                not(term("action", "removed")),
                not(term("path", "mm/a b")),
                term("version", "6.1"),
                term("commit", "551acdc"),
            ])
        );
        assert_eq!(
            parse("  sys_(open|close)  "),
            term("name", "sys_(open|close)")
        );
        assert_eq!(
            parse("path:drivers/(gpu|net)/*"),
            term("path", "drivers/(gpu|net)/*")
        );
        assert_eq!(parse(r#"path:"a b/\"c""#), term("path", r#"a b/"c"#));
        assert_eq!(
            parse(r#""(x)" "OR""#),
            Search::Or(vec![term("name", "(x)"), term("name", "OR")])
        );
        assert_eq!(parse(""), Search::everything());
        assert_eq!(parse("  "), Search::everything());
    }

    #[test]
    fn boolean_operators() {
        assert_eq!(
            parse("kind:function OR NOT kind:function"),
            Search::Or(vec![
                term("kind", "function"),
                not(term("kind", "function"))
            ])
        );
        assert_eq!(
            parse("a kind:define OR b -(kind:function OR action:added)"),
            Search::Or(vec![
                Search::And(vec![term("name", "a"), term("kind", "define")]),
                Search::And(vec![
                    term("name", "b"),
                    not(Search::Or(vec![
                        term("kind", "function"),
                        term("action", "added")
                    ])),
                ]),
            ])
        );
        assert_eq!(
            parse("((a OR b)) AND (c)"),
            Search::And(vec![
                Search::Or(vec![term("name", "a"), term("name", "b")]),
                term("name", "c"),
            ])
        );
        assert_eq!(
            parse("a AND b"),
            Search::And(vec![term("name", "a"), term("name", "b")])
        );
        assert_eq!(parse("NOT NOT a"), not(not(term("name", "a"))));
        assert_eq!(
            parse("(sys_(open|close))"),
            term("name", "sys_(open|close)")
        );
        assert_eq!(parse("-after:v1"), not(term("after", "v1")));
        assert_eq!(
            parse("(kind:function action:added) OR NOT path:mm/"),
            Search::Or(vec![
                Search::And(vec![term("kind", "function"), term("action", "added")]),
                not(term("path", "mm/")),
            ])
        );
    }

    #[test]
    fn same_field_side_by_side_means_any() {
        assert_eq!(
            parse("strbuf_add kind:function strbuf_init -x -y path:a/ path:b/ after:1 after:2"),
            Search::And(vec![
                Search::Or(vec![
                    term("name", "strbuf_add"),
                    term("name", "strbuf_init")
                ]),
                term("kind", "function"),
                not(term("name", "x")),
                not(term("name", "y")),
                Search::Or(vec![term("path", "a/"), term("path", "b/")]),
                term("after", "1"),
                term("after", "2"),
            ])
        );
        assert_eq!(
            parse("(a OR b) c"),
            Search::And(vec![
                Search::Or(vec![term("name", "a"), term("name", "b")]),
                term("name", "c"),
            ])
        );
    }

    #[test]
    fn names_narrow_the_scan() {
        let regex = |text| parse(text).names().map(|pattern| pattern.regex);
        assert_eq!(
            regex("kmalloc* -kmalloc_node*").unwrap(),
            "(?i)^(?:kmalloc.*)$"
        );
        assert_eq!(regex("a b kind:define").unwrap(), "(?i)^(?:a|b)$");
        assert_eq!(regex("kind:define (a OR b)").unwrap(), "(?i)^(?:a|b)$");
        assert_eq!(regex("a OR kind:define"), None);
        assert_eq!(regex("-a"), None);
        assert_eq!(regex("*"), None);
    }

    #[test]
    fn search_mistakes() {
        let error = |text| Search::parse(text).unwrap_err();
        assert!(error("kind:fn").starts_with("kind is one of function, define,"));
        assert!(error("kind:fn").ends_with(r#"not "fn""#));
        assert!(error("size:3").starts_with(r#"Unknown field "size""#));
        assert_eq!(error("path:"), "path: needs a value");
        assert_eq!(error("x NOT"), "NOT needs a term after it");
        assert_eq!(error("x -"), "- needs a term after it");
        assert_eq!(error("x OR"), "OR needs a term after it");
        assert_eq!(error("OR x"), "OR needs a term before it");
        assert_eq!(error("x AND OR y"), "AND needs a term after it");
        assert_eq!(error("(x"), "( needs a matching )");
        assert_eq!(error("("), "( needs a matching )");
        assert_eq!(error("x )"), "Unexpected )");
        assert_eq!(error("()"), "() needs a term inside");
        assert!(error("path:(").starts_with("path: regex parse error"));
        assert!(error("a(").starts_with("name: regex parse error"));
    }

    #[test]
    fn patterns() {
        assert_eq!(exact("kmalloc"), owned("(?i)^(?:kmalloc)$", "kmalloc"));
        assert_eq!(exact("Foo_*"), owned("^(?:Foo_.*)$", "Foo_"));
        assert_eq!(exact("a.*b+"), owned("(?i)^(?:a.*b.+)$", "a"));
        assert_eq!(exact("*x"), owned("(?i)^(?:.*x)$", ""));
        assert_eq!(exact("colou?r"), owned("(?i)^(?:colou?r)$", "colo"));
        assert_eq!(
            exact("sys_(open|close)"),
            owned("(?i)^(?:sys_(open|close))$", "")
        );
        assert_eq!(NamePattern::exact(" * ").unwrap(), None);
        assert_eq!(NamePattern::exact("").unwrap(), None);
        assert_eq!(
            NamePattern::starting("vm_").unwrap(),
            NamePattern {
                regex: "(?i)^(?:vm_).*$".to_string(),
                prefix: "vm_".to_string()
            }
        );
        assert!(NamePattern::exact("(").is_err());
        assert_eq!(
            Term::new("path", "drivers/(gpu|net)/*").path_regex(),
            "^(?:drivers/(gpu|net)/.*)"
        );
    }
}
