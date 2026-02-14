use async_lsp::ResponseError;

use crate::schema::{AvroSchema, AvroType, Field, RecordSchema};

pub fn validate_avro_name(name: &str) -> Result<(), ResponseError> {
    let name_regex = regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();
    if !name_regex.is_match(name) {
        return Err(ResponseError::new(
            async_lsp::ErrorCode::INVALID_PARAMS,
            format!(
                "Invalid name '{}'. Names must start with [A-Za-z_] and contain only [A-Za-z0-9_]",
                name
            ),
        ));
    }
    Ok(())
}

pub fn check_type_name_conflict(
    schema: &AvroSchema,
    old_name: &str,
    new_name: &str,
) -> Result<(), ResponseError> {
    if schema.named_types.contains_key(new_name) && new_name != old_name {
        return Err(ResponseError::new(
            async_lsp::ErrorCode::INVALID_PARAMS,
            format!("Type '{}' already exists", new_name),
        ));
    }
    Ok(())
}

pub fn check_field_name_conflict(
    schema: &AvroSchema,
    target_field: &Field,
    new_name: &str,
) -> Result<(), ResponseError> {
    fn find_parent_record<'a>(
        avro_type: &'a AvroType,
        target_field: &Field,
    ) -> Option<&'a RecordSchema> {
        match avro_type {
            AvroType::Record(record) => {
                for field in &record.fields {
                    if std::ptr::eq(field, target_field) {
                        return Some(record);
                    }
                }
                for field in &record.fields {
                    if let Some(parent) = find_parent_record(&field.field_type, target_field) {
                        return Some(parent);
                    }
                }
                None
            }
            AvroType::Array(array) => find_parent_record(&array.items, target_field),
            AvroType::Map(map) => find_parent_record(&map.values, target_field),
            AvroType::Union(types) => {
                for t in types {
                    if let Some(parent) = find_parent_record(t, target_field) {
                        return Some(parent);
                    }
                }
                None
            }
            _ => None,
        }
    }

    if let Some(parent_record) = find_parent_record(&schema.root, target_field) {
        for field in &parent_record.fields {
            if !std::ptr::eq(field, target_field) && field.name == new_name {
                return Err(ResponseError::new(
                    async_lsp::ErrorCode::INVALID_PARAMS,
                    format!("Field '{}' already exists in this record", new_name),
                ));
            }
        }
    }

    Ok(())
}
