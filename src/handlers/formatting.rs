use std::sync::OnceLock;

use async_lsp::lsp_types::{Position, Range, TextEdit};
use async_lsp::{ErrorCode, ResponseError};

static TRAILING_COMMA_RE: OnceLock<regex::Regex> = OnceLock::new();

fn trailing_comma_re() -> &'static regex::Regex {
    TRAILING_COMMA_RE.get_or_init(|| regex::Regex::new(r",(\s*[}\]])").unwrap())
}

/// Format Avro schema document with proper JSON formatting
/// Removes trailing commas and formats with 2-space indentation
pub fn format_document(text: &str) -> Result<TextEdit, ResponseError> {
    // First, remove trailing commas before parsing
    let cleaned_text = remove_trailing_commas(text);

    // Parse JSON to validate and normalize
    let json: serde_json::Value = serde_json::from_str(&cleaned_text).map_err(|e| {
        ResponseError::new(
            ErrorCode::PARSE_ERROR,
            format!("Invalid JSON, cannot format: {}", e),
        )
    })?;

    // Format with serde_json (uses 2-space indentation by default)
    let formatted = serde_json::to_string_pretty(&json).map_err(|e| {
        ResponseError::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Formatting failed: {}", e),
        )
    })?;

    // Add final newline
    let formatted = format!("{}\n", formatted);

    // Calculate the end position of the document
    let line_count = text.lines().count() as u32;
    let last_line = text.lines().last().unwrap_or("");
    let last_line_length = last_line.len() as u32;

    // Create TextEdit for full document replacement
    Ok(TextEdit {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: line_count.saturating_sub(1),
                character: last_line_length,
            },
        },
        new_text: formatted,
    })
}

/// Remove trailing commas from JSON text
/// This handles cases like {"foo": "bar",} which are invalid JSON
pub fn remove_trailing_commas(text: &str) -> String {
    trailing_comma_re().replace_all(text, "$1").to_string()
}
