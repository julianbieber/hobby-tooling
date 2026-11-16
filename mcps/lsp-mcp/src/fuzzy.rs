//! Ranking workspace symbols against a fuzzy query.

use lsp_types::SymbolInformation;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// A ranked symbol with fuzzy match score
#[derive(Debug, Clone)]
pub struct RankedSymbol {
    pub symbol: SymbolInformation,
    pub score: u32,
}

/// Fuzzy search symbols
pub fn fuzzy_search(symbols: &[SymbolInformation], query: &str) -> Vec<RankedSymbol> {
    // If query is empty, return all symbols with default score
    if query.trim().is_empty() {
        return symbols
            .iter()
            .map(|s| RankedSymbol {
                symbol: s.clone(),
                score: 100,
            })
            .collect();
    }

    let mut matcher = Matcher::new(Config::DEFAULT);

    // Use smart case matching: case-sensitive if query has uppercase, otherwise ignore case
    let case_matching = if query.chars().any(|c| c.is_uppercase()) {
        CaseMatching::Respect
    } else {
        CaseMatching::Ignore
    };

    let pattern = Pattern::parse(query, case_matching, Normalization::Smart);

    let mut buf_indices = Vec::new();
    let mut ranked: Vec<RankedSymbol> = symbols
        .iter()
        .filter_map(|symbol| {
            let searchable = symbol_to_searchable(symbol);
            let haystack = Utf32Str::new(&searchable, &mut buf_indices);
            pattern
                .score(haystack, &mut matcher)
                .map(|score| RankedSymbol {
                    symbol: symbol.clone(),
                    score,
                })
        })
        .collect();

    // Sort by score descending (higher score = better match)
    ranked.sort_by_key(|r| std::cmp::Reverse(r.score));

    ranked
}

/// Convert a symbol to a searchable string
fn symbol_to_searchable(symbol: &SymbolInformation) -> String {
    match &symbol.container_name {
        Some(container) => format!("{}::{}", container, symbol.name),
        None => symbol.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Location, Position, Range, SymbolKind, Url};

    #[expect(
        deprecated,
        reason = "SymbolInformation has no constructor that leaves its deprecated field out"
    )]
    fn make_test_symbol(name: &str, container: Option<&str>) -> SymbolInformation {
        SymbolInformation {
            name: name.to_string(),
            kind: SymbolKind::FUNCTION,
            deprecated: None,
            location: Location {
                uri: Url::parse("file:///test.rs").unwrap(),
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
            },
            container_name: container.map(|s| s.to_string()),
            tags: None,
        }
    }

    #[test]
    fn test_empty_query() {
        let symbols = vec![make_test_symbol("foo", None), make_test_symbol("bar", None)];

        let results = fuzzy_search(&symbols, "");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].score, 100);
    }

    #[test]
    fn test_fuzzy_matching() {
        let symbols = vec![
            make_test_symbol("my_function", None),
            make_test_symbol("another_method", None),
            make_test_symbol("MyClass", None),
        ];

        let results = fuzzy_search(&symbols, "myfn");
        assert!(!results.is_empty());
        assert_eq!(results[0].symbol.name, "my_function");
    }

    #[test]
    fn test_container_search() {
        let symbols = vec![
            make_test_symbol("method", Some("MyClass")),
            make_test_symbol("method", Some("OtherClass")),
        ];

        let results = fuzzy_search(&symbols, "MyClass");
        assert!(!results.is_empty());
        // Should match the symbol in MyClass container
        assert!(
            results
                .iter()
                .any(|r| r.symbol.container_name.as_deref() == Some("MyClass"))
        );
    }

    #[test]
    fn test_score_ordering() {
        let symbols = vec![
            make_test_symbol("test", None),
            make_test_symbol("testing", None),
            make_test_symbol("contest", None),
        ];

        let results = fuzzy_search(&symbols, "test");
        assert!(!results.is_empty());

        // "test" should score higher than "testing" or "contest"
        assert_eq!(results[0].symbol.name, "test");

        // Scores should be in descending order
        for i in 1..results.len() {
            assert!(results[i - 1].score >= results[i].score);
        }
    }
}
