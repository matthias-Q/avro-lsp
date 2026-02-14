use async_lsp::lsp_types::{CodeAction, CodeActionKind, Diagnostic, Url};

use crate::handlers::code_actions::builder::CodeActionBuilder;
use crate::handlers::code_actions::helpers::{
    find_name_range_in_schema, find_namespace_range_in_schema, fix_invalid_name,
    fix_invalid_namespace,
};
use crate::schema::AvroSchema;

/// Create a quick fix for invalid name errors using structured error data
pub(in crate::handlers::code_actions) fn create_fix_invalid_name_structured(
    uri: &Url,
    _schema: &AvroSchema,
    diagnostic: &Diagnostic,
    invalid_name: &str,
    suggested_name: Option<&str>,
) -> Option<CodeAction> {
    let fixed_name = suggested_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| fix_invalid_name(invalid_name));

    let name_range = diagnostic.range;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Fix invalid name: '{}' → '{}'", invalid_name, fixed_name),
        )
        .with_kind(CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .add_edit(name_range, format!("\"{}\"", fixed_name))
        .build(),
    )
}

/// Create a quick fix for invalid namespace errors using structured error data
pub(in crate::handlers::code_actions) fn create_fix_invalid_namespace_structured(
    uri: &Url,
    _schema: &AvroSchema,
    diagnostic: &Diagnostic,
    invalid_namespace: &str,
    suggested_namespace: Option<&str>,
) -> Option<CodeAction> {
    let fixed_namespace = suggested_namespace
        .map(|s| s.to_string())
        .unwrap_or_else(|| fix_invalid_namespace(invalid_namespace));

    let namespace_range = diagnostic.range;

    let new_text = if fixed_namespace.is_empty() {
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
            .with_kind(CodeActionKind::QUICKFIX)
            .with_diagnostics(vec![diagnostic.clone()])
            .add_edit(namespace_range, new_text)
            .build(),
    )
}

/// Create a quick fix for invalid name errors
pub(in crate::handlers::code_actions) fn create_fix_invalid_name(
    uri: &Url,
    schema: &AvroSchema,
    diagnostic: &Diagnostic,
    invalid_name: &str,
) -> Option<CodeAction> {
    let fixed_name = fix_invalid_name(invalid_name);
    let name_range = find_name_range_in_schema(schema, invalid_name, diagnostic.range)?;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!("Fix invalid name: '{}' → '{}'", invalid_name, fixed_name),
        )
        .with_kind(CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .preferred()
        .add_edit(name_range, format!("\"{}\"", fixed_name))
        .build(),
    )
}

/// Create a quick fix for invalid namespace errors
pub(in crate::handlers::code_actions) fn create_fix_invalid_namespace(
    uri: &Url,
    schema: &AvroSchema,
    diagnostic: &Diagnostic,
    invalid_namespace: &str,
) -> Option<CodeAction> {
    let fixed_namespace = fix_invalid_namespace(invalid_namespace);

    if fixed_namespace.is_empty() {
        return create_remove_namespace_action(uri, schema, diagnostic);
    }

    let namespace_range = find_namespace_range_in_schema(schema, diagnostic.range)?;

    Some(
        CodeActionBuilder::new(
            uri.clone(),
            format!(
                "Fix invalid namespace: '{}' → '{}'",
                invalid_namespace, fixed_namespace
            ),
        )
        .with_kind(CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .preferred()
        .add_edit(namespace_range, format!("\"{}\"", fixed_namespace))
        .build(),
    )
}

/// Create action to remove invalid namespace field
pub(in crate::handlers::code_actions) fn create_remove_namespace_action(
    uri: &Url,
    _schema: &AvroSchema,
    diagnostic: &Diagnostic,
) -> Option<CodeAction> {
    Some(
        CodeActionBuilder::new(
            uri.clone(),
            "Replace with valid namespace placeholder".to_string(),
        )
        .with_kind(CodeActionKind::QUICKFIX)
        .with_diagnostics(vec![diagnostic.clone()])
        .add_edit(diagnostic.range, "\"valid_namespace\"".to_string())
        .build(),
    )
}
