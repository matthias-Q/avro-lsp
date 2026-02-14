use std::collections::HashMap;

use async_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, Position, Range, TextEdit, Url, WorkspaceEdit,
};
use once_cell::sync::Lazy;
use regex::Regex;

// Compile regex once at startup instead of repeatedly in hot paths
pub(super) static AVRO_NAME_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").expect("Valid regex pattern"));

/// Builder for creating CodeAction instances with less boilerplate
pub(super) struct CodeActionBuilder {
    uri: Url,
    title: String,
    kind: CodeActionKind,
    is_preferred: bool,
    diagnostics: Option<Vec<Diagnostic>>,
    edits: Vec<TextEdit>,
}

impl CodeActionBuilder {
    /// Create a new builder with required fields
    pub(super) fn new(uri: Url, title: impl Into<String>) -> Self {
        Self {
            uri,
            title: title.into(),
            kind: CodeActionKind::REFACTOR,
            is_preferred: false,
            diagnostics: None,
            edits: Vec::new(),
        }
    }

    /// Set the kind of code action (default: REFACTOR)
    pub(super) fn with_kind(mut self, kind: CodeActionKind) -> Self {
        self.kind = kind;
        self
    }

    /// Mark this action as preferred (default: false)
    pub(super) fn preferred(mut self) -> Self {
        self.is_preferred = true;
        self
    }

    /// Associate diagnostics with this action
    pub(super) fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }

    /// Add a text edit to this action
    pub(super) fn add_edit(mut self, range: Range, new_text: impl Into<String>) -> Self {
        self.edits.push(TextEdit {
            range,
            new_text: new_text.into(),
        });
        self
    }

    /// Add a text edit at a specific position (zero-width range)
    pub(super) fn add_insert(self, position: Position, text: impl Into<String>) -> Self {
        self.add_edit(
            Range {
                start: position,
                end: position,
            },
            text,
        )
    }

    /// Build the final CodeAction
    pub(super) fn build(self) -> CodeAction {
        let mut changes = HashMap::new();
        changes.insert(self.uri, self.edits);

        CodeAction {
            title: self.title,
            kind: Some(self.kind),
            diagnostics: self.diagnostics,
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                document_changes: None,
                change_annotations: None,
            }),
            command: None,
            is_preferred: Some(self.is_preferred),
            disabled: None,
            data: None,
        }
    }
}
