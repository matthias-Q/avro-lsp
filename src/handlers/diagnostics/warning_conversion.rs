use async_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::schema::SchemaWarning;

/// Convert a schema warning to an LSP diagnostic with WARNING severity
pub fn convert_warning(warning: &SchemaWarning) -> Diagnostic {
    let range = warning.range().unwrap_or_else(|| Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 1,
        },
    });

    let warning_data = serde_json::to_value(warning).ok();

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::WARNING), // WARNING, not ERROR
        code: None,
        code_description: None,
        source: Some("avro-lsp".to_string()),
        message: warning.message(),
        related_information: None,
        tags: None,
        data: warning_data,
    }
}
