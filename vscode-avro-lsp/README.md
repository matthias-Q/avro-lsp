# Avro Language Server for VS Code

Language Server Protocol (LSP) support for Apache Avro schema files (`.avsc`).

## Features

- **Real-time validation** - Instant feedback on schema errors
- **Hover information** - Rich type details on hover
- **Auto-completion** - Context-aware suggestions for keys and types
- **Go to definition** - Jump to type declarations
- **Find references** - Locate all usages of a type
- **Rename symbol** - Safely rename types and update references
- **Document formatting** - Consistent JSON formatting
- **Semantic highlighting** - Context-aware syntax coloring
- **Document symbols** - Outline view of schema structure
- **Code actions** - Quick fixes and refactoring suggestions

## Installation

### Supported Platforms

The extension includes pre-built binaries for:
- Linux (x86_64)
- Windows (x86_64)

**macOS users**: See "Building from Source" below.

### Install Extension

1. Download the latest `.vsix` file from [GitHub Releases](https://github.com/matthias-Q/avro-lsp/releases)

2. Install via command line:
   ```bash
   code --install-extension avro-lsp-0.1.0.vsix
   ```

3. Or install via VS Code UI:
   - Open Extensions view (Ctrl+Shift+X / Cmd+Shift+X)
   - Click "..." menu → "Install from VSIX..."
   - Select the downloaded `.vsix` file

4. Open any `.avsc` file - the extension activates automatically!

### macOS Installation

macOS binaries are not included due to cross-compilation complexity. macOS users must:

1. **Build the LSP server from source:**
   ```bash
   git clone https://github.com/matthias-Q/avro-lsp.git
   cd avro-lsp
   cargo build --release
   sudo cp target/release/avro-lsp /usr/local/bin/
   ```

2. **Install the extension** from the `.vsix` file (steps above)

3. **Configure custom binary path** in VS Code settings:
   ```json
   {
     "avro-lsp.server.path": "/usr/local/bin/avro-lsp"
   }
   ```

## Configuration

Optional settings (File → Preferences → Settings or Cmd/Ctrl+,):

```json
{
  // Custom path to avro-lsp binary (leave empty to use bundled)
  "avro-lsp.server.path": "",

  // Trace LSP communication (for debugging)
  "avro-lsp.trace.server": "off"  // Options: "off", "messages", "verbose"
}
```

## Usage

The extension automatically activates when you open `.avsc` files.

### Validation

Invalid schemas show red squiggles with error messages:
- JSON syntax errors
- Missing required fields
- Invalid type references
- Schema constraint violations

### Hover Information

Hover over any element to see:
- Type definitions
- Field details
- Documentation

### Auto-completion

Trigger with `"`, `:`, or `,` to get suggestions for:
- JSON keys (type, name, fields, etc.)
- Avro types (primitives and complex types)
- Named type references

### Go to Definition

Ctrl+Click (or Cmd+Click) on type references to jump to their definitions.

### Find References

Right-click on a type name and select "Find All References" to see all places where the type is used.

### Rename Symbol

Right-click on a type name and select "Rename Symbol" (or press F2) to rename the type and all its references.

### Formatting

Format document with Shift+Alt+F (or Shift+Option+F on macOS):
- Consistent 2-space indentation
- Removes trailing commas
- Preserves schema semantics

### Document Symbols

View schema outline with Ctrl+Shift+O (Cmd+Shift+O):
- All types and fields hierarchically organized
- Quick navigation between definitions

## Troubleshooting

### Extension doesn't activate

- Verify file has `.avsc` extension
- Check Output panel → "Avro Language Server" for errors
- Try reloading VS Code window

### LSP features not working

- Check binary has execute permissions (Linux/macOS): `chmod +x /path/to/avro-lsp`
- Verify server is running: Check Output panel
- Enable verbose trace: `"avro-lsp.trace.server": "verbose"`
- Check Developer Tools console: Help → Toggle Developer Tools

### "Unsupported platform" error on macOS

- Build from source and configure custom path (see macOS Installation above)

### Binary not found error

- Ensure binary is in the configured path
- Try absolute path in settings
- Check file permissions

## Requirements

- VS Code 1.80.0 or higher
- For macOS: Rust toolchain to build from source

## Known Issues

- macOS binaries not included - manual build required
- Manual updates needed for new versions (download new `.vsix`)

## Contributing

Report issues and contribute at: [GitHub Repository](https://github.com/matthias-Q/avro-lsp)

## License

MIT License - See LICENSE file in the repository
