use async_lsp::lsp_types::{Position, Range};

use crate::schema::{AvroSchema, AvroType, SchemaError, UnionSchema};

use super::text_search::find_nested_union_range;

/// Find the position of a validation error using AST
pub(super) fn find_error_position_in_ast(
    error: &SchemaError,
    schema: &AvroSchema,
    text: &str,
) -> Range {
    fn search_type(avro_type: &AvroType, error: &SchemaError, text: &str) -> Option<Range> {
        match error {
            SchemaError::InvalidName { name, range, .. } => {
                if range.is_some() {
                    return *range;
                }
                tracing::debug!("Searching for InvalidName: {}", name);
                match avro_type {
                    AvroType::Record(record) if record.name == *name => {
                        tracing::debug!(
                            "Found record with invalid name at {:?}",
                            record.name_range
                        );
                        record.name_range
                    }
                    AvroType::Enum(enum_schema) if enum_schema.name == *name => {
                        tracing::debug!(
                            "Found enum with invalid name at {:?}",
                            enum_schema.name_range
                        );
                        enum_schema.name_range
                    }
                    AvroType::Fixed(fixed) if fixed.name == *name => {
                        tracing::debug!("Found fixed with invalid name at {:?}", fixed.name_range);
                        fixed.name_range
                    }
                    AvroType::Record(record) => {
                        tracing::debug!("Searching in record: {}", record.name);
                        for field in &record.fields {
                            if field.name == *name {
                                tracing::debug!(
                                    "Found field with invalid name '{}' at {:?}",
                                    field.name,
                                    field.name_range
                                );
                                return field.name_range;
                            }
                            if let Some(range) = search_type(&field.field_type, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Array(array) => search_type(&array.items, error, text),
                    AvroType::Map(map) => search_type(&map.values, error, text),
                    AvroType::Union(UnionSchema { types, .. }) => {
                        for t in types {
                            if let Some(range) = search_type(t, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
            SchemaError::InvalidNamespace {
                namespace, range, ..
            } => {
                if range.is_some() {
                    return *range;
                }
                tracing::debug!("Searching for InvalidNamespace: {}", namespace);
                match avro_type {
                    AvroType::Record(record) => {
                        if record.namespace.as_ref() == Some(namespace) {
                            tracing::debug!(
                                "Found record with invalid namespace, namespace_range: {:?}",
                                record.namespace_range
                            );
                            return record
                                .namespace_range
                                .or(record.name_range)
                                .or(record.range);
                        }
                        for field in &record.fields {
                            if let Some(range) = search_type(&field.field_type, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Enum(enum_schema) => {
                        if enum_schema.namespace.as_ref() == Some(namespace) {
                            tracing::debug!(
                                "Found enum with invalid namespace, namespace_range: {:?}",
                                enum_schema.namespace_range
                            );
                            return enum_schema
                                .namespace_range
                                .or(enum_schema.name_range)
                                .or(enum_schema.range);
                        }
                        None
                    }
                    AvroType::Fixed(fixed) => {
                        if fixed.namespace.as_ref() == Some(namespace) {
                            tracing::debug!(
                                "Found fixed with invalid namespace, namespace_range: {:?}",
                                fixed.namespace_range
                            );
                            return fixed.namespace_range.or(fixed.name_range).or(fixed.range);
                        }
                        None
                    }
                    AvroType::Array(array) => search_type(&array.items, error, text),
                    AvroType::Map(map) => search_type(&map.values, error, text),
                    AvroType::Union(UnionSchema { types, .. }) => {
                        for t in types {
                            if let Some(range) = search_type(t, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
            SchemaError::DuplicateSymbol {
                symbol,
                duplicate_occurrence,
                ..
            } => {
                if duplicate_occurrence.is_some() {
                    return *duplicate_occurrence;
                }
                tracing::debug!("Searching for DuplicateSymbol: {}", symbol);
                match avro_type {
                    AvroType::Enum(enum_schema) => {
                        if enum_schema.symbols.contains(symbol) {
                            tracing::debug!(
                                "Found enum with duplicate symbol at {:?}",
                                enum_schema.range
                            );
                            return enum_schema.range;
                        }
                        None
                    }
                    AvroType::Record(record) => {
                        for field in &record.fields {
                            if let Some(range) = search_type(&field.field_type, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Array(array) => search_type(&array.items, error, text),
                    AvroType::Map(map) => search_type(&map.values, error, text),
                    AvroType::Union(UnionSchema { types, .. }) => {
                        for t in types {
                            if let Some(range) = search_type(t, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
            SchemaError::DuplicateFieldName {
                field,
                duplicate_occurrence,
                ..
            } => {
                if duplicate_occurrence.is_some() {
                    return *duplicate_occurrence;
                }
                tracing::debug!("Searching for DuplicateFieldName: {}", field);
                match avro_type {
                    AvroType::Record(record) => {
                        // Find the duplicate field in the record
                        for rec_field in &record.fields {
                            if rec_field.name == *field {
                                tracing::debug!(
                                    "Found field '{}' with duplicate name at {:?}",
                                    field,
                                    rec_field.name_range
                                );
                                return rec_field.name_range;
                            }
                        }
                        // Fall back to record range if field not found
                        record.range
                    }
                    _ => None,
                }
            }
            SchemaError::Custom {
                message: msg,
                range,
            } => {
                if range.is_some() {
                    return *range;
                }
                tracing::debug!("Searching for Custom error: {}", msg);

                if msg.contains("Record must have at least one field")
                    && let AvroType::Record(record) = avro_type
                {
                    return record.range;
                }

                if msg.contains("Enum must have at least one symbol")
                    && let AvroType::Enum(enum_schema) = avro_type
                {
                    return enum_schema.range;
                }

                if msg.contains("Fixed size must be greater than 0")
                    && let AvroType::Fixed(fixed) = avro_type
                {
                    return fixed.range;
                }

                if msg.contains("Decimal") || msg.contains("precision") || msg.contains("scale") {
                    match avro_type {
                        AvroType::Fixed(fixed) if fixed.logical_type.is_some() => {
                            return fixed.range;
                        }
                        AvroType::PrimitiveObject(prim) if prim.logical_type.is_some() => {
                            return prim.range;
                        }
                        _ => {}
                    }
                }

                if msg.contains("Duration")
                    && let AvroType::Fixed(fixed) = avro_type
                    && fixed.logical_type == Some("duration".to_string())
                {
                    return fixed.range;
                }

                if msg.contains("Invalid logical type")
                    && let AvroType::PrimitiveObject(prim) = avro_type
                {
                    return prim.range;
                }

                match avro_type {
                    AvroType::Record(record) => {
                        if let Some(range) = record.range
                            && msg.contains("field")
                            && record.fields.is_empty()
                        {
                            return Some(range);
                        }
                        for field in &record.fields {
                            if let Some(range) = search_type(&field.field_type, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Array(array) => search_type(&array.items, error, text),
                    AvroType::Map(map) => search_type(&map.values, error, text),
                    AvroType::Union(UnionSchema { types, .. }) => {
                        for t in types {
                            if let Some(range) = search_type(t, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
            SchemaError::UnknownTypeReference { type_name, range } => {
                if range.is_some() {
                    return *range;
                }
                tracing::debug!("Searching for UnknownTypeReference: {}", type_name);
                match avro_type {
                    AvroType::TypeRef(type_ref) if type_ref.name == *type_name => {
                        tracing::debug!("Found TypeRef with unknown type at {:?}", type_ref.range);
                        type_ref.range
                    }
                    AvroType::Record(record) => {
                        for field in &record.fields {
                            if let Some(range) = search_type(&field.field_type, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Array(array) => search_type(&array.items, error, text),
                    AvroType::Map(map) => search_type(&map.values, error, text),
                    AvroType::Union(UnionSchema { types, .. }) => {
                        for t in types {
                            if let Some(range) = search_type(t, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
            SchemaError::NestedUnion { range } => {
                if range.is_some() {
                    return *range;
                }
                tracing::debug!("Searching for NestedUnion");
                match avro_type {
                    AvroType::Union(UnionSchema { types, .. }) => {
                        for t in types {
                            if matches!(t, AvroType::Union(_)) {
                                return None;
                            }
                        }
                        for t in types {
                            if let Some(range) = search_type(t, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Record(record) => {
                        for field in &record.fields {
                            if let AvroType::Union(UnionSchema { types, .. }) = &*field.field_type
                                && types.iter().any(|t| matches!(t, AvroType::Union(_)))
                            {
                                if let Some(field_range) = field.range
                                    && let Some(nested_range) =
                                        find_nested_union_range(text, field_range)
                                {
                                    tracing::debug!(
                                        "Found nested union at precise range: {:?}",
                                        nested_range
                                    );
                                    return Some(nested_range);
                                }
                                tracing::debug!(
                                    "Found nested union in field, returning field range: {:?}",
                                    field.range
                                );
                                return field.range;
                            }
                            if let Some(range) = search_type(&field.field_type, error, text) {
                                return field.range.or(Some(range));
                            }
                        }
                        None
                    }
                    AvroType::Array(array) => {
                        if let AvroType::Union(UnionSchema { types, .. }) = &*array.items
                            && types.iter().any(|t| matches!(t, AvroType::Union(_)))
                        {
                            return None;
                        }
                        search_type(&array.items, error, text)
                    }
                    AvroType::Map(map) => {
                        if let AvroType::Union(UnionSchema { types, .. }) = &*map.values
                            && types.iter().any(|t| matches!(t, AvroType::Union(_)))
                        {
                            return None;
                        }
                        search_type(&map.values, error, text)
                    }
                    _ => None,
                }
            }
            SchemaError::DuplicateUnionType {
                range,
                type_signature,
            } => {
                if range.is_some() {
                    return *range;
                }
                tracing::debug!("Searching for DuplicateUnionType: {}", type_signature);

                let find_duplicate_in_union = |field_range: Range| -> Option<Range> {
                    let start_line = field_range.start.line as usize;
                    let lines: Vec<&str> = text.lines().collect();

                    if start_line >= lines.len() {
                        return None;
                    }

                    let line = lines[start_line];

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

                        if end_pos > 0 {
                            let array_str = &from_bracket[..end_pos];
                            let search_str = format!("\"{}\"", type_signature.to_lowercase());
                            let mut first_pos = None;
                            let mut second_pos = None;
                            let mut search_offset = 0;

                            while let Some(pos) = array_str[search_offset..].find(&search_str) {
                                let abs_pos = search_offset + pos;
                                if first_pos.is_none() {
                                    first_pos = Some(abs_pos);
                                    search_offset = abs_pos + search_str.len();
                                } else {
                                    second_pos = Some(abs_pos);
                                    break;
                                }
                            }

                            if second_pos.is_some() {
                                let char_start = array_start as u32;
                                let char_end = (array_start + end_pos) as u32;

                                return Some(Range {
                                    start: Position {
                                        line: start_line as u32,
                                        character: char_start,
                                    },
                                    end: Position {
                                        line: start_line as u32,
                                        character: char_end,
                                    },
                                });
                            }
                        }
                    }

                    Some(field_range)
                };

                let has_duplicates = |types: &[AvroType]| -> bool {
                    use std::collections::HashSet;
                    let mut signatures = HashSet::new();
                    for t in types {
                        let sig = match t {
                            AvroType::Primitive(p) => format!("{:?}", p),
                            AvroType::PrimitiveObject(p) => format!("{:?}", p.primitive_type),
                            AvroType::Record(r) => format!("record:{}", r.name),
                            AvroType::Enum(e) => format!("enum:{}", e.name),
                            AvroType::Fixed(f) => format!("fixed:{}", f.name),
                            AvroType::Array(_) => "array".to_string(),
                            AvroType::Map(_) => "map".to_string(),
                            AvroType::Union(_) => "union".to_string(),
                            AvroType::TypeRef(type_ref) => format!("ref:{}", type_ref.name),
                            AvroType::Invalid(inv) => format!("invalid:{}", inv.type_name),
                        };
                        if !signatures.insert(sig) {
                            return true;
                        }
                    }
                    false
                };

                match avro_type {
                    AvroType::Union(UnionSchema { types, .. }) => {
                        if has_duplicates(types) {
                            return None;
                        }
                        for t in types {
                            if let Some(range) = search_type(t, error, text) {
                                return Some(range);
                            }
                        }
                        None
                    }
                    AvroType::Record(record) => {
                        for field in &record.fields {
                            if let AvroType::Union(UnionSchema { types, .. }) = &*field.field_type
                                && has_duplicates(types)
                            {
                                if let Some(field_range) = field.range
                                    && let Some(precise_range) =
                                        find_duplicate_in_union(field_range)
                                {
                                    tracing::debug!(
                                        "Found precise duplicate position: {:?}",
                                        precise_range
                                    );
                                    return Some(precise_range);
                                }
                                tracing::debug!(
                                    "Found duplicate union type in field, returning field range: {:?}",
                                    field.range
                                );
                                return field.range;
                            }
                            if let Some(range) = search_type(&field.field_type, error, text) {
                                return field.range.or(Some(range));
                            }
                        }
                        None
                    }
                    AvroType::Array(array) => {
                        if let AvroType::Union(UnionSchema { types, .. }) = &*array.items
                            && has_duplicates(types)
                        {
                            return None;
                        }
                        search_type(&array.items, error, text)
                    }
                    AvroType::Map(map) => {
                        if let AvroType::Union(UnionSchema { types, .. }) = &*map.values
                            && has_duplicates(types)
                        {
                            return None;
                        }
                        search_type(&map.values, error, text)
                    }
                    _ => None,
                }
            }
            SchemaError::MissingField { field } => {
                tracing::debug!("Searching for MissingField: {}", field);
                if field == "fields" {
                    match avro_type {
                        AvroType::Record(record) => {
                            tracing::debug!("Found record missing fields at {:?}", record.range);
                            record.range
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => {
                tracing::debug!("Unsupported error type for position finding: {:?}", error);
                None
            }
        }
    }

    if let Some(range) = search_type(&schema.root, error, text) {
        tracing::debug!("Found error position: {:?}", range);
        return range;
    }

    tracing::warn!("Could not find error position in AST, defaulting to (0,0)");
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 1,
        },
    }
}
