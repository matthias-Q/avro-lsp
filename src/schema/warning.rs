use async_lsp::lsp_types::Range;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SchemaWarning {
    UnionWithMultipleComplexTypes {
        complex_type_names: Vec<String>,
        range: Option<Range>,
        message: String,
    },
    // Future warnings can be added here:
    // LargeRecordWithManyFields { field_count: usize, range: Option<Range> },
    // DeepNesting { depth: usize, range: Option<Range> },
}

impl SchemaWarning {
    pub fn message(&self) -> String {
        match self {
            SchemaWarning::UnionWithMultipleComplexTypes { message, .. } => message.clone(),
        }
    }

    pub fn range(&self) -> Option<Range> {
        match self {
            SchemaWarning::UnionWithMultipleComplexTypes { range, .. } => *range,
        }
    }
}
