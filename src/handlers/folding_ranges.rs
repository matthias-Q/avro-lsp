use async_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

use crate::schema::{AvroSchema, AvroType};

/// Generate folding ranges for an Avro schema
/// Allows collapsing records, enums, fixed types, arrays, and maps
pub fn get_folding_ranges(schema: &AvroSchema, _text: &str) -> Vec<FoldingRange> {
    let mut ranges = Vec::new();
    collect_folding_ranges(&schema.root, &mut ranges);
    ranges
}

fn collect_folding_ranges(avro_type: &AvroType, ranges: &mut Vec<FoldingRange>) {
    match avro_type {
        AvroType::Record(record) => {
            // Fold the entire record definition
            if let Some(range) = &record.range {
                // Only add folding range if there's more than one line
                if range.end.line > range.start.line {
                    ranges.push(FoldingRange {
                        start_line: range.start.line,
                        end_line: range.end.line,
                        start_character: Some(range.start.character),
                        end_character: Some(range.end.character),
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: Some(format!("record {}", record.name)),
                    });
                }
            }

            // Recurse into field types to find nested structures
            for field in &record.fields {
                collect_folding_ranges(&field.field_type, ranges);
            }
        }
        AvroType::Enum(enum_schema) => {
            // Fold the entire enum definition
            if let Some(range) = &enum_schema.range
                && range.end.line > range.start.line
            {
                ranges.push(FoldingRange {
                    start_line: range.start.line,
                    end_line: range.end.line,
                    start_character: Some(range.start.character),
                    end_character: Some(range.end.character),
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: Some(format!("enum {}", enum_schema.name)),
                });
            }
        }
        AvroType::Fixed(fixed) => {
            // Fold fixed type definitions
            if let Some(range) = &fixed.range
                && range.end.line > range.start.line
            {
                ranges.push(FoldingRange {
                    start_line: range.start.line,
                    end_line: range.end.line,
                    start_character: Some(range.start.character),
                    end_character: Some(range.end.character),
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: Some(format!("fixed {}", fixed.name)),
                });
            }
        }
        AvroType::Array(array) => {
            // Recurse into array items
            collect_folding_ranges(&array.items, ranges);
        }
        AvroType::Map(map) => {
            // Recurse into map values
            collect_folding_ranges(&map.values, ranges);
        }
        AvroType::Union(types) => {
            // Recurse into union types
            for avro_type in types {
                collect_folding_ranges(avro_type, ranges);
            }
        }
        // Primitives, PrimitiveObjects, TypeRefs, and Invalid types don't have foldable content
        AvroType::Primitive(_)
        | AvroType::PrimitiveObject(_)
        | AvroType::TypeRef(_)
        | AvroType::Invalid(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AvroParser;

    #[test]
    fn test_fold_simple_record() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "id", "type": "long"}
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");

        let ranges = get_folding_ranges(&schema, schema_text);

        // Should have 1 range for the User record
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_line, 0);
        assert!(ranges[0].end_line > 0);
        assert_eq!(ranges[0].collapsed_text, Some("record User".to_string()));
    }

    #[test]
    fn test_fold_nested_record() {
        let schema_text = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {
      "name": "address",
      "type": {
        "type": "record",
        "name": "Address",
        "fields": [
          {"name": "city", "type": "string"}
        ]
      }
    }
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");

        let ranges = get_folding_ranges(&schema, schema_text);

        // Should have 2 ranges: User record and nested Address record
        assert_eq!(ranges.len(), 2);

        // Find User and Address ranges
        let user_range = ranges
            .iter()
            .find(|r| r.collapsed_text == Some("record User".to_string()));
        let address_range = ranges
            .iter()
            .find(|r| r.collapsed_text == Some("record Address".to_string()));

        assert!(user_range.is_some(), "Should have User folding range");
        assert!(address_range.is_some(), "Should have Address folding range");
    }

    #[test]
    fn test_fold_enum() {
        let schema_text = r#"{
  "type": "enum",
  "name": "Color",
  "symbols": ["RED", "GREEN", "BLUE"]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");

        let ranges = get_folding_ranges(&schema, schema_text);

        // Should have 1 range for the enum
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].collapsed_text, Some("enum Color".to_string()));
    }

    #[test]
    fn test_fold_fixed() {
        let schema_text = r#"{
  "type": "fixed",
  "name": "MD5",
  "size": 16
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");

        let ranges = get_folding_ranges(&schema, schema_text);

        // Should have 1 range for the fixed type
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].collapsed_text, Some("fixed MD5".to_string()));
    }

    #[test]
    fn test_no_fold_single_line() {
        let schema_text = r#"{"type": "record", "name": "User", "fields": []}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");

        let ranges = get_folding_ranges(&schema, schema_text);

        // Should have no ranges for single-line schema
        assert_eq!(ranges.len(), 0);
    }

    #[test]
    fn test_fold_multiple_records() {
        let schema_text = r#"{
  "type": "record",
  "name": "Company",
  "fields": [
    {
      "name": "employees",
      "type": {
        "type": "array",
        "items": {
          "type": "record",
          "name": "Employee",
          "fields": [
            {"name": "name", "type": "string"}
          ]
        }
      }
    }
  ]
}"#;

        let mut parser = AvroParser::new();
        let schema = parser.parse(schema_text).expect("Should parse");

        let ranges = get_folding_ranges(&schema, schema_text);

        // Should have 2 ranges: Company and Employee records
        assert_eq!(ranges.len(), 2);
    }
}
