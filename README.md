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

### Phase 2 (Complete)
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

### Phase 3A (Complete)
- [x] **Document formatting** - Format `.avsc` files with consistent style:
  - Uses 2-space indentation (standard JSON formatting)
  - Automatically removes trailing commas (invalid JSON)
  - Preserves JSON semantics and schema structure
  - Idempotent formatting (format twice = same result)
  - Returns error for invalid JSON rather than silent failure

## Installation

### Building from Source

**All Platforms:**

```bash
git clone <repository-url>
cd avro-lsp
cargo build --release
```

**Linux / macOS:**

```bash
# System-wide installation
sudo cp target/release/avro-lsp /usr/local/bin/

# Or user-local installation (ensure ~/.local/bin is in PATH)
mkdir -p ~/.local/bin
cp target/release/avro-lsp ~/.local/bin/
```

**Windows:**

```powershell
# Copy to a directory in your PATH
copy target\release\avro-lsp.exe C:\Users\YourName\.local\bin\
```

### From Crates.io (coming soon)

```bash
cargo install avro-lsp
```

## Editor Integration

### VS Code

**Linux and Windows users** can install the pre-built extension:

1. Download the latest `.vsix` file from [GitLab Releases](https://gitlab.com/your-username/avro-lsp/-/releases)
2. Install via command line:
   ```bash
   code --install-extension avro-lsp-0.1.0.vsix
   ```
   Or via VS Code UI:
   - Open Extensions view (Ctrl+Shift+X)
   - Click "..." menu → "Install from VSIX..."
   - Select downloaded `.vsix` file

The extension bundles the LSP server binary - no additional installation needed!

**macOS users** must build from source and configure a custom path:

1. Build and install the LSP server (see "Building from Source" above)
2. Install the extension from the `.vsix` file
3. Configure VS Code settings (Ctrl+, or Cmd+,):
   ```json
   {
     "avro-lsp.server.path": "/usr/local/bin/avro-lsp"
   }
   ```

**Optional Configuration:**

```json
{
  "avro-lsp.server.path": "/custom/path/to/avro-lsp",
  "avro-lsp.trace.server": "messages"
}
```

### Neovim

First, build and install the LSP server:

```bash
# Build the release version
cargo build --release

# Install to system binary path
sudo cp target/release/avro-lsp /usr/local/bin/

# Or install to user local bin (ensure ~/.local/bin is in PATH)
mkdir -p ~/.local/bin
cp target/release/avro-lsp ~/.local/bin/
```

**Modern Neovim 0.11+ (Recommended)**

Add this to your `init.lua`:

```lua
-- Filetype detection
vim.filetype.add({
  extension = {
    avsc = 'avsc',
  },
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

**Using nvim-lspconfig (Alternative)**

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.avro_lsp then
  configs.avro_lsp = {
    default_config = {
      cmd = {'avro-lsp'},
      filetypes = {'avsc'},
      root_dir = lspconfig.util.root_pattern('.git', 'avro-schemas', '.'),
      settings = {},
    },
  }
end

lspconfig.avro_lsp.setup{}
```

**Testing**

1. Open a `.avsc` file:
   ```bash
   nvim tests/fixtures/valid/simple_record.avsc
   ```

2. Check LSP status:
   ```vim
   :LspInfo
   ```

3. Verify diagnostics appear for invalid schemas

**Troubleshooting**

LSP not starting:
1. Check if `avro-lsp` is in PATH: `which avro-lsp`
2. Try running manually: `avro-lsp`
3. Check Neovim LSP logs: `:LspLog`

No diagnostics appearing:
1. Check filetype: `:set filetype?`
2. Verify LSP is attached: `:LspInfo`

For more details, see [NEOVIM.md](NEOVIM.md).

### Other Editors

The LSP server uses standard input/output and works with any LSP-compatible editor:

- **Emacs**: Use lsp-mode or eglot
- **Vim**: Use vim-lsp or coc.nvim
- **Sublime Text**: Use LSP package
- **Helix**: Add to languages.toml
- **Kate**: Configure in LSP client settings

Configuration will be similar to the Neovim setup - point the editor's LSP client to the `avro-lsp` binary

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

### Phase 1A (Complete)
- [x] Parsing and validation
- [x] Real-time diagnostics with precise error positioning

### Phase 1B (Complete)
- [x] Hover information (type details, documentation)
- [x] Document symbols (outline view)
- [x] Semantic tokens (better syntax highlighting)

### Phase 2 (Complete)
- [x] Auto-completion with snippet support
- [x] Go to definition

### Phase 3A (Complete)
- [x] Document formatting with trailing comma removal

### Phase 3B (Future)
- [ ] Enhanced validation (default values, logical types, etc.)
- [ ] Code actions (scaffolding, quick fixes)

### Phase 4 (Future)
- [ ] Find references - Find all usages of a type
- [ ] Multi-file support
- [ ] Refactoring support

## Contributing

Contributions are welcome! Please see [AGENTS.md](AGENTS.md) for coding standards and development guidelines.

## License

MIT License - See [LICENSE](LICENSE) file for details

## References

- [Apache Avro Specification](https://avro.apache.org/docs/1.11.1/specification/)
- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
- [async-lsp framework](https://github.com/oxalica/async-lsp)
