// LSP handler modules organized by capability

pub mod code_actions;
pub mod completion;
pub mod definition;
pub mod diagnostics;
pub mod folding_ranges;
pub mod formatting;
pub mod hover;
pub mod inlay_hints;
pub mod rename;
pub mod semantic_tokens;
pub mod symbols;
pub mod workspace_symbols;

// Re-export commonly used types
