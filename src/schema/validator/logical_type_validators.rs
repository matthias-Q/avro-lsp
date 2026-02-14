use regex::Regex;

use super::super::error::{Result, SchemaError};
use super::super::types::{FixedSchema, PrimitiveSchema, PrimitiveType};

pub fn validate_logical_type_for_fixed(
    _name_regex: &Regex,
    logical_type: &str,
    fixed: &FixedSchema,
) -> Result<()> {
    match logical_type {
        "decimal" => {
            if fixed.precision.is_none() {
                return Err(SchemaError::Custom {
                    message: "Decimal logical type requires 'precision' attribute".to_string(),
                    range: fixed.range,
                });
            }
            if let (Some(precision), Some(scale)) = (fixed.precision, fixed.scale)
                && scale > precision
            {
                return Err(SchemaError::Custom {
                    message: format!(
                        "Decimal scale ({}) cannot be greater than precision ({})",
                        scale, precision
                    ),
                    range: fixed.range,
                });
            }
        }
        "duration" => {
            if fixed.size != 12 {
                return Err(SchemaError::Custom {
                    message: "Duration logical type requires fixed size of 12 bytes".to_string(),
                    range: fixed.range,
                });
            }
        }
        _ => {
            return Err(SchemaError::Custom {
                message: format!("Unknown logical type '{}' for fixed type", logical_type),
                range: fixed.range,
            });
        }
    }
    Ok(())
}

pub fn validate_primitive_with_logical_type(
    _name_regex: &Regex,
    primitive: &PrimitiveSchema,
) -> Result<()> {
    if let Some(logical_type) = &primitive.logical_type {
        match (primitive.primitive_type, logical_type.as_str()) {
            (PrimitiveType::Int, "date") => Ok(()),
            (PrimitiveType::Int, "time-millis") => Ok(()),

            (PrimitiveType::Long, "time-micros") => Ok(()),
            (PrimitiveType::Long, "timestamp-millis") => Ok(()),
            (PrimitiveType::Long, "timestamp-micros") => Ok(()),
            (PrimitiveType::Long, "local-timestamp-millis") => Ok(()),
            (PrimitiveType::Long, "local-timestamp-micros") => Ok(()),

            (PrimitiveType::String, "uuid") => Ok(()),

            (PrimitiveType::Bytes, "decimal") => {
                if primitive.precision.is_none() {
                    return Err(SchemaError::Custom {
                        message: "Decimal logical type requires 'precision' attribute"
                            .to_string(),
                        range: primitive.range,
                    });
                }
                if let (Some(precision), Some(scale)) = (primitive.precision, primitive.scale)
                    && scale > precision
                {
                    return Err(SchemaError::Custom {
                        message: format!(
                            "Decimal scale ({}) cannot be greater than precision ({})",
                            scale, precision
                        ),
                        range: primitive.range,
                    });
                }
                Ok(())
            }

            _ => {
                let required_type = match logical_type.as_str() {
                    "date" | "time-millis" => "int",
                    "time-micros"
                    | "timestamp-millis"
                    | "timestamp-micros"
                    | "local-timestamp-millis"
                    | "local-timestamp-micros" => "long",
                    "uuid" => "string",
                    "decimal" => "bytes or fixed",
                    "duration" => "fixed",
                    _ => "unknown",
                };

                Err(SchemaError::Custom {
                    message: format!(
                        "Invalid logical type '{}' for primitive type '{:?}' - requires {}",
                        logical_type, primitive.primitive_type, required_type
                    ),
                    range: primitive.range,
                })
            }
        }
    } else {
        Ok(())
    }
}
