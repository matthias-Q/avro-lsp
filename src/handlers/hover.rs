use async_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Url};

use crate::schema::{AvroSchema, AvroType, PrimitiveType, UnionSchema};
use crate::workspace::Workspace;

/// Check if the cursor position is inside a quoted string
fn is_inside_quoted_string(chars: &[char], pos: usize) -> bool {
    if pos >= chars.len() {
        return false;
    }

    // Count unescaped quotes before this position
    let mut quote_count = 0;
    let mut i = 0;
    while i < pos {
        if chars[i] == '"' && (i == 0 || chars[i - 1] != '\\') {
            quote_count += 1;
        }
        i += 1;
    }

    // If quote_count is odd, we're inside a quoted string
    // Also check if we're exactly on an opening or closing quote (not escaped)
    let on_quote = chars[pos] == '"' && (pos == 0 || chars[pos - 1] != '\\');
    let on_opening_quote = on_quote && quote_count % 2 == 0;
    let on_closing_quote = on_quote && quote_count % 2 == 1;

    quote_count % 2 == 1 || on_opening_quote || on_closing_quote
}

/// Extract the full content of a quoted string at the cursor position
/// Returns the string content without the surrounding quotes
fn extract_quoted_string(chars: &[char], pos: usize) -> Option<String> {
    if !is_inside_quoted_string(chars, pos) {
        return None;
    }

    // Find the opening quote (search backward)
    // If we're on a quote, check if it's opening or closing
    let mut start = pos;

    // If we're on a quote, we need to determine if it's opening or closing
    if chars[start] == '"' && (start == 0 || chars[start - 1] != '\\') {
        // Count quotes before this position
        let mut quote_count = 0;
        let mut i = 0;
        while i < start {
            if chars[i] == '"' && (i == 0 || chars[i - 1] != '\\') {
                quote_count += 1;
            }
            i += 1;
        }

        // If even number of quotes before, this is opening quote
        // If odd number of quotes before, this is closing quote - search backward
        if quote_count % 2 == 1 {
            // This is a closing quote, search backward for opening
            start -= 1;
        }
        // Otherwise we're already on the opening quote
    }

    // Search backward for opening quote if not already on it
    while start > 0 && !(chars[start] == '"' && (start == 0 || chars[start - 1] != '\\')) {
        start -= 1;
    }

    // If we didn't land on a quote, something went wrong
    if start >= chars.len() || chars[start] != '"' {
        return None;
    }

    // Find the closing quote (search forward from start+1)
    let mut end = start + 1;
    while end < chars.len() {
        if chars[end] == '"' && chars[end - 1] != '\\' {
            break;
        }
        end += 1;
    }

    // Extract content between quotes
    if end < chars.len() && chars[end] == '"' {
        Some(chars[start + 1..end].iter().collect())
    } else {
        None
    }
}

/// Extract a regular word (alphanumeric + underscore) at the cursor position
fn extract_word(chars: &[char], pos: usize) -> Option<String> {
    if pos >= chars.len() {
        return None;
    }

    let char_at_pos = chars[pos];
    if !char_at_pos.is_alphanumeric() && char_at_pos != '_' {
        return None;
    }

    // Find start of word
    let mut start = pos;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }

    // Find end of word
    let mut end = pos;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    Some(chars[start..end].iter().collect())
}

/// Get the word at a specific position in the text
pub fn get_word_at_position(text: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;

    let chars: Vec<char> = line.chars().collect();
    let pos = position.character as usize;

    if pos >= chars.len() {
        return None;
    }

    // Try to extract a quoted string first
    if let Some(quoted) = extract_quoted_string(&chars, pos) {
        return Some(quoted);
    }

    // Otherwise, try to extract a regular word
    extract_word(&chars, pos)
}

/// Generate hover information for a word in the schema
pub fn generate_hover(schema: &AvroSchema, text: &str, word: &str) -> Option<Hover> {
    generate_hover_with_workspace(schema, text, word, None, None)
}

/// Generate hover information with workspace support for cross-file type resolution
pub fn generate_hover_with_workspace(
    schema: &AvroSchema,
    text: &str,
    word: &str,
    uri: Option<&Url>,
    workspace: Option<&Workspace>,
) -> Option<Hover> {
    // Check if it's a primitive type
    if let Some(prim) = PrimitiveType::parse(word) {
        let doc = get_primitive_documentation(&prim);
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**Primitive Type**: `{:?}`\n\n{}", prim, doc),
            }),
            range: None,
        });
    }

    // Check if it's a named type in the local schema
    if let Some(named_type) = schema.named_types.get(word) {
        let type_info = format_type_info(named_type);
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: type_info,
            }),
            range: None,
        });
    }

    // If not found locally and we have workspace, try cross-file lookup
    if let Some(workspace) = workspace
        && let Some(uri) = uri
        && let Some(type_info) = workspace.resolve_type(word, uri)
    {
        // Get the actual type definition from global types
        if let Some(global_type) = workspace.get_type(&type_info.qualified_name) {
            let type_info_text = format_type_info(&global_type.type_def);
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: type_info_text,
                }),
                range: None,
            });
        }
    }

    // Check if it's a field name (search for it in the text)
    if let Some(field_info) = find_field_info(schema, word, text) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: field_info,
            }),
            range: None,
        });
    }

    None
}

/// Get documentation for primitive types
fn get_primitive_documentation(prim: &PrimitiveType) -> &'static str {
    match prim {
        PrimitiveType::Null => "No value",
        PrimitiveType::Boolean => "A binary value (true or false)",
        PrimitiveType::Int => "32-bit signed integer",
        PrimitiveType::Long => "64-bit signed integer",
        PrimitiveType::Float => "Single precision (32-bit) IEEE 754 floating-point number",
        PrimitiveType::Double => "Double precision (64-bit) IEEE 754 floating-point number",
        PrimitiveType::Bytes => "Sequence of 8-bit unsigned bytes",
        PrimitiveType::String => "Unicode character sequence",
    }
}

/// Format type information for hover display
fn format_type_info(avro_type: &AvroType) -> String {
    match avro_type {
        AvroType::Record(record) => {
            let mut info = format!("**Record**: `{}`\n\n", record.name);
            if let Some(ns) = &record.namespace {
                info.push_str(&format!("**Namespace**: `{}`\n\n", ns));
            }
            if let Some(doc) = &record.doc {
                info.push_str(&format!("{}\n\n", doc));
            }
            info.push_str("**Fields**:\n");
            for field in &record.fields {
                let type_str = format_type_name(&field.field_type);
                info.push_str(&format!("- `{}`: {}\n", field.name, type_str));
            }
            info
        }
        AvroType::Enum(enum_schema) => {
            let mut info = format!("**Enum**: `{}`\n\n", enum_schema.name);
            if let Some(ns) = &enum_schema.namespace {
                info.push_str(&format!("**Namespace**: `{}`\n\n", ns));
            }
            if let Some(doc) = &enum_schema.doc {
                info.push_str(&format!("{}\n\n", doc));
            }
            info.push_str("**Symbols**: ");
            info.push_str(&enum_schema.symbols.join(", "));
            info
        }
        AvroType::Fixed(fixed) => {
            let mut info = format!("**Fixed**: `{}`\n\n", fixed.name);
            if let Some(ns) = &fixed.namespace {
                info.push_str(&format!("**Namespace**: `{}`\n\n", ns));
            }
            info.push_str(&format!("**Size**: {} bytes", fixed.size));
            info
        }
        AvroType::Array(array) => {
            format!("**Array** of {}", format_type_name(&array.items))
        }
        AvroType::Map(map) => {
            format!(
                "**Map** with values of type {}",
                format_type_name(&map.values)
            )
        }
        AvroType::Union(UnionSchema { types, .. }) => {
            let type_names: Vec<String> = types.iter().map(format_type_name).collect();
            format!("**Union**: {}", type_names.join(" | "))
        }
        AvroType::Primitive(prim) => {
            format!("**Primitive**: `{:?}`", prim)
        }
        AvroType::PrimitiveObject(prim_obj) => {
            let mut info = format!("**Primitive**: `{:?}`\n\n", prim_obj.primitive_type);
            if let Some(logical_type) = &prim_obj.logical_type {
                info.push_str(&format!("**Logical Type**: `{}`\n\n", logical_type));
            }
            if let Some(precision) = prim_obj.precision {
                info.push_str(&format!("**Precision**: {}\n\n", precision));
            }
            if let Some(scale) = prim_obj.scale {
                info.push_str(&format!("**Scale**: {}\n\n", scale));
            }
            info
        }
        AvroType::TypeRef(type_ref) => {
            format!("**Type Reference**: `{}`", type_ref.name)
        }
        AvroType::Invalid(invalid) => {
            format!("**Invalid Type**: `{}`", invalid.type_name)
        }
    }
}

/// Format a type name for display
pub fn format_type_name(avro_type: &AvroType) -> String {
    match avro_type {
        AvroType::Primitive(prim) => format!("`{:?}`", prim).to_lowercase(),
        AvroType::PrimitiveObject(prim_obj) => {
            let base = format!("{:?}", prim_obj.primitive_type).to_lowercase();
            if let Some(logical_type) = &prim_obj.logical_type {
                format!("`{} ({})`", base, logical_type)
            } else {
                format!("`{}`", base)
            }
        }
        AvroType::Record(r) => format!("`{}`", r.name),
        AvroType::Enum(e) => format!("`{}`", e.name),
        AvroType::Fixed(f) => format!("`{}`", f.name),
        AvroType::Array(a) => format!("array<{}>", format_type_name(&a.items)),
        AvroType::Map(m) => format!("map<{}>", format_type_name(&m.values)),
        AvroType::Union(UnionSchema { types, .. }) => {
            let names: Vec<String> = types.iter().map(format_type_name).collect();
            format!("[{}]", names.join(", "))
        }
        AvroType::TypeRef(type_ref) => format!("`{}`", type_ref.name),
        AvroType::Invalid(invalid) => format!("`{} (invalid)`", invalid.type_name),
    }
}

/// Find field information in the schema
fn find_field_info(schema: &AvroSchema, field_name: &str, _text: &str) -> Option<String> {
    // Search through all records for a field with this name
    for named_type in schema.named_types.values() {
        if let AvroType::Record(record) = named_type {
            for field in &record.fields {
                if field.name == field_name {
                    let mut info = format!("**Field**: `{}`\n\n", field.name);
                    info.push_str(&format!(
                        "**Type**: {}\n\n",
                        format_type_name(&field.field_type)
                    ));
                    if let Some(doc) = &field.doc {
                        info.push_str(&format!("**Description**: {}\n\n", doc));
                    }
                    info.push_str(&format!("**In Record**: `{}`", record.name));
                    return Some(info);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_word_fqn_cursor_on_first_segment() {
        let line = r#"    "type": "com.example.common.Address""#;
        let text = line;
        // Cursor on 'c' in "com" (position 12)
        let pos = Position::new(0, 12);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("com.example.common.Address".to_string()));
    }

    #[test]
    fn test_get_word_fqn_cursor_on_dot_after_com() {
        let line = r#"    "type": "com.example.common.Address""#;
        let text = line;
        // Cursor on '.' after "com" (position 15)
        let pos = Position::new(0, 15);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("com.example.common.Address".to_string()));
    }

    #[test]
    fn test_get_word_fqn_cursor_on_example() {
        let line = r#"    "type": "com.example.common.Address""#;
        let text = line;
        // Cursor on 'x' in "example" (position 17)
        let pos = Position::new(0, 17);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("com.example.common.Address".to_string()));
    }

    #[test]
    fn test_get_word_fqn_cursor_on_common() {
        let line = r#"    "type": "com.example.common.Address""#;
        let text = line;
        // Cursor on 'o' in "common" (position 26)
        let pos = Position::new(0, 26);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("com.example.common.Address".to_string()));
    }

    #[test]
    fn test_get_word_fqn_cursor_on_address() {
        let line = r#"    "type": "com.example.common.Address""#;
        let text = line;
        // Cursor on 'A' in "Address" (position 33)
        let pos = Position::new(0, 33);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("com.example.common.Address".to_string()));
    }

    #[test]
    fn test_get_word_fqn_cursor_on_last_char() {
        let line = r#"    "type": "com.example.common.Address""#;
        let text = line;
        // Cursor on 's' in "Address" (position 39)
        let pos = Position::new(0, 39);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("com.example.common.Address".to_string()));
    }

    #[test]
    fn test_get_word_simple_type_name() {
        let line = r#"    "type": "string""#;
        let text = line;
        // Cursor on 's' in "string"
        let pos = Position::new(0, 13);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("string".to_string()));
    }

    #[test]
    fn test_get_word_primitive_type() {
        let line = r#"    "type": "int""#;
        let text = line;
        // Cursor on 'i' in "int"
        let pos = Position::new(0, 13);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("int".to_string()));
    }

    #[test]
    fn test_get_word_field_name() {
        let line = r#"      "name": "user_id""#;
        let text = line;
        // Cursor on 'u' in "user_id"
        let pos = Position::new(0, 15);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("user_id".to_string()));
    }

    #[test]
    fn test_get_word_unquoted_identifier() {
        let line = "    user_id: 123";
        let text = line;
        // Cursor on 'u' in "user_id"
        let pos = Position::new(0, 4);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("user_id".to_string()));
    }

    #[test]
    fn test_get_word_cursor_on_opening_quote() {
        let line = r#"    "type": "Address""#;
        let text = line;
        // Cursor on opening quote before "Address" (position 12)
        let pos = Position::new(0, 12);
        let word = get_word_at_position(text, pos);
        assert_eq!(word, Some("Address".to_string()));
    }

    #[test]
    fn test_is_inside_quoted_string_middle_of_string() {
        let line = r#"    "type": "com.example.Address""#;
        let chars: Vec<char> = line.chars().collect();
        // Position on 'x' in "example"
        assert!(is_inside_quoted_string(&chars, 17));
    }

    #[test]
    fn test_is_inside_quoted_string_on_opening_quote() {
        let line = r#"    "type": "Address""#;
        let chars: Vec<char> = line.chars().collect();
        // Position on opening quote
        assert!(is_inside_quoted_string(&chars, 12));
    }

    #[test]
    fn test_is_inside_quoted_string_outside() {
        let line = r#"    "type": "Address""#;
        let chars: Vec<char> = line.chars().collect();
        // Position on space before opening quote
        assert!(!is_inside_quoted_string(&chars, 11));
    }
}
