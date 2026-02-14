use std::collections::HashMap;

use async_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, Position, Range, TextEdit, Url, WorkspaceEdit,
};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::schema::{AvroSchema, AvroType, EnumSchema, Field, RecordSchema};
use crate::state::{AstNode, find_node_at_position};

// Compile regex once at startup instead of repeatedly in hot paths
static AVRO_NAME_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").expect("Valid regex pattern"));

/// Builder for creating CodeAction instances with less boilerplate
struct CodeActionBuilder {
    uri: Url,
    title: String,
    kind: CodeActionKind,
    is_preferred: bool,
    diagnostics: Option<Vec<Diagnostic>>,
    edits: Vec<TextEdit>,
}

impl CodeActionBuilder {
    /// Create a new builder with required fields
    fn new(uri: Url, title: impl Into<String>) -> Self {
        Self {
            uri,
            title: title.into(),
            kind: CodeActionKind::REFACTOR,
            is_preferred: false,
            diagnostics: None,
            edits: Vec::new(),
        }
    }

    /// Set the kind of code action (default: REFACTOR)
    fn with_kind(mut self, kind: CodeActionKind) -> Self {
        self.kind = kind;
        self
    }

    /// Mark this action as preferred (default: false)
    fn preferred(mut self) -> Self {
        self.is_preferred = true;
        self
    }

    /// Associate diagnostics with this action
    fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }

    /// Add a text edit to this action
    fn add_edit(mut self, range: Range, new_text: impl Into<String>) -> Self {
        self.edits.push(TextEdit {
            range,
            new_text: new_text.into(),
        });
        self
    }

    /// Add a text edit at a specific position (zero-width range)
    fn add_insert(self, position: Position, text: impl Into<String>) -> Self {
        self.add_edit(
            Range {
                start: position,
                end: position,
            },
            text,
        )
    }

    /// Build the final CodeAction
    fn build(self) -> CodeAction {
        let mut changes = HashMap::new();
        changes.insert(self.uri, self.edits);

        CodeAction {
            title: self.title,
            kind: Some(self.kind),
            diagnostics: self.diagnostics,
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }),
            command: None,
            is_preferred: Some(self.is_preferred),
            disabled: None,
            data: None,
        }
    }
}

/// Get code actions available at the given range
pub fn get_code_actions(schema: &AvroSchema, uri: &Url, range: Range) -> Vec<CodeAction> {
    // Use AST traversal to find the node at cursor position
    let node = match find_node_at_position(schema, range.start) {
        Some(n) => n,
        None => return Vec::new(),
    };

    let mut actions = Vec::new();

    match node {
        AstNode::RecordDefinition(record) => {
            // Offer "Add documentation" if record doesn't have doc
            if record.doc.is_none()
                && let Some(action) = create_add_doc_action(uri, record)
            {
                actions.push(action);
            }
            // Offer "Add field to record"
            if let Some(action) = create_add_field_action(uri, record) {
                actions.push(action);
            }
            // Offer "Sort fields alphabetically" if record has multiple fields
            if record.fields.len() > 1
                && let Some(action) = create_sort_fields_action(uri, record)
            {
                actions.push(action);
            }
        }
        AstNode::Field(field) => {
            // Offer "Add documentation" if field doesn't have doc
            if field.doc.is_none()
                && let Some(action) = create_add_doc_action_field(uri, field)
            {
                actions.push(action);
            }

            // Offer "Add field to record" (insert after this field)
            // We need to find the parent record
            if let Some(action) = find_parent_record_and_add_field(uri, schema, field) {
                actions.push(action);
            }

            // Offer "Make nullable" if field type is not already a union with null
            if !is_union(&field.field_type)
                && let Some(action) = create_make_nullable_action(uri, field)
            {
                actions.push(action);
            }

            // Offer "Add default value" if field doesn't have a default
            if field.default.is_none()
                && let Some(action) = create_add_default_value_action(uri, field)
            {
                actions.push(action);
            }
        }
        AstNode::FieldType(field) => {
            // When cursor is on the type value, offer "Make nullable"
            if !is_union(&field.field_type)
                && let Some(action) = create_make_nullable_action(uri, field)
            {
                actions.push(action);
            }
        }
        AstNode::EnumDefinition(enum_schema) => {
            // Offer "Add documentation" if enum doesn't have doc
            if enum_schema.doc.is_none()
                && let Some(action) = create_add_doc_action_enum(uri, enum_schema)
            {
                actions.push(action);
            }
        }
        AstNode::FixedDefinition(fixed_schema) => {
            // Offer "Add documentation" if fixed doesn't have doc
            if fixed_schema.doc.is_none()
                && let Some(action) = create_add_doc_action_fixed(uri, fixed_schema)
            {
                actions.push(action);
            }
        }
    }

    actions
}

fn is_union(avro_type: &AvroType) -> bool {
    matches!(avro_type, AvroType::Union(_))
}

fn format_avro_type_as_json(avro_type: &AvroType) -> String {
    match avro_type {
        AvroType::Primitive(prim) => {
            format!("\"{}\"", format!("{:?}", prim).to_lowercase())
        }
        AvroType::TypeRef(type_ref) => format!("\"{}\"", type_ref.name),
        // For all other types (including Record, Enum, Fixed, Array, Map, Union),
        // use serde_json serialization to preserve the full structure
        _ => serde_json::to_string(avro_type).unwrap_or_else(|_| "\"string\"".to_string()),
    }
}

fn create_make_nullable_action(
    uri: &Url,
    field: &Field,
) -> Option<async_lsp::lsp_types::CodeAction> {
    let type_range = field.type_range.as_ref()?;
    let current_type = format_avro_type_as_json(&field.field_type);
    let new_type = format!("[\"null\", {}]", current_type);

    Some(
        CodeActionBuilder::new(uri.clone(), format!("Make field '{}' nullable", field.name))
            .with_kind(async_lsp::lsp_types::CodeActionKind::REFACTOR)
            .add_edit(*type_range, new_type)
            .build(),
    )
}

/// Create "Add documentation" action for record using AST
fn create_add_doc_action(
    uri: &Url,
    record: &RecordSchema,
) -> Option<async_lsp::lsp_types::CodeAction> {
    let name_range = record.name_range.as_ref()?;

    // Insert doc field after the name line
    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n  \"doc\": \"Description for {}\"", record.name);

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Add documentation for '{}'", record.name),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::REFACTOR)
        .add_insert(insert_position, insert_text)
        .build(),
    )
}

/// Create "Add documentation" action for enum using AST
fn create_add_doc_action_enum(
    uri: &Url,
    enum_schema: &EnumSchema,
) -> Option<async_lsp::lsp_types::CodeAction> {
    let name_range = enum_schema.name_range.as_ref()?;

    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n  \"doc\": \"Description for {}\"", enum_schema.name);

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Add documentation for '{}'", enum_schema.name),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::REFACTOR)
        .add_insert(insert_position, insert_text)
        .build(),
    )
}

/// Create "Add documentation" action for fixed using AST
fn create_add_doc_action_fixed(
    uri: &Url,
    fixed_schema: &crate::schema::FixedSchema,
) -> Option<async_lsp::lsp_types::CodeAction> {
    let name_range = fixed_schema.name_range.as_ref()?;

    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n  \"doc\": \"Description for {}\"", fixed_schema.name);

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Add documentation for '{}'", fixed_schema.name),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::REFACTOR)
        .add_insert(insert_position, insert_text)
        .build(),
    )
}

/// Create "Add documentation" action for field
fn create_add_doc_action_field(
    uri: &Url,
    field: &Field,
) -> Option<async_lsp::lsp_types::CodeAction> {
    let name_range = field.name_range.as_ref()?;

    // Insert doc field after the field name
    let insert_position = Position {
        line: name_range.end.line,
        character: name_range.end.character,
    };
    let insert_text = format!(",\n    \"doc\": \"Description for {}\"", field.name);

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Add documentation for field '{}'", field.name),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::REFACTOR)
        .add_insert(insert_position, insert_text)
        .build(),
    )
}

/// Create "Add field to record" action using AST
fn create_add_field_action(
    uri: &Url,
    record: &RecordSchema,
) -> Option<async_lsp::lsp_types::CodeAction> {
    // Insert at the end of the fields array
    // We need to find the last field's range and insert after it
    let last_field = record.fields.last()?;
    let last_field_range = last_field.range.as_ref()?;

    let insert_position = Position {
        line: last_field_range.end.line,
        character: last_field_range.end.character,
    };

    let new_field = r#"{"name": "new_field", "type": "string"}"#;
    let insert_text = format!(",\n    {}", new_field);

    Some(
        CodeActionBuilder::new(uri.clone(), "Add field to record".to_string())
            .with_kind(async_lsp::lsp_types::CodeActionKind::REFACTOR)
            .add_insert(insert_position, insert_text)
            .build(),
    )
}

/// Helper to find parent record and create add field action
fn find_parent_record_and_add_field(
    uri: &Url,
    schema: &AvroSchema,
    _field: &Field,
) -> Option<async_lsp::lsp_types::CodeAction> {
    // For now, we'll find the root record if it's a record
    // In future, we could walk the tree to find the actual parent
    if let AvroType::Record(record) = &schema.root {
        create_add_field_action(uri, record)
    } else {
        None
    }
}

/// Create "Sort fields alphabetically" action for records
fn create_sort_fields_action(
    uri: &Url,
    record: &RecordSchema,
) -> Option<async_lsp::lsp_types::CodeAction> {
    // Check if fields are already sorted
    let field_names: Vec<&str> = record.fields.iter().map(|f| f.name.as_str()).collect();
    let mut sorted_names = field_names.clone();
    sorted_names.sort();

    if field_names == sorted_names {
        // Already sorted, no action needed
        return None;
    }

    // We need to find the range covering all fields and replace with sorted version
    let first_field = record.fields.first()?;
    let last_field = record.fields.last()?;

    let first_range = first_field.range.as_ref()?;
    let last_range = last_field.range.as_ref()?;

    let fields_range = Range {
        start: first_range.start,
        end: last_range.end,
    };

    // Sort fields by name
    let mut sorted_fields = record.fields.clone();
    sorted_fields.sort_by(|a, b| a.name.cmp(&b.name));

    // Serialize sorted fields as JSON
    let mut sorted_json = Vec::new();
    for (i, field) in sorted_fields.iter().enumerate() {
        let field_json = serde_json::json!({
            "name": field.name,
            "type": &*field.field_type,
            "doc": field.doc,
            "default": field.default,
            "order": field.order,
            "aliases": field.aliases,
        });

        // Remove null fields for cleaner output
        let mut field_map = field_json.as_object()?.clone();
        field_map.retain(|_, v| !v.is_null());

        let field_str = serde_json::to_string_pretty(&field_map).ok()?;

        if i > 0 {
            sorted_json.push(",\n    ".to_string());
        } else {
            sorted_json.push("".to_string());
        }
        sorted_json.push(field_str);
    }

    let new_text = sorted_json.concat();

    Some(
        CodeActionBuilder::new(uri.clone(), "Sort fields alphabetically".to_string())
            .with_kind(async_lsp::lsp_types::CodeActionKind::REFACTOR)
            .add_edit(fields_range, new_text)
            .build(),
    )
}

/// Create "Add default value" action for fields without defaults
fn create_add_default_value_action(
    uri: &Url,
    field: &Field,
) -> Option<async_lsp::lsp_types::CodeAction> {
    // Determine appropriate default value based on type
    let default_value = get_default_for_type(&field.field_type)?;

    let type_range = field.type_range.as_ref()?;

    // Insert after the type field
    let insert_position = Position {
        line: type_range.end.line,
        character: type_range.end.character,
    };

    let insert_text = format!(", \"default\": {}", default_value);

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Add default value for '{}'", field.name),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::REFACTOR)
        .add_insert(insert_position, insert_text)
        .build(),
    )
}

/// Get a sensible default value for an Avro type
fn get_default_for_type(avro_type: &AvroType) -> Option<String> {
    match avro_type {
        AvroType::Primitive(prim) => match prim {
            crate::schema::PrimitiveType::Null => Some("null".to_string()),
            crate::schema::PrimitiveType::Boolean => Some("false".to_string()),
            crate::schema::PrimitiveType::Int => Some("0".to_string()),
            crate::schema::PrimitiveType::Long => Some("0".to_string()),
            crate::schema::PrimitiveType::Float => Some("0.0".to_string()),
            crate::schema::PrimitiveType::Double => Some("0.0".to_string()),
            crate::schema::PrimitiveType::Bytes => Some("\"\"".to_string()),
            crate::schema::PrimitiveType::String => Some("\"\"".to_string()),
        },
        AvroType::Array(_) => Some("[]".to_string()),
        AvroType::Map(_) => Some("{}".to_string()),
        AvroType::Union(types) => {
            // For unions, use the first type's default
            types.first().and_then(get_default_for_type)
        }
        // For complex types (Record, Enum, Fixed) and TypeRefs, don't provide defaults
        // as they require more context
        _ => None,
    }
}

/// Get quick fix code actions from diagnostics
pub fn get_quick_fixes_from_diagnostics(
    schema: Option<&AvroSchema>,
    text: &str,
    uri: &Url,
    diagnostics: &[Diagnostic],
) -> Vec<CodeAction> {
    use crate::schema::SchemaError;

    tracing::debug!(
        "get_quick_fixes_from_diagnostics called with {} diagnostics",
        diagnostics.len()
    );
    let mut actions = Vec::new();

    for diagnostic in diagnostics {
        tracing::info!("Processing diagnostic: {}", diagnostic.message);
        tracing::info!("Diagnostic has data: {}", diagnostic.data.is_some());
        if let Some(ref data) = diagnostic.data {
            tracing::info!("Diagnostic data (raw): {:?}", data);
        }

        // Try to deserialize structured error data from diagnostic
        let structured_error: Option<SchemaError> = diagnostic.data.as_ref().and_then(|data| {
            let result = serde_json::from_value::<SchemaError>(data.clone());
            match &result {
                Ok(err) => tracing::info!("Successfully deserialized SchemaError: {:?}", err),
                Err(e) => tracing::warn!("Failed to deserialize SchemaError: {}", e),
            }
            result.ok()
        });

        tracing::info!("Structured error is Some: {}", structured_error.is_some());

        if let Some(error) = structured_error {
            // Use structured error data - no string parsing needed!
            tracing::info!("Found structured error data, matching on type...");

            match error {
                SchemaError::InvalidName {
                    name, suggested, ..
                } => {
                    tracing::debug!("Invalid name error: {} -> {:?}", name, suggested);
                    if let Some(schema) = schema
                        && let Some(fix) = create_fix_invalid_name_structured(
                            uri,
                            schema,
                            diagnostic,
                            &name,
                            suggested.as_deref(),
                        )
                    {
                        actions.push(fix);
                    }
                }
                SchemaError::InvalidNamespace {
                    namespace,
                    suggested,
                    ..
                } => {
                    tracing::debug!("Invalid namespace error: {} -> {:?}", namespace, suggested);
                    if let Some(schema) = schema
                        && let Some(fix) = create_fix_invalid_namespace_structured(
                            uri,
                            schema,
                            diagnostic,
                            &namespace,
                            suggested.as_deref(),
                        )
                    {
                        actions.push(fix);
                    }
                }
                SchemaError::DuplicateSymbol { symbol, .. } => {
                    tracing::debug!("Duplicate symbol error: {}", symbol);
                    if let Some(fix) = create_fix_duplicate_symbol(uri, text, diagnostic, &symbol) {
                        actions.push(fix);
                    }
                }
                SchemaError::InvalidPrimitiveType {
                    type_name,
                    suggested,
                    ..
                } => {
                    tracing::debug!(
                        "Invalid primitive type error: {} -> {:?}",
                        type_name,
                        suggested
                    );
                    // This doesn't need schema!
                    if let Some(fix) = create_fix_invalid_primitive_type(
                        uri,
                        diagnostic,
                        &type_name,
                        suggested.as_deref(),
                    ) {
                        actions.push(fix);
                    }
                }
                // IMPORTANT: More specific patterns MUST come before generic ones!
                // Check for specific decimal precision error first, before generic logical type error
                SchemaError::Custom { message, .. }
                    if message.contains("Decimal logical type requires")
                        && message.contains("precision") =>
                {
                    tracing::debug!("Missing decimal precision error");
                    if let Some(fix) = create_fix_missing_decimal_precision(uri, text, diagnostic) {
                        actions.push(fix);
                    }
                }
                SchemaError::Custom { message, .. }
                    if message
                        .contains("Duration logical type requires fixed size of 12 bytes") =>
                {
                    tracing::debug!("Invalid duration size error");
                    if let Some(fix) = create_fix_invalid_duration_size(uri, text, diagnostic) {
                        actions.push(fix);
                    }
                }
                SchemaError::Custom { message, .. }
                    if message.contains("logical type") && message.contains("requires") =>
                {
                    tracing::debug!("Logical type error");
                    if let Some(schema) = schema
                        && let Some(fix) = create_fix_logical_type(uri, schema, text, diagnostic)
                    {
                        actions.push(fix);
                    }
                }
                SchemaError::NestedUnion { .. } => {
                    tracing::debug!("Nested union error");
                    if let Some(fix) = create_fix_nested_union(uri, text, diagnostic) {
                        actions.push(fix);
                    }
                }
                SchemaError::DuplicateUnionType { type_signature, .. } => {
                    tracing::debug!("Duplicate union type error: {}", type_signature);
                    if let Some(fix) =
                        create_fix_duplicate_union_type(uri, text, diagnostic, &type_signature)
                    {
                        actions.push(fix);
                    }
                }
                SchemaError::MissingField { field } if field == "fields" => {
                    tracing::debug!("Missing fields error");
                    if let Some(fix) = create_fix_missing_fields(uri, text, diagnostic) {
                        actions.push(fix);
                    }
                }
                SchemaError::Custom { message, .. }
                    if message.contains("Default value") && message.contains("boolean") =>
                {
                    tracing::debug!("Invalid boolean default error");
                    if let Some(fix) = create_fix_invalid_boolean_default(uri, text, diagnostic) {
                        actions.push(fix);
                    }
                }
                SchemaError::Custom { message, .. }
                    if message.contains("Default value") && message.contains("array") =>
                {
                    tracing::debug!("Invalid array default error");
                    if let Some(fix) = create_fix_invalid_array_default(uri, text, diagnostic) {
                        actions.push(fix);
                    }
                }
                SchemaError::Custom { message, .. }
                    if message.contains("Default value") && message.contains("enum symbol") =>
                {
                    tracing::debug!("Invalid enum default error");
                    if let Some(fix) = create_fix_invalid_enum_default(uri, text, diagnostic) {
                        actions.push(fix);
                    }
                }
                SchemaError::Custom { message, .. }
                    if message.contains("Decimal scale")
                        && message.contains("cannot be greater than precision") =>
                {
                    tracing::debug!("Invalid decimal scale error");
                    // Offer multiple quick fixes for decimal scale
                    let fixes = create_fix_invalid_decimal_scale(uri, text, diagnostic);
                    actions.extend(fixes);
                }
                _ => {
                    tracing::debug!("No quick fix available for error type");
                }
            }
        } else {
            // Fallback to string parsing for backward compatibility or diagnostics without structured data
            tracing::warn!(
                "No structured error data for diagnostic: {}",
                diagnostic.message
            );
            tracing::debug!("Falling back to string parsing");
            let msg = diagnostic
                .message
                .strip_prefix("Validation error: ")
                .unwrap_or(&diagnostic.message);

            if let Some(remainder) = msg.strip_prefix("Invalid name '") {
                if let Some(name_end) = remainder.find('\'') {
                    let invalid_name = &remainder[..name_end];
                    if let Some(schema) = schema
                        && let Some(fix) =
                            create_fix_invalid_name(uri, schema, diagnostic, invalid_name)
                    {
                        actions.push(fix);
                    }
                }
            } else if let Some(remainder) = msg.strip_prefix("Invalid namespace '") {
                if let Some(ns_end) = remainder.find('\'') {
                    let invalid_namespace = &remainder[..ns_end];
                    if let Some(schema) = schema
                        && let Some(fix) =
                            create_fix_invalid_namespace(uri, schema, diagnostic, invalid_namespace)
                    {
                        actions.push(fix);
                    }
                }
            } else if msg.contains("logical type") && msg.contains("requires") {
                if let Some(schema) = schema
                    && let Some(fix) = create_fix_logical_type(uri, schema, text, diagnostic)
                {
                    actions.push(fix);
                }
            } else if msg == "Nested unions are not allowed" {
                tracing::debug!("Nested union error (string fallback)");
                if let Some(fix) = create_fix_nested_union(uri, text, diagnostic) {
                    actions.push(fix);
                }
            } else if let Some(remainder) = msg.strip_prefix("Duplicate type in union: ") {
                tracing::debug!(
                    "Duplicate union type error (string fallback): {}",
                    remainder
                );
                if let Some(fix) = create_fix_duplicate_union_type(uri, text, diagnostic, remainder)
                {
                    actions.push(fix);
                }
            } else if let Some(remainder) = msg.strip_prefix("Duplicate symbol '")
                && let Some(symbol_end) = remainder.find('\'')
            {
                let duplicate_symbol = &remainder[..symbol_end];
                if let Some(fix) =
                    create_fix_duplicate_symbol(uri, text, diagnostic, duplicate_symbol)
                {
                    actions.push(fix);
                }
            }
        }
    }

    actions
}

/// Create a quick fix for invalid name errors using structured error data
fn create_fix_invalid_name_structured(
    uri: &Url,
    _schema: &AvroSchema,
    diagnostic: &Diagnostic,
    invalid_name: &str,
    suggested_name: Option<&str>,
) -> Option<CodeAction> {
    // Use suggested name from error if available, otherwise generate one
    let fixed_name = suggested_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| fix_invalid_name(invalid_name));

    // Try to use range from error first, otherwise search for it
    let name_range = diagnostic.range;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Fix invalid name: '{}' → '{}'", invalid_name, fixed_name),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .add_edit(name_range, format!("\"{}\"", fixed_name))
        .build(),
    )
}

/// Create a quick fix for invalid namespace errors using structured error data
fn create_fix_invalid_namespace_structured(
    uri: &Url,
    _schema: &AvroSchema,
    diagnostic: &Diagnostic,
    invalid_namespace: &str,
    suggested_namespace: Option<&str>,
) -> Option<CodeAction> {
    // Use suggested namespace from error if available, otherwise generate one
    let fixed_namespace = suggested_namespace
        .map(|s| s.to_string())
        .unwrap_or_else(|| fix_invalid_namespace(invalid_namespace));

    // Try to use range from error first, otherwise search for it
    let namespace_range = diagnostic.range;

    let new_text = if fixed_namespace.is_empty() {
        // If namespace becomes empty, remove the field entirely
        String::new()
    } else {
        format!("\"{}\"", fixed_namespace)
    };

    let title = if fixed_namespace.is_empty() {
        format!("Remove invalid namespace '{}'", invalid_namespace)
    } else {
        format!(
            "Fix invalid namespace: '{}' → '{}'",
            invalid_namespace, fixed_namespace
        )
    };

    Some(
        CodeActionBuilder::new(uri.clone(), title)
            .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
            .with_diagnostics(vec![diagnostic.clone()])
            .add_edit(namespace_range, new_text)
            .build(),
    )
}

/// Create a quick fix for invalid primitive type errors (typos)
fn create_fix_invalid_primitive_type(
    uri: &Url,
    diagnostic: &Diagnostic,
    invalid_type: &str,
    suggested_type: Option<&str>,
) -> Option<CodeAction> {
    // Must have a suggestion to create a fix
    let fixed_type = suggested_type?;

    // Use the diagnostic range (should be on the "type" field)
    let type_range = diagnostic.range;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Fix typo: '{}' → '{}'", invalid_type, fixed_type),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .add_edit(type_range, format!("\"{}\"", fixed_type))
        .build(),
    )
}

/// Create a quick fix for nested union errors
/// Flattens nested union like [["null", "string"]] to ["null", "string"]
fn create_fix_nested_union(uri: &Url, text: &str, diagnostic: &Diagnostic) -> Option<CodeAction> {
    use async_lsp::lsp_types::{Position, Range};

    tracing::info!(
        "create_fix_nested_union called for diagnostic at range {:?}",
        diagnostic.range
    );

    // Search through the entire text for nested union pattern [[...]]
    let lines: Vec<&str> = text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        // Look for [[ pattern
        if let Some(outer_start) = line.find("[[") {
            tracing::info!("Found [[ at line {}, col {}", line_idx, outer_start);
            // Need to extract the complete [[ ]] array
            // Start from [[ and count brackets to find the matching ]]
            let from_bracket = &line[outer_start..];
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

            if end_pos == 0 {
                tracing::info!("Could not find matching ]]");
                continue;
            }

            let json_str = &from_bracket[..end_pos];
            tracing::info!("Extracted JSON: {}", json_str);

            // Try to parse as JSON value
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
                tracing::info!("Parsed JSON successfully");
                // Check if it's an array containing an array
                if let Some(outer_arr) = value.as_array()
                    && outer_arr.len() == 1
                    && let Some(inner_arr) = outer_arr[0].as_array()
                {
                    tracing::info!("Found nested union pattern!");
                    // Found nested union! Flatten it
                    let flattened = serde_json::to_string(inner_arr).ok()?;

                    // Calculate the range to replace
                    let col_start = outer_start as u32;
                    let col_end = (outer_start + end_pos) as u32;

                    let replace_range = Range {
                        start: Position {
                            line: line_idx as u32,
                            character: col_start,
                        },
                        end: Position {
                            line: line_idx as u32,
                            character: col_end,
                        },
                    };

                    tracing::info!("Creating action with range {:?}", replace_range);

                    return Some(
                        CodeActionBuilder::new(uri.clone(), "Flatten nested union".to_string())
                            .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                            .with_diagnostics(vec![diagnostic.clone()])
                            .preferred()
                            .add_edit(replace_range, flattened)
                            .build(),
                    );
                } else {
                    tracing::info!(
                        "Not a nested union pattern: outer_arr len = {:?}, has inner array = {:?}",
                        value.as_array().map(|a| a.len()),
                        value
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(|v| v.as_array())
                            .is_some()
                    );
                }
            } else {
                tracing::info!("Failed to parse JSON");
            }
        }
    }

    tracing::info!("No nested union fix created, returning None");
    None
}

/// Create a quick fix for duplicate union type errors
/// Removes duplicate types from union like ["null", "string", "null"] → ["null", "string"]
fn create_fix_duplicate_union_type(
    uri: &Url,
    text: &str,
    _diagnostic: &Diagnostic,
    duplicate_type: &str,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{Position, Range};

    // Search through the text for union arrays that contain the duplicate type
    let lines: Vec<&str> = text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        // Look for arrays (unions start with [)
        if let Some(array_start) = line.find('[') {
            // Extract the complete array using bracket matching
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

            if end_pos == 0 {
                continue;
            }

            let json_str = &from_bracket[..end_pos];

            // Try to parse as JSON array
            if let Ok(serde_json::Value::Array(arr)) =
                serde_json::from_str::<serde_json::Value>(json_str)
            {
                // Check if this array contains the duplicate type
                let type_strings: Vec<String> = arr
                    .iter()
                    .filter_map(|v| {
                        match v {
                            serde_json::Value::String(s) => Some(s.clone()),
                            serde_json::Value::Object(obj) if obj.contains_key("type") => {
                                // Complex type - serialize it for comparison
                                serde_json::to_string(v).ok()
                            }
                            _ => None,
                        }
                    })
                    .collect();

                // Count occurrences of the duplicate type (case-insensitive comparison)
                let duplicate_lower = duplicate_type.to_lowercase();
                let count = type_strings
                    .iter()
                    .filter(|t| t.to_lowercase() == duplicate_lower)
                    .count();

                if count > 1 {
                    // Found the union with duplicates! Remove duplicates
                    let mut seen = std::collections::HashSet::new();
                    let deduplicated: Vec<&serde_json::Value> = arr
                        .iter()
                        .filter(|v| {
                            let type_str = match v {
                                serde_json::Value::String(s) => s.clone(),
                                _ => serde_json::to_string(v).unwrap_or_default(),
                            };
                            seen.insert(type_str)
                        })
                        .collect();

                    // Convert back to JSON array
                    let dedup_json: Vec<serde_json::Value> =
                        deduplicated.iter().map(|v| (*v).clone()).collect();
                    let fixed = serde_json::to_string(&dedup_json).ok()?;

                    let col_start = array_start as u32;
                    let col_end = (array_start + end_pos) as u32;

                    let replace_range = Range {
                        start: Position {
                            line: line_idx as u32,
                            character: col_start,
                        },
                        end: Position {
                            line: line_idx as u32,
                            character: col_end,
                        },
                    };

                    return Some(
                        CodeActionBuilder::new(
                            uri.clone(),
                            format!("Remove duplicate '{}' from union", duplicate_type),
                        )
                        .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                        .preferred()
                        .add_edit(replace_range, fixed)
                        .build(),
                    );
                }
            }
        }
    }

    None
}

/// Create a quick fix for missing fields array in record
/// Adds an empty "fields": [] to the record definition
fn create_fix_missing_fields(
    uri: &Url,
    text: &str,
    _diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{Position, Range};

    // Find the record definition that's missing fields
    // Look for a record object with "type": "record" and "name" but no "fields"
    let lines: Vec<&str> = text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        // Look for lines containing "type": "record" or "name":
        if line.contains("\"type\"") && line.contains("\"record\"") {
            // Found a record definition, now find where to insert fields
            // We need to find the end of the object (before the closing })

            // Look ahead to find the closing brace
            for (search_idx, search_line) in lines.iter().enumerate().skip(line_idx) {
                if let Some(brace_pos) = search_line.rfind('}') {
                    // Found closing brace - insert before it
                    // Check if this line only has the brace or has other content
                    let before_brace = &search_line[..brace_pos].trim();

                    // Insert fields before the closing brace
                    let indent = search_line
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>();
                    let new_text = if before_brace.is_empty() {
                        // Closing brace is on its own line
                        format!("{}  \"fields\": []\n", indent)
                    } else {
                        // Closing brace is after other content - add comma
                        ",\n{}  \"fields\": []".to_string()
                    };

                    let col = brace_pos as u32;
                    let replace_range = Range {
                        start: Position {
                            line: search_idx as u32,
                            character: col,
                        },
                        end: Position {
                            line: search_idx as u32,
                            character: col,
                        },
                    };

                    return Some(
                        CodeActionBuilder::new(uri.clone(), "Add empty fields array".to_string())
                            .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                            .preferred()
                            .add_edit(replace_range, new_text)
                            .build(),
                    );
                }
            }
        }
    }

    None
}

/// Create a quick fix for invalid boolean default values
/// Changes invalid values like "yes" to true or false
fn create_fix_invalid_boolean_default(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{Position, Range};

    // Search for "default": "something" or "default": 123 near the diagnostic range
    let lines: Vec<&str> = text.lines().collect();
    let start_line = diagnostic.range.start.line as usize;
    let end_line = (diagnostic.range.end.line as usize).min(lines.len());

    for line_idx in start_line..=end_line {
        if line_idx >= lines.len() {
            break;
        }

        let line = lines[line_idx];

        // Look for "default": followed by a value
        if let Some(default_pos) = line.find("\"default\"") {
            // Find the colon after "default"
            if let Some(colon_pos) = line[default_pos..].find(':') {
                let after_colon = &line[default_pos + colon_pos + 1..];

                // Skip whitespace
                let trimmed = after_colon.trim_start();
                let ws_offset = after_colon.len() - trimmed.len();

                // Find the value (could be quoted string, number, etc.)
                // Look for the value until comma or end
                let value_end = trimmed
                    .find(',')
                    .or_else(|| trimmed.find('}'))
                    .unwrap_or(trimmed.len());
                let value = trimmed[..value_end].trim();

                // Calculate range of the value
                let value_start_col = default_pos + colon_pos + 1 + ws_offset;
                let value_end_col = value_start_col + value.len();

                let value_range = Range {
                    start: Position {
                        line: line_idx as u32,
                        character: value_start_col as u32,
                    },
                    end: Position {
                        line: line_idx as u32,
                        character: value_end_col as u32,
                    },
                };

                // Determine the correct boolean value
                // For strings like "yes", "true", "1" -> true
                // For "no", "false", "0" -> false
                // Default to false for safety
                let lower_value = value.to_lowercase().trim_matches('"').to_string();
                let correct_value = if lower_value.contains("true")
                    || lower_value.contains("yes")
                    || lower_value == "1"
                {
                    "true"
                } else {
                    "false"
                };

                return Some(
                    CodeActionBuilder::new(
                        uri.clone(),
                        format!("Fix invalid boolean default: change to {}", correct_value),
                    )
                    .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                    .with_diagnostics(vec![diagnostic.clone()])
                    .preferred()
                    .add_edit(value_range, correct_value.to_string())
                    .build(),
                );
            }
        }
    }

    None
}

/// Create a quick fix for invalid array default values
/// Changes invalid values like "string" or 123 to []
fn create_fix_invalid_array_default(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{Position, Range};

    // Search for "default": <not-an-array> near the diagnostic range
    let lines: Vec<&str> = text.lines().collect();
    let start_line = diagnostic.range.start.line as usize;
    let end_line = (diagnostic.range.end.line as usize).min(lines.len());

    for line_idx in start_line..=end_line {
        if line_idx >= lines.len() {
            break;
        }

        let line = lines[line_idx];

        // Look for "default": followed by a value
        if let Some(default_pos) = line.find("\"default\"") {
            // Find the colon after "default"
            if let Some(colon_pos) = line[default_pos..].find(':') {
                let after_colon = &line[default_pos + colon_pos + 1..];

                // Skip whitespace
                let trimmed = after_colon.trim_start();
                let ws_offset = after_colon.len() - trimmed.len();

                // Find the value
                let value_end = trimmed
                    .find(',')
                    .or_else(|| trimmed.find('}'))
                    .unwrap_or(trimmed.len());
                let value = trimmed[..value_end].trim();

                // Calculate range of the value
                let value_start_col = default_pos + colon_pos + 1 + ws_offset;
                let value_end_col = value_start_col + value.len();

                let value_range = Range {
                    start: Position {
                        line: line_idx as u32,
                        character: value_start_col as u32,
                    },
                    end: Position {
                        line: line_idx as u32,
                        character: value_end_col as u32,
                    },
                };

                return Some(
                    CodeActionBuilder::new(
                        uri.clone(),
                        "Fix invalid array default: change to []".to_string(),
                    )
                    .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                    .with_diagnostics(vec![diagnostic.clone()])
                    .preferred()
                    .add_edit(value_range, "[]".to_string())
                    .build(),
                );
            }
        }
    }

    None
}

/// Create a quick fix for invalid enum default values
/// Adds the missing symbol to the enum's symbols array
fn create_fix_invalid_enum_default(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{Position, Range};

    // Parse the error message to extract the invalid value: "Default value 'YELLOW' is not a valid enum symbol"
    let msg = &diagnostic.message;
    let invalid_value = if let Some(start) = msg.find('\'') {
        if let Some(end) = msg[start + 1..].find('\'') {
            &msg[start + 1..start + 1 + end]
        } else {
            return None;
        }
    } else {
        return None;
    };

    // Search for "symbols": [...] to find the symbols array and add the missing symbol
    let lines: Vec<&str> = text.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        if line.contains("\"symbols\"") {
            // Try to extract the symbols array
            if let Some(bracket_start) = line.find('[')
                && let Some(bracket_end) = line.find(']')
            {
                let array_str = &line[bracket_start..=bracket_end];

                // Parse as JSON array to verify it's valid
                if let Ok(serde_json::Value::Array(arr)) =
                    serde_json::from_str::<serde_json::Value>(array_str)
                {
                    let current_symbols: Vec<String> = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();

                    // Check if the invalid value is already in the symbols (shouldn't be, but just in case)
                    if current_symbols.iter().any(|s| s == invalid_value) {
                        return None;
                    }

                    // Create new symbols array with the missing symbol added at the end
                    let mut new_symbols = current_symbols.clone();
                    new_symbols.push(invalid_value.to_string());

                    // Serialize back to JSON
                    let new_array_str = serde_json::to_string(&new_symbols).ok()?;

                    // Calculate the range to replace (the entire symbols array)
                    let range = Range {
                        start: Position {
                            line: line_idx as u32,
                            character: bracket_start as u32,
                        },
                        end: Position {
                            line: line_idx as u32,
                            character: (bracket_end + 1) as u32,
                        },
                    };

                    return Some(
                        CodeActionBuilder::new(
                            uri.clone(),
                            format!("Add '{}' to enum symbols", invalid_value),
                        )
                        .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                        .with_diagnostics(vec![diagnostic.clone()])
                        .preferred()
                        .add_edit(range, new_array_str)
                        .build(),
                    );
                }
            }
        }
    }

    None
}

/// Create quick fixes for invalid decimal scale errors
/// Offers two options: reduce scale to match precision, or increase precision to match scale
fn create_fix_invalid_decimal_scale(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Vec<CodeAction> {
    use async_lsp::lsp_types::{CodeActionKind, Position, Range, TextEdit, WorkspaceEdit};
    use std::collections::HashMap;

    let mut fixes = Vec::new();

    // Parse the error message: "Decimal scale (10) cannot be greater than precision (5)"
    let msg = &diagnostic.message;

    // Extract scale and precision values
    let scale_value = if let Some(start) = msg.find("scale (") {
        let after = &msg[start + 7..];
        if let Some(end) = after.find(')') {
            after[..end].parse::<u32>().ok()
        } else {
            None
        }
    } else {
        None
    };

    let precision_value = if let Some(start) = msg.find("precision (") {
        let after = &msg[start + 11..];
        if let Some(end) = after.find(')') {
            after[..end].parse::<u32>().ok()
        } else {
            None
        }
    } else {
        None
    };

    // Both values must be present after the check above
    let (scale, precision) = match (scale_value, precision_value) {
        (Some(s), Some(p)) => (s, p),
        _ => return fixes,
    };

    // Search for "scale": value and "precision": value in the text
    let lines: Vec<&str> = text.lines().collect();
    let start_line = diagnostic.range.start.line as usize;
    let end_line = (diagnostic.range.end.line as usize).min(lines.len());

    let mut scale_range: Option<Range> = None;
    let mut precision_range: Option<Range> = None;

    for line_idx in start_line..=end_line {
        if line_idx >= lines.len() {
            break;
        }

        let line = lines[line_idx];

        // Look for "scale": followed by a number
        if scale_range.is_none()
            && line.contains("\"scale\"")
            && let Some(scale_pos) = line.find("\"scale\"")
            && let Some(colon_pos) = line[scale_pos..].find(':')
        {
            let after_colon = &line[scale_pos + colon_pos + 1..];
            let trimmed = after_colon.trim_start();
            let ws_offset = after_colon.len() - trimmed.len();

            // Find the number value
            let value_end = trimmed
                .find(',')
                .or_else(|| trimmed.find('}'))
                .or_else(|| trimmed.find('\n'))
                .unwrap_or(trimmed.len());
            let value = trimmed[..value_end].trim();

            let value_start_col = scale_pos + colon_pos + 1 + ws_offset;
            let value_end_col = value_start_col + value.len();

            scale_range = Some(Range {
                start: Position {
                    line: line_idx as u32,
                    character: value_start_col as u32,
                },
                end: Position {
                    line: line_idx as u32,
                    character: value_end_col as u32,
                },
            });
        }

        // Look for "precision": followed by a number
        if precision_range.is_none()
            && line.contains("\"precision\"")
            && let Some(prec_pos) = line.find("\"precision\"")
            && let Some(colon_pos) = line[prec_pos..].find(':')
        {
            let after_colon = &line[prec_pos + colon_pos + 1..];
            let trimmed = after_colon.trim_start();
            let ws_offset = after_colon.len() - trimmed.len();

            // Find the number value
            let value_end = trimmed
                .find(',')
                .or_else(|| trimmed.find('}'))
                .or_else(|| trimmed.find('\n'))
                .unwrap_or(trimmed.len());
            let value = trimmed[..value_end].trim();

            let value_start_col = prec_pos + colon_pos + 1 + ws_offset;
            let value_end_col = value_start_col + value.len();

            precision_range = Some(Range {
                start: Position {
                    line: line_idx as u32,
                    character: value_start_col as u32,
                },
                end: Position {
                    line: line_idx as u32,
                    character: value_end_col as u32,
                },
            });
        }

        // Break if we found both
        if scale_range.is_some() && precision_range.is_some() {
            break;
        }
    }

    // Option 1: Set scale to match precision (reduce scale)
    if let Some(range) = scale_range {
        let mut changes = HashMap::new();
        changes.insert(
            uri.clone(),
            vec![TextEdit {
                range,
                new_text: precision.to_string(),
            }],
        );

        fixes.push(CodeAction {
            title: format!("Set scale to {} (match precision)", precision),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }),
            is_preferred: Some(true),
            ..Default::default()
        });
    }

    // Option 2: Set precision to match scale (increase precision)
    if let Some(range) = precision_range {
        let mut changes = HashMap::new();
        changes.insert(
            uri.clone(),
            vec![TextEdit {
                range,
                new_text: scale.to_string(),
            }],
        );

        fixes.push(CodeAction {
            title: format!("Set precision to {} (match scale)", scale),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }),
            is_preferred: Some(false),
            ..Default::default()
        });
    }

    fixes
}

/// Create a quick fix for missing decimal precision attribute
/// Adds a "precision" field with a reasonable default value
fn create_fix_missing_decimal_precision(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{Position, Range};

    // Search for "logicalType": "decimal" and add precision after it
    let lines: Vec<&str> = text.lines().collect();
    let start_line = diagnostic.range.start.line as usize;
    let end_line = (diagnostic.range.end.line as usize).min(lines.len());

    for line_idx in start_line..=end_line {
        if line_idx >= lines.len() {
            break;
        }

        let line = lines[line_idx];

        // Look for "logicalType": "decimal"
        if line.contains("\"logicalType\"") && line.contains("\"decimal\"") {
            // Find the end of this line to insert precision after it
            // We want to insert after the comma or before the closing brace

            // Determine the indentation level
            let indent = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();

            // Find position at end of line (before newline)
            let insert_position = Position {
                line: line_idx as u32,
                character: line.len() as u32,
            };

            // Calculate a reasonable default precision based on the fixed size
            // For fixed type, precision = floor(log10(2^(8*size - 1)))
            // For size=16: max ~38 digits, let's default to 10 which is reasonable
            let default_precision = 10;

            let insert_text = format!(",\n{}\"precision\": {}", indent, default_precision);
            let insert_position_range = Range {
                start: insert_position,
                end: insert_position,
            };

            return Some(
                CodeActionBuilder::new(
                    uri.clone(),
                    format!("Add precision attribute (default: {})", default_precision),
                )
                .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                .with_diagnostics(vec![diagnostic.clone()])
                .preferred()
                .add_edit(insert_position_range, insert_text)
                .build(),
            );
        }
    }

    None
}

/// Create a quick fix for invalid duration size errors
/// Duration logical type requires fixed size of exactly 12 bytes
fn create_fix_invalid_duration_size(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    use async_lsp::lsp_types::{Position, Range};

    // Search for "size": <number> within the diagnostic range and replace it with 12
    let lines: Vec<&str> = text.lines().collect();
    let start_line = diagnostic.range.start.line as usize;
    let end_line = (diagnostic.range.end.line as usize).min(lines.len());

    for line_idx in start_line..=end_line {
        if line_idx >= lines.len() {
            break;
        }

        let line = lines[line_idx];

        // Look for "size": followed by a number
        if line.contains("\"size\"")
            && let Some(size_pos) = line.find("\"size\"")
            && let Some(colon_pos) = line[size_pos..].find(':')
        {
            let after_colon = &line[size_pos + colon_pos + 1..];
            let trimmed = after_colon.trim_start();
            let ws_offset = after_colon.len() - trimmed.len();

            // Find the number value
            let value_end = trimmed
                .find(',')
                .or_else(|| trimmed.find('}'))
                .or_else(|| trimmed.find('\n'))
                .unwrap_or(trimmed.len());
            let value = trimmed[..value_end].trim();

            // Calculate the range for the size value
            let value_start_col = size_pos + colon_pos + 1 + ws_offset;
            let value_end_col = value_start_col + value.len();

            let range = Range {
                start: Position {
                    line: line_idx as u32,
                    character: value_start_col as u32,
                },
                end: Position {
                    line: line_idx as u32,
                    character: value_end_col as u32,
                },
            };

            return Some(
                CodeActionBuilder::new(
                    uri.clone(),
                    "Set size to 12 (required for duration)".to_string(),
                )
                .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
                .with_diagnostics(vec![diagnostic.clone()])
                .preferred()
                .add_edit(range, "12".to_string())
                .build(),
            );
        }
    }

    None
}

/// Create a quick fix for invalid name errors
fn create_fix_invalid_name(
    uri: &Url,
    schema: &AvroSchema,
    diagnostic: &Diagnostic,
    invalid_name: &str,
) -> Option<CodeAction> {
    // Generate a valid name by:
    // 1. If starts with digit, prepend underscore
    // 2. Replace invalid characters with underscores
    let fixed_name = fix_invalid_name(invalid_name);

    // Find the name in the schema to get the exact position
    let name_range = find_name_range_in_schema(schema, invalid_name, diagnostic.range)?;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Fix invalid name: '{}' → '{}'", invalid_name, fixed_name),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .preferred()
        .add_edit(name_range, format!("\"{}\"", fixed_name))
        .build(),
    )
}

/// Create a quick fix for invalid namespace errors
fn create_fix_invalid_namespace(
    uri: &Url,
    schema: &AvroSchema,
    diagnostic: &Diagnostic,
    invalid_namespace: &str,
) -> Option<CodeAction> {
    // Fix namespace by filtering out invalid segments
    let fixed_namespace = fix_invalid_namespace(invalid_namespace);

    if fixed_namespace.is_empty() {
        // Offer to remove the namespace field entirely
        return create_remove_namespace_action(uri, schema, diagnostic);
    }

    // Find the namespace value in the schema
    let namespace_range = find_namespace_range_in_schema(schema, diagnostic.range)?;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!(
                "Fix invalid namespace: '{}' → '{}'",
                invalid_namespace, fixed_namespace
            ),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .preferred()
        .add_edit(namespace_range, format!("\"{}\"", fixed_namespace))
        .build(),
    )
}

/// Create action to remove invalid namespace field
fn create_remove_namespace_action(
    uri: &Url,
    _schema: &AvroSchema,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    // For now, just suggest to fix the namespace manually
    // A more sophisticated implementation would find and remove the entire field
    Some(
        CodeActionBuilder::new(
            uri.clone(),
            "Replace with valid namespace placeholder".to_string(),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .add_edit(diagnostic.range, "\"valid_namespace\"".to_string())
        .build(),
    )
}

/// Create a quick fix for logical type errors
fn create_fix_logical_type(
    uri: &Url,
    _schema: &AvroSchema,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    // Parse the error message to extract the required type
    // e.g., "Invalid logical type 'uuid' for type int - requires string"
    let msg = &diagnostic.message;
    let required_type = if msg.contains("requires string") {
        "string"
    } else if msg.contains("requires int") {
        "int"
    } else if msg.contains("requires long") {
        "long"
    } else if msg.contains("requires bytes") {
        "bytes"
    } else if msg.contains("requires fixed") {
        return None; // Fixed types are more complex
    } else {
        return None;
    };

    // Find the type field in the object
    let type_range = find_primitive_type_range(text, diagnostic.range)?;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Change base type to '{}'", required_type),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .preferred()
        .add_edit(type_range, format!("\"{}\"", required_type))
        .build(),
    )
}

/// Create a quick fix for duplicate symbols
fn create_fix_duplicate_symbol(
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
    duplicate_symbol: &str,
) -> Option<CodeAction> {
    // Find the duplicate symbol in the symbols array and remove it
    let (_first_pos, second_pos) = find_duplicate_symbol_positions(text, duplicate_symbol)?;

    // Remove the second occurrence (including comma)
    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Remove duplicate symbol '{}'", duplicate_symbol),
        )
        .with_kind(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .preferred()
        .add_edit(second_pos, String::new())
        .build(),
    )
}

/// Fix an invalid name according to Avro rules
fn fix_invalid_name(name: &str) -> String {
    // If already valid, return as-is
    if AVRO_NAME_REGEX.is_match(name) {
        return name.to_string();
    }

    let mut fixed = String::new();
    let mut has_valid_char = false;

    for (i, ch) in name.chars().enumerate() {
        if i == 0 {
            // First character must be letter or underscore
            if ch.is_ascii_alphabetic() || ch == '_' {
                fixed.push(ch);
                has_valid_char = true;
            } else if ch.is_ascii_digit() {
                // Prepend underscore if starts with digit
                fixed.push('_');
                fixed.push(ch);
                has_valid_char = true;
            } else {
                // Skip invalid chars at start, we'll add underscore if needed
            }
        } else {
            // Subsequent characters can be letter, digit, or underscore
            if ch.is_ascii_alphanumeric() || ch == '_' {
                fixed.push(ch);
                has_valid_char = true;
            } else {
                // Replace invalid char with underscore (but avoid consecutive underscores)
                if !fixed.ends_with('_') && has_valid_char {
                    fixed.push('_');
                }
            }
        }
    }

    // Ensure we start with valid character
    if fixed.is_empty() || !AVRO_NAME_REGEX.is_match(&fixed) {
        if fixed.is_empty() {
            fixed = "_".to_string();
        } else if let Some(first_char) = fixed.chars().next()
            && !first_char.is_ascii_alphabetic()
            && !fixed.starts_with('_')
        {
            fixed = format!("_{}", fixed);
        }
    }

    fixed
}

/// Fix an invalid namespace by removing or fixing invalid segments
fn fix_invalid_namespace(namespace: &str) -> String {
    let segments: Vec<&str> = namespace.split('.').collect();

    let valid_segments: Vec<String> = segments
        .iter()
        .filter_map(|seg| {
            if AVRO_NAME_REGEX.is_match(seg) {
                // Already valid
                Some(seg.to_string())
            } else {
                // Check if segment has any valid characters
                let has_letter = seg.chars().any(|c| c.is_ascii_alphabetic());
                if has_letter {
                    // Try to fix it if it has letters
                    let fixed = fix_invalid_name(seg);
                    if AVRO_NAME_REGEX.is_match(&fixed) {
                        Some(fixed)
                    } else {
                        None
                    }
                } else {
                    // Skip segments with no letters (pure numbers, symbols)
                    None
                }
            }
        })
        .collect();

    valid_segments.join(".")
}

/// Find the range of a name value in the schema
fn find_name_range_in_schema(
    _schema: &AvroSchema,
    _name: &str,
    diagnostic_range: Range,
) -> Option<Range> {
    // Use diagnostic range as a hint - the name should be near there
    // For simplicity, we'll use the diagnostic range itself
    // A more sophisticated implementation would parse the JSON to find exact positions
    Some(diagnostic_range)
}

/// Find the range of a namespace value in the schema
fn find_namespace_range_in_schema(_schema: &AvroSchema, diagnostic_range: Range) -> Option<Range> {
    // Use diagnostic range directly
    Some(diagnostic_range)
}

/// Find the range of the "type" value in a primitive object
fn find_primitive_type_range(text: &str, diagnostic_range: Range) -> Option<Range> {
    // Search within the diagnostic range area for "type": "..."
    let lines: Vec<&str> = text.lines().collect();

    let start_line = diagnostic_range.start.line as usize;
    let end_line = (diagnostic_range.end.line as usize).min(lines.len());

    for line_num in start_line..=end_line {
        if line_num >= lines.len() {
            break;
        }

        let line = lines[line_num];

        // Look for "type": "int" or similar
        if let Some(type_pos) = line.find("\"type\"") {
            // Find the value after the colon
            if let Some(colon_pos) = line[type_pos..].find(':') {
                let after_colon = &line[type_pos + colon_pos + 1..];
                if let Some(quote_start) = after_colon.find('"')
                    && let Some(quote_end) = after_colon[quote_start + 1..].find('"')
                {
                    let value_start = type_pos + colon_pos + 1 + quote_start;
                    let value_end = value_start + quote_end + 2; // Include both quotes

                    return Some(Range {
                        start: Position {
                            line: line_num as u32,
                            character: value_start as u32,
                        },
                        end: Position {
                            line: line_num as u32,
                            character: value_end as u32,
                        },
                    });
                }
            }
        }
    }

    None
}

/// Find positions of duplicate symbols in the symbols array
fn find_duplicate_symbol_positions(text: &str, symbol: &str) -> Option<(Range, Range)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut positions = Vec::new();

    let search_pattern = format!("\"{}\"", symbol);

    for (line_num, line) in lines.iter().enumerate() {
        let mut search_start = 0;
        while let Some(pos) = line[search_start..].find(&search_pattern) {
            let absolute_pos = search_start + pos;
            let mut start_pos = Position {
                line: line_num as u32,
                character: absolute_pos as u32,
            };
            let mut end_pos = Position {
                line: line_num as u32,
                character: (absolute_pos + search_pattern.len()) as u32,
            };

            // Check if we need to include the comma
            if let Some(comma_pos) = line[absolute_pos + search_pattern.len()..].find(',') {
                // Include comma and any trailing spaces
                end_pos.character += comma_pos as u32 + 1;

                // Skip trailing spaces
                let after_comma = &line[(absolute_pos + search_pattern.len() + comma_pos + 1)..];
                let spaces = after_comma
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .count();
                end_pos.character += spaces as u32;
            } else {
                // Check for preceding comma
                let before_match = &line[..absolute_pos];
                if let Some(comma_pos) = before_match.rfind(',') {
                    // Include preceding comma
                    start_pos = Position {
                        line: line_num as u32,
                        character: comma_pos as u32,
                    };
                }
            }

            positions.push(Range {
                start: start_pos,
                end: end_pos,
            });

            search_start = absolute_pos + search_pattern.len();
        }
    }

    if positions.len() >= 2 {
        Some((positions[0], positions[1]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AvroParser;

    #[test]
    fn test_make_nullable_primitive_type() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the "string" type
        let position = Position {
            line: 4,
            character: 30,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        // Find the "Make nullable" action
        let make_nullable = actions
            .iter()
            .find(|a| a.title.contains("Make") && a.title.contains("nullable"));

        assert!(
            make_nullable.is_some(),
            "Should have 'Make nullable' action"
        );
        let action = make_nullable.unwrap();

        // Check the edit
        let edit = action.edit.as_ref().expect("Should have edit");
        let changes = edit.changes.as_ref().expect("Should have changes");
        let file_edits = changes.get(&uri).expect("Should have edits for file");

        assert_eq!(file_edits.len(), 1, "Should have one edit");
        let text_edit = &file_edits[0];

        // The new text should be ["null", "string"], NOT ["null", `string`]
        assert_eq!(text_edit.new_text, r#"["null", "string"]"#);
        assert!(
            !text_edit.new_text.contains('`'),
            "Should not contain backticks"
        );
    }

    #[test]
    fn test_make_nullable_complex_type() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "address", "type": {"type": "record", "name": "Address", "fields": []}}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the address field type
        let position = Position {
            line: 4,
            character: 40,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");
    }

    #[test]
    fn test_make_nullable_type_reference() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "int"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on "int"
        let position = Position {
            line: 4,
            character: 26,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        let make_nullable = actions
            .iter()
            .find(|a| a.title.contains("Make") && a.title.contains("nullable"));

        assert!(
            make_nullable.is_some(),
            "Should have 'Make nullable' action"
        );
        let action = make_nullable.unwrap();

        let edit = action.edit.as_ref().expect("Should have edit");
        let changes = edit.changes.as_ref().expect("Should have changes");
        let file_edits = changes.get(&uri).expect("Should have edits for file");

        let text_edit = &file_edits[0];
        assert_eq!(text_edit.new_text, r#"["null", "int"]"#);
    }

    #[test]
    fn test_no_make_nullable_on_union() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "email", "type": ["null", "string"]}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the union type
        let position = Position {
            line: 4,
            character: 35,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        // Should not offer "Make nullable" for fields that are already unions
        if !actions.is_empty() {
            let make_nullable = actions
                .iter()
                .find(|a| a.title.contains("Make") && a.title.contains("nullable"));

            assert!(
                make_nullable.is_none(),
                "Should not offer 'Make nullable' for union types"
            );
        }
    }

    #[test]
    fn test_sort_fields_alphabetically() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "zipcode", "type": "string"},
    {"name": "age", "type": "int"},
    {"name": "name", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the record definition
        let position = Position {
            line: 2,
            character: 12,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        let sort_action = actions
            .iter()
            .find(|a| a.title == "Sort fields alphabetically");

        assert!(
            sort_action.is_some(),
            "Should have 'Sort fields alphabetically' action"
        );
    }

    #[test]
    fn test_no_sort_action_when_already_sorted() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "age", "type": "int"},
    {"name": "name", "type": "string"},
    {"name": "zipcode", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        let position = Position {
            line: 2,
            character: 12,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        if !actions.is_empty() {
            let sort_action = actions
                .iter()
                .find(|a| a.title == "Sort fields alphabetically");

            assert!(
                sort_action.is_none(),
                "Should not have sort action when fields are already sorted"
            );
        }
    }

    #[test]
    fn test_add_default_value_to_field() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "age", "type": "int"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the field (on "age")
        let position = Position {
            line: 4,
            character: 15,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        let add_default = actions
            .iter()
            .find(|a| a.title.contains("Add default value"));

        assert!(
            add_default.is_some(),
            "Should have 'Add default value' action"
        );
    }

    #[test]
    fn test_no_add_default_when_default_exists() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "age", "type": "int", "default": 0}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        let position = Position {
            line: 4,
            character: 15,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        if !actions.is_empty() {
            let add_default = actions
                .iter()
                .find(|a| a.title.contains("Add default value"));

            assert!(
                add_default.is_none(),
                "Should not have 'Add default value' when default already exists"
            );
        }
    }

    #[test]
    fn test_add_documentation_to_record() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": []
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the record name
        let position = Position {
            line: 2,
            character: 12,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        let add_doc = actions
            .iter()
            .find(|a| a.title.contains("Add documentation"));

        assert!(add_doc.is_some(), "Should have 'Add documentation' action");
    }

    #[test]
    fn test_add_field_to_record() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "int"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the record definition
        let position = Position {
            line: 2,
            character: 12,
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions");

        let add_field = actions.iter().find(|a| a.title.contains("Add field"));

        assert!(add_field.is_some(), "Should have 'Add field' action");
        let action = add_field.unwrap();

        // Verify the edit inserts valid JSON
        let edit = action.edit.as_ref().expect("Should have edit");
        let changes = edit.changes.as_ref().expect("Should have changes");
        let file_edits = changes.get(&uri).expect("Should have edits for file");

        assert_eq!(file_edits.len(), 1, "Should have one edit");
        let text_edit = &file_edits[0];

        // Should insert a valid field JSON object
        assert!(text_edit.new_text.contains("new_field"));
        assert!(text_edit.new_text.contains("\"name\""));
        assert!(text_edit.new_text.contains("\"type\""));
    }

    #[test]
    fn test_add_doc_to_field() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "int"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Position on the "id" field name
        let position = Position {
            line: 4,
            character: 15, // On "id"
        };

        let actions = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position,
                end: position,
            },
        );

        assert!(!actions.is_empty(), "Should have code actions for field");

        println!(
            "Available actions: {:?}",
            actions.iter().map(|a| &a.title).collect::<Vec<_>>()
        );

        // Find the "Add documentation" action
        let add_doc = actions
            .iter()
            .find(|a| a.title.contains("Add documentation") && a.title.contains("field"));

        assert!(
            add_doc.is_some(),
            "Should have 'Add documentation for field' action. Available: {:?}",
            actions.iter().map(|a| &a.title).collect::<Vec<_>>()
        );

        let action = add_doc.unwrap();

        // Check the edit
        let edit = action.edit.as_ref().expect("Should have edit");
        let changes = edit.changes.as_ref().expect("Should have changes");
        let file_edits = changes.get(&uri).expect("Should have edits for file");

        assert_eq!(file_edits.len(), 1, "Should have one edit");
        let text_edit = &file_edits[0];

        // Should insert doc field after field name
        assert!(
            text_edit.new_text.contains("\"doc\""),
            "Should contain doc field"
        );
        assert!(
            text_edit.new_text.contains("Description for id"),
            "Should contain description"
        );
    }

    #[test]
    fn test_add_doc_to_field_multiple_positions() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "int"},
    {"name": "name", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Test at different positions within a simple field

        // Position 1: On the field name
        let position1 = Position {
            line: 4,
            character: 15, // On "id"
        };

        let actions1 = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position1,
                end: position1,
            },
        );
        assert!(!actions1.is_empty());
        assert!(
            actions1
                .iter()
                .any(|a| a.title.contains("Add documentation for field"))
        );

        // Position 2: On the type value
        let position2 = Position {
            line: 4,
            character: 30, // On "int"
        };

        let actions2 = get_code_actions(
            &schema,
            &uri,
            Range {
                start: position2,
                end: position2,
            },
        );
        assert!(!actions2.is_empty());
        // On type value, we get FieldType actions
        let actions2_vec = actions2;
        let action_titles: Vec<_> = actions2_vec.iter().map(|a| a.title.as_str()).collect();
        println!("Actions at type value: {:?}", action_titles);
        // At type position, we prioritize FieldType for "Make nullable",
        // so Field doc might not be there - that's OK
    }

    // === Quick Fix Tests ===

    #[test]
    fn test_quick_fix_invalid_name() {
        let schema_text = r#"{
  "type": "record",
  "name": "123Invalid",
  "fields": [
    {"name": "value", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create a diagnostic for invalid name
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 2,
                    character: 11,
                },
                end: Position {
                    line: 2,
                    character: 22,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Invalid name '123Invalid': must match [A-Za-z_][A-Za-z0-9_]*".to_string(),
            source: Some("avro-lsp".to_string()),
            ..Default::default()
        };

        let quick_fixes =
            get_quick_fixes_from_diagnostics(Some(&schema), schema_text, &uri, &[diagnostic]);

        assert!(!quick_fixes.is_empty(), "Should have quick fixes");

        // Find the fix invalid name action
        let fix = quick_fixes
            .iter()
            .find(|a| a.title.contains("Fix invalid name"));

        assert!(fix.is_some(), "Should have 'Fix invalid name' action");
        let action = fix.unwrap();

        // Verify it's a QUICKFIX
        assert_eq!(
            action.kind,
            Some(async_lsp::lsp_types::CodeActionKind::QUICKFIX)
        );

        // Verify the fix suggests a valid name
        assert!(
            action.title.contains("_123Invalid"),
            "Should suggest '_123Invalid', got: {}",
            action.title
        );
    }

    #[test]
    fn test_quick_fix_invalid_namespace() {
        let schema_text = r#"{
  "type": "record",
  "name": "Test",
  "namespace": "123.invalid",
  "fields": [
    {"name": "value", "type": "string"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create a diagnostic for invalid namespace
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 3,
                    character: 16,
                },
                end: Position {
                    line: 3,
                    character: 29,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Invalid namespace '123.invalid': must be dot-separated names".to_string(),
            source: Some("avro-lsp".to_string()),
            ..Default::default()
        };

        let quick_fixes =
            get_quick_fixes_from_diagnostics(Some(&schema), schema_text, &uri, &[diagnostic]);

        assert!(!quick_fixes.is_empty(), "Should have quick fixes");

        // Should have at least one fix for the namespace
        let fix = quick_fixes.iter().find(|a| a.title.contains("namespace"));

        assert!(fix.is_some(), "Should have namespace fix action");
    }

    #[test]
    fn test_quick_fix_logical_type() {
        let schema_text = r#"{
  "type": "record",
  "name": "InvalidLogicalType",
  "fields": [
    {
      "name": "bad_uuid",
      "type": {
        "type": "int",
        "logicalType": "uuid"
      }
    }
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create a diagnostic for logical type error
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 7,
                    character: 8,
                },
                end: Position {
                    line: 10,
                    character: 7,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Invalid logical type 'uuid' for type int - requires string".to_string(),
            source: Some("avro-lsp".to_string()),
            ..Default::default()
        };

        let quick_fixes =
            get_quick_fixes_from_diagnostics(Some(&schema), schema_text, &uri, &[diagnostic]);

        assert!(!quick_fixes.is_empty(), "Should have quick fixes");

        // Find the fix logical type action
        let fix = quick_fixes
            .iter()
            .find(|a| a.title.contains("Change base type"));

        assert!(fix.is_some(), "Should have 'Change base type' action");
        let action = fix.unwrap();

        // Verify it suggests changing to string
        assert!(
            action.title.contains("string"),
            "Should suggest changing to 'string', got: {}",
            action.title
        );
    }

    #[test]
    fn test_quick_fix_duplicate_symbol() {
        let schema_text = r#"{
  "type": "enum",
  "name": "Colors",
  "symbols": ["RED", "GREEN", "RED"]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");
        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create a diagnostic for duplicate symbol
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 3,
                    character: 15,
                },
                end: Position {
                    line: 3,
                    character: 38,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Duplicate symbol 'RED' in enum".to_string(),
            source: Some("avro-lsp".to_string()),
            ..Default::default()
        };

        let quick_fixes =
            get_quick_fixes_from_diagnostics(Some(&schema), schema_text, &uri, &[diagnostic]);

        assert!(!quick_fixes.is_empty(), "Should have quick fixes");

        // Find the remove duplicate action
        let fix = quick_fixes
            .iter()
            .find(|a| a.title.contains("Remove duplicate"));

        assert!(fix.is_some(), "Should have 'Remove duplicate' action");
        let action = fix.unwrap();

        // Verify it mentions the symbol name
        assert!(
            action.title.contains("RED"),
            "Should mention symbol 'RED', got: {}",
            action.title
        );
    }

    #[test]
    fn test_fix_invalid_name_function() {
        assert_eq!(fix_invalid_name("123abc"), "_123abc");
        assert_eq!(fix_invalid_name("valid_name"), "valid_name");
        assert_eq!(fix_invalid_name("with-dash"), "with_dash");
        assert_eq!(fix_invalid_name("with space"), "with_space");
        assert_eq!(fix_invalid_name("!!!"), "_");
    }

    #[test]
    fn test_end_to_end_invalid_name_flow() {
        // This test simulates the EXACT flow that happens when editor triggers code action
        let text = r#"{
  "type": "record",
  "name": "123Invalid",
  "fields": [
    {"name": "value", "type": "string"}
  ]
}"#;

        // Step 1: Parse (what server does on didOpen)
        let mut parser = AvroParser::new();
        let schema = parser.parse(text).expect("Should parse");

        // Step 2: Get diagnostics (what server does after parsing)
        let diagnostics = crate::handlers::diagnostics::parse_and_validate(text);

        eprintln!("\n=== DIAGNOSTICS (what server sends to editor) ===");
        for (i, diag) in diagnostics.iter().enumerate() {
            eprintln!("Diagnostic {}: '{}'", i, diag.message);
            eprintln!("  Range: {:?}", diag.range);
        }

        assert!(!diagnostics.is_empty(), "Should have diagnostics");

        // Step 3: Get code actions (what server does when editor requests code actions)
        let uri = Url::parse("file:///test.avsc").unwrap();
        let quick_fixes = get_quick_fixes_from_diagnostics(Some(&schema), text, &uri, &diagnostics);

        eprintln!("\n=== QUICK FIXES (what server should return) ===");
        for (i, fix) in quick_fixes.iter().enumerate() {
            eprintln!("Fix {}: '{}'", i, fix.title);
        }

        // THIS IS THE KEY TEST - if this fails, code actions won't work in editor
        assert!(
            !quick_fixes.is_empty(),
            "Should generate quick fixes! If this fails, the string parsing is broken."
        );

        // Verify the fix is correct
        let fix = &quick_fixes[0];
        assert!(
            fix.title.contains("123Invalid") && fix.title.contains("_123Invalid"),
            "Fix should suggest renaming to _123Invalid, got: {}",
            fix.title
        );
    }

    #[test]
    fn test_fix_invalid_namespace_function() {
        // Valid namespace stays the same
        assert_eq!(
            fix_invalid_namespace("com.example.test"),
            "com.example.test"
        );
        // Pure number segment is removed
        assert_eq!(fix_invalid_namespace("123.invalid"), "invalid");
        // Segment starting with number but containing letters is fixed
        assert_eq!(
            fix_invalid_namespace("valid.123invalid"),
            "valid._123invalid"
        );
        // Dashes are replaced with underscores
        assert_eq!(
            fix_invalid_namespace("com.test-dash.app"),
            "com.test_dash.app"
        );
    }

    #[test]
    fn test_quick_fix_invalid_primitive_type() {
        let schema_text = r#"{
  "type": "record",
  "name": "TestRecord",
  "fields": [
    {
      "name": "typo_field",
      "type": {
        "type": "strign"
      }
    }
  ]
}"#;

        let uri = Url::parse("file:///test.avsc").unwrap();

        // This should produce an InvalidPrimitiveType error with suggestion "string"
        let diagnostics =
            crate::handlers::diagnostics::parse_and_validate_with_workspace(schema_text, None);

        assert_eq!(
            diagnostics.len(),
            1,
            "Expected 1 diagnostic, got {}",
            diagnostics.len()
        );
        assert!(
            diagnostics[0].message.contains("Invalid primitive type")
                && diagnostics[0].message.contains("strign")
                && diagnostics[0].message.contains("string"),
            "Unexpected error message: {}",
            diagnostics[0].message
        );

        // For code actions, we need a dummy schema (the actual schema failed to parse)
        // But the quick fix doesn't need the schema for InvalidPrimitiveType
        let mut parser = crate::schema::AvroParser::new();
        let dummy_schema = parser.parse(r#"{"type": "null"}"#).unwrap();

        // Generate quick fixes
        let quick_fixes =
            get_quick_fixes_from_diagnostics(Some(&dummy_schema), schema_text, &uri, &diagnostics);

        // Should have one quick fix
        assert_eq!(
            quick_fixes.len(),
            1,
            "Expected 1 quick fix, got {}",
            quick_fixes.len()
        );
        assert!(quick_fixes[0].title.contains("strign"));
        assert!(quick_fixes[0].title.contains("string"));
        assert!(quick_fixes[0].title.contains("Fix typo"));
    }

    #[test]
    fn test_quick_fix_primitive_typo_variants() {
        // Test various typos and their expected corrections
        // Only typos within Levenshtein distance ≤ 3
        let test_cases = vec![
            ("bites", "bytes"),
            ("lon", "long"),
            ("flot", "float"),
            ("nul", "null"),
            ("boolena", "boolean"),
            ("strin", "string"),
            ("doubl", "double"),
            ("strign", "string"),
        ];

        for (typo, expected) in test_cases {
            let schema_text = format!(
                r#"{{
  "type": "record",
  "name": "TestRecord",
  "fields": [
    {{
      "name": "field",
      "type": {{
        "type": "{}"
      }}
    }}
  ]
}}"#,
                typo
            );

            let uri = Url::parse("file:///test.avsc").unwrap();
            let diagnostics =
                crate::handlers::diagnostics::parse_and_validate_with_workspace(&schema_text, None);

            assert_eq!(
                diagnostics.len(),
                1,
                "Expected 1 diagnostic for typo '{}', got {}",
                typo,
                diagnostics.len()
            );
            assert!(
                diagnostics[0].message.contains("Invalid primitive type"),
                "Expected 'Invalid primitive type' in message for typo '{}', got: {}",
                typo,
                diagnostics[0].message
            );
            assert!(
                diagnostics[0].message.contains(expected),
                "Expected suggestion '{}' for typo '{}', got message: {}",
                expected,
                typo,
                diagnostics[0].message
            );

            // Verify quick fix is generated
            let mut parser = crate::schema::AvroParser::new();
            let dummy_schema = parser.parse(r#"{"type": "null"}"#).unwrap();
            let quick_fixes = get_quick_fixes_from_diagnostics(
                Some(&dummy_schema),
                &schema_text,
                &uri,
                &diagnostics,
            );

            assert_eq!(
                quick_fixes.len(),
                1,
                "Expected 1 quick fix for typo '{}', got {}",
                typo,
                quick_fixes.len()
            );
            assert!(
                quick_fixes[0].title.contains(expected),
                "Expected quick fix title to contain '{}' for typo '{}', got: {}",
                expected,
                typo,
                quick_fixes[0].title
            );
        }
    }

    #[test]
    fn test_quick_fix_primitive_typo_in_nested_structures() {
        // Test typo inside map
        let map_schema = r#"{
  "type": "record",
  "name": "TestMap",
  "fields": [
    {
      "name": "lookup",
      "type": {
        "type": "map",
        "values": {
          "type": "strin"
        }
      }
    }
  ]
}"#;

        let _uri = Url::parse("file:///test.avsc").unwrap();
        let diagnostics =
            crate::handlers::diagnostics::parse_and_validate_with_workspace(map_schema, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("strin"));
        assert!(diagnostics[0].message.contains("string"));

        // Test typo inside union
        let union_schema = r#"{
  "type": "record",
  "name": "TestUnion",
  "fields": [
    {
      "name": "optional_value",
      "type": [
        "null",
        {
          "type": "doubl"
        }
      ]
    }
  ]
}"#;

        let diagnostics =
            crate::handlers::diagnostics::parse_and_validate_with_workspace(union_schema, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("doubl"));
        assert!(diagnostics[0].message.contains("double"));
    }

    #[test]
    fn test_no_suggestion_for_very_different_types() {
        // Test that we don't suggest anything for types that are very different (> 3 edits)
        let schema_text = r#"{
  "type": "record",
  "name": "TestRecord",
  "fields": [
    {
      "name": "field",
      "type": {
        "type": "completelyinvalid"
      }
    }
  ]
}"#;

        let diagnostics =
            crate::handlers::diagnostics::parse_and_validate_with_workspace(schema_text, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Invalid primitive type"));
        // Should not contain "Did you mean" since no close match exists
        assert!(!diagnostics[0].message.contains("Did you mean"));
    }

    #[test]
    fn test_end_to_end_parse_error_quick_fix() {
        // This test simulates the full flow:
        // 1. Parse a schema with an invalid primitive type
        // 2. Get diagnostics (schema is None because parse failed)
        // 3. Request quick fixes with None schema
        // 4. Verify quick fix is generated and can be applied

        let schema_text = r#"{
  "type": "record",
  "name": "TestRecord",
  "fields": [
    {
      "name": "myfield",
      "type": {
        "type": "strign"
      }
    }
  ]
}"#;

        let uri = Url::parse("file:///test.avsc").unwrap();

        // Step 1: Parse with error recovery - should succeed with parse_errors
        let mut parser = crate::schema::AvroParser::new();
        let parse_result = parser.parse(schema_text);
        assert!(
            parse_result.is_ok(),
            "Schema should parse with error recovery"
        );
        let schema = parse_result.unwrap();
        assert!(!schema.parse_errors.is_empty(), "Should have parse errors");

        // Step 2: Get diagnostics (which includes structured error data)
        let diagnostics =
            crate::handlers::diagnostics::parse_and_validate_with_workspace(schema_text, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("strign"));
        assert!(diagnostics[0].message.contains("string"));
        assert!(
            diagnostics[0].data.is_some(),
            "Diagnostic should have structured error data"
        );

        // Step 3: Request quick fixes with None schema (simulating what state.rs does)
        let quick_fixes = get_quick_fixes_from_diagnostics(None, schema_text, &uri, &diagnostics);

        // Step 4: Verify quick fix was generated
        assert_eq!(
            quick_fixes.len(),
            1,
            "Should have 1 quick fix even with None schema"
        );
        let fix = &quick_fixes[0];
        assert_eq!(fix.title, "Fix typo: 'strign' → 'string'");
        assert!(fix.edit.is_some(), "Quick fix should have edit");

        // Verify the edit would fix the typo
        let edit = fix.edit.as_ref().unwrap();
        assert!(edit.changes.is_some());
        let changes = edit.changes.as_ref().unwrap();
        assert!(changes.contains_key(&uri));
        let text_edits = &changes[&uri];
        assert_eq!(text_edits.len(), 1);
        assert_eq!(text_edits[0].new_text, "\"string\"");
    }

    #[test]
    fn test_quick_fix_nested_union() {
        // Test that nested union [["null", "string"]] gets fixed to ["null", "string"]
        let schema_text = r#"{
  "type": "record",
  "name": "Test",
  "fields": [
    {"name": "value", "type": [["null", "string"]]}
  ]
}"#;

        let uri = Url::parse("file:///test.avsc").unwrap();

        // Parse should succeed, but validation should fail
        let mut parser = crate::schema::AvroParser::new();
        let parse_result = parser.parse(schema_text);
        assert!(parse_result.is_ok(), "Schema should parse successfully");

        // Get diagnostics
        let diagnostics =
            crate::handlers::diagnostics::parse_and_validate_with_workspace(schema_text, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Nested unions"));

        // Get schema for code actions
        let schema = parse_result.ok();

        // Request quick fixes
        let quick_fixes =
            get_quick_fixes_from_diagnostics(schema.as_ref(), schema_text, &uri, &diagnostics);

        // Verify quick fix was generated
        assert_eq!(
            quick_fixes.len(),
            1,
            "Should have 1 quick fix for nested union"
        );
        let fix = &quick_fixes[0];
        assert_eq!(fix.title, "Flatten nested union");
        assert!(fix.edit.is_some(), "Quick fix should have edit");

        // Verify the edit flattens the union
        let edit = fix.edit.as_ref().unwrap();
        assert!(edit.changes.is_some());
        let changes = edit.changes.as_ref().unwrap();
        assert!(changes.contains_key(&uri));
        let text_edits = &changes[&uri];
        assert_eq!(text_edits.len(), 1);

        // Should flatten to ["null","string"] (no spaces in JSON serialization)
        let expected = r#"["null","string"]"#;
        assert_eq!(text_edits[0].new_text, expected);
    }

    #[test]
    fn test_quick_fix_duplicate_union_type() {
        // Test that duplicate union types ["null", "string", "null"] gets fixed to ["null", "string"]
        let schema_text = r#"{
  "type": "record",
  "name": "Test",
  "fields": [
    {"name": "value", "type": ["null", "string", "null"]}
  ]
}"#;

        let uri = Url::parse("file:///test.avsc").unwrap();

        // Parse should succeed, but validation should fail
        let mut parser = crate::schema::AvroParser::new();
        let parse_result = parser.parse(schema_text);
        assert!(parse_result.is_ok(), "Schema should parse successfully");

        // Get diagnostics
        let diagnostics =
            crate::handlers::diagnostics::parse_and_validate_with_workspace(schema_text, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Duplicate type in union"));

        // Get schema for code actions
        let schema = parse_result.ok();

        // Request quick fixes
        let quick_fixes =
            get_quick_fixes_from_diagnostics(schema.as_ref(), schema_text, &uri, &diagnostics);

        // Verify quick fix was generated
        assert_eq!(
            quick_fixes.len(),
            1,
            "Should have 1 quick fix for duplicate union type"
        );
        let fix = &quick_fixes[0];
        assert!(fix.title.contains("Remove duplicate"));
        assert!(fix.edit.is_some(), "Quick fix should have edit");

        // Verify the edit removes the duplicate
        let edit = fix.edit.as_ref().unwrap();
        assert!(edit.changes.is_some());
        let changes = edit.changes.as_ref().unwrap();
        assert!(changes.contains_key(&uri));
        let text_edits = &changes[&uri];
        assert_eq!(text_edits.len(), 1);

        // Should have only one occurrence of "null"
        let fixed_text = &text_edits[0].new_text;
        assert_eq!(
            fixed_text.matches("\"null\"").count(),
            1,
            "Should have exactly one 'null' after fix"
        );
        assert!(
            fixed_text.contains("\"string\""),
            "Should still have 'string'"
        );
    }

    #[test]
    fn test_quick_fix_missing_fields() {
        // Test that missing fields array gets added to record
        let schema_text = r#"{
  "type": "record",
  "name": "Incomplete"
}"#;

        let uri = Url::parse("file:///test.avsc").unwrap();

        // Parse should fail
        let mut parser = crate::schema::AvroParser::new();
        let parse_result = parser.parse(schema_text);
        assert!(parse_result.is_err(), "Schema should fail to parse");

        // Get diagnostics
        let diagnostics =
            crate::handlers::diagnostics::parse_and_validate_with_workspace(schema_text, None);
        assert_eq!(diagnostics.len(), 1);
        // The error message might be generic, but structured data should have the field info
        assert!(
            diagnostics[0].data.is_some(),
            "Should have structured error data"
        );

        // Request quick fixes (schema is None because parse failed)
        let quick_fixes = get_quick_fixes_from_diagnostics(None, schema_text, &uri, &diagnostics);

        // Verify quick fix was generated
        assert_eq!(
            quick_fixes.len(),
            1,
            "Should have 1 quick fix for missing fields"
        );
        let fix = &quick_fixes[0];
        assert_eq!(fix.title, "Add empty fields array");
        assert!(fix.edit.is_some(), "Quick fix should have edit");

        // Verify the edit adds fields array
        let edit = fix.edit.as_ref().unwrap();
        assert!(edit.changes.is_some());
        let changes = edit.changes.as_ref().unwrap();
        assert!(changes.contains_key(&uri));
        let text_edits = &changes[&uri];
        assert_eq!(text_edits.len(), 1);

        // Should add "fields": []
        let new_text = &text_edits[0].new_text;
        assert!(new_text.contains("\"fields\""), "Should add fields key");
        assert!(new_text.contains("[]"), "Should add empty array");
    }

    #[test]
    fn test_quick_fix_invalid_enum_default_adds_symbol() {
        let schema_text = r#"{
  "type": "record",
  "name": "BadEnumDefault",
  "fields": [
    {
      "name": "color",
      "type": {
        "type": "enum",
        "name": "Color",
        "symbols": ["RED", "GREEN", "BLUE"]
      },
      "default": "YELLOW"
    }
  ]
}"#;

        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create diagnostic for invalid enum default
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 4,
                    character: 4,
                },
                end: Position {
                    line: 12,
                    character: 5,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Validation error: Default value 'YELLOW' is not a valid enum symbol"
                .to_string(),
            source: Some("avro-lsp".to_string()),
            data: Some(serde_json::json!({
                "Custom": {
                    "message": "Default value 'YELLOW' is not a valid enum symbol",
                    "range": null
                }
            })),
            ..Default::default()
        };

        let quick_fixes = get_quick_fixes_from_diagnostics(None, schema_text, &uri, &[diagnostic]);

        // Verify quick fix was generated
        assert_eq!(
            quick_fixes.len(),
            1,
            "Should have 1 quick fix for invalid enum default"
        );
        let fix = &quick_fixes[0];
        assert_eq!(fix.title, "Add 'YELLOW' to enum symbols");
        assert!(fix.edit.is_some(), "Quick fix should have edit");

        // Verify the edit adds YELLOW to symbols array
        let edit = fix.edit.as_ref().unwrap();
        assert!(edit.changes.is_some());
        let changes = edit.changes.as_ref().unwrap();
        assert!(changes.contains_key(&uri));
        let text_edits = &changes[&uri];
        assert_eq!(text_edits.len(), 1);

        // Should replace ["RED", "GREEN", "BLUE"] with ["RED","GREEN","BLUE","YELLOW"]
        let new_text = &text_edits[0].new_text;
        assert!(new_text.contains("YELLOW"), "Should add YELLOW to symbols");
        assert!(new_text.contains("RED"), "Should preserve RED");
        assert!(new_text.contains("GREEN"), "Should preserve GREEN");
        assert!(new_text.contains("BLUE"), "Should preserve BLUE");

        // Verify it's a proper JSON array
        let parsed: serde_json::Value =
            serde_json::from_str(new_text).expect("Should be valid JSON array");
        assert!(parsed.is_array(), "Should be a JSON array");
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 4, "Should have 4 symbols now");
        assert_eq!(arr[3].as_str().unwrap(), "YELLOW", "YELLOW should be last");
    }

    #[test]
    fn test_quick_fix_invalid_decimal_scale() {
        let schema_text = r#"{
  "type": "record",
  "name": "BadDecimalScale",
  "fields": [
    {
      "name": "price",
      "type": {
        "type": "fixed",
        "name": "DecimalBadScale",
        "size": 8,
        "logicalType": "decimal",
        "precision": 5,
        "scale": 10
      }
    }
  ]
}"#;

        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create diagnostic for invalid decimal scale
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 6,
                    character: 14,
                },
                end: Position {
                    line: 13,
                    character: 7,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Validation error: Decimal scale (10) cannot be greater than precision (5)"
                .to_string(),
            source: Some("avro-lsp".to_string()),
            data: Some(serde_json::json!({
                "Custom": {
                    "message": "Decimal scale (10) cannot be greater than precision (5)",
                    "range": null
                }
            })),
            ..Default::default()
        };

        let quick_fixes = get_quick_fixes_from_diagnostics(None, schema_text, &uri, &[diagnostic]);

        // Should offer 2 quick fixes
        assert_eq!(
            quick_fixes.len(),
            2,
            "Should have 2 quick fixes for decimal scale"
        );

        // Fix 1: Set scale to 5 (match precision)
        let fix1 = &quick_fixes[0];
        assert_eq!(fix1.title, "Set scale to 5 (match precision)");
        assert!(
            fix1.is_preferred.unwrap_or(false),
            "First fix should be preferred"
        );

        // Verify edit replaces scale value
        let edit1 = fix1.edit.as_ref().unwrap();
        let changes1 = edit1.changes.as_ref().unwrap();
        let text_edits1 = &changes1[&uri];
        assert_eq!(text_edits1.len(), 1);
        assert_eq!(text_edits1[0].new_text, "5", "Should set scale to 5");

        // Fix 2: Set precision to 10 (match scale)
        let fix2 = &quick_fixes[1];
        assert_eq!(fix2.title, "Set precision to 10 (match scale)");
        assert!(
            !fix2.is_preferred.unwrap_or(true),
            "Second fix should not be preferred"
        );

        // Verify edit replaces precision value
        let edit2 = fix2.edit.as_ref().unwrap();
        let changes2 = edit2.changes.as_ref().unwrap();
        let text_edits2 = &changes2[&uri];
        assert_eq!(text_edits2.len(), 1);
        assert_eq!(text_edits2[0].new_text, "10", "Should set precision to 10");
    }

    #[test]
    fn test_quick_fix_invalid_duration_size() {
        let schema_text = r#"{
  "type": "record",
  "name": "BadDurationSize",
  "fields": [
    {
      "name": "elapsed",
      "type": {
        "type": "fixed",
        "name": "BadDuration",
        "size": 16,
        "logicalType": "duration"
      }
    }
  ]
}"#;

        let uri = Url::parse("file:///test.avsc").unwrap();

        // Create diagnostic for invalid duration size
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 6,
                    character: 14,
                },
                end: Position {
                    line: 11,
                    character: 7,
                },
            },
            severity: Some(async_lsp::lsp_types::DiagnosticSeverity::ERROR),
            message: "Validation error: Duration logical type requires fixed size of 12 bytes"
                .to_string(),
            source: Some("avro-lsp".to_string()),
            data: Some(serde_json::json!({
                "Custom": {
                    "message": "Duration logical type requires fixed size of 12 bytes",
                    "range": null
                }
            })),
            ..Default::default()
        };

        let quick_fixes = get_quick_fixes_from_diagnostics(None, schema_text, &uri, &[diagnostic]);

        // Should offer 1 quick fix
        assert_eq!(
            quick_fixes.len(),
            1,
            "Should have 1 quick fix for duration size"
        );

        // Fix: Set size to 12 (required for duration)
        let fix = &quick_fixes[0];
        assert_eq!(fix.title, "Set size to 12 (required for duration)");
        assert!(fix.is_preferred.unwrap_or(false), "Fix should be preferred");

        // Verify edit replaces size value
        let edit = fix.edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let text_edits = &changes[&uri];
        assert_eq!(text_edits.len(), 1);
        assert_eq!(text_edits[0].new_text, "12", "Should set size to 12");

        // Verify the range targets the size value "16"
        assert_eq!(
            text_edits[0].range.start.line, 9,
            "Should be on the size line"
        );
        assert_eq!(
            text_edits[0].range.start.character, 16,
            "Should start at the size value"
        );
    }
}
