# avro-lsp Parser Demo - Quick Reference

This is a quick reference guide for demonstrating the avro-lsp parser during presentations.

## Quick Start

```bash
# Parse and show AST (tree view)
just print-ast schema.avsc

# Parse and show AST (JSON view)
just print-ast-json schema.avsc
```

## Demo Examples

### 1. Simple Record
```bash
just print-ast tests/fixtures/valid/simple_record.avsc
```

Shows:
- Basic record structure
- Primitive types (string, int)
- Position tracking (line:column ranges)
- Named types registry
- Semantic tokens count

### 2. Complex Schema with All Types
```bash
just print-ast tests/fixtures/valid/comprehensive_types.avsc
```

Demonstrates:
- All primitive types (null, boolean, int, long, float, double, bytes, string)
- Logical types (date, time, timestamp, uuid, decimal, duration)
- Complex types (record, enum, array, map, union, fixed)
- Nested structures
- Documentation fields

### 3. Error Recovery
```bash
just print-ast tests/fixtures/invalid/primitive_typo_boolean.avsc
```

Shows:
- Parse errors with suggestions ("boolena" → "boolean")
- Partial AST with `Invalid` nodes
- Continued parsing despite errors
- Error position tracking

### 4. JSON Normalized Output
```bash
just print-ast-json tests/fixtures/valid/simple_record.avsc
```

Demonstrates:
- Normalized JSON representation
- Serde serialization of AST
- Clean schema output

## Key Features to Highlight

### 1. Position Tracking
Every AST node includes source location:
```
range: 0:0 - 7:1
```
This enables IDE features like:
- Jump to definition
- Hover information
- Error diagnostics
- Code actions

### 2. Semantic Tokens (Captured During Parsing)
The parser **automatically captures semantic tokens** as it parses, enabling rich syntax highlighting:
- **Keywords**: "type", "name", "fields", "namespace", "doc", etc.
- **Types**: primitive types ("string", "int"), logical types ("timestamp-millis")
- **Struct names**: Record type declarations
- **Enum names**: Enum type declarations
- **Enum members**: Individual enum symbols
- **Properties**: Field names
- **Modifiers**: DECLARATION, READONLY flags

These tokens are collected in `schema.semantic_tokens` and converted to LSP format in `src/handlers/semantic_tokens.rs`.

**Example**: Parsing `simple_record.avsc` captures 13 semantic tokens for keywords, type names, and field names.

### 3. Code Actions (Quick Fixes & Refactorings)
The LSP provides **automatic code actions** based on parse errors and AST structure:

**Quick Fixes** (from validation errors):
- Fix invalid primitive type typos ("boolena" → "boolean")
- Fix invalid names (add suggestions)
- Fix nested unions
- Fix duplicate symbols
- Fix invalid logical types
- Fix missing required fields
- Fix invalid default values
- Fix decimal precision/scale issues

**Refactoring Actions** (position-based):
- Add documentation to record/enum/field
- Add field to record
- Make field nullable (wrap in union)
- Add default value to field
- Sort fields alphabetically

Code actions are implemented in `src/handlers/code_actions/`:
- `quick_fixes/` - Error-based fixes
- `refactoring/` - Context-aware refactorings

### 4. Error Recovery
The parser builds a partial AST even with errors:
- Allows IDE to work with incomplete/broken schemas
- Provides suggestions for typos
- Continues validation after errors

### Semantic Tokens (Captured During Parsing)
The parser **automatically captures semantic tokens** as it parses, enabling rich syntax highlighting:
- **Keywords**: "type", "name", "fields", "namespace", "doc", etc.
- **Types**: primitive types ("string", "int"), logical types ("timestamp-millis")
- **Struct names**: Record type declarations
- **Enum names**: Enum type declarations
- **Enum members**: Individual enum symbols
- **Properties**: Field names
- **Modifiers**: DECLARATION, READONLY flags

These tokens are collected in `schema.semantic_tokens` and converted to LSP format in `src/handlers/semantic_tokens.rs`.

**Example**: Parsing `simple_record.avsc` captures 13 semantic tokens for keywords, type names, and field names.

### AST Node Types

| Type | Description | Example |
|------|-------------|---------|
| `Primitive` | Built-in types | `"string"`, `"int"` |
| `PrimitiveObject` | Logical types | `{"type": "long", "logicalType": "timestamp-millis"}` |
| `Record` | Named type with fields | User, Address |
| `Enum` | Named type with symbols | Status, Color |
| `Array` | Collection of items | `{"type": "array", "items": "string"}` |
| `Map` | Key-value pairs | `{"type": "map", "values": "int"}` |
| `Union` | Multiple types | `["null", "string"]` |
| `Fixed` | Fixed-size bytes | MD5Hash, UUID |
| `TypeRef` | Reference to named type | `"Address"` in another field |
| `Invalid` | Error recovery node | For broken schemas |

## Parser Architecture

```
JSON Text
    ↓
JSON Parser (with position tracking)
    ↓
Avro Parser (captures semantic tokens inline)
    ↓
AST (AvroType tree) + Semantic Tokens + Parse Errors
    ↓
Validator (produces warnings & diagnostics)
    ↓
LSP Features:
  - Diagnostics (errors with ranges)
  - Semantic Tokens (for syntax highlighting)
  - Code Actions (quick fixes + refactorings)
  - Hover (type info)
  - Completion
  - Go to Definition
  - Formatting
```

**Key Implementation Files**:
- `src/schema/json_parser.rs` - JSON parser with position tracking
- `src/schema/parser.rs` - Avro parser that captures semantic tokens inline
- `src/schema/types.rs` - AST type definitions
- `src/schema/validator/` - Validation rules
- `src/handlers/semantic_tokens.rs` - Convert tokens to LSP format
- `src/handlers/code_actions/` - Quick fixes and refactorings

## Code Location

- **Parser**: `src/schema/parser.rs`
- **AST Types**: `src/schema/types.rs`
- **Example Script**: `examples/print_ast.rs`

## Common Use Cases

1. **Understanding schemas**: Visualize complex nested structures
2. **Debugging**: See exactly how the parser interprets your schema
3. **Learning Avro**: Explore different schema patterns
4. **Testing**: Verify parser behavior with different inputs
5. **Presentations**: Show AST structure during talks

## CLI Alternative

The LSP also includes a CLI linting mode:

```bash
# Lint a single file
cargo run -- lint schema.avsc

# Lint a directory (recursive)
cargo run -- lint schemas/

# Lint with workspace mode (cross-file type resolution)
cargo run -- lint --workspace schemas/
```

This provides beautiful error output with miette:
- Source code snippets
- Syntax highlighting
- Color-coded severity
- Clear error messages with context
