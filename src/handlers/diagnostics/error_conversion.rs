use async_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::schema::SchemaError;

use super::json_position::{extract_error_position_with_context, improve_error_message};

/// Convert a parse error to a diagnostic
pub(super) fn convert_parse_error(error: &SchemaError, text: &str) -> Diagnostic {
    let error_msg = error.to_string();
    tracing::debug!("JSON parse error: {}", error_msg);

    let (position_range, adjusted_msg) = match error {
        SchemaError::MissingFieldWithContext {
            range: Some(r),
            field,
            context,
            ..
        } => {
            tracing::debug!("Using position from error context: {:?}", r);
            let msg = format!("Missing required field '{}' in {}", field, context);
            (*r, msg)
        }
        SchemaError::MissingFieldWithContext {
            range: None,
            field: _,
            context: _,
            ..
        } => {
            let (pos, was_adjusted) = extract_error_position_with_context(&error_msg, text);
            let range = Range {
                start: pos,
                end: Position {
                    line: pos.line,
                    character: pos.character + 1,
                },
            };
            let msg = improve_error_message(&error_msg, &pos, was_adjusted);
            (range, msg)
        }
        SchemaError::InvalidPrimitiveType {
            type_name,
            range: Some(r),
            suggested,
        } => {
            tracing::debug!("Using position from InvalidPrimitiveType: {:?}", r);
            let msg = if let Some(suggestion) = suggested {
                format!(
                    "Invalid primitive type '{}'. Did you mean '{}'?",
                    type_name, suggestion
                )
            } else {
                format!("Invalid primitive type '{}'", type_name)
            };
            (*r, msg)
        }
        SchemaError::InvalidPrimitiveType {
            type_name,
            range: None,
            suggested,
        } => {
            let (pos, _) = extract_error_position_with_context(&error_msg, text);
            let range = Range {
                start: pos,
                end: Position {
                    line: pos.line,
                    character: pos.character + 1,
                },
            };
            let msg = if let Some(suggestion) = suggested {
                format!(
                    "Invalid primitive type '{}'. Did you mean '{}'?",
                    type_name, suggestion
                )
            } else {
                format!("Invalid primitive type '{}'", type_name)
            };
            (range, msg)
        }
        SchemaError::DuplicateJsonKey {
            key,
            first_occurrence: _,
            duplicate_occurrence: Some(dup_range),
        } => {
            tracing::debug!("Using duplicate occurrence position: {:?}", dup_range);
            let msg = format!("Duplicate key '{}'", key);
            (*dup_range, msg)
        }
        SchemaError::DuplicateJsonKey {
            key,
            first_occurrence: Some(first_range),
            duplicate_occurrence: None,
        } => {
            tracing::debug!("Using first occurrence position: {:?}", first_range);
            let msg = format!("Duplicate key '{}'", key);
            (*first_range, msg)
        }
        _ => {
            let (pos, was_adjusted) = extract_error_position_with_context(&error_msg, text);
            let range = Range {
                start: pos,
                end: Position {
                    line: pos.line,
                    character: pos.character + 1,
                },
            };
            let msg = improve_error_message(&error_msg, &pos, was_adjusted);
            (range, msg)
        }
    };

    tracing::debug!("Extracted position range: {:?}", position_range);

    let error_data = serde_json::to_value(error).ok();
    if let Some(ref data) = error_data {
        tracing::debug!("Diagnostic data being sent: {}", data);
    }

    Diagnostic {
        range: position_range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("avro-lsp".to_string()),
        message: adjusted_msg,
        related_information: None,
        tags: None,
        data: error_data,
    }
}
