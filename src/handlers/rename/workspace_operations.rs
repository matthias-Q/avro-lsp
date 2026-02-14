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
    if let Some(type_info) = workspace.resolve_type(old_name, current_uri)
        && type_info.defined_in != *current_uri
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

    let references = workspace.find_all_references(old_name);

    let mut edits_by_file: HashMap<Url, Vec<_>> = HashMap::new();
    for location in references {
        if location.uri == *current_uri {
            continue;
        }

        edits_by_file
            .entry(location.uri)
            .or_default()
            .push(location.range);
    }

    for (file_uri, ranges) in edits_by_file {
        let edits: Vec<TextEdit> = ranges
            .into_iter()
            .map(|range| TextEdit {
                range,
                new_text: format!("\"{}\"", new_name),
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
    let workspace_refs = workspace.find_all_references(type_name);

    workspace_refs
        .into_iter()
        .filter(|loc| loc.uri != *current_uri)
        .collect()
}
