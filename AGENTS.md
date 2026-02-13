# AGENTS.md - Developer Guide for avro-lsp

This guide provides essential information for AI coding agents and human developers working on avro-lsp, a Language Server Protocol implementation for editing Apache Avro schema files (`.avsc`).

**Project Goal**: Provide IDE-like features (diagnostics, validation, hover, semantic highlighting, completion, go to definition, formatting, code actions) for `.avsc` files in editors like Neovim.

**Current Status**: Phase 3C Complete ✅ - Code actions implemented

**Tech Stack**: Rust (edition 2024), async-lsp framework, serde/serde_json for parsing, tokio async runtime

**Avro Specification**: https://avro.apache.org/docs/1.11.1/specification/

## ✅ Completed Features

### Phase 1A - Core Validation ✅
- Real-time diagnostics with precise error positioning
- JSON syntax validation
- Schema validation (names, namespaces, type references, union constraints)

### Phase 1B - IDE Features ✅
- **Hover information** - Rich type details for all schema elements
- **Document symbols** - Hierarchical outline view
- **Semantic tokens** - Meaning-aware syntax highlighting

### Phase 2 - Navigation & Completion ✅
- **Auto-completion** - Context-aware suggestions with snippet support
  - Smart cursor positioning inside quotes and brackets
  - Suggests keys, types, and named type references
  - Triggered by `"`, `:`, `,`
- **Go to definition** - Navigate to type declarations
  - Jump to definition by clicking on type references
  - Works for records, enums, and fixed types

### Phase 3A - Document Formatting ✅
- **Document formatting** - Format `.avsc` files with consistent style
  - Uses 2-space indentation (standard JSON formatting)
  - Automatically removes trailing commas (invalid JSON)
  - Returns error for invalid JSON
  - Idempotent formatting (format twice = same result)
  - Integrated with editor format commands (e.g., `:lua vim.lsp.buf.format()` in Neovim)

### Phase 3C - Code Actions ✅
- **Code actions** - Context-aware refactoring and quick fixes
  - **Add field to record** - Insert new field scaffold in fields array
  - **Add documentation** - Add doc field to record/enum/fixed definitions
  - **Make field nullable** - Wrap field type in union with null
  - Actions appear contextually based on cursor position
  - Integrated with editor code action commands (e.g., `:lua vim.lsp.buf.code_action()` in Neovim)

---

## Build & Development Commands

### Building
```bash
cargo build              # Debug build
cargo build --release    # Optimized release build
cargo check             # Fast syntax/type check without codegen
cargo clean             # Remove build artifacts
```

### Running
```bash
cargo run               # Run the LSP server (debug mode)
cargo run --release     # Run optimized version
```

### Code Quality
```bash
cargo fmt               # Auto-format all code
cargo fmt -- --check    # Check formatting without modifying files
cargo clippy            # Run linter for common mistakes
cargo clippy -- -D warnings  # Treat all warnings as errors (CI mode)
```

---

## Testing Commands

### Running Tests
```bash
cargo test                           # Run all tests in parallel
cargo test -- --nocapture           # Show println! output during tests
cargo test -- --test-threads=1      # Run tests sequentially (for debugging)
```

### Running Specific Tests
```bash
cargo test test_name                 # Run tests matching "test_name" (substring)
cargo test test_name -- --exact      # Run only exact match "test_name"
cargo test module_name::             # Run all tests in a module
cargo test --lib                     # Run only unit tests
cargo test --test integration_name   # Run specific integration test file
```

### Test with nextest (if available)
```bash
cargo nextest run                    # Faster test execution
cargo nextest run test_name          # Run specific test with nextest
```

---

## Code Style Guidelines

### Import Organization
Group imports with blank lines between groups:
1. Standard library (`std::`, `core::`)
2. External crates (`async_lsp::`, `serde::`, `lsp_types::`)
3. Internal modules (`crate::`, `super::`, `self::`)

```rust
use std::collections::HashMap;
use std::path::PathBuf;

use async_lsp::lsp_types::{Position, TextDocumentIdentifier};
use serde::{Deserialize, Serialize};

use crate::parser::AvroSchema;
use crate::state::ServerState;
```

### Formatting Standards
- **Indentation**: 4 spaces (no tabs)
- **Line width**: 100 characters max
- **Trailing commas**: Required in multi-line expressions
- Use `cargo fmt` to auto-format - it enforces consistent style

### Type Annotations
- Use explicit return types for public functions
- Type inference is acceptable for local variables when obvious
- Use type aliases for complex types:
  ```rust
  type SchemaMap = HashMap<String, AvroSchema>;
  type LspResult<T> = Result<T, ResponseError>;
  ```

### Naming Conventions
- **Functions/variables**: `snake_case`
  ```rust
  fn parse_avro_schema() { }
  let schema_path = PathBuf::new();
  ```
- **Types/Traits/Enums**: `PascalCase`
  ```rust
  struct AvroSchema { }
  trait SchemaValidator { }
  enum SchemaType { }
  ```
- **Constants**: `SCREAMING_SNAKE_CASE`
  ```rust
  const MAX_SCHEMA_SIZE: usize = 1024 * 1024;
  ```
- **Lifetimes**: Short, descriptive: `'a`, `'schema`, `'static`

### Error Handling
- Use `Result<T, E>` for fallible operations
- Avoid `.unwrap()` and `.expect()` in production code paths
- Use `.expect()` with descriptive messages only for initialization/setup
- Propagate errors with `?` operator
- Consider defining custom error types with `thiserror` or similar
  ```rust
  // Good
  fn load_schema(path: &Path) -> Result<Schema, SchemaError> {
      let content = std::fs::read_to_string(path)?;
      parse_schema(&content)
  }
  
  // Avoid in production paths
  let schema = load_schema(path).unwrap(); // Bad!
  
  // Acceptable for initialization
  let config = Config::load().expect("Config must exist at startup");
  ```

### Documentation
- Use `///` for public API documentation
- Use `//!` for module-level documentation
- Document **why**, not **what** (code shows what)
- Include examples in doc comments for complex functions
  ```rust
  /// Resolves schema references within an Avro schema definition.
  ///
  /// This handles named types and allows schemas to reference other
  /// schemas by name, following the Avro specification.
  pub fn resolve_references(schema: &Schema) -> Result<Schema, Error> {
      // implementation
  }
  ```

### Async/Await Patterns
- This is an async LSP server - use async/await properly
- Avoid blocking operations in async contexts
- Use `tokio::spawn` or similar for background tasks
- Use `async fn` for LSP handlers
  ```rust
  async fn handle_completion(&self, params: CompletionParams) -> Result<CompletionResponse> {
      // async implementation
  }
  ```

### LSP-Specific Patterns
- Follow async-lsp framework conventions
- LSP handlers should return `Result<T, ResponseError>`
- Maintain server state in a thread-safe structure (Arc<RwLock<T>> or similar)
- Use LSP types from `lsp_types` crate for protocol conformance
- Log important events for debugging LSP interactions

### Common Rust Idioms
- Prefer pattern matching over nested if/else
- Use iterator chains instead of manual loops when appropriate
- Use the `?` operator for error propagation
- Prefer `impl Trait` for return types when appropriate
- Use `#[derive(...)]` for common trait implementations
  ```rust
  // Good: Iterator chain
  let valid_schemas: Vec<_> = schemas
      .iter()
      .filter(|s| s.is_valid())
      .collect();
  
  // Good: Pattern matching
  match schema_type {
      SchemaType::Record => handle_record(),
      SchemaType::Enum => handle_enum(),
      _ => handle_primitive(),
  }
  ```

### Comments
- Prefer self-documenting code over comments
- Use comments to explain complex algorithms or non-obvious decisions
- Keep comments up-to-date when code changes
- Use `// TODO:`, `// FIXME:`, `// NOTE:` for special markers

---

## Testing Best Practices

- Write unit tests in the same file using `#[cfg(test)]` module
- Use `#[test]` attribute for test functions
- Use descriptive test names: `test_parse_record_schema_with_nested_fields`
- Test edge cases and error conditions
- Mock external dependencies when testing LSP handlers
- Use `assert_eq!`, `assert!`, and `assert_matches!` for assertions

---

## Project-Specific Notes

### What are .avsc files?
`.avsc` files are JSON-formatted Apache Avro schema definitions. They define data structures for serialization with primitive types (null, boolean, int, long, float, double, bytes, string) and complex types (record, enum, array, map, union, fixed).

Example:
```json
{
  "type": "record",
  "name": "User",
  "namespace": "com.example",
  "fields": [
    {"name": "id", "type": "long"},
    {"name": "username", "type": "string"},
    {"name": "email", "type": ["null", "string"], "default": null}
  ]
}
```

### Avro Validation Rules (Phase 1A)

The LSP must validate these rules and report diagnostics:

1. **Valid JSON** - Must parse as JSON
2. **Required attributes**:
   - Records: `type`, `name`, `fields` (array of field objects)
   - Enums: `type`, `name`, `symbols` (array of strings)
   - Arrays: `type`, `items`
   - Maps: `type`, `values`
   - Fixed: `type`, `name`, `size` (integer)
   - Fields: `name`, `type`
3. **Name validation** - Names must match `[A-Za-z_][A-Za-z0-9_]*`
4. **Namespace validation** - Dot-separated names or empty string
5. **Type references** - Referenced types must be defined (primitives or in same file)
6. **Symbol uniqueness** - Enum symbols must be unique within enum
7. **Union constraints**:
   - Cannot contain duplicate types (except named types with different names)
   - Cannot contain nested unions directly
8. **Primitive types** - Must be one of: null, boolean, int, long, float, double, bytes, string

### LSP Development Patterns

#### Server State Management
- Use `Arc<RwLock<ServerState>>` for thread-safe shared state
- Store parsed schemas per document URL
- Clear state on `textDocument/didClose`

#### Error Handling in LSP Handlers
- LSP handlers return `Result<T, ResponseError>`
- Convert internal errors to LSP `ResponseError` with proper codes
- Always send diagnostics even if empty (clears previous errors)

#### Adding New LSP Handlers
```rust
// 1. Define handler in src/handlers/
async fn handle_hover(
    state: &ServerState,
    params: HoverParams,
) -> Result<Option<Hover>, ResponseError> {
    // Implementation
}

// 2. Register in server setup (src/server.rs)
server.on_hover(|state, params| async move {
    handle_hover(&state, params).await
});
```

#### Diagnostics Pattern
```rust
// After parsing/validation, always publish diagnostics
let diagnostics = validator.validate(&schema)?;
client.publish_diagnostics(uri.clone(), diagnostics, version).await;
```

### Test Fixtures

Test fixtures live in `tests/fixtures/`:
- `valid/` - Schemas that should parse without errors
- `invalid/` - Schemas with specific validation errors

When adding validation rules, add corresponding test fixtures.

### Performance Considerations
- LSP operations should complete in <100ms for typical schemas
- Parse schema only on `didOpen` and `didChange`, cache results
- Validation should be incremental where possible
- Use `tracing` for performance debugging

### Dependencies Rationale
- `async-lsp` - LSP framework with async support
- `serde`/`serde_json` - Parse JSON schemas
- `tokio` - Async runtime for LSP
- `tower` - Required by async-lsp for middleware
- `anyhow`/`thiserror` - Error handling
- `regex` - Name/namespace validation
- `tracing` - Structured logging

### Neovim Integration

After building, install the LSP:
```bash
cargo build --release
sudo cp target/release/avro-lsp /usr/local/bin/
```

**Neovim 0.11+** (recommended) - Add to your `init.lua`:
```lua
-- Filetype detection
vim.filetype.add({
  extension = { avsc = 'avsc' },
})

-- LSP configuration
vim.api.nvim_create_autocmd('FileType', {
  pattern = 'avsc',
  callback = function(args)
    vim.lsp.start({
      name = 'avro-lsp',
      cmd = {'avro-lsp'},
      root_dir = vim.fs.root(args.buf, {'.git', 'avro-schemas'}),
    })
  end,
})
```

**Older versions with nvim-lspconfig**:
```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.avro_lsp then
  configs.avro_lsp = {
    default_config = {
      cmd = {'avro-lsp'},
      filetypes = {'avsc'},
      root_dir = lspconfig.util.root_pattern('.git', 'avro-schemas'),
      settings = {},
    },
  }
end

lspconfig.avro_lsp.setup{}
```

### Manual Testing with Neovim
```bash
# Terminal 1: Run LSP with debug logging
RUST_LOG=debug cargo run

# Terminal 2: Test with Neovim
nvim tests/fixtures/valid/simple_record.avsc

# In Neovim, check LSP status:
:LspInfo
```

---

## Pre-commit Checklist

Before committing code, ensure:
- [ ] `cargo fmt` has been run
- [ ] `cargo clippy` produces no warnings
- [ ] `cargo test` passes all tests
- [ ] Public APIs are documented
- [ ] New features have tests
- [ ] Error handling is appropriate (no unwrap in production paths)
