use async_lsp::lsp_types::Range;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a complete Avro schema document
#[derive(Debug, Clone, PartialEq)]
pub struct AvroSchema {
    pub root: AvroType,
    pub named_types: HashMap<String, AvroType>,
}

/// Represents an Avro type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AvroType {
    Primitive(PrimitiveType),
    Record(RecordSchema),
    Enum(EnumSchema),
    Array(ArraySchema),
    Map(MapSchema),
    Union(Vec<AvroType>),
    Fixed(FixedSchema),
    /// Reference to a named type by string
    TypeRef(TypeRefSchema),
}

/// A reference to a named type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TypeRefSchema {
    pub name: String,
    #[serde(skip)]
    pub range: Option<Range>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrimitiveType {
    Null,
    Boolean,
    Int,
    Long,
    Float,
    Double,
    Bytes,
    String,
}

impl PrimitiveType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "null" => Some(PrimitiveType::Null),
            "boolean" => Some(PrimitiveType::Boolean),
            "int" => Some(PrimitiveType::Int),
            "long" => Some(PrimitiveType::Long),
            "float" => Some(PrimitiveType::Float),
            "double" => Some(PrimitiveType::Double),
            "bytes" => Some(PrimitiveType::Bytes),
            "string" => Some(PrimitiveType::String),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordSchema {
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    pub fields: Vec<Field>,

    // Position tracking (not serialized)
    #[serde(skip)]
    pub range: Option<Range>,
    #[serde(skip)]
    pub name_range: Option<Range>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: Box<AvroType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,

    // Position tracking (not serialized)
    #[serde(skip)]
    pub range: Option<Range>,
    #[serde(skip)]
    pub name_range: Option<Range>,
    #[serde(skip)]
    pub type_range: Option<Range>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumSchema {
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    pub symbols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,

    // Position tracking (not serialized)
    #[serde(skip)]
    pub range: Option<Range>,
    #[serde(skip)]
    pub name_range: Option<Range>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArraySchema {
    #[serde(rename = "type")]
    pub type_name: String,
    pub items: Box<AvroType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSchema {
    #[serde(rename = "type")]
    pub type_name: String,
    pub values: Box<AvroType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixedSchema {
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    pub size: usize,
    #[serde(skip_serializing_if = "Option::is_none", rename = "logicalType")]
    pub logical_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precision: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<usize>,

    // Position tracking (not serialized)
    #[serde(skip)]
    pub range: Option<Range>,
    #[serde(skip)]
    pub name_range: Option<Range>,
}
