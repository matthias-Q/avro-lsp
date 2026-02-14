//! Quick fixes for default value errors in Avro schemas

use async_lsp::lsp_types::{CodeAction, Diagnostic, Position, Range, Url};

use crate::handlers::code_actions::builder::CodeActionBuilder;

/// Create a quick fix for invalid boolean default values
/// Changes invalid values like "yes", "true", "1" to true, or "no", "false", "0" to false
pub(in crate::handlers::code_actions) fn create_fix_invalid_boolean_default(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    let lines: Vec<&str> = text.lines().collect();
    let start_line = diagnostic.range.start.line as usize;
    let end_line = (diagnostic.range.end.line as usize).min(lines.len());

    for line_idx in start_line..=end_line {
        if line_idx >= lines.len() {
            break;
        }

        let line = lines[line_idx];

        if let Some(default_pos) = line.find("\"default\"")
            && let Some(colon_pos) = line[default_pos..].find(':')
        {
            let after_colon = &line[default_pos + colon_pos + 1..];

            let trimmed = after_colon.trim_start();
            let ws_offset = after_colon.len() - trimmed.len();

            let value_end = trimmed
                .find(',')
                .or_else(|| trimmed.find('}'))
                .unwrap_or(trimmed.len());
            let value = trimmed[..value_end].trim();

            let value_start_col = default_pos + colon_pos + 1 + ws_offset;
            let value_end_col = value_start_col + value.len();

            let value_range = Range {
                start: Position {
                    line: line_idx as u32,
                    character: value_start_col as u32,
                },
                end: Position {
                    line: line_idx as u32,
                    character: value_end_col as u32,
                },
            };

            // Determine the correct boolean value
            // For strings like "yes", "true", "1" -> true
            // For "no", "false", "0" -> false
            // Default to false for safety
            let lower_value = value.to_lowercase().trim_matches('"').to_string();
            let correct_value = if lower_value.contains("true")
                || lower_value.contains("yes")
                || lower_value == "1"
            {
                "true"
            } else {
                "false"
            };

            return Some(
                CodeActionBuilder::new(
                    uri.clone(),
                    format!("Fix invalid boolean default: change to {}", correct_value),
                )
                .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                .with_diagnostics(vec![diagnostic.clone()])
                .preferred()
                .add_edit(value_range, correct_value.to_string())
                .build(),
            );
        }
    }

    None
}

/// Create a quick fix for invalid array default values
/// Changes invalid values like "string" or 123 to []
pub(in crate::handlers::code_actions) fn create_fix_invalid_array_default(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    let lines: Vec<&str> = text.lines().collect();
    let start_line = diagnostic.range.start.line as usize;
    let end_line = (diagnostic.range.end.line as usize).min(lines.len());

    for line_idx in start_line..=end_line {
        if line_idx >= lines.len() {
            break;
        }

        let line = lines[line_idx];

        if let Some(default_pos) = line.find("\"default\"")
            && let Some(colon_pos) = line[default_pos..].find(':')
        {
            let after_colon = &line[default_pos + colon_pos + 1..];

            let trimmed = after_colon.trim_start();
            let ws_offset = after_colon.len() - trimmed.len();

            let value_end = trimmed
                .find(',')
                .or_else(|| trimmed.find('}'))
                .unwrap_or(trimmed.len());
            let value = trimmed[..value_end].trim();

            let value_start_col = default_pos + colon_pos + 1 + ws_offset;
            let value_end_col = value_start_col + value.len();

            let value_range = Range {
                start: Position {
                    line: line_idx as u32,
                    character: value_start_col as u32,
                },
                end: Position {
                    line: line_idx as u32,
                    character: value_end_col as u32,
                },
            };

            return Some(
                CodeActionBuilder::new(
                    uri.clone(),
                    "Fix invalid array default: change to []".to_string(),
                )
                .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                .with_diagnostics(vec![diagnostic.clone()])
                .preferred()
                .add_edit(value_range, "[]".to_string())
                .build(),
            );
        }
    }

    None
}

/// Create a quick fix for invalid enum default values
/// Adds the missing symbol to the enum's symbols array
pub(in crate::handlers::code_actions) fn create_fix_invalid_enum_default(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    let msg = &diagnostic.message;
    let invalid_value = if let Some(start) = msg.find('\'') {
        if let Some(end) = msg[start + 1..].find('\'') {
            &msg[start + 1..start + 1 + end]
        } else {
            return None;
        }
    } else {
        return None;
    };

    let lines: Vec<&str> = text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        if line.contains("\"symbols\"")
            && let Some(bracket_start) = line.find('[')
            && let Some(bracket_end) = line.find(']')
        {
            let array_str = &line[bracket_start..=bracket_end];

            if let Ok(serde_json::Value::Array(arr)) =
                serde_json::from_str::<serde_json::Value>(array_str)
            {
                let current_symbols: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();

                if current_symbols.iter().any(|s| s == invalid_value) {
                    return None;
                }

                let mut new_symbols = current_symbols.clone();
                new_symbols.push(invalid_value.to_string());

                let new_array_str = serde_json::to_string(&new_symbols).ok()?;
                let range = Range {
                    start: Position {
                        line: line_idx as u32,
                        character: bracket_start as u32,
                    },
                    end: Position {
                        line: line_idx as u32,
                        character: (bracket_end + 1) as u32,
                    },
                };

                return Some(
                    CodeActionBuilder::new(
                        uri.clone(),
                        format!("Add '{}' to enum symbols", invalid_value),
                    )
                    .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                    .with_diagnostics(vec![diagnostic.clone()])
                    .preferred()
                    .add_edit(range, new_array_str)
                    .build(),
                );
            }
        }
    }

    None
}
