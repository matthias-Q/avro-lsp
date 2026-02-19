//! Print AST - Example script for parsing and displaying Avro schema AST
//!
//! Usage:
//!     cargo run --example print_ast -- <schema.avsc>
//!     cargo run --example print_ast -- --json <schema.avsc>
//!
//! This script demonstrates the avro-lsp parser by:
//! 1. Reading an .avsc file
//! 2. Parsing it into an AST (Abstract Syntax Tree)
//! 3. Capturing semantic tokens during parsing (for syntax highlighting)
//! 4. Pretty-printing the AST structure
//!
//! The parser automatically captures:
//! - Semantic tokens (for syntax highlighting in IDEs)
//! - Position ranges for every node (for LSP features)
//! - Parse errors with suggestions (for diagnostics and quick fixes)
//!
//! Use the --json flag to output the AST as formatted JSON instead of the debug tree view.

use std::env;
use std::fs;
use std::process;

use avro_lsp::schema;
use avro_lsp::schema::parser::AvroParser;
use avro_lsp::schema::types::AvroType;
use avro_lsp::schema::{AvroValidator, SchemaError};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} [--json] <schema.avsc>", args[0]);
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  cargo run --example print_ast -- schema.avsc");
        eprintln!("  cargo run --example print_ast -- --json schema.avsc");
        process::exit(1);
    }

    let (json_output, file_path) = if args.len() == 3 && args[1] == "--json" {
        (true, &args[2])
    } else if args.len() == 2 {
        (false, &args[1])
    } else {
        eprintln!("Invalid arguments");
        eprintln!("Usage: {} [--json] <schema.avsc>", args[0]);
        process::exit(1);
    };

    // Read the schema file
    let content = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", file_path, e);
            process::exit(1);
        }
    };

    // Parse the schema
    let mut parser = AvroParser::new();
    let schema = match parser.parse(&content) {
        Ok(schema) => schema,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            process::exit(1);
        }
    };

    // Validate the schema to catch additional errors
    let validator = AvroValidator::new();
    let validation_error = validator.validate(&schema).err();

    println!("=== Avro Schema AST for '{}' ===", file_path);
    println!();
    println!("File size: {} bytes", content.len());
    println!();

    // Print parse errors if any
    if !schema.parse_errors.is_empty() {
        println!("Parse Errors ({}):", schema.parse_errors.len());
        for error in &schema.parse_errors {
            println!("  - {:?}", error);
        }
        println!();
    }

    // Print validation errors if any
    if let Some(validation_err) = &validation_error {
        println!("Validation Errors (1):");
        println!("  - {:?}", validation_err);
        println!();
    }

    // Print warnings if any
    if !schema.warnings.is_empty() {
        println!("Warnings ({}):", schema.warnings.len());
        for warning in &schema.warnings {
            println!("  - {:?}", warning);
        }
        println!();
    }

    if json_output {
        // Output as formatted JSON
        println!("Root AST (JSON format):");
        println!();
        match serde_json::to_string_pretty(&schema.root) {
            Ok(json) => println!("{}", json),
            Err(e) => {
                eprintln!("Error serializing AST to JSON: {}", e);
                process::exit(1);
            }
        }
    } else {
        // Output as pretty-printed tree
        println!("Root AST (Tree view):");
        println!();
        print_ast(&schema.root, 0);

        // Print named types registry
        if !schema.named_types.is_empty() {
            println!();
            println!("Named Types Registry ({} types):", schema.named_types.len());
            for (name, _type) in &schema.named_types {
                println!("  - {}", name);
            }
        }

        // Print semantic tokens
        if !schema.semantic_tokens.is_empty() {
            println!();
            println!(
                "Semantic Tokens ({} tokens captured during parsing):",
                schema.semantic_tokens.len()
            );
            println!();

            // Group tokens by type for better readability
            use std::collections::HashMap;
            let mut tokens_by_type: HashMap<String, Vec<&schema::SemanticTokenData>> =
                HashMap::new();

            for token in &schema.semantic_tokens {
                let type_name = format!("{:?}", token.token_type);
                tokens_by_type
                    .entry(type_name)
                    .or_insert_with(Vec::new)
                    .push(token);
            }

            // Print summary by type
            for (token_type, tokens) in tokens_by_type.iter() {
                println!("  {} ({} tokens):", token_type, tokens.len());
                for token in tokens.iter().take(5) {
                    let modifiers = if token.modifiers.bits() == 0 {
                        String::new()
                    } else {
                        format!(" [modifiers: {:?}]", token.modifiers)
                    };
                    println!(
                        "    - {}:{}-{}:{}{}",
                        token.range.start.line,
                        token.range.start.character,
                        token.range.end.line,
                        token.range.end.character,
                        modifiers
                    );
                }
                if tokens.len() > 5 {
                    println!("    ... and {} more", tokens.len() - 5);
                }
            }
        }
    }

    // Print code action suggestions
    let has_errors = !schema.parse_errors.is_empty() || validation_error.is_some();

    if has_errors {
        println!();
        println!("Code Action Suggestions:");

        let total_errors =
            schema.parse_errors.len() + if validation_error.is_some() { 1 } else { 0 };
        println!(
            "  The LSP would provide quick fixes for the {} error(s) above:",
            total_errors
        );

        // Show quick fixes for parse errors
        for error in &schema.parse_errors {
            match error {
                SchemaError::InvalidPrimitiveType {
                    type_name,
                    suggested,
                    ..
                } => {
                    if let Some(suggestion) = suggested {
                        println!(
                            "  - Quick Fix: Replace '{}' with '{}'",
                            type_name, suggestion
                        );
                    }
                }
                SchemaError::UnknownField {
                    field, suggested, ..
                } => {
                    if let Some(suggestion) = suggested {
                        println!("  - Quick Fix: Replace '{}' with '{}'", field, suggestion);
                    } else {
                        println!("  - Quick Fix: Remove unknown field '{}'", field);
                    }
                }
                SchemaError::DuplicateJsonKey { key, .. } => {
                    println!("  - Quick Fix: Remove duplicate key '{}'", key);
                }
                _ => {}
            }
        }

        // Show quick fixes for validation error
        if let Some(error) = &validation_error {
            match error {
                SchemaError::InvalidName {
                    name, suggested, ..
                } => {
                    if let Some(suggestion) = suggested {
                        println!("  - Quick Fix: Replace '{}' with '{}'", name, suggestion);
                    } else {
                        println!("  - Quick Fix: Fix invalid name '{}'", name);
                    }
                }
                SchemaError::InvalidNamespace {
                    namespace,
                    suggested,
                    ..
                } => {
                    if let Some(suggestion) = suggested {
                        println!(
                            "  - Quick Fix: Replace namespace '{}' with '{}'",
                            namespace, suggestion
                        );
                    } else {
                        println!("  - Quick Fix: Fix invalid namespace '{}'", namespace);
                    }
                }
                SchemaError::NestedUnion { .. } => {
                    println!("  - Quick Fix: Flatten nested union");
                }
                SchemaError::DuplicateSymbol { symbol, .. } => {
                    println!("  - Quick Fix: Remove duplicate symbol '{}'", symbol);
                }
                SchemaError::DuplicateUnionType { .. } => {
                    println!("  - Quick Fix: Remove duplicate type from union");
                }
                SchemaError::MissingField { field, .. } => {
                    println!("  - Quick Fix: Add missing field '{}'", field);
                }
                SchemaError::DuplicateFieldName { field, .. } => {
                    println!("  - Quick Fix: Rename duplicate field '{}'", field);
                }
                SchemaError::UnknownTypeReference { type_name, .. } => {
                    println!("  - Quick Fix: Define missing type '{}'", type_name);
                }
                _ => {
                    println!("  - Quick Fix available for this error");
                }
            }
        }

        println!();
        println!("  Refactoring actions available (when cursor is on a node):");
        println!("  - Add documentation to record/enum/field");
        println!("  - Add field to record");
        println!("  - Make field nullable");
        println!("  - Add default value");
        println!("  - Sort fields alphabetically");
    } else {
        println!();
        println!("Code Action Suggestions:");
        println!("  No errors detected. Refactoring actions are available:");
        println!("  - Add documentation to record/enum/field");
        println!("  - Add field to record");
        println!("  - Make field nullable");
        println!("  - Add default value");
        println!("  - Sort fields alphabetically");
    }

    // Exit with error code if there were parse or validation errors
    if has_errors {
        process::exit(1);
    }
}

/// Pretty-print the AST with indentation
fn print_ast(ast: &AvroType, indent: usize) {
    let prefix = "  ".repeat(indent);

    match ast {
        AvroType::Primitive(prim) => {
            println!("{}Primitive({:?})", prefix, prim);
        }

        AvroType::PrimitiveObject(prim_schema) => {
            println!("{}PrimitiveObject {{", prefix);
            println!("{}  type: {:?}", prefix, prim_schema.primitive_type);
            if let Some(ref logical_type) = prim_schema.logical_type {
                println!("{}  logicalType: {:?}", prefix, logical_type);
            }
            if let Some(precision) = prim_schema.precision {
                println!("{}  precision: {}", prefix, precision);
            }
            if let Some(scale) = prim_schema.scale {
                println!("{}  scale: {}", prefix, scale);
            }
            if let Some(range) = prim_schema.range {
                println!(
                    "{}  range: {}:{} - {}:{}",
                    prefix,
                    range.start.line,
                    range.start.character,
                    range.end.line,
                    range.end.character
                );
            }
            println!("{}}}", prefix);
        }

        AvroType::Record(record) => {
            println!("{}Record {{", prefix);
            println!("{}  name: {:?}", prefix, record.name);
            if let Some(ref ns) = record.namespace {
                println!("{}  namespace: {:?}", prefix, ns);
            }
            if let Some(ref doc) = record.doc {
                println!("{}  doc: {:?}", prefix, doc);
            }
            if let Some(ref aliases) = record.aliases {
                println!("{}  aliases: {:?}", prefix, aliases);
            }
            if let Some(range) = record.range {
                println!(
                    "{}  range: {}:{} - {}:{}",
                    prefix,
                    range.start.line,
                    range.start.character,
                    range.end.line,
                    range.end.character
                );
            }
            println!("{}  fields: [", prefix);
            for field in &record.fields {
                println!("{}    Field {{", prefix);
                println!("{}      name: {:?}", prefix, field.name);
                if let Some(ref doc) = field.doc {
                    println!("{}      doc: {:?}", prefix, doc);
                }
                if let Some(ref default) = field.default {
                    println!("{}      default: {}", prefix, default);
                }
                println!("{}      type:", prefix);
                print_ast(&field.field_type, indent + 3);
                println!("{}    }}", prefix);
            }
            println!("{}  ]", prefix);
            println!("{}}}", prefix);
        }

        AvroType::Enum(enum_schema) => {
            println!("{}Enum {{", prefix);
            println!("{}  name: {:?}", prefix, enum_schema.name);
            if let Some(ref ns) = enum_schema.namespace {
                println!("{}  namespace: {:?}", prefix, ns);
            }
            if let Some(ref doc) = enum_schema.doc {
                println!("{}  doc: {:?}", prefix, doc);
            }
            println!("{}  symbols: {:?}", prefix, enum_schema.symbols);
            if let Some(ref default) = enum_schema.default {
                println!("{}  default: {:?}", prefix, default);
            }
            if let Some(range) = enum_schema.range {
                println!(
                    "{}  range: {}:{} - {}:{}",
                    prefix,
                    range.start.line,
                    range.start.character,
                    range.end.line,
                    range.end.character
                );
            }
            println!("{}}}", prefix);
        }

        AvroType::Array(array) => {
            println!("{}Array {{", prefix);
            println!("{}  items:", prefix);
            print_ast(&array.items, indent + 2);
            if let Some(ref default) = array.default {
                println!("{}  default: {:?}", prefix, default);
            }
            println!("{}}}", prefix);
        }

        AvroType::Map(map) => {
            println!("{}Map {{", prefix);
            println!("{}  values:", prefix);
            print_ast(&map.values, indent + 2);
            if let Some(ref default) = map.default {
                println!("{}  default: {:?}", prefix, default);
            }
            println!("{}}}", prefix);
        }

        AvroType::Union(union) => {
            println!("{}Union {{", prefix);
            println!("{}  types: [", prefix);
            for union_type in &union.types {
                print_ast(union_type, indent + 2);
            }
            println!("{}  ]", prefix);
            if let Some(range) = union.range {
                println!(
                    "{}  range: {}:{} - {}:{}",
                    prefix,
                    range.start.line,
                    range.start.character,
                    range.end.line,
                    range.end.character
                );
            }
            println!("{}}}", prefix);
        }

        AvroType::Fixed(fixed) => {
            println!("{}Fixed {{", prefix);
            println!("{}  name: {:?}", prefix, fixed.name);
            if let Some(ref ns) = fixed.namespace {
                println!("{}  namespace: {:?}", prefix, ns);
            }
            println!("{}  size: {}", prefix, fixed.size);
            if let Some(ref logical_type) = fixed.logical_type {
                println!("{}  logicalType: {:?}", prefix, logical_type);
            }
            if let Some(precision) = fixed.precision {
                println!("{}  precision: {}", prefix, precision);
            }
            if let Some(scale) = fixed.scale {
                println!("{}  scale: {}", prefix, scale);
            }
            if let Some(range) = fixed.range {
                println!(
                    "{}  range: {}:{} - {}:{}",
                    prefix,
                    range.start.line,
                    range.start.character,
                    range.end.line,
                    range.end.character
                );
            }
            println!("{}}}", prefix);
        }

        AvroType::TypeRef(type_ref) => {
            println!("{}TypeRef({})", prefix, type_ref.name);
            if let Some(range) = type_ref.range {
                println!(
                    "{}  range: {}:{} - {}:{}",
                    prefix,
                    range.start.line,
                    range.start.character,
                    range.end.line,
                    range.end.character
                );
            }
        }

        AvroType::Invalid(invalid) => {
            println!("{}Invalid {{", prefix);
            println!("{}  type_name: {:?}", prefix, invalid.type_name);
            if let Some(range) = invalid.range {
                println!(
                    "{}  range: {}:{} - {}:{}",
                    prefix,
                    range.start.line,
                    range.start.character,
                    range.end.line,
                    range.end.character
                );
            }
            println!("{}}}", prefix);
        }
    }
}
