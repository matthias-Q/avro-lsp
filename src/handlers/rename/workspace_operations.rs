use std::collections::HashMap;

use async_lsp::lsp_types::{Location, TextEdit, Url};

use crate::workspace::Workspace;

pub fn add_cross_file_rename_edits(
    workspace: &Workspace,
    current_uri: &Url,
    old_name: &str,
    new_name: &str,
    changes: &mut HashMap<Url, Vec<TextEdit>>,
) {
    // Resolve the type to get its qualified name and definition location
    let type_info = match workspace.resolve_type(old_name, current_uri) {
        Some(info) => info,
        None => return, // Type not found
    };

    // Add rename edit for the definition if it's in another file
    if type_info.defined_in != *current_uri
        && let Some(def_range) = type_info.definition_range
    {
        let def_edit = TextEdit {
            range: def_range,
            new_text: format!("\"{}\"", new_name),
        };
        changes
            .entry(type_info.defined_in.clone())
            .or_default()
            .push(def_edit);
    }

    // Get all references to the type (with their original ref_text form)
    let all_refs = workspace.find_all_references(old_name);

    // Determine the old simple name (last segment after last dot, or the name itself)
    let old_simple = old_name.rsplit('.').next().unwrap_or(old_name);

    for type_ref_loc in all_refs {
        // Skip current file (handled locally)
        if type_ref_loc.location.uri == *current_uri {
            continue;
        }

        // Build the replacement text preserving the original reference form.
        // If the source wrote a FQN (e.g. "com.example.Address"), replace only
        // the last segment: "com.example.Address2".
        // If the source wrote a simple name (e.g. "Address"), emit just "Address2".
        let new_text = if type_ref_loc.ref_text.contains('.') {
            // FQN reference: replace the simple-name suffix
            let prefix = &type_ref_loc.ref_text[..type_ref_loc.ref_text.len() - old_simple.len()];
            format!("{}{}", prefix, new_name)
        } else {
            // Simple name reference
            new_name.to_string()
        };

        changes
            .entry(type_ref_loc.location.uri)
            .or_default()
            .push(TextEdit {
                range: type_ref_loc.location.range,
                new_text,
            });
    }
}

pub fn collect_cross_file_references(
    workspace: &Workspace,
    current_uri: &Url,
    type_name: &str,
) -> Vec<Location> {
    // Get all references using the context-aware API
    let all_refs = workspace.find_all_references_from(type_name, current_uri);

    // Filter to exclude current file (already handled locally), then extract Location
    all_refs
        .into_iter()
        .filter(|loc| loc.location.uri != *current_uri)
        .map(|loc| loc.location)
        .collect()
}
