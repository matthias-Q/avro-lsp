use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_lsp::lsp_types::{Diagnostic, Location, Position, Range, Url};

use crate::schema::{AvroParser, AvroSchema, AvroType, AvroValidator, TypeResolver, UnionSchema};

/// Information about a named type definition
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields will be used when implementing cross-file features
pub struct TypeInfo {
    /// The type definition
    pub type_def: AvroType,
    /// URI of the file where this type is defined
    pub defined_in: Url,
    /// Full qualified name (with namespace if present)
    pub qualified_name: String,
    /// Namespace of the type (if any)
    pub namespace: Option<String>,
    /// Range in the source file where the type is defined
    pub definition_range: Option<Range>,
}

/// Manages a workspace of Avro schema files
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields will be used when implementing cross-file features
pub struct Workspace {
    /// Root path of the workspace
    root_path: Option<PathBuf>,
    /// Parsed schemas by URI
    schemas: HashMap<Url, AvroSchema>,
    /// Global type registry: maps type names to their definitions
    /// Keys can be either simple names or fully-qualified names
    global_types: HashMap<String, TypeInfo>,
    /// Inverted reference index: qualified type name → all locations that reference it.
    /// Updated incrementally on update_file / remove_file.
    ref_index: HashMap<String, Vec<Location>>,
}

impl Workspace {
    /// Create a new empty workspace
    pub fn new() -> Self {
        Self {
            root_path: None,
            schemas: HashMap::new(),
            global_types: HashMap::new(),
            ref_index: HashMap::new(),
        }
    }

    /// Create a workspace with a root path
    pub fn with_root(root_path: PathBuf) -> Self {
        Self {
            root_path: Some(root_path),
            schemas: HashMap::new(),
            global_types: HashMap::new(),
            ref_index: HashMap::new(),
        }
    }

    /// Get the workspace root path
    #[allow(dead_code)] // Will be used for cross-file features
    pub fn root_path(&self) -> Option<&Path> {
        self.root_path.as_deref()
    }

    /// Update or add a schema file
    pub fn update_file(&mut self, uri: Url, text: String) -> Result<(), String> {
        let mut parser = AvroParser::new();
        let schema = parser
            .parse(&text)
            .map_err(|e| format!("Parse error: {}", e))?;

        // Remove stale ref-index entries for this file before re-indexing
        self.unindex_refs(&uri);

        // Update global type registry
        self.register_types(&uri, &schema);

        // Store the schema, then build the ref-index for it.
        // We need global_types to be up to date for resolve_type, so insert first.
        self.schemas.insert(uri.clone(), schema);
        self.index_refs(&uri);

        Ok(())
    }

    /// Remove a schema file
    pub fn remove_file(&mut self, uri: &Url) {
        // Remove from schemas
        if let Some(schema) = self.schemas.remove(uri) {
            // Remove types defined in this file from global registry
            self.unregister_types(uri, &schema);
            // Remove ref-index entries for this file
            self.unindex_refs(uri);
        }
    }

    /// Register types from a schema in the global type registry
    fn register_types(&mut self, uri: &Url, schema: &AvroSchema) {
        for (name, avro_type) in &schema.named_types {
            let (namespace, definition_range) = match avro_type {
                AvroType::Record(record) => (record.namespace.clone(), record.name_range),
                AvroType::Enum(enum_schema) => {
                    (enum_schema.namespace.clone(), enum_schema.name_range)
                }
                AvroType::Fixed(fixed) => (fixed.namespace.clone(), fixed.name_range),
                _ => (None, None),
            };

            let qualified_name = if let Some(ref ns) = namespace {
                format!("{}.{}", ns, name)
            } else {
                name.clone()
            };

            let type_info = TypeInfo {
                type_def: avro_type.clone(),
                defined_in: uri.clone(),
                qualified_name: qualified_name.clone(),
                namespace: namespace.clone(),
                definition_range,
            };

            // Always register the fully qualified name
            self.global_types
                .insert(qualified_name.clone(), type_info.clone());

            // For namespaced types, also register the simple name for type_exists() checks
            // For null-namespace types (no namespace), we do NOT register the simple name
            // in the global registry - they should only be visible within their own file
            if namespace.is_some() {
                // Note: If multiple types have the same simple name in different namespaces,
                // the last one registered wins here. This is OK because resolve_type()
                // will do proper namespace-aware resolution.
                self.global_types.insert(name.clone(), type_info);
            }
        }
    }

    /// Remove types defined in a file from the global registry
    fn unregister_types(&mut self, uri: &Url, _schema: &AvroSchema) {
        // Remove all types that were defined in this file
        self.global_types.retain(|_, info| &info.defined_in != uri);
    }

    /// Build ref-index entries for all TypeRefs in the given file.
    /// Must be called after the schema has been inserted into self.schemas
    /// and global_types is up to date.
    fn index_refs(&mut self, uri: &Url) {
        let Some(schema) = self.schemas.get(uri) else {
            return;
        };
        // Capture the file's namespace so we can resolve simple names correctly
        let file_namespace = self.get_schema_namespace(&schema.root);
        // Collect (qualified_name, location) pairs without holding a borrow on self
        let mut entries: Vec<(String, Location)> = Vec::new();
        Self::collect_typerefs_for_index(
            &schema.root,
            uri,
            &self.global_types,
            file_namespace.as_deref(),
            &mut entries,
        );
        for (qualified_name, location) in entries {
            self.ref_index
                .entry(qualified_name)
                .or_default()
                .push(location);
        }
    }

    /// Remove all ref-index entries that originate from the given file.
    fn unindex_refs(&mut self, uri: &Url) {
        for locations in self.ref_index.values_mut() {
            locations.retain(|loc| &loc.uri != uri);
        }
        // Drop empty vecs to keep memory tidy
        self.ref_index.retain(|_, v| !v.is_empty());
    }

    /// Recursively walk an AvroType, resolving every TypeRef and recording
    /// (qualified_name → Location) pairs into `out`.
    /// `file_namespace`: the namespace of the file being indexed (for simple-name resolution).
    fn collect_typerefs_for_index(
        avro_type: &AvroType,
        uri: &Url,
        global_types: &HashMap<String, TypeInfo>,
        file_namespace: Option<&str>,
        out: &mut Vec<(String, Location)>,
    ) {
        match avro_type {
            AvroType::TypeRef(type_ref) => {
                // Mirror resolve_type logic: fully-qualified → direct lookup;
                // simple name with namespace → try "ns.name"; no namespace → only local.
                let resolved_qualified = if type_ref.name.contains('.') {
                    // Fully qualified: look up directly
                    global_types
                        .get(&type_ref.name)
                        .map(|info| info.qualified_name.as_str())
                        .unwrap_or(&type_ref.name)
                } else if let Some(ns) = file_namespace {
                    // Namespaced file: try "ns.simpleName"
                    let qualified = format!("{}.{}", ns, &type_ref.name);
                    global_types
                        .get(&qualified)
                        .map(|info| info.qualified_name.as_str())
                        .unwrap_or(&type_ref.name)
                } else {
                    // No namespace: this file can only reference types local to itself.
                    // Don't index these into the cross-file ref-index.
                    return;
                };

                if let Some(range) = type_ref.range {
                    out.push((
                        resolved_qualified.to_string(),
                        Location {
                            uri: uri.clone(),
                            range,
                        },
                    ));
                }
            }
            AvroType::Record(record) => {
                for field in &record.fields {
                    Self::collect_typerefs_for_index(
                        &field.field_type,
                        uri,
                        global_types,
                        file_namespace,
                        out,
                    );
                }
            }
            AvroType::Array(array) => {
                Self::collect_typerefs_for_index(
                    &array.items,
                    uri,
                    global_types,
                    file_namespace,
                    out,
                );
            }
            AvroType::Map(map) => {
                Self::collect_typerefs_for_index(
                    &map.values,
                    uri,
                    global_types,
                    file_namespace,
                    out,
                );
            }
            AvroType::Union(UnionSchema { types, .. }) => {
                for t in types {
                    Self::collect_typerefs_for_index(t, uri, global_types, file_namespace, out);
                }
            }
            _ => {}
        }
    }

    /// Get type information by qualified name
    pub fn get_type(&self, qualified_name: &str) -> Option<&TypeInfo> {
        self.global_types.get(qualified_name)
    }

    /// Resolve a type reference (check local schema first, then workspace)
    /// Implements Avro namespace resolution rules:
    /// 1. If name contains a dot (qualified), use as-is
    /// 2. If name is simple, check local file first
    /// 3. Then try current namespace + name
    /// 4. Finally try null namespace (name only)
    #[allow(dead_code)] // Will be used for cross-file validation
    pub fn resolve_type(&self, name: &str, from_file: &Url) -> Option<&TypeInfo> {
        // If name contains a dot, it's fully qualified - look it up directly
        if name.contains('.') {
            return self.global_types.get(name);
        }

        // Try to get the namespace from the file in the workspace
        let namespace = self
            .schemas
            .get(from_file)
            .and_then(|schema| self.get_schema_namespace(&schema.root));

        self.resolve_type_with_namespace(name, from_file, namespace.as_deref())
    }

    /// Resolve a type with an explicit namespace context
    /// This is useful when the file hasn't been added to the workspace yet
    pub fn resolve_type_with_namespace(
        &self,
        name: &str,
        from_file: &Url,
        namespace: Option<&str>,
    ) -> Option<&TypeInfo> {
        // If name contains a dot, it's fully qualified - look it up directly
        if name.contains('.') {
            return self.global_types.get(name);
        }

        // Check if it's defined locally in the same file (if file is in workspace)
        if let Some(schema) = self.schemas.get(from_file)
            && schema.named_types.contains_key(name)
        {
            // It's a local type - construct qualified name and look it up
            let namespace = self.get_schema_namespace(&schema.root);
            let qualified_name = if let Some(ns) = namespace {
                format!("{}.{}", ns, name)
            } else {
                name.to_string()
            };
            return self.global_types.get(&qualified_name);
        }

        // Not local - use namespace-aware resolution
        if let Some(ns) = namespace {
            // Current file has a namespace - try qualified lookup.
            // Build the qualified key on the stack to avoid a heap allocation on
            // the happy path: concatenate into a small stack buffer via format! only
            // when we know we need it.
            let qualified = format!("{}.{}", ns, name);
            if let Some(type_info) = self.global_types.get(&qualified) {
                return Some(type_info);
            }
            // No fallback - if the current file has a namespace and references
            // a simple name, it must be in the same namespace per Avro rules
            return None;
        }

        // Current file has no namespace - can only reference types in same file
        // (already handled above in local check)
        None
    }

    /// Extract the namespace from a schema's root type
    fn get_schema_namespace(&self, root_type: &AvroType) -> Option<String> {
        match root_type {
            AvroType::Record(record) => record.namespace.clone(),
            AvroType::Enum(enum_schema) => enum_schema.namespace.clone(),
            AvroType::Fixed(fixed) => fixed.namespace.clone(),
            _ => None,
        }
    }

    /// Find all references to a type across the workspace.
    /// source_uri: The file where the type is defined/being searched from.
    /// Uses the pre-built inverted ref-index for O(1) lookup.
    #[allow(dead_code)]
    pub fn find_all_references_from(&self, type_name: &str, source_uri: &Url) -> Vec<Location> {
        let target_qualified_name = self
            .resolve_type(type_name, source_uri)
            .map(|info| info.qualified_name.as_str())
            .unwrap_or(type_name);

        self.ref_index
            .get(target_qualified_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Legacy method - kept for backward compatibility.
    /// Uses the pre-built inverted ref-index for O(1) lookup.
    #[allow(dead_code)]
    pub fn find_all_references(&self, type_name: &str) -> Vec<Location> {
        // Try to find the qualified name via simple name lookup in global_types
        let target = self
            .global_types
            .get(type_name)
            .map(|info| info.qualified_name.as_str())
            .unwrap_or(type_name);

        self.ref_index.get(target).cloned().unwrap_or_default()
    }

    /// Validate all schemas in the workspace
    #[allow(dead_code)] // Will be used for workspace-wide validation
    pub fn validate_all(&self) -> HashMap<Url, Vec<Diagnostic>> {
        let mut diagnostics = HashMap::new();
        let validator = AvroValidator::new();

        for (uri, schema) in &self.schemas {
            let mut schema_diagnostics = Vec::new();

            // Validate with workspace context
            if let Err(e) = self.validate_schema_with_workspace(schema, &validator) {
                // Convert error to diagnostic
                let diagnostic = Diagnostic {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 1,
                        },
                    },
                    severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
                    code: None,
                    code_description: None,
                    source: Some("avro-lsp".to_string()),
                    message: e.to_string(),
                    related_information: None,
                    tags: None,
                    data: None,
                };
                schema_diagnostics.push(diagnostic);
            }

            diagnostics.insert(uri.clone(), schema_diagnostics);
        }

        diagnostics
    }

    /// Validate a schema with workspace context for cross-file type resolution
    #[allow(dead_code)] // Helper for validate_all
    fn validate_schema_with_workspace(
        &self,
        schema: &AvroSchema,
        validator: &AvroValidator,
    ) -> Result<(), crate::schema::SchemaError> {
        // For now, just use regular validation
        // In future, we'll enhance the validator to check cross-file references
        validator.validate(schema)
    }

    /// Get a schema by URI
    #[allow(dead_code)] // Will be used for cross-file operations
    pub fn get_schema(&self, uri: &Url) -> Option<&AvroSchema> {
        self.schemas.get(uri)
    }

    /// Get all URIs in the workspace
    #[allow(dead_code)] // Will be used for workspace operations
    pub fn uris(&self) -> impl Iterator<Item = &Url> {
        self.schemas.keys()
    }

    /// Check if workspace contains a file
    #[allow(dead_code)] // Will be used for workspace operations
    pub fn contains(&self, uri: &Url) -> bool {
        self.schemas.contains_key(uri)
    }

    /// Get all schemas in the workspace (used for workspace symbol search)
    pub fn get_all_schemas(&self) -> &HashMap<Url, AvroSchema> {
        &self.schemas
    }
}

/// Implement TypeResolver for Workspace to support cross-file type checking
impl TypeResolver for Workspace {
    fn type_exists(&self, name: &str) -> bool {
        self.global_types.contains_key(name)
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_add_and_remove() {
        let mut workspace = Workspace::new();
        let uri = Url::parse("file:///test.avsc").unwrap();
        let schema_text = r#"{"type": "record", "name": "User", "fields": []}"#;

        // Add file
        workspace
            .update_file(uri.clone(), schema_text.to_string())
            .unwrap();
        assert!(workspace.contains(&uri));

        // Remove file
        workspace.remove_file(&uri);
        assert!(!workspace.contains(&uri));
    }

    #[test]
    fn test_type_resolution_same_file() {
        let mut workspace = Workspace::new();
        let uri = Url::parse("file:///test.avsc").unwrap();
        let schema_text = r#"{
            "type": "record",
            "name": "User",
            "fields": [
                {"name": "address", "type": "Address"}
            ]
        }"#;

        workspace
            .update_file(uri.clone(), schema_text.to_string())
            .ok();

        // Should not resolve Address (it's not defined)
        assert!(workspace.resolve_type("Address", &uri).is_none());
    }

    #[test]
    fn test_global_type_registry() {
        let mut workspace = Workspace::new();
        let uri = Url::parse("file:///user.avsc").unwrap();
        let schema_text = r#"{"type": "record", "name": "User", "fields": []}"#;

        workspace
            .update_file(uri.clone(), schema_text.to_string())
            .unwrap();

        // User type should be in global registry
        let type_info = workspace.resolve_type("User", &uri);
        assert!(type_info.is_some());
        assert_eq!(type_info.unwrap().qualified_name, "User");
    }

    #[test]
    fn test_namespace_qualified_names() {
        let mut workspace = Workspace::new();
        let uri = Url::parse("file:///user.avsc").unwrap();
        let schema_text = r#"{
            "type": "record",
            "name": "User",
            "namespace": "com.example",
            "fields": []
        }"#;

        workspace
            .update_file(uri.clone(), schema_text.to_string())
            .unwrap();

        // Should be accessible by both simple and qualified name
        assert!(workspace.resolve_type("User", &uri).is_some());
        assert!(workspace.resolve_type("com.example.User", &uri).is_some());
    }

    #[test]
    fn test_cross_file_validation_works() {
        use crate::handlers::diagnostics::parse_and_validate_with_workspace;

        // Create a workspace and add the Address schema
        let mut workspace = Workspace::new();

        let address_uri = Url::parse("file:///workspace/address.avsc").unwrap();
        let address_schema = r#"{
  "type": "record",
  "name": "Address",
  "namespace": "com.example",
  "fields": [
    {"name": "street", "type": "string"},
    {"name": "city", "type": "string"}
  ]
}"#;

        // Register Address in workspace
        workspace
            .update_file(address_uri, address_schema.to_string())
            .unwrap();

        // Now validate User schema that references Address
        let user_schema = r#"{
  "type": "record",
  "name": "User",
  "namespace": "com.example",
  "fields": [
    {"name": "id", "type": "long"},
    {"name": "address", "type": "Address"}
  ]
}"#;

        // Without workspace - should have error
        let diagnostics_without = parse_and_validate_with_workspace(user_schema, None);
        assert!(
            !diagnostics_without.is_empty(),
            "Should have 'Unknown type reference: Address' error"
        );
        assert!(
            diagnostics_without[0].message.contains("Address"),
            "Error should mention Address type: {}",
            diagnostics_without[0].message
        );

        // With workspace - should be valid!
        let diagnostics_with = parse_and_validate_with_workspace(user_schema, Some(&workspace));
        assert!(
            diagnostics_with.is_empty(),
            "Should have no errors with workspace context, but got: {:?}",
            diagnostics_with
        );
    }

    #[test]
    fn test_multiple_cross_file_references() {
        use crate::handlers::diagnostics::parse_and_validate_with_workspace;

        let mut workspace = Workspace::new();

        // Add Address
        let address_uri = Url::parse("file:///workspace/address.avsc").unwrap();
        let address_schema = r#"{
  "type": "record",
  "name": "Address",
  "namespace": "com.example",
  "fields": [{"name": "city", "type": "string"}]
}"#;
        workspace
            .update_file(address_uri, address_schema.to_string())
            .unwrap();

        // Add User
        let user_uri = Url::parse("file:///workspace/user.avsc").unwrap();
        let user_schema = r#"{
  "type": "record",
  "name": "User",
  "namespace": "com.example",
  "fields": [
    {"name": "id", "type": "long"},
    {"name": "address", "type": "Address"}
  ]
}"#;
        workspace
            .update_file(user_uri, user_schema.to_string())
            .unwrap();

        // Company references both Address and User
        let company_schema = r#"{
  "type": "record",
  "name": "Company",
  "namespace": "com.example",
  "fields": [
    {"name": "name", "type": "string"},
    {"name": "hqAddress", "type": "Address"},
    {"name": "ceo", "type": "User"}
  ]
}"#;

        let diagnostics = parse_and_validate_with_workspace(company_schema, Some(&workspace));
        assert!(
            diagnostics.is_empty(),
            "Company should validate with both Address and User in workspace, but got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_qualified_name_cross_file() {
        use crate::handlers::diagnostics::parse_and_validate_with_workspace;

        let mut workspace = Workspace::new();

        // Address in com.example namespace
        let address_uri = Url::parse("file:///workspace/address.avsc").unwrap();
        let address_schema = r#"{
  "type": "record",
  "name": "Address",
  "namespace": "com.example",
  "fields": [{"name": "city", "type": "string"}]
}"#;
        workspace
            .update_file(address_uri, address_schema.to_string())
            .unwrap();

        // User references com.example.Address from different namespace
        let user_schema = r#"{
  "type": "record",
  "name": "Customer",
  "namespace": "com.other",
  "fields": [
    {"name": "id", "type": "long"},
    {"name": "address", "type": "com.example.Address"}
   ]
}"#;

        let diagnostics = parse_and_validate_with_workspace(user_schema, Some(&workspace));
        assert!(
            diagnostics.is_empty(),
            "Should validate qualified cross-namespace reference, but got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_namespace_isolation_for_references() {
        use crate::handlers::rename::find_references_with_workspace;
        use async_lsp::lsp_types::Position;

        let mut workspace = Workspace::new();

        // Add Address with namespace com.example
        let address_uri = Url::parse("file:///multi-file/address.avsc").unwrap();
        let address_schema_text = r#"{
  "type": "record",
  "name": "Address",
  "namespace": "com.example",
  "fields": [{"name": "street", "type": "string"}]
}"#;
        workspace
            .update_file(address_uri.clone(), address_schema_text.to_string())
            .unwrap();

        // Add User with namespace com.example referencing Address
        let user_uri = Url::parse("file:///multi-file/user.avsc").unwrap();
        let user_schema_text = r#"{
  "type": "record",
  "name": "User",
  "namespace": "com.example",
  "fields": [
    {"name": "id", "type": "long"},
    {"name": "address", "type": "Address"}
  ]
}"#;
        workspace
            .update_file(user_uri.clone(), user_schema_text.to_string())
            .unwrap();

        // Add another Address with NO namespace (should be isolated)
        let isolated_address_uri = Url::parse("file:///other/address.avsc").unwrap();
        let isolated_schema_text = r#"{
  "type": "record",
  "name": "Address",
  "fields": [{"name": "street", "type": "string"}]
}"#;
        workspace
            .update_file(
                isolated_address_uri.clone(),
                isolated_schema_text.to_string(),
            )
            .unwrap();

        // Add Person with NO namespace referencing the null-namespace Address
        let person_uri = Url::parse("file:///other/person.avsc").unwrap();
        let person_schema_text = r#"{
  "type": "record",
  "name": "Person",
  "fields": [
    {"name": "name", "type": "string"},
    {"name": "address", "type": "Address"}
  ]
}"#;
        workspace
            .update_file(person_uri.clone(), person_schema_text.to_string())
            .unwrap();

        // Parse schemas for find_references_with_workspace
        let address_schema = AvroParser::new().parse(address_schema_text).unwrap();
        let isolated_schema = AvroParser::new().parse(isolated_schema_text).unwrap();

        // Find references to "Address" from com.example.Address (line 3, col 11)
        let refs_namespaced = find_references_with_workspace(
            &address_schema,
            &address_uri,
            Position::new(2, 11),
            false,
            Some(&workspace),
        );

        // Should find reference in user.avsc but NOT in person.avsc
        let refs = refs_namespaced.expect("Should find references for namespaced Address");
        assert_eq!(
            refs.len(),
            1,
            "Should find exactly 1 reference to com.example.Address (in user.avsc), but found: {:#?}",
            refs
        );
        assert_eq!(
            refs[0].uri, user_uri,
            "Reference should be in user.avsc, not person.avsc"
        );

        // Find references to null-namespace "Address" (line 3, col 11)
        let refs_null = find_references_with_workspace(
            &isolated_schema,
            &isolated_address_uri,
            Position::new(2, 11),
            false,
            Some(&workspace),
        );

        // Should NOT find cross-file references (null namespace is file-local only)
        // But person.avsc has its own "Address" reference that won't resolve across files
        assert!(
            refs_null.is_none() || refs_null.as_ref().unwrap().is_empty(),
            "Should find 0 cross-file references for null-namespace Address (file-local only), but found: {:#?}",
            refs_null
        );
    }
}
