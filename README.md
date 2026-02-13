# avro-lsp

A Language Server Protocol (LSP) implementation for Apache Avro schema files (`.avsc`).

## Features

### Phase 1A (Complete)
- **Real-time validation** of Avro schemas with diagnostics
- **JSON syntax checking**
- **Schema validation** according to Apache Avro specification:
  - Name and namespace validation
  - Required field checking
  - Type reference validation
  - Enum symbol uniqueness
  - Union constraint validation
  - Primitive type validation
- **Precise error positioning** - errors shown at the exact location in the file

### Phase 1B (Complete)
- [x] **Hover information** - Rich type information when hovering over:
  - Type names (records, enums, fixed types)
  - Field names
  - Primitive types
  - Type references
- [x] **Document symbols** - Outline view of all types and fields in schema
- [x] **Semantic tokens** - Meaning-aware syntax highlighting:
  - Keywords (type, name, fields, etc.) highlighted distinctly
  - Type names (records, enums) with declaration modifiers
  - Field names and properties
  - Primitive types with readonly modifiers
  - Enum symbols

### Phase 2 (Complete) ✅
- [x] **Auto-completion** - Context-aware suggestions with snippet support:
  - Suggests JSON keys based on context (record, enum, field attributes)
  - Suggests all Avro types (primitives and complex types)
  - Suggests named type references from the current schema
  - Triggered by `"`, `:`, and `,` characters
  - Includes documentation for each completion item
  - Smart cursor positioning inside quotes and brackets
- [x] **Go to definition** - Jump to type declarations
  - Navigate to type definitions by clicking on type references
  - Works for all named types (records, enums, fixed)
  - Returns to original position easily with editor's jump-back command

### Phase 3A (Complete) ✅
- [x] **Document formatting** - Format `.avsc` files with consistent style:
  - Uses 2-space indentation (standard JSON formatting)
  - Automatically removes trailing commas (invalid JSON)
  - Preserves JSON semantics and schema structure
  - Idempotent formatting (format twice = same result)
  - Returns error for invalid JSON rather than silent failure

## Installation

### From Source

```bash
git clone <repository-url>
cd avro-lsp
cargo build --release
sudo cp target/release/avro-lsp /usr/local/bin/
```

### From Crates.io (coming soon)

```bash
cargo install avro-lsp
```

## Editor Integration

### Neovim

See [NEOVIM.md](NEOVIM.md) for detailed Neovim configuration instructions.

Quick setup:

```lua
-- In your Neovim config
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.avro_lsp then
  configs.avro_lsp = {
    default_config = {
      cmd = {'avro-lsp'},
      filetypes = {'avsc'},
      root_dir = lspconfig.util.root_pattern('.git'),
      settings = {},
    },
  }
end

lspconfig.avro_lsp.setup{}
```

### Other Editors

The LSP server communicates via standard input/output and should work with any editor that supports LSP, including:

- VS Code (requires extension)
- Emacs (via lsp-mode or eglot)
- Vim (via vim-lsp or coc.nvim)
- Sublime Text (via LSP package)

## Usage

The LSP server runs as a background process managed by your editor. It validates Avro schemas in real-time as you edit `.avsc` files.

### Example

Given this invalid Avro schema:

```json
{
  "type": "record",
  "name": "123Invalid",
  "fields": []
}
```

The LSP will report an error: `Invalid name '123Invalid': must match [A-Za-z_][A-Za-z0-9_]*`

## Development

See [AGENTS.md](AGENTS.md) for detailed development guidelines.

### Building

```bash
cargo build              # Debug build
cargo build --release    # Release build
```

### Testing

```bash
cargo test                          # Run all tests
cargo test test_name                # Run specific test
cargo test -- --nocapture           # Show println! output
```

### Code Quality

```bash
cargo fmt                # Auto-format code
cargo clippy             # Run linter
```

## Project Structure

```
avro-lsp/
├── src/
│   ├── main.rs          # Entry point, stdio transport setup
│   ├── server.rs        # LSP server implementation
│   ├── state.rs         # Document state management
│   └── schema/          # Avro schema parsing and validation
│       ├── error.rs     # Error types
│       ├── types.rs     # Avro type definitions (AST)
│       ├── parser.rs    # JSON → Avro schema parser
│       └── validator.rs # Schema validation rules
├── tests/
│   └── fixtures/        # Test .avsc files
│       ├── valid/       # Valid schemas
│       └── invalid/     # Invalid schemas (for testing diagnostics)
├── AGENTS.md            # Development guide for AI agents and humans
├── NEOVIM.md            # Neovim-specific setup guide
└── README.md            # This file
```

## Validation Rules

The LSP validates the following aspects of Avro schemas:

1. **Valid JSON** - Must parse as JSON
2. **Required attributes** - Records need `type`, `name`, `fields`, etc.
3. **Name validation** - Must match `[A-Za-z_][A-Za-z0-9_]*`
4. **Namespace validation** - Dot-separated names or empty
5. **Type references** - All referenced types must exist
6. **Symbol uniqueness** - Enum symbols must be unique
7. **Union constraints** - No duplicate types, no nested unions
8. **Primitive types** - Must be valid: null, boolean, int, long, float, double, bytes, string

## Roadmap

### Phase 1A ✅
- [x] Parsing and validation
- [x] Real-time diagnostics with precise error positioning

### Phase 1B ✅
- [x] Hover information (type details, documentation)
- [x] Document symbols (outline view)
- [x] Semantic tokens (better syntax highlighting)

### Phase 2 ✅
- [x] Auto-completion with snippet support
- [x] Go to definition

### Phase 3 (Future) 🔮
- [ ] Find references - Find all usages of a type
- [ ] Multi-file support
- [ ] Code formatting
- [ ] Refactoring support

## Contributing

Contributions are welcome! Please see [AGENTS.md](AGENTS.md) for coding standards and development guidelines.

## License

[Add your license here]

## References

- [Apache Avro Specification](https://avro.apache.org/docs/1.11.1/specification/)
- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
- [async-lsp framework](https://github.com/oxalica/async-lsp)
