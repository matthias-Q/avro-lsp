use crate::schema::AvroSchema;
use async_lsp::lsp_types::SemanticToken;

// Token type indices (must match server.rs capabilities)
const TOKEN_TYPE_KEYWORD: u32 = 0;
const TOKEN_TYPE_TYPE: u32 = 1;
const TOKEN_TYPE_ENUM: u32 = 2;
const TOKEN_TYPE_STRUCT: u32 = 3;
const TOKEN_TYPE_PROPERTY: u32 = 4;
const TOKEN_TYPE_ENUM_MEMBER: u32 = 5;

/// Token modifiers (bit flags)
#[allow(dead_code)]
const TOKEN_MODIFIER_DECLARATION: u32 = 0x01;
#[allow(dead_code)]
const TOKEN_MODIFIER_READONLY: u32 = 0x02;

/// Internal token representation with absolute positions
#[derive(Debug, Clone)]
struct Token {
    line: u32,
    character: u32,
    length: u32,
    token_type: u32,
    token_modifiers: u32,
}

/// Build semantic tokens from an Avro schema
/// Tokens are captured during parsing and converted to LSP format here
pub fn build_semantic_tokens(schema: &AvroSchema) -> Vec<SemanticToken> {
    use crate::schema::SemanticTokenType;

    // Convert SemanticTokenData to internal Token format
    let mut tokens: Vec<Token> = schema
        .semantic_tokens
        .iter()
        .map(|token_data| {
            // Map our token types to LSP token type indices
            let token_type = match token_data.token_type {
                SemanticTokenType::Keyword => TOKEN_TYPE_KEYWORD,
                SemanticTokenType::Type => TOKEN_TYPE_TYPE,
                SemanticTokenType::Enum => TOKEN_TYPE_ENUM,
                SemanticTokenType::Struct => TOKEN_TYPE_STRUCT,
                SemanticTokenType::Property => TOKEN_TYPE_PROPERTY,
                SemanticTokenType::EnumMember => TOKEN_TYPE_ENUM_MEMBER,
            };

            // Calculate token length from range
            let length = (token_data.range.end.character - token_data.range.start.character) as u32;

            Token {
                line: token_data.range.start.line,
                character: token_data.range.start.character,
                length,
                token_type,
                token_modifiers: token_data.modifiers.bits(),
            }
        })
        .collect();

    // Sort tokens by position (required by LSP)
    tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.character.cmp(&b.character)));

    // Apply delta encoding
    delta_encode_tokens(tokens)
}

/// Apply LSP delta encoding to tokens
/// Converts absolute positions to relative (delta) positions
fn delta_encode_tokens(tokens: Vec<Token>) -> Vec<SemanticToken> {
    let mut result = Vec::new();
    let mut prev_line = 0;
    let mut prev_character = 0;

    for token in tokens {
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
