mod complex_validators;
mod default_validators;
mod logical_type_validators;
mod name_validators;

use std::collections::HashMap;

use regex::Regex;

use super::error::Result;
use super::types::{AvroSchema, AvroType, RecordSchema};
pub use complex_validators::TypeResolver;

struct LocalTypeResolver<'a> {
    named_types: &'a HashMap<String, AvroType>,
}

impl<'a> TypeResolver for LocalTypeResolver<'a> {
    fn type_exists(&self, name: &str) -> bool {
        self.named_types.contains_key(name)
    }
}

pub struct AvroValidator {
    name_regex: Regex,
}

impl AvroValidator {
    pub fn new() -> Self {
        Self {
            name_regex: Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap(),
        }
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
            &self.name_regex,
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
            &self.name_regex,
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
        name_validators::validate_name_with_range(name, range, &self.name_regex)
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
        name_validators::validate_namespace_with_range(namespace, range, &self.name_regex)
    }

    #[allow(dead_code)]
    fn validate_enum(&self, enum_schema: &crate::schema::types::EnumSchema) -> Result<()> {
        complex_validators::validate_enum(&self.name_regex, enum_schema)
    }
}

impl Default for AvroValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
