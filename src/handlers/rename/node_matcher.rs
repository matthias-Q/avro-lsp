use async_lsp::ResponseError;
use async_lsp::lsp_types::{Position, PrepareRenameResponse, Range};

use crate::schema::{AvroSchema, AvroType, Field};
use crate::state::{AstNode, find_node_at_position, position_in_range};

#[derive(Debug, Clone)]
pub enum SymbolType {
    RecordType,
    EnumType,
    FixedType,
    Field { field: Box<Field> },
    TypeReference,
}

#[derive(Debug, Clone)]
pub struct RenameInfo {
    pub symbol_type: SymbolType,
    pub old_name: String,
    pub name_range: Range,
}

pub fn extract_rename_info(
    schema: &AvroSchema,
    position: Position,
) -> Result<RenameInfo, ResponseError> {
    let node = find_node_at_position(schema, position).ok_or_else(|| {
        ResponseError::new(
            async_lsp::ErrorCode::INVALID_PARAMS,
            "No renameable symbol at cursor position",
        )
    })?;

    match node {
        AstNode::RecordDefinition(record) => {
            if let Some(name_range) = &record.name_range
                && position_in_range(position, name_range)
            {
                Ok(RenameInfo {
                    symbol_type: SymbolType::RecordType,
                    old_name: record.name.clone(),
                    name_range: *name_range,
                })
            } else {
                Err(ResponseError::new(
                    async_lsp::ErrorCode::INVALID_PARAMS,
                    "Cursor not on record name",
                ))
            }
        }
        AstNode::EnumDefinition(enum_schema) => {
            if let Some(name_range) = &enum_schema.name_range
                && position_in_range(position, name_range)
            {
                Ok(RenameInfo {
                    symbol_type: SymbolType::EnumType,
                    old_name: enum_schema.name.clone(),
                    name_range: *name_range,
                })
            } else {
                Err(ResponseError::new(
                    async_lsp::ErrorCode::INVALID_PARAMS,
                    "Cursor not on enum name",
                ))
            }
        }
        AstNode::Field(field) => {
            if let Some(name_range) = &field.name_range
                && position_in_range(position, name_range)
            {
                Ok(RenameInfo {
                    symbol_type: SymbolType::Field {
                        field: Box::new(field.clone()),
                    },
                    old_name: field.name.clone(),
                    name_range: *name_range,
                })
            } else {
                Err(ResponseError::new(
                    async_lsp::ErrorCode::INVALID_PARAMS,
                    "Cursor not on field name",
                ))
            }
        }
        AstNode::FieldType(field) => {
            if let AvroType::TypeRef(type_ref) = &*field.field_type
                && let Some(type_range) = &type_ref.range
                && position_in_range(position, type_range)
            {
                Ok(RenameInfo {
                    symbol_type: SymbolType::TypeReference,
                    old_name: type_ref.name.clone(),
                    name_range: *type_range,
                })
            } else {
                Err(ResponseError::new(
                    async_lsp::ErrorCode::INVALID_PARAMS,
                    "Cursor not on type reference",
                ))
            }
        }
        AstNode::FixedDefinition(fixed) => {
            if let Some(name_range) = &fixed.name_range
                && position_in_range(position, name_range)
            {
                Ok(RenameInfo {
                    symbol_type: SymbolType::FixedType,
                    old_name: fixed.name.clone(),
                    name_range: *name_range,
                })
            } else {
                Err(ResponseError::new(
                    async_lsp::ErrorCode::INVALID_PARAMS,
                    "Cursor not on fixed name",
                ))
            }
        }
    }
}

pub fn extract_type_name_for_references(schema: &AvroSchema, position: Position) -> Option<&str> {
    let node = find_node_at_position(schema, position)?;

    match node {
        AstNode::RecordDefinition(record) => {
            if let Some(name_range) = &record.name_range {
                if position_in_range(position, name_range) {
                    Some(&record.name)
                } else {
                    None
                }
            } else {
                None
            }
        }
        AstNode::EnumDefinition(enum_schema) => {
            if let Some(name_range) = &enum_schema.name_range {
                if position_in_range(position, name_range) {
                    Some(&enum_schema.name)
                } else {
                    None
                }
            } else {
                None
            }
        }
        AstNode::FieldType(field) => {
            if let AvroType::TypeRef(type_ref) = &*field.field_type {
                if let Some(type_range) = &type_ref.range {
                    if position_in_range(position, type_range) {
                        Some(&type_ref.name)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        AstNode::FixedDefinition(fixed) => {
            if let Some(name_range) = &fixed.name_range {
                if position_in_range(position, name_range) {
                    Some(&fixed.name)
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn extract_renameable_symbol(
    schema: &AvroSchema,
    position: Position,
) -> Option<PrepareRenameResponse> {
    let node = find_node_at_position(schema, position)?;

    match node {
        AstNode::RecordDefinition(record) => {
            if let Some(name_range) = &record.name_range
                && position_in_range(position, name_range)
            {
                return Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: *name_range,
                    placeholder: record.name.clone(),
                });
            }
        }
        AstNode::EnumDefinition(enum_schema) => {
            if let Some(name_range) = &enum_schema.name_range
                && position_in_range(position, name_range)
            {
                return Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: *name_range,
                    placeholder: enum_schema.name.clone(),
                });
            }
        }
        AstNode::Field(field) => {
            if let Some(name_range) = &field.name_range
                && position_in_range(position, name_range)
            {
                return Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: *name_range,
                    placeholder: field.name.clone(),
                });
            }
        }
        AstNode::FieldType(field) => {
            if let AvroType::TypeRef(type_ref) = &*field.field_type
                && let Some(type_range) = &type_ref.range
                && position_in_range(position, type_range)
            {
                return Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: *type_range,
                    placeholder: type_ref.name.clone(),
                });
            }
        }
        AstNode::FixedDefinition(fixed) => {
            if let Some(name_range) = &fixed.name_range
                && position_in_range(position, name_range)
            {
                return Some(PrepareRenameResponse::RangeWithPlaceholder {
                    range: *name_range,
                    placeholder: fixed.name.clone(),
                });
            }
        }
    }

    None
}
