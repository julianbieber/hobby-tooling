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
