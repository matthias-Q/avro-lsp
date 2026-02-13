# avro-lsp

A Language Server Protocol (LSP) implementation for Apache Avro schema files (`.avsc`).

## Features

### Diagnostics and Validation
- **Real-time error detection** - Instant feedback as you type
- **JSON syntax validation** - Catches malformed JSON immediately
- **Avro schema validation** according to Apache Avro specification:
  - Name and namespace validation (`[A-Za-z_][A-Za-z0-9_]*`)
  - Required field checking (type, name, fields, etc.)
  - Type reference validation (all referenced types must exist)
  - Enum symbol uniqueness
  - Union constraint validation (no duplicate types, no nested unions)
  - Primitive type validation
  - **Default value validation** - Ensures defaults match field types
  - **Logical types support** - Validates decimal (precision/scale), duration, and other logical types
- **Precise error positioning** - Errors shown at exact locations with clear messages

### Code Intelligence

- **Hover information** - Rich details when hovering over schema elements:
  - Type definitions with full structure
  - Field information including types and documentation
  - Primitive type descriptions
  - Type reference details

- **Auto-completion** - Smart suggestions as you type:
  - Context-aware JSON key suggestions (type, name, fields, etc.)
  - All Avro types (primitives: null, boolean, int, long, float, double, bytes, string)
  - Complex types (record, enum, array, map, fixed)
  - Named type references from current schema
  - Triggered by `"`, `:`, and `,` characters
  - Includes documentation for each suggestion
  - Smart cursor positioning inside quotes and brackets

- **Go to definition** - Navigate to type declarations:
  - Jump to type definitions with Ctrl+Click (Cmd+Click on macOS)
  - Works for records, enums, and fixed types
  - Quick navigation within large schemas

- **Find references** - Locate all usages of a type:
  - Find where types are referenced throughout the schema
  - See all uses of records, enums, and fixed types
  - Understand type dependencies

- **Rename symbol** - Safely rename types:
  - Rename types and update all references automatically
  - Preview changes before applying
  - Maintains schema consistency

- **Document symbols** - Hierarchical outline view:
  - See all types and fields at a glance
  - Navigate quickly between definitions
  - Understand schema structure instantly

### Code Quality

- **Document formatting** - Format `.avsc` files with consistent style:
  - Standard 2-space JSON indentation
  - Automatically removes trailing commas (invalid in JSON)
  - Preserves schema semantics
  - Idempotent (formatting twice produces same result)

- **Code actions** - Context-aware quick fixes and refactoring:
  - Add field to record - Insert new field scaffold
  - Add documentation - Add doc field to types
  - Make field nullable - Wrap type in union with null
  - Contextual suggestions based on cursor position

### Editor Experience

- **Syntax highlighting** - Semantic token-based highlighting:
  - Keywords (type, name, fields) distinctly colored
  - Type names with declaration modifiers
  - Field names and properties
  - Primitive types consistently highlighted
  - Enum symbols
  - Context-aware coloring that understands schema structure

## Installation

### Building from Source

**All Platforms:**

```bash
git clone https://gitlab.build-unite.unite.eu/matthias.queitsch/avro-lsp.git
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

## Editor Integration

### VS Code

**Linux and Windows users** can install the pre-built extension:

1. Download the latest `.vsix` file from [GitLab Releases](https://gitlab.build-unite.unite.eu/matthias.queitsch/avro-lsp/-/releases)
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

### IntelliJ IDEA (LSP4IntelliJ)

IntelliJ IDEA and other JetBrains IDEs can use avro-lsp through the [LSP4IntelliJ plugin](https://github.com/redhat-developer/lsp4ij).

**Installation:**

1. Build and install the LSP server (see "Building from Source" above)

2. Install the LSP4IntelliJ plugin:
   - Open Settings/Preferences (Ctrl+Alt+S / Cmd+,)
   - Go to **Plugins** → **Marketplace**
   - Search for **"LSP4IntelliJ"** or **"Language Server Protocol Support"**
   - Click **Install** and restart IntelliJ

3. Configure avro-lsp:
   - Open Settings/Preferences → **Languages & Frameworks** → **Language Servers**
   - Click **"+"** to add a new server
   - Configure as follows:
     - **Name**: `Avro LSP`
     - **Language/File name patterns**: `*.avsc`
     - **Command**: `/usr/local/bin/avro-lsp` (or your installation path)
     - **Configuration**: Leave empty (optional)
   - Click **OK** to save

4. Test the setup:
   - Create or open a `.avsc` file
   - You should see diagnostics, hover tooltips, and auto-completion working
   - Check LSP status in: **Tools** → **Language Server** → **Show Language Server Status**

**Troubleshooting:**

- **LSP not starting**: Verify the path to `avro-lsp` is correct and the binary is executable
- **No diagnostics**: Check LSP status window for connection errors
- **Features not working**: Ensure the file extension is `.avsc` and matches the pattern in settings


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
├── vscode-avro-lsp/     # VS Code extension
├── AGENTS.md            # Development guide for AI agents and humans
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


## Contributing

Contributions are welcome! Please see [AGENTS.md](AGENTS.md) for coding standards and development guidelines.

## License

MIT License - See [LICENSE](LICENSE) file for details

## References

- [Apache Avro Specification](https://avro.apache.org/docs/1.11.1/specification/)
- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
- [async-lsp framework](https://github.com/oxalica/async-lsp)
