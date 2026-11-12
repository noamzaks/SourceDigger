use std::hash::{DefaultHasher, Hash, Hasher};

use tree_sitter::{Node, Parser};

use crate::{Kind, Symbol};

const MAXIMUM_SIGNATURE_LENGTH: usize = 300;

pub(crate) fn parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .expect("the bundled C grammar matches the tree-sitter version");
    parser
}

pub(crate) fn extract_symbols(parser: &mut Parser, code: &[u8]) -> Vec<Symbol> {
    let tree = parser
        .parse(code, None)
        .expect("parsing without a timeout or cancellation always returns a tree");
    let mut symbols = Vec::new();
    let mut cursor = tree.walk();
    let mut visited_children = false;
    loop {
        if !visited_children {
            add_symbols(cursor.node(), code, &mut symbols);
            if cursor.goto_first_child() {
                continue;
            }
        }
        if cursor.goto_next_sibling() {
            visited_children = false;
            continue;
        }
        if !cursor.goto_parent() {
            return symbols;
        }
        visited_children = true;
    }
}

fn add_symbols(node: Node, code: &[u8], symbols: &mut Vec<Symbol>) {
    match node.kind() {
        "preproc_def" | "preproc_function_def" => symbols.extend(define(node, code)),
        "function_definition" => symbols.extend(function(node, code)),
        "declaration" if is_top_level(node) && !is_extern(node, code) => {
            symbols.extend(variables(node, code))
        }
        "type_definition" if is_top_level(node) => symbols.extend(typedefs(node, code)),
        "expression_statement" if is_top_level(node) => symbols.extend(invocation(node, code)),
        "struct_specifier" if !in_function(node) => {
            symbols.extend(composite(Kind::Struct, node, code))
        }
        "union_specifier" if !in_function(node) => {
            symbols.extend(composite(Kind::Union, node, code))
        }
        "enum_specifier" if !in_function(node) => symbols.extend(composite(Kind::Enum, node, code)),
        "enumerator" if !in_function(node) => symbols.extend(enumerator(node, code)),
        _ => {}
    }
}

fn composite(kind: Kind, node: Node, code: &[u8]) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let body = node.child_by_field_name("body")?;
    Some(symbol(kind, node, name, code, body.start_byte()))
}

fn enumerator(node: Node, code: &[u8]) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    Some(symbol(Kind::Enumerator, node, name, code, node.end_byte()))
}

fn typedefs(node: Node, code: &[u8]) -> Vec<Symbol> {
    let mut cursor = node.walk();
    let declarators: Vec<Node> = node
        .children_by_field_name("declarator", &mut cursor)
        .collect();
    let Some(first) = declarators.first() else {
        return Vec::new();
    };
    let prefix = specifiers(node, *first, code);

    let mut symbols = Vec::new();
    for declarator in declarators {
        let Some(name) = declarator_name(declarator) else {
            continue;
        };
        let text = normalize(&code[declarator.byte_range()]);
        symbols.push(Symbol {
            name: text_of(name, code),
            kind: Kind::Typedef,
            line: line_of(name),
            signature: truncate(format!("{prefix} {text}")),
            digest: digest(&code[node.byte_range()]),
            span: node.byte_range(),
        });
    }
    symbols
}

fn invocation(node: Node, code: &[u8]) -> Option<Symbol> {
    let call = node.named_child(0)?;
    if call.kind() != "call_expression" {
        return None;
    }
    let function = call.child_by_field_name("function")?;
    if function.kind() != "identifier" {
        return None;
    }
    let arguments = call.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let name = arguments
        .named_children(&mut cursor)
        .find(|argument| argument.kind() == "identifier")
        .unwrap_or(function);
    Some(symbol(Kind::Invocation, node, name, code, call.end_byte()))
}

fn define(node: Node, code: &[u8]) -> Option<Symbol> {
    let name = node.child_by_field_name("name")?;
    let end = match node.child_by_field_name("parameters") {
        Some(parameters) => parameters.end_byte(),
        None => node
            .child_by_field_name("value")
            .map_or(name.end_byte(), |value| value.end_byte()),
    };
    Some(symbol(Kind::Define, node, name, code, end))
}

fn function(node: Node, code: &[u8]) -> Option<Symbol> {
    let name = declarator_name(node.child_by_field_name("declarator")?)?;
    let end = node.child_by_field_name("body")?.start_byte();
    Some(symbol(Kind::Function, node, name, code, end))
}

fn variables(node: Node, code: &[u8]) -> Vec<Symbol> {
    let mut cursor = node.walk();
    let declarators: Vec<Node> = node
        .children_by_field_name("declarator", &mut cursor)
        .filter_map(|declarator| match declarator.kind() {
            "init_declarator" => declarator.child_by_field_name("declarator"),
            _ => Some(declarator),
        })
        .collect();
    let Some(first) = declarators.first() else {
        return Vec::new();
    };
    let specifiers = specifiers(node, *first, code);

    let mut symbols = Vec::new();
    for declarator in declarators {
        if is_prototype(declarator) {
            continue;
        }
        let Some(name) = declarator_name(declarator) else {
            continue;
        };
        let text = normalize(&code[declarator.byte_range()]);
        symbols.push(Symbol {
            name: text_of(name, code),
            kind: Kind::Variable,
            line: line_of(name),
            signature: truncate(format!("{specifiers} {text}")),
            digest: digest(&code[node.byte_range()]),
            span: node.byte_range(),
        });
    }
    symbols
}

fn specifiers(node: Node, first_declarator: Node, code: &[u8]) -> String {
    let body = node
        .child_by_field_name("type")
        .and_then(|base| base.child_by_field_name("body"));
    let end = match body {
        Some(body) => body.start_byte(),
        None => first_declarator.start_byte(),
    };
    normalize(&code[node.start_byte()..end])
}

fn symbol(kind: Kind, node: Node, name: Node, code: &[u8], signature_end: usize) -> Symbol {
    Symbol {
        name: text_of(name, code),
        kind,
        line: line_of(name),
        signature: truncate(normalize(&code[node.start_byte()..signature_end])),
        digest: digest(&code[node.byte_range()]),
        span: node.byte_range(),
    }
}

fn declarator_name(declarator: Node) -> Option<Node> {
    match declarator.kind() {
        "identifier" | "type_identifier" if declarator.is_missing() => None,
        "identifier" | "type_identifier" => Some(declarator),
        "parenthesized_declarator" => declarator_name(declarator.named_child(0)?),
        _ => declarator_name(declarator.child_by_field_name("declarator")?),
    }
}

fn is_prototype(declarator: Node) -> bool {
    match declarator.kind() {
        "function_declarator" => declarator
            .child_by_field_name("declarator")
            .is_some_and(|inner| inner.kind() == "identifier"),
        "pointer_declarator" | "attributed_declarator" => declarator
            .child_by_field_name("declarator")
            .is_some_and(is_prototype),
        _ => false,
    }
}

fn is_top_level(node: Node) -> bool {
    let unindented = node.start_position().column == 0;
    let mut recovered = false;
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        match parent.kind() {
            "translation_unit" => return !recovered || unindented,
            "ERROR" => recovered = true,
            kind if kind.starts_with("preproc_") => {}
            _ => return false,
        }
        ancestor = parent.parent();
    }
    recovered && unindented
}

fn in_function(node: Node) -> bool {
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if parent.kind() == "function_definition" {
            return true;
        }
        ancestor = parent.parent();
    }
    false
}

fn is_extern(node: Node, code: &[u8]) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|child| {
        child.kind() == "storage_class_specifier" && &code[child.byte_range()] == b"extern"
    })
}

fn text_of(node: Node, code: &[u8]) -> String {
    String::from_utf8_lossy(&code[node.byte_range()]).into_owned()
}

fn line_of(node: Node) -> u32 {
    u32::try_from(node.start_position().row + 1).unwrap_or(u32::MAX)
}

fn digest(text: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for word in text.split(u8::is_ascii_whitespace) {
        if !word.is_empty() {
            word.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn normalize(text: &[u8]) -> String {
    let text = String::from_utf8_lossy(text);
    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|word| *word != "\\")
        .collect();
    words.join(" ")
}

fn truncate(mut text: String) -> String {
    if text.len() <= MAXIMUM_SIGNATURE_LENGTH {
        return text;
    }
    let mut end = MAXIMUM_SIGNATURE_LENGTH;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push('…');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(code: &str) -> Vec<(String, Kind, u32, String)> {
        extract_symbols(&mut parser(), code.as_bytes())
            .into_iter()
            .map(|symbol| (symbol.name, symbol.kind, symbol.line, symbol.signature))
            .collect()
    }

    fn entry(name: &str, kind: Kind, line: u32, signature: &str) -> (String, Kind, u32, String) {
        (name.to_string(), kind, line, signature.to_string())
    }

    #[test]
    fn functions() {
        let code = "static int add(int a,\n    int b)\n{\n    int local = a;\n    return local + b;\n}\n\
                    char *\nname(struct thing *t) { return t->name; }\n\
                    int prototype(void);\n";
        assert_eq!(
            extract(code),
            vec![
                entry("add", Kind::Function, 1, "static int add(int a, int b)"),
                entry("name", Kind::Function, 8, "char * name(struct thing *t)"),
            ]
        );
    }

    #[test]
    fn defines() {
        let code = "#define SIZE 4096\n#ifdef BIG\n#define LIMIT (SIZE * 2)\n#else\n#define LIMIT SIZE\n#endif\n\
                    #define MAX(a, b) \\\n    ((a) > (b) ? (a) : (b))\n#define EMPTY\n\
                    void f(void) {\n#define INSIDE 1\n}\n";
        assert_eq!(
            extract(code),
            vec![
                entry("SIZE", Kind::Define, 1, "#define SIZE 4096"),
                entry("LIMIT", Kind::Define, 3, "#define LIMIT (SIZE * 2)"),
                entry("LIMIT", Kind::Define, 5, "#define LIMIT SIZE"),
                entry("MAX", Kind::Define, 7, "#define MAX(a, b)"),
                entry("EMPTY", Kind::Define, 9, "#define EMPTY"),
                entry("f", Kind::Function, 10, "void f(void)"),
                entry("INSIDE", Kind::Define, 11, "#define INSIDE 1"),
            ]
        );
    }

    #[test]
    fn variables() {
        let code = "static const struct ops my_ops = {\n    .open = my_open,\n};\nint a, *b = &a;\n\
                    extern int elsewhere;\nint (*handler)(int) = 0;\n#ifdef X\nlong configured;\n#endif\n\
                    typedef int number;\n";
        assert_eq!(
            extract(code),
            vec![
                entry(
                    "my_ops",
                    Kind::Variable,
                    1,
                    "static const struct ops my_ops"
                ),
                entry("a", Kind::Variable, 4, "int a"),
                entry("b", Kind::Variable, 4, "int *b"),
                entry("handler", Kind::Variable, 6, "int (*handler)(int)"),
                entry("configured", Kind::Variable, 8, "long configured"),
                entry("number", Kind::Typedef, 10, "typedef int number"),
            ]
        );
    }

    #[test]
    fn types() {
        let code = "struct page {\n    unsigned long flags;\n};\n\
                    typedef struct {\n    int counter;\n} atomic_t, *atomic_pointer;\n\
                    typedef int (*handler_t)(int);\n\
                    union value { int i; float f; } global;\n\
                    enum zone { ZONE_DMA, ZONE_NORMAL = 2 };\n\
                    struct page *first(void) { struct local { int x; } l; return 0; }\n";
        assert_eq!(
            extract(code),
            vec![
                entry("page", Kind::Struct, 1, "struct page"),
                entry("atomic_t", Kind::Typedef, 6, "typedef struct atomic_t"),
                entry(
                    "atomic_pointer",
                    Kind::Typedef,
                    6,
                    "typedef struct *atomic_pointer"
                ),
                entry(
                    "handler_t",
                    Kind::Typedef,
                    7,
                    "typedef int (*handler_t)(int)"
                ),
                entry("global", Kind::Variable, 8, "union value global"),
                entry("value", Kind::Union, 8, "union value"),
                entry("zone", Kind::Enum, 9, "enum zone"),
                entry("ZONE_DMA", Kind::Enumerator, 9, "ZONE_DMA"),
                entry("ZONE_NORMAL", Kind::Enumerator, 9, "ZONE_NORMAL = 2"),
                entry("first", Kind::Function, 10, "struct page *first(void)"),
            ]
        );
    }

    #[test]
    fn invocations() {
        let code = "int f(void) { return 0; }\nEXPORT_SYMBOL_GPL(f);\nMODULE_LICENSE(\"GPL\");\n\
                    void g(void) { register_thing(f); }\n";
        assert_eq!(
            extract(code),
            vec![
                entry("f", Kind::Function, 1, "int f(void)"),
                entry("f", Kind::Invocation, 2, "EXPORT_SYMBOL_GPL(f)"),
                entry(
                    "MODULE_LICENSE",
                    Kind::Invocation,
                    3,
                    "MODULE_LICENSE(\"GPL\")"
                ),
                entry("g", Kind::Function, 4, "void g(void)"),
            ]
        );
    }

    #[test]
    fn functions_defined_through_macros_are_invocations() {
        let code = "SYSCALL_DEFINE3(open, const char __user *, filename, int, flags, umode_t, mode)\n\
                    {\n    return 0;\n}\n";
        let symbols = extract(code);
        assert_eq!(
            (symbols[0].0.as_str(), symbols[0].1),
            ("open", Kind::Invocation)
        );
    }

    #[test]
    fn digests_follow_the_whole_definition() {
        let before = extract_symbols(&mut parser(), b"int f(void) { return 0; }\n");
        let after = extract_symbols(&mut parser(), b"int f(void) { return 1; }\n");
        let realigned = extract_symbols(&mut parser(), b"int f(void)\n{\n\treturn  0;\n}\n");
        assert_eq!(before[0].signature, after[0].signature);
        assert_ne!(before[0].digest, after[0].digest);
        assert_eq!(before[0].digest, realigned[0].digest);
    }

    #[test]
    fn file_scope_survives_parse_errors() {
        let code = "{\n#ifdef A\n\tdo_thing(p);\nEXPORT_SYMBOL(copy);\nint later;\n";
        let symbols: Vec<_> = extract(code)
            .into_iter()
            .map(|(name, kind, ..)| (name, kind))
            .collect();
        assert!(
            symbols.contains(&("copy".to_string(), Kind::Invocation)),
            "{symbols:?}"
        );
        assert!(
            symbols.contains(&("later".to_string(), Kind::Variable)),
            "{symbols:?}"
        );
        assert!(!symbols.iter().any(|(name, _)| name == "p"), "{symbols:?}");
        assert_eq!(symbols.len(), 2, "{symbols:?}");
    }

    #[test]
    fn macro_declarations_have_no_name() {
        let code = "static DEFINE_PER_CPU(unsigned long, jiffies_seen);\n\
                    static DEVICE_ATTR(filter, 0644, show_filter, store_filter);\n";
        let symbols = extract(code);
        assert!(
            symbols.iter().all(|(name, ..)| !name.is_empty()),
            "{symbols:?}"
        );
    }

    #[test]
    fn invalid_utf8_and_long_signatures() {
        let mut code = b"#define NAME \"caf\xe9\"\n".to_vec();
        code.extend(format!("#define LONG {}\n", "x".repeat(1000)).as_bytes());
        let symbols = extract_symbols(&mut parser(), &code);
        assert_eq!(symbols[0].signature, "#define NAME \"caf\u{fffd}\"");
        assert!(symbols[1].signature.ends_with('…'));
        assert!(symbols[1].signature.len() <= MAXIMUM_SIGNATURE_LENGTH + '…'.len_utf8());
    }
}
