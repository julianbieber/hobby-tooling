use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocumentSymbolParams {
    #[schemars(description = "The file URI to get symbols from")]
    pub uri: String,
    #[schemars(description = "Language identifier (e.g., 'rust', 'python')")]
    pub language_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentSymbol {
    #[schemars(description = "Symbol name")]
    pub name: String,

    #[schemars(description = "Symbol kind (e.g., Function, Class, Variable)")]
    pub kind: String,

    #[schemars(description = "Symbol detail (e.g., function signature)")]
    pub detail: Option<String>,

    #[schemars(description = "Start line number (0-indexed)")]
    pub start_line: u32,

    #[schemars(description = "Start character position (0-indexed)")]
    pub start_character: u32,

    #[schemars(description = "End line number (0-indexed)")]
    pub end_line: u32,

    #[schemars(description = "End character position (0-indexed)")]
    pub end_character: u32,

    #[schemars(description = "Children symbols (for nested structures)")]
    pub children: Vec<DocumentSymbol>,
}
