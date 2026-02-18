use async_lsp::lsp_types::{Position, Range};
use nom::{
    IResult,
    branch::alt,
    bytes::complete::{escaped, tag, take_while},
    character::complete::{char, multispace0, one_of},
    combinator::{opt, value},
    multi::separated_list0,
    number::complete::double,
    sequence::preceded,
};
use nom_locate::LocatedSpan;

/// Input type with position tracking
pub type Span<'a> = LocatedSpan<&'a str>;

/// JSON value with position information
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null(Range),
    Bool(bool, Range),
    Number(f64, Range),
    String {
        content: String,
        full_range: Range,    // Includes quotes: "value"
        content_range: Range, // Without quotes: value
    },
    Array(Vec<JsonValue>, Range),
    Object {
        map: indexmap::IndexMap<String, (Range, JsonValue)>,
        range: Range,
        /// List of ALL key-range pairs (including duplicates)
        all_keys: Vec<(String, Range)>,
    },
}

impl JsonValue {
    pub fn range(&self) -> Range {
        match self {
            JsonValue::Null(r) => *r,
            JsonValue::Bool(_, r) => *r,
            JsonValue::Number(_, r) => *r,
            JsonValue::String { full_range, .. } => *full_range,
            JsonValue::Array(_, r) => *r,
            JsonValue::Object { range, .. } => *range,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            JsonValue::String { content, .. } => Some(content),
            _ => None,
        }
    }

    /// Get string with both full range (with quotes) and content range (without quotes)
    pub fn as_string_with_ranges(&self) -> Option<(&str, Range, Range)> {
        match self {
            JsonValue::String {
                content,
                full_range,
                content_range,
            } => Some((content, *full_range, *content_range)),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&indexmap::IndexMap<String, (Range, JsonValue)>> {
        match self {
            JsonValue::Object { map, .. } => Some(map),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<JsonValue>> {
        match self {
            JsonValue::Array(a, _) => Some(a),
            _ => None,
        }
    }
}

/// Convert Span position to LSP Position
fn span_to_position(span: Span) -> Position {
    Position {
        line: span.location_line() - 1, // LSP uses 0-based line numbers
        character: span.get_utf8_column() as u32 - 1, // LSP uses 0-based columns
    }
}

/// Convert a byte offset in a string to a Position
fn offset_to_position_in_str(text: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut column = 0u32;

    for (i, ch) in text.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }

    Position {
        line,
        character: column,
    }
}

/// Create a Range from start and end spans
fn make_range(start: Span, end: Span) -> Range {
    Range {
        start: span_to_position(start),
        end: span_to_position(end),
    }
}

/// Parse whitespace (spaces, tabs, newlines)
fn ws(input: Span) -> IResult<Span, Span> {
    multispace0(input)
}

/// Parse null
fn parse_null(input: Span) -> IResult<Span, JsonValue> {
    let start = input;
    let (input, _) = tag("null")(input)?;
    let range = make_range(start, input);
    Ok((input, JsonValue::Null(range)))
}

/// Parse boolean
fn parse_bool(input: Span) -> IResult<Span, JsonValue> {
    let start = input;
    let (input, b) = alt((value(true, tag("true")), value(false, tag("false"))))(input)?;
    let range = make_range(start, input);
    Ok((input, JsonValue::Bool(b, range)))
}

/// Parse number
fn parse_number(input: Span) -> IResult<Span, JsonValue> {
    let start = input;
    let (input, n) = double(input)?;
    let range = make_range(start, input);
    Ok((input, JsonValue::Number(n, range)))
}

/// Parse string
fn parse_string(input: Span) -> IResult<Span, JsonValue> {
    let start = input;
    let (input, _) = char('"')(input)?;
    let content_start = input; // Position after opening quote

    let (input, s) = opt(escaped(
        take_while(|c| c != '"' && c != '\\'),
        '\\',
        one_of(r#""\/bfnrt"#),
    ))(input)?;

    let content_end = input; // Position before closing quote
    let (input, _) = char('"')(input)?;
    let end = input;

    let full_range = make_range(start, end); // "value" with quotes
    let content_range = make_range(content_start, content_end); // value without quotes
    let string_value = s.map(|s| s.fragment().to_string()).unwrap_or_default();

    Ok((
        input,
        JsonValue::String {
            content: string_value,
            full_range,
            content_range,
        },
    ))
}

/// Parse a JSON value (recursive)
fn parse_value(input: Span) -> IResult<Span, JsonValue> {
    preceded(
        ws,
        alt((
            parse_null,
            parse_bool,
            parse_number,
            parse_string,
            parse_array,
            parse_object,
        )),
    )(input)
}

/// Parse array: [ value, value, ... ]
fn parse_array(input: Span) -> IResult<Span, JsonValue> {
    let start = input;
    let (input, _) = char('[')(input)?;
    let (input, _) = ws(input)?;
    let (input, elements) = separated_list0(preceded(ws, char(',')), parse_value)(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char(']')(input)?;
    let range = make_range(start, input);
    Ok((input, JsonValue::Array(elements, range)))
}

/// Parse object key-value pair
fn parse_key_value(input: Span) -> IResult<Span, (String, Range, JsonValue)> {
    let (input, _) = ws(input)?;
    let (input, key) = parse_string(input)?;
    let (key_str, key_content_range) = match key {
        JsonValue::String {
            content,
            content_range,
            ..
        } => (content, content_range),
        _ => unreachable!(),
    };
    let (input, _) = ws(input)?;
    let (input, _) = char(':')(input)?;
    let (input, value) = parse_value(input)?;
    Ok((input, (key_str, key_content_range, value)))
}

/// Parse object: { "key": value, ... }
fn parse_object(input: Span) -> IResult<Span, JsonValue> {
    let start = input;
    let (input, _) = char('{')(input)?;
    let (input, _) = ws(input)?;
    let (input, pairs) = separated_list0(preceded(ws, char(',')), parse_key_value)(input)?;
    let (input, _) = ws(input)?;
    let (input, _) = char('}')(input)?;
    let range = make_range(start, input);

    // Convert to IndexMap with key ranges
    // Store ALL pairs (including duplicates) for later validation
    let mut map = indexmap::IndexMap::new();
    let mut all_keys = Vec::new();

    for (key, key_range, value) in pairs {
        all_keys.push((key.clone(), key_range));
        // Keep first occurrence of each key
        map.entry(key).or_insert((key_range, value));
    }

    Ok((
        input,
        JsonValue::Object {
            map,
            range,
            all_keys,
        },
    ))
}

/// Main entry point: parse JSON with position tracking
pub fn parse_json(input: &str) -> Result<JsonValue, String> {
    let span = Span::new(input);
    match parse_value(span) {
        Ok((remaining, value)) => {
            // Check if we consumed all input
            let remaining_trimmed = remaining.fragment().trim();
            if !remaining_trimmed.is_empty() {
                let position = span_to_position(remaining);
                Err(format!(
                    "Parse error at line {}, column {}: Unexpected trailing content",
                    position.line + 1,
                    position.character + 1
                ))
            } else {
                Ok(value)
            }
        }
        Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => {
            // Extract position from nom error
            let error_position = span_to_position(e.input);
            let mut actual_position = error_position;

            // Try to find the actual location of missing comma by scanning the full input
            // Look for specific patterns that indicate missing commas:
            // 1. } followed by { (missing comma between objects in array)
            // 2. ] followed by [ (missing comma between arrays)
            // 3. } followed by " (missing comma between object and next field key)
            // 4. " followed by " (missing comma between strings or after string value)

            let bytes = input.as_bytes();
            let mut i = 0;

            while i < bytes.len() {
                let current = bytes[i];

                // Find the next non-whitespace character
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                    j += 1;
                }

                if j < bytes.len() {
                    let next = bytes[j];

                    // Check for missing comma patterns
                    let is_missing_comma = match (current, next) {
                        // Object followed by object (in array)
                        (b'}', b'{') => true,
                        // Array followed by array
                        (b']', b'[') => true,
                        // Closing quote followed by opening quote (between strings or after value)
                        (b'"', b'"') => {
                            // Need to check if this is not inside a string
                            // For now, assume it's a missing comma if we have this pattern
                            // after skipping whitespace
                            true
                        }
                        // Object closing followed by quote (missing comma before next key)
                        (b'}', b'"') => {
                            // Check if we're not at the end of an array
                            // by looking if there's a reasonable amount of content after
                            j + 1 < bytes.len()
                        }
                        _ => false,
                    };

                    if is_missing_comma {
                        // Report position right after the first delimiter
                        let pos_after = i + 1;
                        actual_position = offset_to_position_in_str(input, pos_after);
                        break;
                    }
                }
                i += 1;
            }

            Err(format!(
                "Parse error at line {}, column {}",
                actual_position.line + 1,
                actual_position.character + 1
            ))
        }
        Err(nom::Err::Incomplete(_)) => Err("Parse error: incomplete input".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_null() {
        let result = parse_json("null").unwrap();
        assert!(matches!(result, JsonValue::Null(_)));
    }

    #[test]
    fn test_parse_bool() {
        let result = parse_json("true").unwrap();
        assert!(matches!(result, JsonValue::Bool(true, _)));
    }

    #[test]
    fn test_parse_string() {
        let result = parse_json(r#""hello""#).unwrap();
        match result {
            JsonValue::String { content, .. } => assert_eq!(content, "hello"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_parse_array() {
        let result = parse_json(r#"[1, 2, 3]"#).unwrap();
        match result {
            JsonValue::Array(arr, _) => assert_eq!(arr.len(), 3),
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_missing_comma_in_array() {
        let input = r#"[
  {"name": "first"}
  {"name": "second"}
]"#;

        let result = parse_json(input);
        assert!(
            result.is_err(),
            "Should fail to parse array with missing comma"
        );

        let err_msg = result.unwrap_err();

        // The error should now point to line 2 (after the first object)
        // where the comma is missing
        assert!(
            err_msg.contains("line 2") || err_msg.contains("line 3"),
            "Error should be on line 2 or 3, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_missing_comma_in_object_fields() {
        let input = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"}
    {"name": "age", "type": "int"}
  ]
}"#;

        let result = parse_json(input);
        assert!(
            result.is_err(),
            "Should fail to parse with missing comma in array"
        );

        let err_msg = result.unwrap_err();

        // The error should be reported at line 5 or 6
        // Line 5: {"name": "name", "type": "string"}  (missing comma after this)
        // Line 6: {"name": "age", "type": "int"}      (unexpected object here)
        assert!(
            err_msg.contains("line 5") || err_msg.contains("line 6"),
            "Error should be on line 5 or 6, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_parse_object() {
        let result = parse_json(r#"{"name": "test", "age": 42}"#).unwrap();
        match result {
            JsonValue::Object { map, .. } => {
                assert_eq!(map.len(), 2);
                assert!(map.contains_key("name"));
                assert!(map.contains_key("age"));
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_positions() {
        let result = parse_json(r#"{"name": "test"}"#).unwrap();
        let range = result.range();
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
    }

    #[test]
    fn test_missing_comma_after_string_property() {
        // Missing comma between "doc" property and "default" property (line 5 in editor, line 4 0-indexed)
        let input = r#"{
  "name": "username",
  "type": "string",
  "doc": "The user's username"
  "default": "anonymous"
}"#;
        let err = parse_json(input).unwrap_err();
        // Should detect error on line 4 (0-indexed)
        assert!(
            err.contains("line 4"),
            "Error should be on line 4 (0-indexed): {}",
            err
        );
        println!("✓ Correct error detected: {}", err);
    }

    #[test]
    fn test_valid_closing_braces_not_flagged() {
        // This should NOT be flagged as missing comma - } followed by ] is valid
        let input = r#"{
  "fields": [
    {
      "name": "id",
      "type": "long"
    }
  ]
}"#;
        let result = parse_json(input);
        // Should parse successfully
        assert!(
            result.is_ok(),
            "Valid JSON should parse successfully: {:?}",
            result.err()
        );
    }
}
