//! Code actions for Avro schemas
//!
//! This module provides LSP code actions for `.avsc` files, including:
//! - Refactoring actions (add field, add documentation, make nullable, sort fields)
//! - Quick fixes for validation errors (invalid names, types, logical types, etc.)

mod builder;
mod helpers;
mod quick_fixes;
mod refactoring;

use async_lsp::lsp_types::{CodeAction, Diagnostic, Range, Url};

use crate::schema::AvroSchema;
use crate::state::{AstNode, find_node_at_position};

use helpers::is_union;
use quick_fixes::{
    create_fix_duplicate_symbol, create_fix_duplicate_union_type, create_fix_invalid_array_default,
    create_fix_invalid_boolean_default, create_fix_invalid_decimal_scale,
    create_fix_invalid_duration_size, create_fix_invalid_enum_default, create_fix_invalid_name,
    create_fix_invalid_name_structured, create_fix_invalid_namespace,
    create_fix_invalid_namespace_structured, create_fix_invalid_primitive_type,
    create_fix_logical_type, create_fix_missing_decimal_precision, create_fix_missing_fields,
    create_fix_nested_union,
};
use refactoring::{
    create_add_default_value_action, create_add_doc_action, create_add_doc_action_enum,
    create_add_doc_action_field, create_add_doc_action_fixed, create_add_field_action,
    create_make_nullable_action, create_sort_fields_action, find_parent_record_and_add_field,
};

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
            if record.doc.is_none()
                && let Some(action) = create_add_doc_action(uri, record)
            {
                actions.push(action);
            }
            if let Some(action) = create_add_field_action(uri, record) {
                actions.push(action);
            }
            if record.fields.len() > 1
                && let Some(action) = create_sort_fields_action(uri, record)
            {
                actions.push(action);
            }
        }
        AstNode::Field(field) => {
            if field.doc.is_none()
                && let Some(action) = create_add_doc_action_field(uri, field)
            {
                actions.push(action);
            }

            if let Some(action) = find_parent_record_and_add_field(uri, schema, field) {
                actions.push(action);
            }

            if !is_union(&field.field_type)
                && let Some(action) = create_make_nullable_action(uri, field)
            {
                actions.push(action);
            }

            if field.default.is_none()
                && let Some(action) = create_add_default_value_action(uri, field)
            {
                actions.push(action);
            }
        }
        AstNode::FieldType(field) => {
            if !is_union(&field.field_type)
                && let Some(action) = create_make_nullable_action(uri, field)
            {
                actions.push(action);
            }
        }
        AstNode::EnumDefinition(enum_schema) => {
            if enum_schema.doc.is_none()
                && let Some(action) = create_add_doc_action_enum(uri, enum_schema)
            {
                actions.push(action);
            }
        }
        AstNode::FixedDefinition(fixed_schema) => {
            if fixed_schema.doc.is_none()
                && let Some(action) = create_add_doc_action_fixed(uri, fixed_schema)
            {
                actions.push(action);
            }
        }
    }

    actions
}

/// Get quick fixes for diagnostics
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
                    if let Some(fix) = create_fix_invalid_primitive_type(
                        uri,
                        diagnostic,
                        &type_name,
                        suggested.as_deref(),
                    ) {
                        actions.push(fix);
                    }
                }
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
                    let fixes = create_fix_invalid_decimal_scale(uri, text, diagnostic);
                    actions.extend(fixes);
                }
                _ => {
                    tracing::debug!("No quick fix available for error type");
                }
            }
        } else {
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
