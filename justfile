# Build the project in release mode
build:
    cargo build --release

# Run tests
test:
    cargo test

# Run clippy linter
clippy:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Check code formatting
fmt-check:
    cargo fmt -- --check

# Clean build artifacts
clean:
    cargo clean
    rm -rf vscode-avro-lsp/node_modules
    rm -rf vscode-avro-lsp/out
    rm -rf vscode-avro-lsp/dist
    rm -rf vscode-avro-lsp/bin
    rm -f vscode-avro-lsp/*.vsix
    rm -f *.avsc

# Run the binary
run:
    cargo run --release

# Print AST for an Avro schema file (tree view)
print-ast FILEPATH:
    cargo run --example print_ast -- {{FILEPATH}}

# Print AST for an Avro schema file (JSON view)
print-ast-json FILEPATH:
    cargo run --example print_ast -- --json {{FILEPATH}}

# Make a release, defaults to `patch`
release TYPE="patch":
    #!/usr/bin/env bash
    set -euo pipefail
    CURRENT_VERSION=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
    IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"
    if [ "{{TYPE}}" = "major" ]; then
        NEW_VERSION="$((MAJOR + 1)).0.0"
    elif [ "{{TYPE}}" = "minor" ]; then
        NEW_VERSION="$MAJOR.$((MINOR + 1)).0"
    else
        NEW_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
    fi
    echo "Bumping version from $CURRENT_VERSION to v$NEW_VERSION"

    # Update Cargo.toml
    sed -i "s/^version = \".*\"/version = \"$NEW_VERSION\"/" Cargo.toml
    cargo check

    # Update VS Code extension package.json
    echo "Updating VS Code extension version to $NEW_VERSION"
    sed -i "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" vscode-avro-lsp/package.json

    # Commit version bumps
    git add Cargo.toml Cargo.lock vscode-avro-lsp/package.json
    git commit -m "chore(release): Release version v$NEW_VERSION"
    git tag "v$NEW_VERSION"

    # Generate changelog
    echo "Generating changelog for v$NEW_VERSION..."
    git cliff -l --current -t "v$NEW_VERSION" --prepend CHANGELOG.md
    git add CHANGELOG.md
    git commit --amend --no-edit
    git tag -f "v$NEW_VERSION"

    echo "Release v$NEW_VERSION created. Run 'just push-release' to push."

# Push release to remote with tags (branch first, then tag as a separate push to trigger CI correctly)
push-release:
    git push
    git push --tags

# Build VS Code extension
build-extension:
    cd vscode-avro-lsp && npm install && npm run build

# Package VS Code extension into .vsix file (includes building the LSP binary)
package-extension: build
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Copying LSP binary to extension..."
    mkdir -p vscode-avro-lsp/bin
    cp target/release/avro-lsp vscode-avro-lsp/bin/avro-lsp-linux-x64
    chmod +x vscode-avro-lsp/bin/avro-lsp-linux-x64
    echo "Building extension..."
    cd vscode-avro-lsp
    npm install
    npm run compile
    echo "Packaging extension..."
    npx vsce package --no-git-tag-version

# Install VS Code extension locally for testing
install-extension: package-extension
    code --install-extension vscode-avro-lsp/*.vsix

# Show all available recipes
help:
    @just --list

# Publish to crates.io (patch/minor/major), e.g. `just publish` or `just publish minor`
publish TYPE="patch" *FLAGS:
    cargo release {{TYPE}} {{FLAGS}}
