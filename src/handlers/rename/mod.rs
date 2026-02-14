use std::collections::HashMap;

use async_lsp::ResponseError;
use async_lsp::lsp_types::{
    Location, Position, PrepareRenameResponse, TextEdit, Url, WorkspaceEdit,
};

use crate::schema::AvroSchema;
use crate::workspace::Workspace;

mod local_references;
mod local_rename;
mod node_matcher;
mod validation;
mod workspace_operations;

#[cfg(test)]
mod tests;

#[allow(dead_code)]
pub fn rename(
    schema: &AvroSchema,
    text: &str,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Result<Option<WorkspaceEdit>, ResponseError> {
    validation::validate_avro_name(new_name)?;

    let rename_info = node_matcher::extract_rename_info(schema, position)?;

    local_rename::rename_in_file(schema, text, uri, &rename_info, new_name)
}

pub fn rename_with_workspace(
    schema: &AvroSchema,
    text: &str,
    uri: &Url,
    position: Position,
    new_name: &str,
    workspace: Option<&Workspace>,
) -> Result<Option<WorkspaceEdit>, ResponseError> {
    validation::validate_avro_name(new_name)?;

    let rename_info = node_matcher::extract_rename_info(schema, position)?;

    let mut changes = HashMap::new();

    match &rename_info.symbol_type {
        node_matcher::SymbolType::Field { field } => {
            validation::check_field_name_conflict(schema, field, new_name)?;

            let edits = vec![TextEdit {
                range: rename_info.name_range,
                new_text: new_name.to_string(),
            }];
            changes.insert(uri.clone(), edits);
        }
        node_matcher::SymbolType::RecordType
        | node_matcher::SymbolType::EnumType
        | node_matcher::SymbolType::FixedType
        | node_matcher::SymbolType::TypeReference => {
            validation::check_type_name_conflict(schema, &rename_info.old_name, new_name)?;

            let current_file_edits = local_rename::collect_type_rename_edits(
                schema,
                text,
                &rename_info.old_name,
                new_name,
            );
            if !current_file_edits.is_empty() {
                changes.insert(uri.clone(), current_file_edits);
            }

            if let Some(workspace) = workspace {
                workspace_operations::add_cross_file_rename_edits(
                    workspace,
                    uri,
                    &rename_info.old_name,
                    new_name,
                    &mut changes,
                );
            }
        }
    }

    if changes.is_empty() {
        return Ok(None);
    }

    Ok(Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }))
}

pub fn prepare_rename(schema: &AvroSchema, position: Position) -> Option<PrepareRenameResponse> {
    node_matcher::extract_renameable_symbol(schema, position)
}

#[allow(dead_code)]
pub fn find_references(
    schema: &AvroSchema,
    uri: &Url,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    local_references::find_in_file(schema, uri, position, include_declaration)
}

pub fn find_references_with_workspace(
    schema: &AvroSchema,
    uri: &Url,
    position: Position,
    include_declaration: bool,
    workspace: Option<&Workspace>,
) -> Option<Vec<Location>> {
    let type_name = node_matcher::extract_type_name_for_references(schema, position)?;

    let references = local_rename::collect_type_references(schema, type_name, include_declaration);
    let mut locations: Vec<Location> = references
        .into_iter()
        .map(|range| Location {
            uri: uri.clone(),
            range,
        })
        .collect();

    if let Some(workspace) = workspace {
        let cross_file_refs =
            workspace_operations::collect_cross_file_references(workspace, uri, type_name);
        locations.extend(cross_file_refs);
    }

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}
