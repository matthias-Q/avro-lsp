use async_lsp::lsp_types::Range;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SchemaError {
    #[error("Invalid JSON: {message}")]
    InvalidJson { message: String },

    #[error("Missing required field: {field}")]
    MissingField { field: String },

    #[error("Missing required field '{field}' in {context}")]
    MissingFieldWithContext {
        field: String,
        context: String,
        range: Option<Range>,
    },

    #[error("Invalid type: expected {expected}, found {found}")]
    InvalidType {
        expected: String,
        found: String,
        range: Option<Range>,
    },

    #[error("Invalid name '{name}': must match [A-Za-z_][A-Za-z0-9_]*")]
    InvalidName {
        name: String,
        range: Option<Range>,
        suggested: Option<String>,
    },

    #[error("Invalid namespace '{namespace}': must be dot-separated names")]
    InvalidNamespace {
        namespace: String,
        range: Option<Range>,
        suggested: Option<String>,
    },

    #[error("Unknown type reference: {type_name}")]
    UnknownTypeReference {
        type_name: String,
        range: Option<Range>,
    },

    #[error("Duplicate symbol '{symbol}' in enum")]
    DuplicateSymbol {
        symbol: String,
        first_occurrence: Option<Range>,
        duplicate_occurrence: Option<Range>,
    },

    #[error("Duplicate field name '{field}' in record '{record}'")]
    DuplicateFieldName {
        field: String,
        record: String,
        first_occurrence: Option<Range>,
        duplicate_occurrence: Option<Range>,
    },

    #[error("Duplicate JSON key '{key}' in object")]
    DuplicateJsonKey {
        key: String,
        first_occurrence: Option<Range>,
        duplicate_occurrence: Option<Range>,
    },

    #[error("Duplicate type in union: {type_signature}")]
    DuplicateUnionType {
        type_signature: String,
        range: Option<Range>,
    },

    #[error("Nested unions are not allowed")]
    NestedUnion { range: Option<Range> },

    #[error("Invalid primitive type: {type_name}")]
    InvalidPrimitiveType {
        type_name: String,
        range: Option<Range>,
        suggested: Option<String>,
    },

    #[error("Unknown field '{field}' in {context}")]
    UnknownField {
        field: String,
        context: String,
        range: Option<Range>,
        suggested: Option<String>,
    },

    #[error("{message}")]
    Custom {
        message: String,
        range: Option<Range>,
    },
}

// Implement From<serde_json::Error> manually since we changed the variant
impl From<serde_json::Error> for SchemaError {
    fn from(err: serde_json::Error) -> Self {
        SchemaError::InvalidJson {
            message: err.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, SchemaError>;
