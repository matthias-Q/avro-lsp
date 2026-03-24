use async_lsp::lsp_types::{Position, Range};
use nom::{
    IResult,
    branch::alt,
    bytes::complete::{tag, take},
    character::complete::{char, multispace0},
    combinator::value,
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
        /// Duplicate key errors detected during parsing: (key, first_range, duplicate_range)
        duplicate_keys: Vec<(String, Range, Range)>,
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

/// Decode JSON escape sequences
fn decode_json_escapes(s: &str) -> Result<String, String> {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('/') => result.push('/'),
                Some('b') => result.push('\u{0008}'), // backspace
                Some('f') => result.push('\u{000C}'), // form feed
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('u') => {
                    // Unicode escape: \uXXXX (4 hex digits)
                    let hex: String = chars.by_ref().take(4).collect();
                    if hex.len() != 4 {
                        return Err(format!("Invalid unicode escape: \\u{}", hex));
                    }
                    let code_point = u32::from_str_radix(&hex, 16)
                        .map_err(|_| format!("Invalid hex in unicode escape: \\u{}", hex))?;
                    let ch = char::from_u32(code_point).ok_or_else(|| {
                        format!("Invalid unicode code point: U+{:04X}", code_point)
                    })?;
                    result.push(ch);
                }
                Some(c) => return Err(format!("Invalid escape sequence: \\{}", c)),
                None => return Err("Unexpected end after backslash".to_string()),
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

/// Parse string
fn parse_string(input: Span) -> IResult<Span, JsonValue> {
    let start = input;
    let (input, _) = char('"')(input)?;
    let content_start = input; // Position after opening quote

    // Find the closing quote, tracking escape sequences
    let current = input;
    let mut chars_iter = current.fragment().char_indices().peekable();
    let mut end_index = 0;

    while let Some((i, ch)) = chars_iter.next() {
        match ch {
            '"' => {
                // Found closing quote
                end_index = i;
                break;
            }
            '\\' => {
                // Skip the next character (it's escaped)
                if let Some((_, next_ch)) = chars_iter.next() {
                    // For \uXXXX, we need to skip 4 more hex digits
                    if next_ch == 'u' {
                        // Skip 4 hex digits
                        for _ in 0..4 {
                            chars_iter.next();
                        }
                    }
                } else {
                    // Backslash at end of string - invalid
                    return Err(nom::Err::Failure(nom::error::Error::new(
                        current,
                        nom::error::ErrorKind::Escaped,
                    )));
                }
            }
            _ => {
                // Regular character, continue
            }
        }
    }

    // Extract the raw string content (with escape sequences)
    let raw_content = &current.fragment()[..end_index];

    // IMPORTANT: nom's `take` with &str counts CHARACTERS, not bytes!
    // We need to convert the byte index to a character count.
    // Count how many characters are in the first `end_index` bytes.
    let char_count = raw_content.chars().count();

    // Advance past the content (using character count, not byte count)
    let (input, _) = take(char_count)(current)?;
    let content_end = input;

    // Parse closing quote
    let (input, _) = char('"')(input)?;
    let end = input;

    let full_range = make_range(start, end); // "value" with quotes
    let content_range = make_range(content_start, content_end); // value without quotes

    // Decode escape sequences
    let string_value = decode_json_escapes(raw_content).map_err(|_| {
        nom::Err::Failure(nom::error::Error::new(
            start,
            nom::error::ErrorKind::Escaped,
        ))
    })?;

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

    // Convert to IndexMap with key ranges, detecting duplicates inline
    let mut map = indexmap::IndexMap::new();
    let mut duplicate_keys = Vec::new();

    for (key, key_range, value) in pairs {
        if let Some((first_range, _)) = map.get(&key) {
            // Duplicate: record the first occurrence range and the duplicate range
            duplicate_keys.push((key, *first_range, key_range));
        } else {
            map.insert(key, (key_range, value));
        }
    }

    Ok((
        input,
        JsonValue::Object {
            map,
            range,
            duplicate_keys,
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

            // Try to find the actual location of the syntax error by scanning the full input.
            // Look for specific patterns:
            // 1. , followed by } or ] (trailing comma)
            // 2. } followed by { (missing comma between objects in array)
            // 3. ] followed by [ (missing comma between arrays)
            // 4. } followed by " (missing comma between object and next field key)
            // 5. " followed by " (missing comma between strings or after string value)

            let bytes = input.as_bytes();
            let mut i = 0;
            let mut is_trailing_comma = false;

            while i < bytes.len() {
                let current = bytes[i];

                // Find the next non-whitespace character
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                    j += 1;
                }

                if j < bytes.len() {
                    let next = bytes[j];

                    // Check for trailing comma and missing comma patterns
                    let matched = match (current, next) {
                        // Trailing comma before closing brace or bracket
                        // e.g. {"key": "val",} or [1, 2,]
                        (b',', b'}') | (b',', b']') => {
                            is_trailing_comma = true;
                            true
                        }
                        // Object followed by object (in array)
                        (b'}', b'{') => true,
                        // Array followed by array
                        (b']', b'[') => true,
                        // Closing quote followed by opening quote (between strings or after value)
                        (b'"', b'"') => {
                            // Only flag as a missing-comma if there is whitespace between the
                            // two quotes. Adjacent quotes (j == i + 1) form an empty-string
                            // literal "" and must not be treated as a missing comma.
                            j > i + 1
                        }
                        // Object closing followed by quote (missing comma before next key)
                        (b'}', b'"') => {
                            // Check if we're not at the end of an array
                            // by looking if there's a reasonable amount of content after
                            j + 1 < bytes.len()
                        }
                        _ => false,
                    };

                    if matched {
                        // For trailing commas, point at the comma itself.
                        // For missing commas, point right after the preceding delimiter.
                        let report_pos = if is_trailing_comma { i } else { i + 1 };
                        actual_position = offset_to_position_in_str(input, report_pos);
                        break;
                    }
                }
                i += 1;
            }

            if is_trailing_comma {
                Err(format!(
                    "Trailing comma at line {}, column {}",
                    actual_position.line + 1,
                    actual_position.character + 1
                ))
            } else {
                Err(format!(
                    "Parse error at line {}, column {}",
                    actual_position.line + 1,
                    actual_position.character + 1
                ))
            }
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
    fn test_parse_string_with_utf8() {
        // Test with UTF-8 multibyte characters (curly apostrophe U+2019)
        // Note: Using format! to include the actual UTF-8 character
        let json = format!(r#"{{"doc": "The user{}s title"}}"#, '\u{2019}');
        let result = parse_json(&json).unwrap();
        match result {
            JsonValue::Object { map, .. } => {
                let (_, val) = map.get("doc").expect("Should have 'doc' key");
                match val {
                    JsonValue::String { content, .. } => {
                        assert_eq!(content, &format!("The user{}s title", '\u{2019}'));
                        // Verify the curly apostrophe is preserved
                        assert!(content.contains('\u{2019}'));
                    }
                    _ => panic!("Expected string value"),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_parse_avro_schema_with_utf8() {
        // Real-world test case: Avro schema with UTF-8 in doc strings
        // Note: {{ and }} are escaped braces in format! strings
        let apostrophe = '\u{2019}';
        let json = format!(
            r#"{{
  "type": "record",
  "name": "User",
  "fields": [
    {{
      "name": "title",
      "type": ["null", "string"],
      "default": null,
      "doc": "The user{}s title in the system."
    }}
  ]
}}"#,
            apostrophe
        );
        let result = parse_json(&json);
        assert!(
            result.is_ok(),
            "Should parse schema with UTF-8 characters: {:?}",
            result.err()
        );
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

    #[test]
    fn test_trailing_comma_in_array_reports_correct_line() {
        // Trailing comma after last element in array - error must point at line 6
        let input = r#"{
  "type": "record",
  "name": "User",
  "fields": [
    {"name": "name", "type": "string"},
    {"name": "age", "type": "int"},
  ]
}"#;
        // Lines (1-indexed):
        // 6:     {"name": "age", "type": "int"},   <- trailing comma is here

        let result = parse_json(input);
        assert!(result.is_err(), "Should fail to parse with trailing comma");

        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("line 6"),
            "Error should point to line 6 (the trailing comma), got: '{}'",
            err_msg
        );
        assert!(
            err_msg.contains("Trailing comma"),
            "Error message should mention 'Trailing comma', got: '{}'",
            err_msg
        );
    }

    #[test]
    fn test_trailing_comma_in_object_reports_correct_line() {
        // Trailing comma after last field in object
        let input = r#"{
  "type": "record",
  "name": "User",
}"#;
        // Line 3: "name": "User",   <- trailing comma

        let result = parse_json(input);
        assert!(
            result.is_err(),
            "Should fail to parse with trailing comma in object"
        );

        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("line 3"),
            "Error should point to line 3 (the trailing comma), got: '{}'",
            err_msg
        );
        assert!(
            err_msg.contains("Trailing comma"),
            "Error message should mention 'Trailing comma', got: '{}'",
            err_msg
        );
    }

    #[test]
    fn test_empty_string_value_does_not_cause_false_positive() {
        // A field with an empty-string default followed by another field used to
        // falsely trigger the missing-comma detector on the "" literal (line 9)
        // instead of reporting the actual missing comma on line 13.
        let input = r#"{
  "type": "record",
  "name": "Note",
  "fields": [
    {
      "name": "note",
      "type": "string",
      "default": "",
      "doc": "Optional note"
    },
    {
      "name": "billing_address",
      "type": "Address"
      "doc": "Billing address"
    }
  ]
}"#;
        // Line 13 (1-indexed): "type": "Address"  <- missing comma at end of this line
        let err = parse_json(input).unwrap_err();
        assert!(
            err.contains("line 13"),
            "Error should point to the actual missing-comma location (line 13), got: '{err}'"
        );
        assert!(
            !err.contains("line 9"),
            "Error must NOT point to the empty-string literal on line 9, got: '{err}'"
        );
    }

    #[test]
    fn test_escape_sequences_basic() {
        // Test basic escape sequences
        let result = parse_json(r#""Hello\nWorld""#).unwrap();
        match result {
            JsonValue::String { content, .. } => {
                assert_eq!(content, "Hello\nWorld", "\\n should decode to newline");
            }
            _ => panic!("Expected string"),
        }

        let result = parse_json(r#""Tab\there""#).unwrap();
        match result {
            JsonValue::String { content, .. } => {
                assert_eq!(content, "Tab\there", "\\t should decode to tab");
            }
            _ => panic!("Expected string"),
        }

        let result = parse_json(r#""Quote: \"text\"""#).unwrap();
        match result {
            JsonValue::String { content, .. } => {
                assert_eq!(content, "Quote: \"text\"", "\\\" should decode to quote");
            }
            _ => panic!("Expected string"),
        }

        let result = parse_json(r#""Backslash: \\""#).unwrap();
        match result {
            JsonValue::String { content, .. } => {
                assert_eq!(content, "Backslash: \\", "\\\\ should decode to backslash");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_escape_sequences_unicode() {
        // Test Unicode escape sequences
        let result = parse_json(r#""\u00FF""#).unwrap();
        match result {
            JsonValue::String { content, .. } => {
                assert_eq!(content, "\u{00FF}", "\\u00FF should decode to ÿ");
            }
            _ => panic!("Expected string"),
        }

        let result = parse_json(r#""\u0041\u0042\u0043""#).unwrap();
        match result {
            JsonValue::String { content, .. } => {
                assert_eq!(content, "ABC", "\\u0041\\u0042\\u0043 should decode to ABC");
            }
            _ => panic!("Expected string"),
        }

        // Test emoji (higher code points still work in 4-digit range)
        let result = parse_json(r#""\u2764""#).unwrap();
        match result {
            JsonValue::String { content, .. } => {
                assert_eq!(content, "\u{2764}", "\\u2764 should decode to ❤");
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_escape_sequences_mixed() {
        // Test mixed escape sequences
        let result = parse_json(r#""Line1\nLine2\tTab\u0041""#).unwrap();
        match result {
            JsonValue::String { content, .. } => {
                assert_eq!(
                    content, "Line1\nLine2\tTabA",
                    "Mixed escapes should all decode correctly"
                );
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_escape_sequences_in_object() {
        // Test escape sequences within a JSON object (real-world scenario)
        let result = parse_json(r#"{"type": "string", "default": "Hello\nWorld\u00FF"}"#).unwrap();
        match result {
            JsonValue::Object { map, .. } => {
                let (_, default_value) = map.get("default").unwrap();
                match default_value {
                    JsonValue::String { content, .. } => {
                        assert_eq!(content, "Hello\nWorld\u{00FF}");
                    }
                    _ => panic!("Expected string value"),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_decode_json_escapes() {
        // Test the decode_json_escapes function directly
        assert_eq!(decode_json_escapes("hello").unwrap(), "hello");
        assert_eq!(
            decode_json_escapes("hello\\nworld").unwrap(),
            "hello\nworld"
        );
        assert_eq!(decode_json_escapes("\\t\\r\\n").unwrap(), "\t\r\n");
        assert_eq!(decode_json_escapes("\\\"quote\\\"").unwrap(), "\"quote\"");
        assert_eq!(decode_json_escapes("\\\\backslash").unwrap(), "\\backslash");
        assert_eq!(decode_json_escapes("\\u00FF").unwrap(), "\u{00FF}");
        assert_eq!(decode_json_escapes("A\\u0042C").unwrap(), "ABC");

        // Test all JSON escape sequences from RFC 8259
        assert_eq!(decode_json_escapes("\\/").unwrap(), "/"); // forward slash
        assert_eq!(decode_json_escapes("\\b").unwrap(), "\u{0008}"); // backspace
        assert_eq!(decode_json_escapes("\\f").unwrap(), "\u{000C}"); // form feed

        // Test invalid escape sequences
        assert!(decode_json_escapes("\\x").is_err());
        assert!(decode_json_escapes("\\u00").is_err()); // Too few hex digits
        assert!(decode_json_escapes("\\uXYZW").is_err()); // Invalid hex
    }
}
