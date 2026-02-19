use regex::Regex;

use super::super::error::{Result, SchemaError};
use super::super::parser::levenshtein_distance;
use super::super::types::{FixedSchema, PrimitiveSchema, PrimitiveType};
use super::super::warning::SchemaWarning;

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
                        message: "Decimal logical type requires 'precision' attribute".to_string(),
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
                    _ => {
                        // Unknown logical type - this will be caught as a warning
                        // Don't error here, just return Ok per Avro spec
                        return Ok(());
                    }
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

/// Check if a primitive has an unknown logical type value and return a warning
pub fn check_unknown_logical_type_warning(primitive: &PrimitiveSchema) -> Option<SchemaWarning> {
    if let Some(logical_type) = &primitive.logical_type {
        // List of all valid logical types
        const VALID_LOGICAL_TYPES: &[&str] = &[
            "date",
            "time-millis",
            "time-micros",
            "timestamp-millis",
            "timestamp-micros",
            "local-timestamp-millis",
            "local-timestamp-micros",
            "uuid",
            "decimal",
            "duration",
        ];

        // Check if this is a known logical type
        if !VALID_LOGICAL_TYPES.contains(&logical_type.as_str()) {
            // Unknown logical type - suggest a correction
            let suggested = suggest_logical_type(logical_type);

            return Some(SchemaWarning::InvalidLogicalType {
                logical_type: logical_type.clone(),
                primitive_type: format!("{:?}", primitive.primitive_type),
                range: primitive.logical_type_range,
                suggested,
            });
        }
    }
    None
}

/// Suggest a logical type based on edit distance
fn suggest_logical_type(input: &str) -> Option<String> {
    const VALID_LOGICAL_TYPES: &[&str] = &[
        "date",
        "time-millis",
        "time-micros",
        "timestamp-millis",
        "timestamp-micros",
        "local-timestamp-millis",
        "local-timestamp-micros",
        "uuid",
        "decimal",
        "duration",
    ];

    let mut best_match: Option<(usize, &str)> = None;

    for &valid_type in VALID_LOGICAL_TYPES {
        let distance = levenshtein_distance(input, valid_type);
        // Allow up to 60% of input length as threshold, minimum 2
        let input_based_threshold = ((input.len() as f64 * 0.6).ceil() as usize).max(2);
        // Also consider the valid type length to be lenient
        let valid_based_threshold = ((valid_type.len() as f64 * 0.6).ceil() as usize).max(2);
        let max_distance = input_based_threshold.max(valid_based_threshold);

        if distance <= max_distance {
            match best_match {
                None => best_match = Some((distance, valid_type)),
                Some((best_dist, _)) if distance < best_dist => {
                    best_match = Some((distance, valid_type))
                }
                _ => {}
            }
        }
    }

    best_match.map(|(_, lt)| lt.to_string())
}
