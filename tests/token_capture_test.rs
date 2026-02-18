/// Test to verify tokens are being captured during parsing
use avro_lsp::schema::parser::AvroParser;

#[test]
fn test_tokens_are_captured() {
    let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "long"}
  ]
}"#;

    let mut parser = AvroParser::new();
    let schema = parser.parse(schema_text).expect("Failed to parse schema");

    // We should have captured tokens for:
    // - "type" keyword (line 1)
    // - "record" value (line 1)
    // - "name" keyword (line 2)
    // - "User" value (line 2)
    // - "fields" keyword (line 3)
    // - "name" keyword (line 4)
    // - "id" value (line 4)
    // - "type" keyword (line 4)
    // - "long" value (line 4)

    println!("Captured {} tokens", schema.semantic_tokens.len());
    for (i, token) in schema.semantic_tokens.iter().enumerate() {
        println!(
            "  Token {}: Line {}, Char {} (type: {:?})",
            i, token.range.start.line, token.range.start.character, token.token_type
        );
    }

    assert!(
        schema.semantic_tokens.len() > 0,
        "Should have captured some tokens!"
    );
    assert!(
        schema.semantic_tokens.len() >= 9,
        "Should have captured at least 9 tokens, got {}",
        schema.semantic_tokens.len()
    );
}

#[test]
fn test_enum_symbols_captured() {
    let schema_text = r#"{
  "type": "enum",
  "name": "Color",
  "symbols": ["RED", "GREEN", "BLUE"]
}"#;

    let mut parser = AvroParser::new();
    let schema = parser.parse(schema_text).expect("Failed to parse schema");

    println!("Captured {} tokens for enum", schema.semantic_tokens.len());

    // Should capture: type, enum, name, Color, symbols, RED, GREEN, BLUE
    assert!(
        schema.semantic_tokens.len() >= 8,
        "Should have captured at least 8 tokens, got {}",
        schema.semantic_tokens.len()
    );

    // Check that we have enum member tokens
    let enum_member_tokens: Vec<_> = schema
        .semantic_tokens
        .iter()
        .filter(|t| {
            matches!(
                t.token_type,
                avro_lsp::schema::SemanticTokenType::EnumMember
            )
        })
        .collect();

    assert_eq!(
        enum_member_tokens.len(),
        3,
        "Should have 3 enum member tokens for RED, GREEN, BLUE"
    );
}
