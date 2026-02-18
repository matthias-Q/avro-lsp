use std::collections::HashMap;

use async_lsp::lsp_types::Range;
use regex::Regex;
use serde_json::Value;

use super::super::error::{Result, SchemaError};
use super::super::types::{AvroType, PrimitiveType, UnionSchema};

pub fn validate_default_value(
    name_regex: &Regex,
    default: &Value,
    field_type: &AvroType,
    named_types: &HashMap<String, AvroType>,
    field_range: Option<Range>,
) -> Result<()> {
    match field_type {
        AvroType::Primitive(prim) => {
            validate_primitive_default(name_regex, default, *prim, field_range)
        }
        AvroType::PrimitiveObject(prim_obj) => {
            validate_primitive_default(name_regex, default, prim_obj.primitive_type, field_range)
        }
        AvroType::Record(_) => validate_record_default(name_regex, default, field_range),
        AvroType::Enum(enum_schema) => {
            validate_enum_default(name_regex, default, &enum_schema.symbols, field_range)
        }
        AvroType::Array(_) => validate_array_default(name_regex, default, field_range),
        AvroType::Map(_) => validate_map_default(name_regex, default, field_range),
        AvroType::Fixed(_) => validate_fixed_default(name_regex, default, field_range),
        AvroType::Union(UnionSchema { types, .. }) => {
            validate_union_default(name_regex, default, types, named_types, field_range)
        }
        AvroType::TypeRef(type_ref) => validate_typeref_default(
            name_regex,
            default,
            &type_ref.name,
            named_types,
            field_range,
        ),
        AvroType::Invalid(_) => Ok(()),
    }
}

fn validate_primitive_default(
    _name_regex: &Regex,
    default: &Value,
    prim: PrimitiveType,
    field_range: Option<Range>,
) -> Result<()> {
    match prim {
        PrimitiveType::Null => {
            if !default.is_null() {
                return Err(SchemaError::Custom {
                    message: "Default value for null type must be null".to_string(),
                    range: field_range,
                });
            }
        }
        PrimitiveType::Boolean => {
            if !default.is_boolean() {
                return Err(SchemaError::Custom {
                    message: "Default value for boolean type must be true or false".to_string(),
                    range: field_range,
                });
            }
        }
        PrimitiveType::Int | PrimitiveType::Long => {
            if !default.is_number() {
                return Err(SchemaError::Custom {
                    message: format!("Default value for {:?} type must be a number", prim),
                    range: field_range,
                });
            }
        }
        PrimitiveType::Float | PrimitiveType::Double => {
            if !default.is_number() {
                return Err(SchemaError::Custom {
                    message: format!("Default value for {:?} type must be a number", prim),
                    range: field_range,
                });
            }
        }
        PrimitiveType::String => {
            if !default.is_string() {
                return Err(SchemaError::Custom {
                    message: "Default value for string type must be a string".to_string(),
                    range: field_range,
                });
            }
        }
        PrimitiveType::Bytes => {
            if !default.is_string() {
                return Err(SchemaError::Custom {
                    message: "Default value for bytes type must be a string".to_string(),
                    range: field_range,
                });
            }
        }
    }
    Ok(())
}

fn validate_record_default(
    _name_regex: &Regex,
    default: &Value,
    field_range: Option<Range>,
) -> Result<()> {
    if !default.is_object() {
        return Err(SchemaError::Custom {
            message: "Default value for record type must be an object".to_string(),
            range: field_range,
        });
    }
    Ok(())
}

fn validate_enum_default(
    _name_regex: &Regex,
    default: &Value,
    symbols: &[String],
    field_range: Option<Range>,
) -> Result<()> {
    if let Value::String(s) = default {
        if !symbols.contains(s) {
            return Err(SchemaError::Custom {
                message: format!("Default value '{}' is not a valid enum symbol", s),
                range: field_range,
            });
        }
    } else {
        return Err(SchemaError::Custom {
            message: "Default value for enum type must be a string".to_string(),
            range: field_range,
        });
    }
    Ok(())
}

fn validate_array_default(
    _name_regex: &Regex,
    default: &Value,
    field_range: Option<Range>,
) -> Result<()> {
    if !default.is_array() {
        return Err(SchemaError::Custom {
            message: "Default value for array type must be an array".to_string(),
            range: field_range,
        });
    }
    Ok(())
}

fn validate_map_default(
    _name_regex: &Regex,
    default: &Value,
    field_range: Option<Range>,
) -> Result<()> {
    if !default.is_object() {
        return Err(SchemaError::Custom {
            message: "Default value for map type must be an object".to_string(),
            range: field_range,
        });
    }
    Ok(())
}

fn validate_fixed_default(
    _name_regex: &Regex,
    default: &Value,
    field_range: Option<Range>,
) -> Result<()> {
    if !default.is_string() {
        return Err(SchemaError::Custom {
            message: "Default value for fixed type must be a string".to_string(),
            range: field_range,
        });
    }
    Ok(())
}

fn validate_union_default(
    name_regex: &Regex,
    default: &Value,
    types: &[AvroType],
    named_types: &HashMap<String, AvroType>,
    field_range: Option<Range>,
) -> Result<()> {
    if let Some(first_type) = types.first() {
        validate_default_value(name_regex, default, first_type, named_types, field_range)?;
    } else {
        return Err(SchemaError::Custom {
            message: "Union must have at least one type".to_string(),
            range: field_range,
        });
    }
    Ok(())
}

fn validate_typeref_default(
    name_regex: &Regex,
    default: &Value,
    type_name: &str,
    named_types: &HashMap<String, AvroType>,
    field_range: Option<Range>,
) -> Result<()> {
    if let Some(resolved_type) = named_types.get(type_name) {
        validate_default_value(name_regex, default, resolved_type, named_types, field_range)?;
    }
    Ok(())
}
