use std::collections::HashSet;

use async_lsp::lsp_types::{CodeAction, CodeActionKind, Diagnostic, Position, Range, Url};

use crate::handlers::code_actions::builder::CodeActionBuilder;
use crate::handlers::code_actions::helpers::find_duplicate_symbol_positions;

/// Create a quick fix for invalid primitive type errors (typos)
pub(in crate::handlers::code_actions) fn create_fix_invalid_primitive_type(
    uri: &Url,
    diagnostic: &Diagnostic,
    invalid_type: &str,
    suggested_type: Option<&str>,
) -> Option<CodeAction> {
    let fixed_type = suggested_type?;
    let type_range = diagnostic.range;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Fix typo: '{}' → '{}'", invalid_type, fixed_type),
        )
        .with_kind(CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .add_edit(type_range, format!("\"{}\"", fixed_type))
        .build(),
    )
}

/// Create a quick fix for nested union errors
/// Flattens nested union like [["null", "string"]] to ["null", "string"]
pub(in crate::handlers::code_actions) fn create_fix_nested_union(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    tracing::info!(
        "create_fix_nested_union called for diagnostic at range {:?}",
        diagnostic.range
    );

    let lines: Vec<&str> = text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(outer_start) = line.find("[[") {
            tracing::info!("Found [[ at line {}, col {}", line_idx, outer_start);
            let from_bracket = &line[outer_start..];
            let mut bracket_count = 0;
            let mut end_pos = 0;

            for (idx, ch) in from_bracket.char_indices() {
                if ch == '[' {
                    bracket_count += 1;
                } else if ch == ']' {
                    bracket_count -= 1;
                    if bracket_count == 0 {
                        end_pos = idx + 1;
                        break;
                    }
                }
            }

            if end_pos == 0 {
                tracing::info!("Could not find matching ]]");
                continue;
            }

            let json_str = &from_bracket[..end_pos];
            tracing::info!("Extracted JSON: {}", json_str);

            if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
                tracing::info!("Parsed JSON successfully");
                if let Some(outer_arr) = value.as_array()
                    && outer_arr.len() == 1
                    && let Some(inner_arr) = outer_arr[0].as_array()
                {
                    tracing::info!("Found nested union pattern!");
                    let flattened = serde_json::to_string(inner_arr).ok()?;

                    let col_start = outer_start as u32;
                    let col_end = (outer_start + end_pos) as u32;

                    let replace_range = Range {
                        start: Position {
                            line: line_idx as u32,
                            character: col_start,
                        },
                        end: Position {
                            line: line_idx as u32,
                            character: col_end,
                        },
                    };

                    tracing::info!("Creating action with range {:?}", replace_range);

                    return Some(
                        CodeActionBuilder::new(uri.clone(), "Flatten nested union".to_string())
                            .with_kind(CodeActionKind::QUICKFIX)
                            .with_diagnostics(vec![diagnostic.clone()])
                            .preferred()
                            .add_edit(replace_range, flattened)
                            .build(),
                    );
                } else {
                    tracing::info!(
                        "Not a nested union pattern: outer_arr len = {:?}, has inner array = {:?}",
                        value.as_array().map(|a| a.len()),
                        value
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(|v| v.as_array())
                            .is_some()
                    );
                }
            } else {
                tracing::info!("Failed to parse JSON");
            }
        }
    }

    tracing::info!("No nested union fix created, returning None");
    None
}

/// Create a quick fix for duplicate union type errors
/// Removes duplicate types from union like ["null", "string", "null"] → ["null", "string"]
pub(in crate::handlers::code_actions) fn create_fix_duplicate_union_type(
    uri: &Url,
    text: &str,
    _diagnostic: &Diagnostic,
    duplicate_type: &str,
) -> Option<CodeAction> {
    let lines: Vec<&str> = text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(array_start) = line.find('[') {
            let from_bracket = &line[array_start..];
            let mut bracket_count = 0;
            let mut end_pos = 0;

            for (idx, ch) in from_bracket.char_indices() {
                if ch == '[' {
                    bracket_count += 1;
                } else if ch == ']' {
                    bracket_count -= 1;
                    if bracket_count == 0 {
                        end_pos = idx + 1;
                        break;
                    }
                }
            }

            if end_pos == 0 {
                continue;
            }

            let json_str = &from_bracket[..end_pos];

            if let Ok(serde_json::Value::Array(arr)) =
                serde_json::from_str::<serde_json::Value>(json_str)
            {
                let type_strings: Vec<String> = arr
                    .iter()
                    .filter_map(|v| match v {
                        serde_json::Value::String(s) => Some(s.clone()),
                        serde_json::Value::Object(obj) if obj.contains_key("type") => {
                            serde_json::to_string(v).ok()
                        }
                        _ => None,
                    })
                    .collect();

                let duplicate_lower = duplicate_type.to_lowercase();
                let count = type_strings
                    .iter()
                    .filter(|t| t.to_lowercase() == duplicate_lower)
                    .count();

                if count > 1 {
                    let mut seen = HashSet::new();
                    let deduplicated: Vec<&serde_json::Value> = arr
                        .iter()
                        .filter(|v| {
                            let type_str = match v {
                                serde_json::Value::String(s) => s.clone(),
                                _ => serde_json::to_string(v).unwrap_or_default(),
                            };
                            seen.insert(type_str)
                        })
                        .collect();

                    let dedup_json: Vec<serde_json::Value> =
                        deduplicated.iter().map(|v| (*v).clone()).collect();
                    let fixed = serde_json::to_string(&dedup_json).ok()?;

                    let col_start = array_start as u32;
                    let col_end = (array_start + end_pos) as u32;

                    let replace_range = Range {
                        start: Position {
                            line: line_idx as u32,
                            character: col_start,
                        },
                        end: Position {
                            line: line_idx as u32,
                            character: col_end,
                        },
                    };

                    return Some(
                        CodeActionBuilder::new(
                            uri.clone(),
                            format!("Remove duplicate '{}' from union", duplicate_type),
                        )
                        .with_kind(CodeActionKind::QUICKFIX)
                        .preferred()
                        .add_edit(replace_range, fixed)
                        .build(),
                    );
                }
            }
        }
    }

    None
}

/// Create a quick fix for missing fields array in record
/// Adds an empty "fields": [] to the record definition
pub(in crate::handlers::code_actions) fn create_fix_missing_fields(
    uri: &Url,
    text: &str,
    _diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    let lines: Vec<&str> = text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        if line.contains("\"type\"") && line.contains("\"record\"") {
            for (search_idx, search_line) in lines.iter().enumerate().skip(line_idx) {
                if let Some(brace_pos) = search_line.rfind('}') {
                    let before_brace = &search_line[..brace_pos].trim();

                    let indent = search_line
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>();
                    let new_text = if before_brace.is_empty() {
                        format!("{}  \"fields\": []\n", indent)
                    } else {
                        ",\n{}  \"fields\": []".to_string()
                    };

                    let col = brace_pos as u32;
                    let replace_range = Range {
                        start: Position {
                            line: search_idx as u32,
                            character: col,
                        },
                        end: Position {
                            line: search_idx as u32,
                            character: col,
                        },
                    };

                    return Some(
                        CodeActionBuilder::new(uri.clone(), "Add empty fields array".to_string())
                            .with_kind(CodeActionKind::QUICKFIX)
                            .preferred()
                            .add_edit(replace_range, new_text)
                            .build(),
                    );
                }
            }
        }
    }

    None
}

/// Create a quick fix for duplicate symbols
pub(in crate::handlers::code_actions) fn create_fix_duplicate_symbol(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
    duplicate_symbol: &str,
) -> Option<CodeAction> {
    let (_first_pos, second_pos) = find_duplicate_symbol_positions(text, duplicate_symbol)?;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Remove duplicate symbol '{}'", duplicate_symbol),
        )
        .with_kind(CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .preferred()
        .add_edit(second_pos, String::new())
        .build(),
    )
}

/// Create a quick fix for unknown field name errors (typos like "logicalType2" → "logicalType")
pub(in crate::handlers::code_actions) fn create_fix_unknown_field(
    uri: &Url,
    diagnostic: &Diagnostic,
    invalid_field: &str,
    suggested_field: Option<&str>,
) -> Option<CodeAction> {
    let fixed_field = suggested_field?;
    let field_range = diagnostic.range;

    // The diagnostic range is for the field name content (without quotes),
    // so we replace with just the corrected field name (no quotes)
    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Fix typo: '{}' → '{}'", invalid_field, fixed_field),
        )
        .with_kind(CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .preferred()
        .add_edit(field_range, fixed_field.to_string())
        .build(),
    )
}

/// Create a quick fix for invalid logical type value warnings (typos like "unite_uuid" → "uuid")
pub(in crate::handlers::code_actions) fn create_fix_invalid_logical_type_value(
    uri: &Url,
    diagnostic: &Diagnostic,
    invalid_value: &str,
    suggested_value: Option<&str>,
) -> Option<CodeAction> {
    let fixed_value = suggested_value?;
    let value_range = diagnostic.range;

    // The diagnostic range is for the logical type value content (without quotes),
    // so we replace with just the corrected value (no quotes)
    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Fix logical type: '{}' → '{}'", invalid_value, fixed_value),
        )
        .with_kind(CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .preferred()
        .add_edit(value_range, fixed_value.to_string())
        .build(),
    )
}
