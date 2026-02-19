mod complex_validators;
mod default_validators;
mod logical_type_validators;
mod name_validators;

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use super::error::Result;
use super::types::{AvroSchema, AvroType, RecordSchema};
use super::warning::SchemaWarning;
pub use complex_validators::TypeResolver;

static NAME_REGEX: OnceLock<Regex> = OnceLock::new();

pub(crate) fn name_regex() -> &'static Regex {
    NAME_REGEX.get_or_init(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap())
}

struct LocalTypeResolver<'a> {
    named_types: &'a HashMap<String, AvroType>,
}

impl<'a> TypeResolver for LocalTypeResolver<'a> {
    fn type_exists(&self, name: &str) -> bool {
        self.named_types.contains_key(name)
    }
}

pub struct AvroValidator;

impl AvroValidator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, schema: &AvroSchema) -> Result<()> {
        self.validate_type(&schema.root, &schema.named_types)?;
        Ok(())
    }

    pub fn validate_with_resolver(
        &self,
        schema: &AvroSchema,
        resolver: &dyn TypeResolver,
    ) -> Result<()> {
        self.validate_type_with_resolver(&schema.root, &schema.named_types, resolver)?;
        Ok(())
    }

    fn validate_type(
        &self,
        avro_type: &AvroType,
        named_types: &HashMap<String, AvroType>,
    ) -> Result<()> {
        let resolver = LocalTypeResolver { named_types };
        self.validate_type_with_resolver(avro_type, named_types, &resolver)
    }

    fn validate_type_with_resolver(
        &self,
        avro_type: &AvroType,
        named_types: &HashMap<String, AvroType>,
        resolver: &dyn TypeResolver,
    ) -> Result<()> {
        complex_validators::validate_type_with_resolver(
            name_regex(),
            avro_type,
            named_types,
            resolver,
        )
    }

    #[allow(dead_code)]
    fn validate_record(
        &self,
        record: &RecordSchema,
        named_types: &HashMap<String, AvroType>,
    ) -> Result<()> {
        let resolver = LocalTypeResolver { named_types };
        self.validate_record_with_resolver(record, named_types, &resolver)
    }

    fn validate_record_with_resolver(
        &self,
        record: &RecordSchema,
        named_types: &HashMap<String, AvroType>,
        resolver: &dyn TypeResolver,
    ) -> Result<()> {
        complex_validators::validate_record_with_resolver(
            name_regex(),
            record,
            named_types,
            resolver,
        )
    }

    #[allow(dead_code)]
    fn validate_name(&self, name: &str) -> Result<()> {
        self.validate_name_with_range(name, None)
    }

    fn validate_name_with_range(
        &self,
        name: &str,
        range: Option<async_lsp::lsp_types::Range>,
    ) -> Result<()> {
        name_validators::validate_name_with_range(name, range, name_regex())
    }

    /// Collect warnings from a schema (e.g., unions with complex types)
    pub fn collect_warnings(&self, schema: &AvroSchema) -> Vec<SchemaWarning> {
        let mut warnings = Vec::new();
        self.collect_warnings_from_type(&schema.root, &mut warnings);
        warnings
    }

    fn collect_warnings_from_type(&self, avro_type: &AvroType, warnings: &mut Vec<SchemaWarning>) {
        match avro_type {
            AvroType::Union(union_schema) => {
                // Check for complex union patterns
                warnings.extend(complex_validators::check_union_complexity_warnings(
                    union_schema,
                ));
                // Recursively check nested types
                for t in &union_schema.types {
                    self.collect_warnings_from_type(t, warnings);
                }
            }
            AvroType::Record(record) => {
                for field in &record.fields {
                    self.collect_warnings_from_type(&field.field_type, warnings);
                }
            }
            AvroType::Array(array) => {
                self.collect_warnings_from_type(&array.items, warnings);
            }
            AvroType::Map(map) => {
                self.collect_warnings_from_type(&map.values, warnings);
            }
            AvroType::PrimitiveObject(prim) => {
                // Check for unknown logical types
                if let Some(warning) =
                    logical_type_validators::check_unknown_logical_type_warning(prim)
                {
                    warnings.push(warning);
                }
            }
            _ => {
                // Primitives, enums, fixed, etc. - no nested types to check
            }
        }
    }

    #[allow(dead_code)]
    fn validate_namespace(&self, namespace: &str) -> Result<()> {
        self.validate_namespace_with_range(namespace, None)
    }

    fn validate_namespace_with_range(
        &self,
        namespace: &str,
        range: Option<async_lsp::lsp_types::Range>,
    ) -> Result<()> {
        name_validators::validate_namespace_with_range(namespace, range, name_regex())
    }

    #[allow(dead_code)]
    fn validate_enum(&self, enum_schema: &crate::schema::types::EnumSchema) -> Result<()> {
        complex_validators::validate_enum(name_regex(), enum_schema)
    }
}

impl Default for AvroValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
