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

    // Get all references to the simple name
    let all_refs = workspace.find_all_references(old_name);

    let mut edits_by_file: HashMap<Url, Vec<_>> = HashMap::new();
    for location in all_refs {
        // Skip current file (handled locally)
        if location.uri == *current_uri {
            continue;
        }

        // Only include references that resolve to the same qualified type
        if let Some(resolved) = workspace.resolve_type(old_name, &location.uri)
            && resolved.qualified_name == type_info.qualified_name
        {
            edits_by_file
                .entry(location.uri)
                .or_default()
                .push(location.range);
        }
    }

    for (file_uri, ranges) in edits_by_file {
        let edits: Vec<TextEdit> = ranges
            .into_iter()
            .map(|range| TextEdit {
                range,
                new_text: new_name.to_string(),
            })
            .collect();

        if !edits.is_empty() {
            changes.entry(file_uri).or_default().extend(edits);
        }
    }
}

pub fn collect_cross_file_references(
    workspace: &Workspace,
    current_uri: &Url,
    type_name: &str,
) -> Vec<Location> {
    // Get all references using the context-aware API
    let all_refs = workspace.find_all_references_from(type_name, current_uri);

    // Filter to exclude current file (already handled locally)
    all_refs
        .into_iter()
        .filter(|loc| loc.uri != *current_uri)
        .collect()
}
