//! Quick fixes for diagnostic errors in Avro schemas
//!
//! This module contains categorized quick fix functions that generate
//! code actions to automatically fix validation errors.

mod default_fixes;
mod logical_type_fixes;
mod name_fixes;
mod type_fixes;

// Re-export all quick fix functions for use within the code_actions module
pub(super) use default_fixes::*;
pub(super) use logical_type_fixes::*;
pub(super) use name_fixes::*;
pub(super) use type_fixes::*;
