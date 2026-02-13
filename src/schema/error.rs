use async_lsp::lsp_types::Range;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SchemaError {
    #[error("Invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Missing required field '{field}' in {context}")]
    MissingFieldWithContext {
        field: String,
        context: String,
        range: Option<Range>,
    },

    #[error("Invalid type: expected {expected}, found {found}")]
    InvalidType { expected: String, found: String },

    #[error("Invalid name '{0}': must match [A-Za-z_][A-Za-z0-9_]*")]
    InvalidName(String),

    #[error("Invalid namespace '{0}': must be dot-separated names")]
    InvalidNamespace(String),

    #[error("Unknown type reference: {0}")]
    UnknownTypeReference(String),

    #[error("Duplicate symbol '{0}' in enum")]
    DuplicateSymbol(String),

    #[error("Duplicate type in union: {0}")]
    DuplicateUnionType(String),

    #[error("Nested unions are not allowed")]
    NestedUnion,

    #[error("Invalid primitive type: {0}")]
    InvalidPrimitiveType(String),

    #[error("{0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, SchemaError>;
