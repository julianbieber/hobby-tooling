use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use super::navigation::Position;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HoverParams {
    #[schemars(description = "The file URI to analyze")]
    pub uri: String,
    #[schemars(description = "Position in the document")]
    pub position: Position,
    #[schemars(description = "Language identifier (e.g., 'rust', 'python')")]
    pub language_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct HoverResult {
    #[schemars(description = "The hover content (markdown formatted)")]
    pub contents: String,
}
