use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use super::navigation::Position;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompletionParams {
    #[schemars(description = "The file URI to get completions for")]
    pub uri: String,
    #[schemars(description = "Cursor position in the document")]
    pub position: Position,
    #[schemars(description = "Language identifier (e.g., 'rust', 'python')")]
    pub language_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CompletionResult {
    #[schemars(description = "Whether the completion list is incomplete (more items available with further typing)")]
    pub is_incomplete: bool,
    #[schemars(description = "The completion items")]
    pub items: Vec<CompletionItem>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CompletionItem {
    #[schemars(description = "The label of this completion item (displayed in the list)")]
    pub label: String,
    #[schemars(description = "The kind of completion (e.g., Function, Variable, Field)")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[schemars(description = "Additional detail, like type or symbol information")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[schemars(description = "Documentation for this item")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[schemars(description = "Text to insert when selecting this completion (if different from label)")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
    #[schemars(description = "String used for sorting (when different from label)")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_text: Option<String>,
    #[schemars(description = "String used for filtering (when different from label)")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_text: Option<String>,
}

/// Convert an lsp_types::CompletionItemKind to a human-readable string
pub fn completion_item_kind_to_string(kind: lsp_types::CompletionItemKind) -> &'static str {
    match kind {
        lsp_types::CompletionItemKind::TEXT => "Text",
        lsp_types::CompletionItemKind::METHOD => "Method",
        lsp_types::CompletionItemKind::FUNCTION => "Function",
        lsp_types::CompletionItemKind::CONSTRUCTOR => "Constructor",
        lsp_types::CompletionItemKind::FIELD => "Field",
        lsp_types::CompletionItemKind::VARIABLE => "Variable",
        lsp_types::CompletionItemKind::CLASS => "Class",
        lsp_types::CompletionItemKind::INTERFACE => "Interface",
        lsp_types::CompletionItemKind::MODULE => "Module",
        lsp_types::CompletionItemKind::PROPERTY => "Property",
        lsp_types::CompletionItemKind::UNIT => "Unit",
        lsp_types::CompletionItemKind::VALUE => "Value",
        lsp_types::CompletionItemKind::ENUM => "Enum",
        lsp_types::CompletionItemKind::KEYWORD => "Keyword",
        lsp_types::CompletionItemKind::SNIPPET => "Snippet",
        lsp_types::CompletionItemKind::COLOR => "Color",
        lsp_types::CompletionItemKind::FILE => "File",
        lsp_types::CompletionItemKind::REFERENCE => "Reference",
        lsp_types::CompletionItemKind::FOLDER => "Folder",
        lsp_types::CompletionItemKind::ENUM_MEMBER => "EnumMember",
        lsp_types::CompletionItemKind::CONSTANT => "Constant",
        lsp_types::CompletionItemKind::STRUCT => "Struct",
        lsp_types::CompletionItemKind::EVENT => "Event",
        lsp_types::CompletionItemKind::OPERATOR => "Operator",
        lsp_types::CompletionItemKind::TYPE_PARAMETER => "TypeParameter",
        _ => "Unknown",
    }
}

/// Extract documentation as a plain string from an lsp_types::Documentation
pub fn documentation_to_string(doc: &lsp_types::Documentation) -> String {
    match doc {
        lsp_types::Documentation::String(s) => s.clone(),
        lsp_types::Documentation::MarkupContent(markup) => markup.value.clone(),
    }
}

/// Convert an lsp_types::CompletionItem to our simplified CompletionItem
pub fn convert_completion_item(item: &lsp_types::CompletionItem) -> CompletionItem {
    CompletionItem {
        label: item.label.clone(),
        kind: item.kind.map(completion_item_kind_to_string).map(String::from),
        detail: item.detail.clone(),
        documentation: item.documentation.as_ref().map(documentation_to_string),
        insert_text: item.insert_text.clone(),
        sort_text: item.sort_text.clone(),
        filter_text: item.filter_text.clone(),
    }
}
