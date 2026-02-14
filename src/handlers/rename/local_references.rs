use async_lsp::lsp_types::{Location, Position, Url};

use crate::schema::AvroSchema;

use super::local_rename::collect_type_references;
use super::node_matcher;

#[allow(dead_code)]
pub fn find_in_file(
    schema: &AvroSchema,
    uri: &Url,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    let type_name = node_matcher::extract_type_name_for_references(schema, position)?;

    let references = collect_type_references(schema, type_name, include_declaration);

    let locations: Vec<Location> = references
        .into_iter()
        .map(|range| Location {
            uri: uri.clone(),
            range,
        })
        .collect();

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}
