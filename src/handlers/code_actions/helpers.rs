use async_lsp::lsp_types::{Position, Range};

use super::builder::AVRO_NAME_REGEX;
use crate::schema::{AvroSchema, AvroType, UnionSchema};

/// Check if an AvroType is a Union
pub(super) fn is_union(avro_type: &AvroType) -> bool {
    matches!(avro_type, AvroType::Union(_))
}

/// Format an AvroType as a JSON string for code actions
pub(super) fn format_avro_type_as_json(avro_type: &AvroType) -> String {
    match avro_type {
        AvroType::Primitive(prim) => {
            format!("\"{}\"", format!("{:?}", prim).to_lowercase())
        }
        AvroType::TypeRef(type_ref) => format!("\"{}\"", type_ref.name),
        _ => serde_json::to_string(avro_type).unwrap_or_else(|_| "\"string\"".to_string()),
    }
}

/// Get a sensible default value for an Avro type
pub(super) fn get_default_for_type(avro_type: &AvroType) -> Option<String> {
    match avro_type {
        AvroType::Primitive(prim) => match prim {
            crate::schema::PrimitiveType::Null => Some("null".to_string()),
            crate::schema::PrimitiveType::Boolean => Some("false".to_string()),
            crate::schema::PrimitiveType::Int => Some("0".to_string()),
            crate::schema::PrimitiveType::Long => Some("0".to_string()),
            crate::schema::PrimitiveType::Float => Some("0.0".to_string()),
            crate::schema::PrimitiveType::Double => Some("0.0".to_string()),
            crate::schema::PrimitiveType::Bytes => Some("\"\"".to_string()),
            crate::schema::PrimitiveType::String => Some("\"\"".to_string()),
        },
        AvroType::Array(_) => Some("[]".to_string()),
        AvroType::Map(_) => Some("{}".to_string()),
        AvroType::Union(UnionSchema { types, .. }) => types.first().and_then(get_default_for_type),
        _ => None,
    }
}

/// Fix an invalid name according to Avro rules
pub(super) fn fix_invalid_name(name: &str) -> String {
    if AVRO_NAME_REGEX.is_match(name) {
        return name.to_string();
    }

    let mut fixed = String::new();
    let mut has_valid_char = false;

    for (i, ch) in name.chars().enumerate() {
        if i == 0 {
            if ch.is_ascii_alphabetic() || ch == '_' {
                fixed.push(ch);
                has_valid_char = true;
            } else if ch.is_ascii_digit() {
                fixed.push('_');
                fixed.push(ch);
                has_valid_char = true;
            }
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            fixed.push(ch);
            has_valid_char = true;
        } else if !fixed.ends_with('_') && has_valid_char {
            fixed.push('_');
        }
    }

    if fixed.is_empty() || !AVRO_NAME_REGEX.is_match(&fixed) {
        if fixed.is_empty() {
            fixed = "_".to_string();
        } else if let Some(first_char) = fixed.chars().next()
            && !first_char.is_ascii_alphabetic()
            && !fixed.starts_with('_')
        {
            fixed = format!("_{}", fixed);
        }
    }

    fixed
}

/// Fix an invalid namespace by removing or fixing invalid segments
pub(super) fn fix_invalid_namespace(namespace: &str) -> String {
    let segments: Vec<&str> = namespace.split('.').collect();

    let valid_segments: Vec<String> = segments
        .iter()
        .filter_map(|seg| {
            if AVRO_NAME_REGEX.is_match(seg) {
                // Already valid
                Some(seg.to_string())
            } else {
                // Check if segment has any valid characters
                let has_letter = seg.chars().any(|c| c.is_ascii_alphabetic());
                if has_letter {
                    // Try to fix it if it has letters
                    let fixed = fix_invalid_name(seg);
                    if AVRO_NAME_REGEX.is_match(&fixed) {
                        Some(fixed)
                    } else {
                        None
                    }
                } else {
                    // Skip segments with no letters (pure numbers, symbols)
                    None
                }
            }
        })
        .collect();

    valid_segments.join(".")
}

/// Find the range of a name value in the schema
pub(super) fn find_name_range_in_schema(
    _schema: &AvroSchema,
    _name: &str,
    diagnostic_range: Range,
) -> Option<Range> {
    // Use diagnostic range as a hint - the name should be near there
    // For simplicity, we'll use the diagnostic range itself
    // A more sophisticated implementation would parse the JSON to find exact positions
    Some(diagnostic_range)
}

/// Find the range of a namespace value in the schema
pub(super) fn find_namespace_range_in_schema(
    _schema: &AvroSchema,
    diagnostic_range: Range,
) -> Option<Range> {
    // Use diagnostic range directly
    Some(diagnostic_range)
}

/// Find the range of the "type" value in a primitive object
pub(super) fn find_primitive_type_range(text: &str, diagnostic_range: Range) -> Option<Range> {
    // Search within the diagnostic range area for "type": "..."
    let lines: Vec<&str> = text.lines().collect();

    let start_line = diagnostic_range.start.line as usize;
    let end_line = (diagnostic_range.end.line as usize).min(lines.len());

    for line_num in start_line..=end_line {
        if line_num >= lines.len() {
            break;
        }

        let line = lines[line_num];

        // Look for "type": "int" or similar
        if let Some(type_pos) = line.find("\"type\"") {
            // Find the value after the colon
            if let Some(colon_pos) = line[type_pos..].find(':') {
                let after_colon = &line[type_pos + colon_pos + 1..];
                if let Some(quote_start) = after_colon.find('"')
                    && let Some(quote_end) = after_colon[quote_start + 1..].find('"')
                {
                    let value_start = type_pos + colon_pos + 1 + quote_start;
                    let value_end = value_start + quote_end + 2; // Include both quotes

                    return Some(Range {
                        start: Position {
                            line: line_num as u32,
                            character: value_start as u32,
                        },
                        end: Position {
                            line: line_num as u32,
                            character: value_end as u32,
                        },
                    });
                }
            }
        }
    }

    None
}

/// Find positions of duplicate symbols in the symbols array
pub(super) fn find_duplicate_symbol_positions(text: &str, symbol: &str) -> Option<(Range, Range)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut positions = Vec::new();

    let search_pattern = format!("\"{}\"", symbol);

    for (line_num, line) in lines.iter().enumerate() {
        let mut search_start = 0;
        while let Some(pos) = line[search_start..].find(&search_pattern) {
            let absolute_pos = search_start + pos;
            let mut start_pos = Position {
                line: line_num as u32,
                character: absolute_pos as u32,
            };
            let mut end_pos = Position {
                line: line_num as u32,
                character: (absolute_pos + search_pattern.len()) as u32,
            };

            // Check if we need to include the comma
            if let Some(comma_pos) = line[absolute_pos + search_pattern.len()..].find(',') {
                // Include comma and any trailing spaces
                end_pos.character += comma_pos as u32 + 1;

                // Skip trailing spaces
                let after_comma = &line[(absolute_pos + search_pattern.len() + comma_pos + 1)..];
                let spaces = after_comma
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .count();
                end_pos.character += spaces as u32;
            } else {
                // Check for preceding comma
                let before_match = &line[..absolute_pos];
                if let Some(comma_pos) = before_match.rfind(',') {
                    // Include preceding comma
                    start_pos = Position {
                        line: line_num as u32,
                        character: comma_pos as u32,
                    };
                }
            }

            positions.push(Range {
                start: start_pos,
                end: end_pos,
            });

            search_start = absolute_pos + search_pattern.len();
        }
    }

    // Return first and second occurrence (to remove the second one)
    if positions.len() >= 2 {
        Some((positions[0], positions[1]))
    } else {
        None
    }
}
