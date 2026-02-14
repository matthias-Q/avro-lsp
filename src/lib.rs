//! # avro-lsp
//!
//! A Language Server Protocol (LSP) implementation for Apache Avro schema files (`.avsc`).
//!
//! This library provides IDE-like features including:
//! - Diagnostics and validation
//! - Hover information
//! - Auto-completion
//! - Go to definition
//! - Find references
//! - Rename refactoring
//! - Document formatting
//! - Semantic highlighting
//! - Code actions and quick fixes
//! - Inlay hints
//! - Folding ranges
//!
//! ## Modules
//!
//! - [`handlers`] - LSP request handlers for various capabilities
//! - [`schema`] - Avro schema parsing, validation, and type system
//! - [`server`] - LSP server implementation
//! - [`state`] - Server state management and document tracking
//! - [`workspace`] - Multi-file workspace support

pub mod handlers;
pub mod schema;
pub mod server;
pub mod state;
pub mod workspace;

// Re-export commonly used types for convenience
pub use server::AvroLanguageServer;
pub use state::ServerState;
pub use workspace::Workspace;
