use regex::Regex;

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
    }
}
