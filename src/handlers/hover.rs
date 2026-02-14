use async_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::schema::{AvroSchema, AvroType, PrimitiveType};

/// Get the word at a specific position in the text
pub fn get_word_at_position(text: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;

    let chars: Vec<char> = line.chars().collect();
    let pos = position.character as usize;

    if pos >= chars.len() {
        return None;
    }

    // Check if we're on a quote or alphanumeric character
    let char_at_pos = chars[pos];
    if !char_at_pos.is_alphanumeric() && char_at_pos != '_' && char_at_pos != '"' {
        return None;
    }

    // Find the start of the word (or quoted string)
    let mut start = pos;
    let in_quotes = char_at_pos == '"' || (pos > 0 && chars[pos - 1] == '"');

    if in_quotes {
        // Find the opening quote
        while start > 0 && chars[start] != '"' {
            start -= 1;
        }
        // Find the closing quote
        let mut end = start + 1;
        while end < chars.len() && chars[end] != '"' {
            end += 1;
        }
        if end < chars.len() {
            return Some(chars[start + 1..end].iter().collect());
        }
    } else {
        // Regular word
        while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
            start -= 1;
        }
        let mut end = pos;
        while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
            end += 1;
        }
        return Some(chars[start..end].iter().collect());
    }

    None
}

/// Generate hover information for a word in the schema
pub fn generate_hover(schema: &AvroSchema, text: &str, word: &str) -> Option<Hover> {
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

    // Check if it's a named type in the schema
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
        AvroType::Union(types) => {
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
        AvroType::Union(types) => {
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
