pub mod error;
pub mod json_parser;
pub mod parser;
pub mod types;
pub mod validator;
pub mod warning;

pub use error::SchemaError;
pub use parser::AvroParser;
pub use types::*;
pub use validator::{AvroValidator, TypeResolver};
pub use warning::SchemaWarning;
