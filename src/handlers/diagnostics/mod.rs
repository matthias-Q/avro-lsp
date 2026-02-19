use async_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::schema::{AvroParser, AvroValidator};
use crate::workspace::Workspace;

mod error_conversion;
mod json_position;
mod position_finder;
mod text_search;
mod warning_conversion;

#[cfg(test)]
mod tests;

use error_conversion::convert_parse_error;
use position_finder::find_error_position_in_ast;
use warning_conversion::convert_warning;

/// Parse and validate Avro schema text, returning diagnostics
/// If workspace is provided, cross-file type references will be validated
#[allow(dead_code)] // Used for backward compatibility
pub fn parse_and_validate(text: &str) -> Vec<Diagnostic> {
    parse_and_validate_with_workspace(text, None)
}

/// Parse and validate with optional workspace for cross-file type checking
pub fn parse_and_validate_with_workspace(
    text: &str,
    workspace: Option<&Workspace>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Try to parse
    let mut parser = AvroParser::new();
    let schema = match parser.parse(text) {
        Ok(schema) => schema,
        Err(e) => {
            diagnostics.push(convert_parse_error(&e, text));
            return diagnostics;
        }
    };

    // Check for parse errors that were collected during error recovery
    for parse_error in &schema.parse_errors {
        diagnostics.push(convert_parse_error(parse_error, text));
    }

    // Add warnings from parser (e.g., unknown fields)
    for warning in &schema.warnings {
        diagnostics.push(convert_warning(warning));
    }

    // Try to validate - use workspace if provided for cross-file type checking
    let validator = AvroValidator::new();
    let validation_result = if let Some(ws) = workspace {
        validator.validate_with_resolver(&schema, ws)
    } else {
        validator.validate(&schema)
    };

    if let Err(e) = validation_result {
        let position_range = find_error_position_in_ast(&e, &schema, text);
        let error_data = serde_json::to_value(&e).ok();

        diagnostics.push(Diagnostic {
            range: position_range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("avro-lsp".to_string()),
            message: format!("Validation error: {}", e),
            related_information: None,
            tags: None,
            data: error_data,
        });
    }

    // Collect and add warnings (non-blocking issues)
    let warnings = validator.collect_warnings(&schema);
    for warning in &warnings {
        diagnostics.push(convert_warning(warning));
    }

    diagnostics
}
