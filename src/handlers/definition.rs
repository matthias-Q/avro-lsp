use async_lsp::lsp_types::{Location, Url};

use crate::handlers::symbols;
use crate::schema::AvroSchema;

/// Find the definition of a symbol at the given position
pub fn find_definition(schema: &AvroSchema, text: &str, word: &str, uri: &Url) -> Option<Location> {
    // Check if the word is a named type in the schema
    if schema.named_types.contains_key(word) {
        // Find where this type is defined (its name declaration)
        let range = symbols::find_name_range(text, word)?;

        return Some(Location {
            uri: uri.clone(),
            range,
        });
    }

    // Not a type reference we can navigate to
    None
}
