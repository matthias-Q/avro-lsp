use std::collections::{HashMap, HashSet};

use regex::Regex;

use super::super::error::{Result, SchemaError};
use super::super::types::{AvroType, EnumSchema, FixedSchema, PrimitiveType, RecordSchema};
use super::default_validators::validate_default_value;
use super::logical_type_validators::validate_logical_type_for_fixed;
use super::name_validators::{validate_name_with_range, validate_namespace_with_range};

pub trait TypeResolver {
    fn type_exists(&self, name: &str) -> bool;
}

pub fn validate_record_with_resolver(
    name_regex: &Regex,
    record: &RecordSchema,
    named_types: &HashMap<String, AvroType>,
    resolver: &dyn TypeResolver,
) -> Result<()> {
    validate_name_with_range(&record.name, record.name_range, name_regex)?;

    if let Some(namespace) = &record.namespace {
        validate_namespace_with_range(namespace, record.namespace_range, name_regex)?;
    }

    if record.fields.is_empty() {
        return Err(SchemaError::Custom {
            message: "Record must have at least one field".to_string(),
            range: record.range,
        });
    }

    // Check for duplicate field names
    let mut seen_fields: HashMap<&str, Option<async_lsp::lsp_types::Range>> = HashMap::new();
    for field in &record.fields {
        if let Some(first_occurrence) = seen_fields.get(field.name.as_str()) {
            return Err(SchemaError::DuplicateFieldName {
                field: field.name.clone(),
                record: record.name.clone(),
                first_occurrence: *first_occurrence,
                duplicate_occurrence: field.name_range,
            });
        }
        seen_fields.insert(&field.name, field.name_range);
    }

    for field in &record.fields {
        validate_name_with_range(&field.name, field.name_range, name_regex)?;
        validate_type_with_resolver(name_regex, &field.field_type, named_types, resolver)?;

        if let Some(default_value) = &field.default {
            validate_default_value(
                name_regex,
                default_value,
                &field.field_type,
                named_types,
                field.range,
            )?;
        }
    }

    Ok(())
}

pub fn validate_enum(name_regex: &Regex, enum_schema: &EnumSchema) -> Result<()> {
    validate_name_with_range(&enum_schema.name, enum_schema.name_range, name_regex)?;

    if let Some(namespace) = &enum_schema.namespace {
        validate_namespace_with_range(namespace, enum_schema.namespace_range, name_regex)?;
    }

    if enum_schema.symbols.is_empty() {
        return Err(SchemaError::Custom {
            message: "Enum must have at least one symbol".to_string(),
            range: enum_schema.range,
        });
    }

    let mut seen = HashSet::new();
    for symbol in &enum_schema.symbols {
        validate_name_with_range(symbol, None, name_regex)?;
        if !seen.insert(symbol) {
            return Err(SchemaError::DuplicateSymbol {
                symbol: symbol.clone(),
                first_occurrence: None,
                duplicate_occurrence: None,
            });
        }
    }

    if let Some(default) = &enum_schema.default
        && !enum_schema.symbols.contains(default)
    {
        return Err(SchemaError::Custom {
            message: format!("Default value '{}' is not in symbols list", default),
            range: enum_schema.range,
        });
    }

    Ok(())
}

pub fn validate_fixed(name_regex: &Regex, fixed: &FixedSchema) -> Result<()> {
    validate_name_with_range(&fixed.name, fixed.name_range, name_regex)?;

    if let Some(namespace) = &fixed.namespace {
        validate_namespace_with_range(namespace, fixed.namespace_range, name_regex)?;
    }

    if fixed.size == 0 {
        return Err(SchemaError::Custom {
            message: "Fixed size must be greater than 0".to_string(),
            range: fixed.range,
        });
    }

    if let Some(logical_type) = &fixed.logical_type {
        validate_logical_type_for_fixed(name_regex, logical_type, fixed)?;
    }

    Ok(())
}

pub fn validate_union_with_resolver(
    name_regex: &Regex,
    types: &[AvroType],
    named_types: &HashMap<String, AvroType>,
    resolver: &dyn TypeResolver,
) -> Result<()> {
    if types.is_empty() {
        return Err(SchemaError::Custom {
            message: "Union cannot be empty".to_string(),
            range: None,
        });
    }

    for t in types {
        if matches!(t, AvroType::Union(_)) {
            return Err(SchemaError::NestedUnion { range: None });
        }
    }

    let mut type_signatures = HashSet::new();
    for t in types {
        let signature = type_signature(t);
        if !type_signatures.insert(signature.clone()) {
            return Err(SchemaError::DuplicateUnionType {
                type_signature: signature,
                range: None,
            });
        }
    }

    for t in types {
        validate_type_with_resolver(name_regex, t, named_types, resolver)?;
    }

    Ok(())
}

pub fn validate_type_reference_with_resolver(
    _name_regex: &Regex,
    name: &str,
    named_types: &HashMap<String, AvroType>,
    resolver: &dyn TypeResolver,
) -> Result<()> {
    if PrimitiveType::parse(name).is_some() {
        return Ok(());
    }

    if named_types.contains_key(name) || resolver.type_exists(name) {
        return Ok(());
    }

    Err(SchemaError::UnknownTypeReference {
        type_name: name.to_string(),
        range: None,
    })
}

pub fn validate_type_with_resolver(
    name_regex: &Regex,
    avro_type: &AvroType,
    named_types: &HashMap<String, AvroType>,
    resolver: &dyn TypeResolver,
) -> Result<()> {
    use super::logical_type_validators::validate_primitive_with_logical_type;

    match avro_type {
        AvroType::Primitive(_) => Ok(()),
        AvroType::PrimitiveObject(prim) => validate_primitive_with_logical_type(name_regex, prim),
        AvroType::Record(record) => {
            validate_record_with_resolver(name_regex, record, named_types, resolver)
        }
        AvroType::Enum(enum_schema) => validate_enum(name_regex, enum_schema),
        AvroType::Array(array) => {
            validate_type_with_resolver(name_regex, &array.items, named_types, resolver)
        }
        AvroType::Map(map) => {
            validate_type_with_resolver(name_regex, &map.values, named_types, resolver)
        }
        AvroType::Union(types) => {
            validate_union_with_resolver(name_regex, types, named_types, resolver)
        }
        AvroType::Fixed(fixed) => validate_fixed(name_regex, fixed),
        AvroType::TypeRef(type_ref) => {
            validate_type_reference_with_resolver(name_regex, &type_ref.name, named_types, resolver)
        }
        AvroType::Invalid(_) => Ok(()),
    }
}

fn type_signature(avro_type: &AvroType) -> String {
    match avro_type {
        AvroType::Primitive(p) => format!("{:?}", p),
        AvroType::PrimitiveObject(p) => format!("{:?}", p.primitive_type),
        AvroType::Record(r) => format!("record:{}", r.name),
        AvroType::Enum(e) => format!("enum:{}", e.name),
        AvroType::Fixed(f) => format!("fixed:{}", f.name),
        AvroType::Array(_) => "array".to_string(),
        AvroType::Map(_) => "map".to_string(),
        AvroType::Union(_) => "union".to_string(),
        AvroType::TypeRef(type_ref) => format!("ref:{}", type_ref.name),
        AvroType::Invalid(invalid) => format!("invalid:{}", invalid.type_name),
    }
}
