//! Positions, ranges and locations as the navigation tools exchange them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Position {
    #[schemars(description = "Line number (0-indexed)")]
    pub line: u32,
    #[schemars(description = "Character position (0-indexed)")]
    pub character: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GotoDefinitionParams {
    #[schemars(description = "The file URI to analyze")]
    pub uri: String,
    #[schemars(description = "Position in the document")]
    pub position: Position,
    #[schemars(description = "Language identifier (e.g., 'rust', 'python')")]
    pub language_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindReferencesParams {
    #[schemars(description = "The file URI to analyze")]
    pub uri: String,
    #[schemars(description = "Position in the document")]
    pub position: Position,
    #[schemars(description = "Language identifier (e.g., 'rust', 'python')")]
    pub language_id: String,
    #[schemars(description = "Include the declaration in the results")]
    #[serde(default)]
    pub include_declaration: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Range {
    #[schemars(description = "Start position")]
    pub start: Position,
    #[schemars(description = "End position")]
    pub end: Position,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Location {
    #[schemars(description = "The URI of the document")]
    pub uri: String,
    #[schemars(description = "The range in the document")]
    pub range: Range,
}

impl From<Position> for lsp_types::Position {
    fn from(p: Position) -> Self {
        lsp_types::Position {
            line: p.line,
            character: p.character,
        }
    }
}

impl From<lsp_types::Position> for Position {
    fn from(p: lsp_types::Position) -> Self {
        Position {
            line: p.line,
            character: p.character,
        }
    }
}

impl From<Range> for lsp_types::Range {
    fn from(r: Range) -> Self {
        lsp_types::Range {
            start: r.start.into(),
            end: r.end.into(),
        }
    }
}

impl From<lsp_types::Range> for Range {
    fn from(r: lsp_types::Range) -> Self {
        Range {
            start: r.start.into(),
            end: r.end.into(),
        }
    }
}

impl From<lsp_types::Location> for Location {
    fn from(loc: lsp_types::Location) -> Self {
        Location {
            uri: loc.uri.to_string(),
            range: loc.range.into(),
        }
    }
}
