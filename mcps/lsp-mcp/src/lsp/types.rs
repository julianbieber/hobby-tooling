//! Rendering LSP symbols as text.

use lsp_types::*;

/// Convert SymbolInformation to a human-readable string
pub fn symbol_to_string(symbol: &SymbolInformation) -> String {
    let kind_str = format!("{:?}", symbol.kind);

    match &symbol.container_name {
        Some(container) => format!("{} [{}] ({})", container, symbol.name, kind_str),
        None => format!("{} ({})", symbol.name, kind_str),
    }
}

/// Get a searchable string from a symbol for fuzzy matching
pub fn symbol_to_searchable(symbol: &SymbolInformation) -> String {
    match &symbol.container_name {
        Some(container) => format!("{}::{}", container, symbol.name),
        None => symbol.name.clone(),
    }
}

/// Format symbol kind as a short string
pub fn symbol_kind_to_string(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::FILE => "File",
        SymbolKind::MODULE => "Module",
        SymbolKind::NAMESPACE => "Namespace",
        SymbolKind::PACKAGE => "Package",
        SymbolKind::CLASS => "Class",
        SymbolKind::METHOD => "Method",
        SymbolKind::PROPERTY => "Property",
        SymbolKind::FIELD => "Field",
        SymbolKind::CONSTRUCTOR => "Constructor",
        SymbolKind::ENUM => "Enum",
        SymbolKind::INTERFACE => "Interface",
        SymbolKind::FUNCTION => "Function",
        SymbolKind::VARIABLE => "Variable",
        SymbolKind::CONSTANT => "Constant",
        SymbolKind::STRING => "String",
        SymbolKind::NUMBER => "Number",
        SymbolKind::BOOLEAN => "Boolean",
        SymbolKind::ARRAY => "Array",
        SymbolKind::OBJECT => "Object",
        SymbolKind::KEY => "Key",
        SymbolKind::NULL => "Null",
        SymbolKind::ENUM_MEMBER => "EnumMember",
        SymbolKind::STRUCT => "Struct",
        SymbolKind::EVENT => "Event",
        SymbolKind::OPERATOR => "Operator",
        SymbolKind::TYPE_PARAMETER => "TypeParameter",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[expect(
        deprecated,
        reason = "SymbolInformation has no constructor that leaves its deprecated field out"
    )]
    fn make_test_symbol(
        name: &str,
        container: Option<&str>,
        kind: SymbolKind,
    ) -> SymbolInformation {
        SymbolInformation {
            name: name.to_string(),
            kind,
            deprecated: None,
            location: Location {
                uri: Url::parse("file:///test.rs").unwrap(),
                range: Range {
                    start: Position {
                        line: 10,
                        character: 5,
                    },
                    end: Position {
                        line: 10,
                        character: 15,
                    },
                },
            },
            container_name: container.map(|s| s.to_string()),
            tags: None,
        }
    }

    #[test]
    fn test_symbol_to_string_no_container() {
        let symbol = make_test_symbol("test_function", None, SymbolKind::FUNCTION);
        let result = symbol_to_string(&symbol);
        assert!(result.contains("test_function"));
        assert!(result.contains("Function") || result.contains("FUNCTION"));
    }

    #[test]
    fn test_symbol_to_string_with_container() {
        let symbol = make_test_symbol("method", Some("MyClass"), SymbolKind::METHOD);
        let result = symbol_to_string(&symbol);
        assert!(result.contains("MyClass"));
        assert!(result.contains("method"));
        assert!(result.contains("Method") || result.contains("METHOD"));
    }

    #[test]
    fn test_symbol_to_searchable_no_container() {
        let symbol = make_test_symbol("standalone", None, SymbolKind::FUNCTION);
        assert_eq!(symbol_to_searchable(&symbol), "standalone");
    }

    #[test]
    fn test_symbol_to_searchable_with_container() {
        let symbol = make_test_symbol("method", Some("Container"), SymbolKind::METHOD);
        assert_eq!(symbol_to_searchable(&symbol), "Container::method");
    }

    #[test]
    fn test_symbol_kind_to_string() {
        assert_eq!(symbol_kind_to_string(SymbolKind::FUNCTION), "Function");
        assert_eq!(symbol_kind_to_string(SymbolKind::CLASS), "Class");
        assert_eq!(symbol_kind_to_string(SymbolKind::METHOD), "Method");
        assert_eq!(symbol_kind_to_string(SymbolKind::STRUCT), "Struct");
        assert_eq!(symbol_kind_to_string(SymbolKind::ENUM), "Enum");
        assert_eq!(symbol_kind_to_string(SymbolKind::VARIABLE), "Variable");
        assert_eq!(symbol_kind_to_string(SymbolKind::CONSTANT), "Constant");
    }
}
