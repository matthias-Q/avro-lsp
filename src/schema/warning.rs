use async_lsp::lsp_types::Range;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SchemaWarning {
    UnionWithMultipleComplexTypes {
        complex_type_names: Vec<String>,
        range: Option<Range>,
        message: String,
    },
    UnknownField {
        field: String,
        context: String,
        range: Option<Range>,
        suggested: Option<String>,
    },
    InvalidLogicalType {
        logical_type: String,
        primitive_type: String,
        range: Option<Range>,
        suggested: Option<String>,
    },
    // Future warnings can be added here:
    // LargeRecordWithManyFields { field_count: usize, range: Option<Range> },
    // DeepNesting { depth: usize, range: Option<Range> },
}

impl SchemaWarning {
    pub fn message(&self) -> String {
        match self {
            SchemaWarning::UnionWithMultipleComplexTypes { message, .. } => message.clone(),
            SchemaWarning::UnknownField {
                field,
                context,
                suggested,
                ..
            } => {
                let mut msg = format!("Unknown field '{}' in {}", field, context);
                if let Some(suggestion) = suggested {
                    msg.push_str(&format!(". Did you mean '{}'?", suggestion));
                }
                msg
            }
            SchemaWarning::InvalidLogicalType {
                logical_type,
                suggested,
                ..
            } => {
                let mut msg = format!("Unknown logical type '{}' will be ignored", logical_type);
                if let Some(suggestion) = suggested {
                    msg.push_str(&format!(". Did you mean '{}'?", suggestion));
                }
                msg
            }
        }
    }

    pub fn range(&self) -> Option<Range> {
        match self {
            SchemaWarning::UnionWithMultipleComplexTypes { range, .. } => *range,
            SchemaWarning::UnknownField { range, .. } => *range,
            SchemaWarning::InvalidLogicalType { range, .. } => *range,
        }
    }
}
