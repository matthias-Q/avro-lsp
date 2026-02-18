use avro_lsp::handlers::semantic_tokens::build_semantic_tokens;
/// Integration tests to verify semantic token implementation
/// Tests that the AST-based implementation produces correct token output
use avro_lsp::schema::parser::AvroParser;

#[test]
fn test_tokens_simple_record() {
    let schema_text = std::fs::read_to_string("tests/fixtures/valid/simple_record.avsc")
        .expect("Failed to read test fixture");

    let mut parser = AvroParser::new();
    let schema = parser.parse(&schema_text).expect("Failed to parse schema");

    let tokens = build_semantic_tokens(&schema);

    println!("simple_record: {} tokens", tokens.len());

    // Should produce tokens for keywords, types, names, etc.
    assert!(
        tokens.len() >= 10,
        "Should capture at least 10 tokens for simple record, got {}",
        tokens.len()
    );
}

#[test]
fn test_tokens_enum_example() {
    let schema_text = std::fs::read_to_string("tests/fixtures/valid/enum_example.avsc")
        .expect("Failed to read test fixture");

    let mut parser = AvroParser::new();
    let schema = parser.parse(&schema_text).expect("Failed to parse schema");

    let tokens = build_semantic_tokens(&schema);

    println!("enum_example: {} tokens", tokens.len());

    assert!(
        tokens.len() >= 5,
        "Should capture enum tokens, got {}",
        tokens.len()
    );
}

#[test]
fn test_tokens_comprehensive_types() {
    let schema_text = std::fs::read_to_string("tests/fixtures/valid/comprehensive_types.avsc")
        .expect("Failed to read test fixture");

    let mut parser = AvroParser::new();
    let schema = parser.parse(&schema_text).expect("Failed to parse schema");

    let tokens = build_semantic_tokens(&schema);

    println!("comprehensive_types: {} tokens", tokens.len());

    // Should capture substantial semantic information
    assert!(
        tokens.len() >= 100,
        "Should capture many tokens for comprehensive schema, got {}",
        tokens.len()
    );
}

#[test]
fn test_tokens_logical_types() {
    let schema_text = std::fs::read_to_string("tests/fixtures/valid/logical_types.avsc")
        .expect("Failed to read test fixture");

    let mut parser = AvroParser::new();
    let schema = parser.parse(&schema_text).expect("Failed to parse schema");

    let tokens = build_semantic_tokens(&schema);

    println!("logical_types: {} tokens", tokens.len());

    assert!(
        tokens.len() >= 15,
        "Should capture logical type tokens, got {}",
        tokens.len()
    );
}

#[test]
fn test_tokens_are_delta_encoded() {
    let schema_text = r#"{
  "type": "record",
  "name": "Test",
  "fields": [
    {"name": "a", "type": "int"},
    {"name": "b", "type": "string"}
  ]
}"#;

    let mut parser = AvroParser::new();
    let schema = parser.parse(schema_text).expect("Failed to parse schema");

    let tokens = build_semantic_tokens(&schema);

    // Verify tokens are delta encoded (not all starting from 0)
    assert!(tokens.len() > 0);

    // First token should have delta_line and delta_start
    // Subsequent tokens should have deltas relative to previous
    let mut had_non_zero_delta_start = false;
    for token in &tokens[1..] {
        if token.delta_start > 0 {
            had_non_zero_delta_start = true;
            break;
        }
    }

    assert!(
        had_non_zero_delta_start,
        "Tokens should be delta encoded with non-zero delta_start values"
    );
}
