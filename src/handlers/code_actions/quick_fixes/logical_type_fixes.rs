//! Quick fixes for logical type errors in Avro schemas (decimal, duration)

use std::collections::HashMap;

use async_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, Position, Range, TextEdit, Url, WorkspaceEdit,
};

use crate::handlers::code_actions::builder::CodeActionBuilder;
use crate::handlers::code_actions::helpers::find_primitive_type_range;
use crate::schema::AvroSchema;

/// Create quick fixes for invalid decimal scale errors
/// Offers two options: reduce scale to match precision, or increase precision to match scale
pub(in crate::handlers::code_actions) fn create_fix_invalid_decimal_scale(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Vec<CodeAction> {
    let mut fixes = Vec::new();
    let msg = &diagnostic.message;

    let scale_value = if let Some(start) = msg.find("scale (") {
        let after = &msg[start + 7..];
        if let Some(end) = after.find(')') {
            after[..end].parse::<u32>().ok()
        } else {
            None
        }
    } else {
        None
    };

    let precision_value = if let Some(start) = msg.find("precision (") {
        let after = &msg[start + 11..];
        if let Some(end) = after.find(')') {
            after[..end].parse::<u32>().ok()
        } else {
            None
        }
    } else {
        None
    };

    let (scale, precision) = match (scale_value, precision_value) {
        (Some(s), Some(p)) => (s, p),
        _ => return fixes,
    };

    let lines: Vec<&str> = text.lines().collect();
    let start_line = diagnostic.range.start.line as usize;
    let end_line = (diagnostic.range.end.line as usize).min(lines.len());

    let mut scale_range: Option<Range> = None;
    let mut precision_range: Option<Range> = None;

    for line_idx in start_line..=end_line {
        if line_idx >= lines.len() {
            break;
        }

        let line = lines[line_idx];

        if scale_range.is_none()
            && line.contains("\"scale\"")
            && let Some(scale_pos) = line.find("\"scale\"")
            && let Some(colon_pos) = line[scale_pos..].find(':')
        {
            let after_colon = &line[scale_pos + colon_pos + 1..];
            let trimmed = after_colon.trim_start();
            let ws_offset = after_colon.len() - trimmed.len();

            let value_end = trimmed
                .find(',')
                .or_else(|| trimmed.find('}'))
                .or_else(|| trimmed.find('\n'))
                .unwrap_or(trimmed.len());
            let value = trimmed[..value_end].trim();

            let value_start_col = scale_pos + colon_pos + 1 + ws_offset;
            let value_end_col = value_start_col + value.len();

            scale_range = Some(Range {
                start: Position {
                    line: line_idx as u32,
                    character: value_start_col as u32,
                },
                end: Position {
                    line: line_idx as u32,
                    character: value_end_col as u32,
                },
            });
        }

        if precision_range.is_none()
            && line.contains("\"precision\"")
            && let Some(prec_pos) = line.find("\"precision\"")
            && let Some(colon_pos) = line[prec_pos..].find(':')
        {
            let after_colon = &line[prec_pos + colon_pos + 1..];
            let trimmed = after_colon.trim_start();
            let ws_offset = after_colon.len() - trimmed.len();

            let value_end = trimmed
                .find(',')
                .or_else(|| trimmed.find('}'))
                .or_else(|| trimmed.find('\n'))
                .unwrap_or(trimmed.len());
            let value = trimmed[..value_end].trim();

            let value_start_col = prec_pos + colon_pos + 1 + ws_offset;
            let value_end_col = value_start_col + value.len();

            precision_range = Some(Range {
                start: Position {
                    line: line_idx as u32,
                    character: value_start_col as u32,
                },
                end: Position {
                    line: line_idx as u32,
                    character: value_end_col as u32,
                },
            });
        }

        if scale_range.is_some() && precision_range.is_some() {
            break;
        }
    }

    if let Some(range) = scale_range {
        let mut changes = HashMap::new();
        changes.insert(
            uri.clone(),
            vec![TextEdit {
                range,
                new_text: precision.to_string(),
            }],
        );

        fixes.push(CodeAction {
            title: format!("Set scale to {} (match precision)", precision),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }),
            is_preferred: Some(true),
            ..Default::default()
        });
    }

    if let Some(range) = precision_range {
        let mut changes = HashMap::new();
        changes.insert(
            uri.clone(),
            vec![TextEdit {
                range,
                new_text: scale.to_string(),
            }],
        );

        fixes.push(CodeAction {
            title: format!("Set precision to {} (match scale)", scale),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }),
            is_preferred: Some(false),
            ..Default::default()
        });
    }

    fixes
}

/// Create a quick fix for missing decimal precision attribute
/// Adds a "precision" field with a reasonable default value
pub(in crate::handlers::code_actions) fn create_fix_missing_decimal_precision(
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

        if line.contains("\"logicalType\"") && line.contains("\"decimal\"") {
            let indent = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();

            let insert_position = Position {
                line: line_idx as u32,
                character: line.len() as u32,
            };

            let default_precision = 10;

            let insert_text = format!(",\n{}\"precision\": {}", indent, default_precision);
            let insert_position_range = Range {
                start: insert_position,
                end: insert_position,
            };

            return Some(
                CodeActionBuilder::new(
                    uri.clone(),
                    format!("Add precision attribute (default: {})", default_precision),
                )
                .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                .with_diagnostics(vec![diagnostic.clone()])
                .preferred()
                .add_edit(insert_position_range, insert_text)
                .build(),
            );
        }
    }

    None
}

/// Create a quick fix for invalid duration size errors
/// Duration logical type requires fixed size of exactly 12 bytes
pub(in crate::handlers::code_actions) fn create_fix_invalid_duration_size(
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

        if line.contains("\"size\"")
            && let Some(size_pos) = line.find("\"size\"")
            && let Some(colon_pos) = line[size_pos..].find(':')
        {
            let after_colon = &line[size_pos + colon_pos + 1..];
            let trimmed = after_colon.trim_start();
            let ws_offset = after_colon.len() - trimmed.len();

            let value_end = trimmed
                .find(',')
                .or_else(|| trimmed.find('}'))
                .or_else(|| trimmed.find('\n'))
                .unwrap_or(trimmed.len());
            let value = trimmed[..value_end].trim();

            let value_start_col = size_pos + colon_pos + 1 + ws_offset;
            let value_end_col = value_start_col + value.len();

            let range = Range {
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
                    "Set size to 12 (required for duration)".to_string(),
                )
                .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                .with_diagnostics(vec![diagnostic.clone()])
                .preferred()
                .add_edit(range, "12".to_string())
                .build(),
            );
        }
    }

    None
}

/// Create a quick fix for logical type errors
/// Changes the base type to match the logical type requirements
pub(in crate::handlers::code_actions) fn create_fix_logical_type(
    uri: &Url,
    _schema: &AvroSchema,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    let msg = &diagnostic.message;
    let required_type = if msg.contains("requires string") {
        "string"
    } else if msg.contains("requires int") {
        "int"
    } else if msg.contains("requires long") {
        "long"
    } else if msg.contains("requires bytes") {
        "bytes"
    } else {
        return None;
    };

    let type_range = find_primitive_type_range(text, diagnostic.range)?;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Change base type to '{}'", required_type),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .preferred()
        .add_edit(type_range, format!("\"{}\"", required_type))
        .build(),
    )
}
