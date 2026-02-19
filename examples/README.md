# Print AST Example

This example demonstrates the avro-lsp parser by reading an `.avsc` file, parsing it into an Abstract Syntax Tree (AST), and displaying it in a human-readable format.

## What is an AST?

An Abstract Syntax Tree (AST) is a tree representation of the abstract syntactic structure of the Avro schema. The avro-lsp parser converts JSON text into a structured tree of Rust types that represent the schema's components (records, fields, types, etc.).

## Building

```bash
cargo build --example print_ast
```

## Usage

### Using just (recommended)

If you have [just](https://github.com/casey/just) installed:

```bash
# Tree view (default)
just print-ast schema.avsc

# JSON view
just print-ast-json schema.avsc
```

### Using cargo directly

Display the AST as an indented tree structure:

```bash
cargo run --example print_ast -- schema.avsc
```

Example output:
```
=== Avro Schema AST for 'schema.avsc' ===

File size: 135 bytes

Root AST (Tree view):

Record {
  name: "User"
  namespace: "com.example"
  range: 0:0 - 7:1
  fields: [
    Field {
      name: "name"
      type:
      Primitive(String)
    }
    Field {
      name: "age"
      type:
      Primitive(Int)
    }
  ]
}

Named Types Registry (1 types):
  - User

Semantic Tokens (13 tokens captured during parsing):

  Keyword (8 tokens):
    - 1:3-1:7
    - 1:11-1:17
    ... and 6 more
  Property (2 tokens):
    - 4:14-4:18 [modifiers: SemanticTokenModifiers(DECLARATION)]
    - 5:14-5:17 [modifiers: SemanticTokenModifiers(DECLARATION)]
  Type (2 tokens):
    - 4:30-4:36 [modifiers: SemanticTokenModifiers(READONLY)]
    - 5:29-5:32 [modifiers: SemanticTokenModifiers(READONLY)]
  Struct (1 tokens):
    - 2:11-2:15 [modifiers: SemanticTokenModifiers(DECLARATION)]

Code Action Suggestions:
  No errors detected. Refactoring actions are available:
  - Add documentation to record/enum/field
  - Add field to record
  - Make field nullable
  - Add default value
  - Sort fields alphabetically
```

### JSON View

Display the AST as formatted JSON (normalized representation):

Using just:
```bash
just print-ast-json schema.avsc
```

Using cargo:
```bash
cargo run --example print_ast -- --json schema.avsc
```

Example output:
```json
{
  "type": "record",
  "name": "User",
  "namespace": "com.example",
  "fields": [
    {
      "name": "name",
      "type": "string"
    },
    {
      "name": "age",
      "type": "int"
    }
  ]
}
```

## Try It Out

The repository includes test fixtures you can use to explore different schema types:

```bash
# Simple record with basic fields
just print-ast tests/fixtures/valid/simple_record.avsc

# Complex schema with all Avro types
just print-ast tests/fixtures/valid/comprehensive_types.avsc

# Enums
just print-ast tests/fixtures/valid/enum_example.avsc

# Unions (nullable fields)
just print-ast tests/fixtures/valid/union_example.avsc

# Arrays and Maps
just print-ast tests/fixtures/valid/array_map_example.avsc

# Nested records
just print-ast tests/fixtures/valid/nested_record.avsc

# Logical types (timestamps, dates, UUIDs, decimals)
just print-ast tests/fixtures/valid/logical_types.avsc

# JSON output format
just print-ast-json tests/fixtures/valid/simple_record.avsc
```

Or using cargo directly:

```bash
cargo run --example print_ast -- tests/fixtures/valid/simple_record.avsc
```

## What Gets Displayed?

The AST display includes:

1. **Root Type** - The main schema type (usually a Record)
2. **Named Types Registry** - All named types defined in the schema (Records, Enums, Fixed types)
3. **Semantic Tokens** - Count of tokens captured **during parsing** for syntax highlighting
4. **Parse Errors** - Any validation errors found during parsing (with suggestions for quick fixes)
5. **Warnings** - Non-blocking issues like deprecated features

### Semantic Tokens

The parser **automatically captures semantic tokens inline during parsing** (not in a separate pass). These tokens enable rich syntax highlighting in IDEs:

- **Keywords**: JSON property names ("type", "name", "fields", "namespace", "doc", etc.)
- **Types**: Primitive types, logical types, custom type names
- **Enum/Struct declarations**: Named type definitions
- **Properties**: Field names
- **Enum members**: Individual enum symbol values

Each token includes:
- Position range (line:column)
- Token type (Keyword, Type, Enum, Struct, Property, EnumMember)
- Modifiers (DECLARATION, READONLY)

The semantic tokens are later converted to LSP format in `src/handlers/semantic_tokens.rs` for editor integration.

## Understanding the AST Structure

### Position Tracking

Each node includes optional `range` fields showing the source location:
```
range: 0:0 - 7:1
       │ │   │ │
       │ │   │ └─ end character
       │ │   └─── end line
       │ └────── start character
       └──────── start line
```

### AST Node Types

The parser creates these AST node types:

- **Primitive** - Built-in types: null, boolean, int, long, float, double, bytes, string
- **PrimitiveObject** - Primitive with logical type metadata (e.g., timestamp-millis)
- **Record** - Named type with fields
- **Enum** - Named type with symbols
- **Array** - Container holding items of a single type
- **Map** - Key-value container (keys are always strings)
- **Union** - Multiple possible types (e.g., `["null", "string"]`)
- **Fixed** - Fixed-size byte sequence
- **TypeRef** - Reference to a named type defined elsewhere
- **Invalid** - Error recovery node for invalid types (enables partial parsing)

## Error Handling

The parser uses error recovery to build a partial AST even when there are validation errors:

```bash
cargo run --example print_ast -- tests/fixtures/invalid/primitive_typo_boolean.avsc
```

This will show:
- **Parse errors with suggestions** (e.g., "boolena" → "boolean")
- A **partial AST** with `Invalid` nodes where errors occurred
- Allows analysis of broken schemas without failing completely

These parse errors can be used to generate **code actions (quick fixes)** in the LSP:
- The LSP provides automatic "Quick Fix" actions based on these errors
- For example: "Replace 'boolena' with 'boolean'"
- Code actions are implemented in `src/handlers/code_actions/quick_fixes/`

### Code Actions Available

The LSP provides two types of code actions:

**Quick Fixes** (error-based):
- Fix invalid primitive types
- Fix invalid names/namespaces
- Fix nested unions
- Fix duplicate symbols
- Fix invalid logical types
- Fix decimal precision/scale
- Fix invalid default values

**Refactorings** (position-based):
- Add documentation
- Add field to record
- Make field nullable
- Add default value
- Sort fields alphabetically

## Use Cases

This example is useful for:

1. **Understanding the parser** - See how JSON schemas map to structured AST nodes
2. **Debugging schemas** - Visualize complex nested structures
3. **Presentations/Talks** - Show how the LSP parses Avro schemas
4. **Learning Avro** - Explore how different schema patterns are represented
5. **Testing parser changes** - Verify AST structure after modifications

## Implementation Notes

The example uses:
- `AvroParser::parse()` to convert JSON text → AST
- Position tracking (LSP Range types) for each node
- Serde for JSON serialization of the normalized AST
- Pretty-printing with recursive tree traversal
