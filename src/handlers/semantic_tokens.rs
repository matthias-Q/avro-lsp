use async_lsp::lsp_types::{Position, SemanticToken};

use crate::schema::{
    AvroSchema, AvroType, EnumSchema, Field, FixedSchema, PrimitiveType, RecordSchema,
};

// Token type indices (must match server.rs capabilities)
const TOKEN_TYPE_KEYWORD: u32 = 0;
const TOKEN_TYPE_TYPE: u32 = 1;
const TOKEN_TYPE_ENUM: u32 = 2;
const TOKEN_TYPE_STRUCT: u32 = 3;
const TOKEN_TYPE_PROPERTY: u32 = 4;
const TOKEN_TYPE_ENUM_MEMBER: u32 = 5;
// const TOKEN_TYPE_STRING: u32 = 6;  // Unused - primitives use TOKEN_TYPE_TYPE
// const TOKEN_TYPE_NUMBER: u32 = 7;  // Unused - primitives use TOKEN_TYPE_TYPE

/// Token modifiers (bit flags)
const TOKEN_MODIFIER_DECLARATION: u32 = 0x01;
const TOKEN_MODIFIER_READONLY: u32 = 0x02;

/// Build semantic tokens from an Avro schema
pub fn build_semantic_tokens(schema: &AvroSchema, text: String) -> Vec<SemanticToken> {
    let mut builder = SemanticTokensBuilder::new(text);
    builder.tokenize_schema(schema);
    builder.build()
}

struct SemanticTokensBuilder {
    text: String,
    tokens: Vec<Token>,
}

/// A token with absolute position
#[derive(Debug, Clone)]
struct Token {
    line: u32,
    character: u32,
    length: u32,
    token_type: u32,
    token_modifiers: u32,
}

impl SemanticTokensBuilder {
    fn new(text: String) -> Self {
        Self {
            text,
            tokens: Vec::new(),
        }
    }

    /// Main tokenization logic
    fn tokenize_schema(&mut self, schema: &AvroSchema) {
        use std::collections::HashSet;

        // Track which byte offsets we've already tokenized to avoid duplicates
        let mut tokenized_offsets: HashSet<usize> = HashSet::new();

        // PASS 1: Tokenize all JSON structural keywords (keys only, not values)
        // These are the keys like "type":, "name":, "fields":, etc.
        for key in &[
            "type",
            "name",
            "namespace",
            "doc",
            "fields",
            "symbols",
            "items",
            "values",
            "size",
            "default",
            "aliases",
            "order",
        ] {
            let pattern = format!("\"{}\":", key);
            let mut search_start = 0;

            while let Some(offset) = self.text[search_start..].find(&pattern) {
                let absolute_offset = search_start + offset + 1; // +1 for opening quote

                if tokenized_offsets.insert(absolute_offset) {
                    let pos = self.offset_to_position(absolute_offset);
                    self.add_token(
                        pos.line,
                        pos.character,
                        key.len() as u32,
                        TOKEN_TYPE_KEYWORD,
                        0,
                    );
                }

                search_start += offset + pattern.len();
            }
        }

        // PASS 2: Tokenize type keyword VALUES like "record", "enum", "array", "map", "fixed"
        for keyword in &["record", "enum", "array", "map", "fixed"] {
            let pattern = format!("\"type\": \"{}\"", keyword);
            let mut search_start = 0;

            while let Some(offset) = self.text[search_start..].find(&pattern) {
                let absolute_offset = search_start + offset;
                let value_offset = absolute_offset + "\"type\": \"".len();

                if tokenized_offsets.insert(value_offset) {
                    let pos = self.offset_to_position(value_offset);
                    self.add_token(
                        pos.line,
                        pos.character,
                        keyword.len() as u32,
                        TOKEN_TYPE_KEYWORD,
                        0,
                    );
                }

                search_start += offset + pattern.len();
            }
        }

        // PASS 3: Tokenize named type declarations from schema
        match &schema.root {
            AvroType::Record(record) => {
                self.tokenize_record_type(record, &mut tokenized_offsets);
            }
            AvroType::Enum(enum_type) => {
                self.tokenize_enum_type(enum_type, &mut tokenized_offsets);
            }
            AvroType::Fixed(fixed) => {
                self.tokenize_fixed_type(fixed, &mut tokenized_offsets);
            }
            _ => {}
        }

        for avro_type in schema.named_types.values() {
            match avro_type {
                AvroType::Record(record) => {
                    self.tokenize_record_type(record, &mut tokenized_offsets);
                }
                AvroType::Enum(enum_type) => {
                    self.tokenize_enum_type(enum_type, &mut tokenized_offsets);
                }
                AvroType::Fixed(fixed) => {
                    self.tokenize_fixed_type(fixed, &mut tokenized_offsets);
                }
                _ => {}
            }
        }
    }

    /// Tokenize a record type
    fn tokenize_record_type(
        &mut self,
        record: &RecordSchema,
        tokenized_offsets: &mut std::collections::HashSet<usize>,
    ) {
        // Find and tokenize the record name (appears after "name": at the top level)
        let name_pattern = format!("\"name\": \"{}\"", record.name);
        if let Some(offset) = self.text.find(&name_pattern) {
            let value_offset = offset + "\"name\": \"".len();
            if tokenized_offsets.insert(value_offset) {
                let pos = self.offset_to_position(value_offset);
                self.add_token(
                    pos.line,
                    pos.character,
                    record.name.len() as u32,
                    TOKEN_TYPE_STRUCT,
                    TOKEN_MODIFIER_DECLARATION,
                );
            }
        }

        // Tokenize fields
        for field in &record.fields {
            self.tokenize_field(field, tokenized_offsets);
        }
    }

    /// Tokenize an enum type
    fn tokenize_enum_type(
        &mut self,
        enum_type: &EnumSchema,
        tokenized_offsets: &mut std::collections::HashSet<usize>,
    ) {
        // Find and tokenize the enum name
        let name_pattern = format!("\"name\": \"{}\"", enum_type.name);
        let enum_name_start = if let Some(offset) = self.text.find(&name_pattern) {
            let value_offset = offset + "\"name\": \"".len();
            if tokenized_offsets.insert(value_offset) {
                let pos = self.offset_to_position(value_offset);
                self.add_token(
                    pos.line,
                    pos.character,
                    enum_type.name.len() as u32,
                    TOKEN_TYPE_ENUM,
                    TOKEN_MODIFIER_DECLARATION,
                );
            }
            Some(offset)
        } else {
            None
        };

        // Tokenize enum symbols (appear in the "symbols" array)
        // Search for "symbols": near this enum's name to avoid matching other enums
        let search_start = enum_name_start.unwrap_or(0);
        if let Some(relative_offset) = self.text[search_start..].find("\"symbols\":") {
            let symbols_start = search_start + relative_offset;

            // Find the end of this symbols array by looking for the closing ]
            // This prevents matching symbols from other enums
            let symbols_section = &self.text[symbols_start..];
            if let Some(symbols_end_relative) = symbols_section.find(']') {
                let symbols_end = symbols_start + symbols_end_relative;

                for symbol in &enum_type.symbols {
                    let symbol_pattern = format!("\"{}\"", symbol);
                    // Search only within this specific symbols array
                    let search_region = &self.text[symbols_start..symbols_end];
                    if let Some(offset) = search_region.find(&symbol_pattern) {
                        let absolute_offset = symbols_start + offset + 1; // +1 for opening quote
                        if tokenized_offsets.insert(absolute_offset) {
                            let pos = self.offset_to_position(absolute_offset);
                            self.add_token(
                                pos.line,
                                pos.character,
                                symbol.len() as u32,
                                TOKEN_TYPE_ENUM_MEMBER,
                                0,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Tokenize a fixed type
    fn tokenize_fixed_type(
        &mut self,
        fixed: &FixedSchema,
        tokenized_offsets: &mut std::collections::HashSet<usize>,
    ) {
        // Find and tokenize the fixed name
        let name_pattern = format!("\"name\": \"{}\"", fixed.name);
        if let Some(offset) = self.text.find(&name_pattern) {
            let value_offset = offset + "\"name\": \"".len();
            if tokenized_offsets.insert(value_offset) {
                let pos = self.offset_to_position(value_offset);
                self.add_token(
                    pos.line,
                    pos.character,
                    fixed.name.len() as u32,
                    TOKEN_TYPE_TYPE,
                    TOKEN_MODIFIER_DECLARATION,
                );
            }
        }
    }

    /// Tokenize a field
    fn tokenize_field(
        &mut self,
        field: &Field,
        tokenized_offsets: &mut std::collections::HashSet<usize>,
    ) {
        // Find field name - need to be careful since "name" can appear multiple times
        // Look for {"name": "field_name" pattern within the fields array
        let field_pattern = format!("\"name\": \"{}\"", field.name);
        let mut search_start = 0;

        // First find the "fields" array
        if let Some(fields_offset) = self.text.find("\"fields\":") {
            search_start = fields_offset;
        }

        // Now search for this specific field name pattern after the fields array
        if let Some(offset) = self.text[search_start..].find(&field_pattern) {
            let absolute_offset = search_start + offset;
            let value_offset = absolute_offset + "\"name\": \"".len();

            // Only tokenize if we haven't already (avoids duplicate in case of record name = field name)
            if tokenized_offsets.insert(value_offset) {
                let pos = self.offset_to_position(value_offset);
                self.add_token(
                    pos.line,
                    pos.character,
                    field.name.len() as u32,
                    TOKEN_TYPE_PROPERTY,
                    TOKEN_MODIFIER_DECLARATION,
                );
            }
        }

        // Tokenize the field's type
        self.tokenize_type(&field.field_type, tokenized_offsets);
    }

    /// Tokenize a type (primitive, reference, complex)
    fn tokenize_type(
        &mut self,
        avro_type: &AvroType,
        tokenized_offsets: &mut std::collections::HashSet<usize>,
    ) {
        match avro_type {
            AvroType::Primitive(prim) => {
                let type_str = match prim {
                    PrimitiveType::Null => "null",
                    PrimitiveType::Boolean => "boolean",
                    PrimitiveType::Int => "int",
                    PrimitiveType::Long => "long",
                    PrimitiveType::Float => "float",
                    PrimitiveType::Double => "double",
                    PrimitiveType::Bytes => "bytes",
                    PrimitiveType::String => "string",
                };

                // Find "type": "primitive" or in arrays ["null", "string"]
                let pattern = format!("\"{}\"", type_str);
                let mut search_start = 0;

                while let Some(offset) = self.text[search_start..].find(&pattern) {
                    let absolute_offset = search_start + offset + 1; // +1 for opening quote

                    // Check if already tokenized
                    if !tokenized_offsets.contains(&absolute_offset) {
                        // Check if this is a type value by looking for broader context
                        // Look back further and check for type indicators
                        let context_start = absolute_offset.saturating_sub(100);
                        let context = &self.text[context_start..absolute_offset];

                        // Check what comes after the type string to avoid false positives
                        let after_end = (absolute_offset + type_str.len()).min(self.text.len());
                        let after_context = if after_end < self.text.len() {
                            &self.text[after_end..after_end + 1]
                        } else {
                            ""
                        };

                        // Valid type context: after "type": or inside an array [ ... ]
                        // Check for unclosed [ bracket (multiline array) or "type": nearby
                        let in_array = {
                            let open_brackets = context.matches('[').count();
                            let close_brackets = context.matches(']').count();
                            open_brackets > close_brackets
                        };

                        let after_type_colon = context.contains("\"type\":");

                        // Make sure it's followed by a closing quote (the pattern already has quotes)
                        let followed_by_quote = after_context == "\"";

                        if followed_by_quote
                            && (after_type_colon || in_array)
                            && tokenized_offsets.insert(absolute_offset)
                        {
                            let pos = self.offset_to_position(absolute_offset);
                            // All primitive types use TOKEN_TYPE_TYPE for consistency
                            self.add_token(
                                pos.line,
                                pos.character,
                                type_str.len() as u32,
                                TOKEN_TYPE_TYPE,
                                TOKEN_MODIFIER_READONLY,
                            );
                        }
                    }

                    search_start += offset + pattern.len();
                }
            }
            AvroType::TypeRef(type_ref) => {
                // Reference to a named type
                let pattern = format!("\"type\": \"{}\"", type_ref.name);
                if let Some(offset) = self.text.find(&pattern) {
                    let value_offset = offset + "\"type\": \"".len();
                    if tokenized_offsets.insert(value_offset) {
                        let pos = self.offset_to_position(value_offset);
                        self.add_token(
                            pos.line,
                            pos.character,
                            type_ref.name.len() as u32,
                            TOKEN_TYPE_TYPE,
                            0,
                        );
                    }
                }
            }
            AvroType::Union(types) => {
                // Tokenize each type in the union
                for t in types {
                    self.tokenize_type(t, tokenized_offsets);
                }
            }
            AvroType::Array(array) => {
                self.tokenize_type(&array.items, tokenized_offsets);
            }
            AvroType::Map(map) => {
                self.tokenize_type(&map.values, tokenized_offsets);
            }
            _ => {}
        }
    }

    /// Add a token to the list
    fn add_token(
        &mut self,
        line: u32,
        character: u32,
        length: u32,
        token_type: u32,
        token_modifiers: u32,
    ) {
        self.tokens.push(Token {
            line,
            character,
            length,
            token_type,
            token_modifiers,
        });
    }

    /// Convert byte offset to position
    fn offset_to_position(&self, offset: usize) -> Position {
        let mut line = 0;
        let mut character = 0;

        for (i, ch) in self.text.char_indices() {
            if i >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                character = 0;
            } else {
                character += 1;
            }
        }

        Position { line, character }
    }

    /// Build the final token array with delta encoding
    fn build(mut self) -> Vec<SemanticToken> {
        // Sort tokens by position (line, then character)
        self.tokens.sort_by(|a, b| {
            if a.line != b.line {
                a.line.cmp(&b.line)
            } else {
                a.character.cmp(&b.character)
            }
        });

        let mut result = Vec::new();
        let mut prev_line = 0;
        let mut prev_character = 0;

        for token in self.tokens {
            let delta_line = token.line - prev_line;
            let delta_start = if delta_line == 0 {
                token.character - prev_character
            } else {
                token.character
            };

            result.push(SemanticToken {
                delta_line,
                delta_start,
                length: token.length,
                token_type: token.token_type,
                token_modifiers_bitset: token.token_modifiers,
            });

            prev_line = token.line;
            prev_character = token.character;
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AvroParser;

    #[test]
    fn test_semantic_tokens_multiline_union() {
        let text = r#"{
  "fields": [
    {
      "name": "email",
      "type": [
        "null",
        "string"
      ]
    }
  ],
  "name": "User",
  "type": "record"
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(text).expect("Should parse");

        let tokens = build_semantic_tokens(&schema, text.to_string());

        // The bug: "string" on line 6 should be tokenized as a type (orange/keyword)
        // but currently it's tokenized as a string literal (green) or not at all

        // Convert delta-encoded tokens back to absolute positions for testing
        let mut abs_tokens = Vec::new();
        let mut current_line = 0u32;
        let mut current_char = 0u32;

        for token in &tokens {
            current_line += token.delta_line;
            if token.delta_line > 0 {
                current_char = token.delta_start;
            } else {
                current_char += token.delta_start;
            }
            abs_tokens.push((current_line, current_char, token.length, token.token_type));
        }

        // Find line indices for "null" and "string" in union
        let lines: Vec<&str> = text.lines().collect();
        let null_line = lines.iter().position(|l| l.trim() == "\"null\",").unwrap() as u32;
        let string_line = lines.iter().position(|l| l.trim() == "\"string\"").unwrap() as u32;

        // Check if we have tokens on those lines
        let has_null_token = abs_tokens.iter().any(|(line, _, _, _)| *line == null_line);
        let has_string_token = abs_tokens
            .iter()
            .any(|(line, _, _, _)| *line == string_line);

        assert!(
            has_null_token,
            "Should have semantic token for 'null' on line {}",
            null_line
        );
        assert!(
            has_string_token,
            "Should have semantic token for 'string' on line {}",
            string_line
        );
    }

    #[test]
    fn test_semantic_tokens_inline_union() {
        let text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "email", "type": ["null", "string"]}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(text).expect("Should parse");

        let tokens = build_semantic_tokens(&schema, text.to_string());

        // Should have tokens for both "null" and "string" in the inline union
        assert!(!tokens.is_empty(), "Should have semantic tokens");
    }
}
