use std::collections::HashMap;

use async_lsp::lsp_types::{Range, TextEdit, Url, WorkspaceEdit};
use async_lsp::ResponseError;

use crate::schema::{AvroSchema, AvroType};

use super::node_matcher::{RenameInfo, SymbolType};
use super::validation;

#[allow(dead_code)]
pub fn rename_in_file(
    schema: &AvroSchema,
    text: &str,
    uri: &Url,
    rename_info: &RenameInfo,
    new_name: &str,
) -> Result<Option<WorkspaceEdit>, ResponseError> {
    validation::validate_avro_name(new_name)?;

    match &rename_info.symbol_type {
        SymbolType::Field { field } => {
            validation::check_field_name_conflict(schema, field, new_name)?;

            let edits = vec![TextEdit {
                range: rename_info.name_range,
                new_text: new_name.to_string(),
            }];

            let mut changes = HashMap::new();
            changes.insert(uri.clone(), edits);

            Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }))
        }
        SymbolType::RecordType | SymbolType::EnumType | SymbolType::FixedType | SymbolType::TypeReference => {
            validation::check_type_name_conflict(schema, &rename_info.old_name, new_name)?;

            let edits = collect_type_rename_edits(schema, text, &rename_info.old_name, new_name);

            if edits.is_empty() {
                return Ok(None);
            }

            let mut changes = HashMap::new();
            changes.insert(uri.clone(), edits);

            Ok(Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }))
        }
    }
}

pub fn collect_type_rename_edits(
    schema: &AvroSchema,
    _text: &str,
    old_name: &str,
    new_name: &str,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();

    let ranges = collect_type_references(schema, old_name, true);

    for range in ranges {
        edits.push(TextEdit {
            range,
            new_text: format!("\"{}\"", new_name),
        });
    }

    edits
}

pub fn collect_type_references(
    schema: &AvroSchema,
    type_name: &str,
    include_declaration: bool,
) -> Vec<Range> {
    let mut ranges = Vec::new();

    if include_declaration
        && let Some(named_type) = schema.named_types.get(type_name)
        && let Some(range) = get_type_name_range(named_type)
    {
        ranges.push(range);
    }

    collect_type_references_in_type(&schema.root, type_name, &mut ranges);

    ranges
}

fn get_type_name_range(avro_type: &AvroType) -> Option<Range> {
    match avro_type {
        AvroType::Record(record) => record.name_range,
        AvroType::Enum(enum_schema) => enum_schema.name_range,
        AvroType::Fixed(fixed) => fixed.name_range,
        _ => None,
    }
}

fn collect_type_references_in_type(avro_type: &AvroType, type_name: &str, ranges: &mut Vec<Range>) {
    match avro_type {
        AvroType::TypeRef(type_ref) if type_ref.name == type_name => {
            if let Some(range) = type_ref.range {
                ranges.push(range);
            }
        }
        AvroType::Record(record) => {
            for field in &record.fields {
                collect_type_references_in_type(&field.field_type, type_name, ranges);
            }
        }
        AvroType::Array(array) => {
            collect_type_references_in_type(&array.items, type_name, ranges);
        }
        AvroType::Map(map) => {
            collect_type_references_in_type(&map.values, type_name, ranges);
        }
        AvroType::Union(types) => {
            for t in types {
                collect_type_references_in_type(t, type_name, ranges);
            }
        }
        _ => {}
    }
}
