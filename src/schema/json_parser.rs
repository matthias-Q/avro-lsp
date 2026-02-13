use async_lsp::lsp_types::{Position, Range};
use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take_while},
    character::complete::{char, multispace0, one_of},
    combinator::{opt, value},
    multi::separated_list0,
    number::complete::double,
    sequence::preceded,
    IResult,
};
use nom_locate::LocatedSpan;
use std::collections::HashMap;

/// Input type with position tracking
pub type Span<'a> = LocatedSpan<&'a str>;

/// JSON value with position information
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null(Range),
    Bool(bool, Range),
    Number(f64, Range),
    String(String, Range),
    Array(Vec<JsonValue>, Range),
    Object(HashMap<String, JsonValue>, Range),
}

impl JsonValue {
    pub fn range(&self) -> Range {
        match self {
            JsonValue::Null(r) => *r,
            JsonValue::Bool(_, r) => *r,
            JsonValue::Number(_, r) => *r,
            JsonValue::String(_, r) => *r,
            JsonValue::Array(_, r) => *r,
            JsonValue::Object(_, r) => *r,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            JsonValue::String(s, _) => Some(s),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, JsonValue>> {
        match self {
            JsonValue::Object(o, _) => Some(o),
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
        line: (span.location_line() - 1),
        character: (span.get_column() - 1) as u32,
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
    let (input, s) = opt(escaped(
        take_while(|c| c != '"' && c != '\\'),
        '\\',
        one_of(r#""\/bfnrt"#),
    ))(input)?;
    let (input, _) = char('"')(input)?;
    let range = make_range(start, input);
    let string_value = s.map(|s| s.fragment().to_string()).unwrap_or_default();
    Ok((input, JsonValue::String(string_value, range)))
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
fn parse_key_value(input: Span) -> IResult<Span, (String, JsonValue)> {
    let (input, _) = ws(input)?;
    let (input, key) = parse_string(input)?;
    let key_str = match key {
        JsonValue::String(s, _) => s,
        _ => unreachable!(),
    };
    let (input, _) = ws(input)?;
    let (input, _) = char(':')(input)?;
    let (input, value) = parse_value(input)?;
    Ok((input, (key_str, value)))
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
    let map: HashMap<String, JsonValue> = pairs.into_iter().collect();
    Ok((input, JsonValue::Object(map, range)))
}

/// Main entry point: parse JSON with position tracking
pub fn parse_json(input: &str) -> Result<JsonValue, String> {
    let span = Span::new(input);
    match parse_value(span) {
        Ok((remaining, value)) => {
            // Check if we consumed all input
            let remaining_trimmed = remaining.fragment().trim();
            if !remaining_trimmed.is_empty() {
                Err(format!(
                    "Unexpected trailing content: {}",
                    remaining_trimmed
                ))
            } else {
                Ok(value)
            }
        }
        Err(e) => Err(format!("Parse error: {}", e)),
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
            JsonValue::String(s, _) => assert_eq!(s, "hello"),
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
    fn test_parse_object() {
        let result = parse_json(r#"{"name": "test", "age": 42}"#).unwrap();
        match result {
            JsonValue::Object(obj, _) => {
                assert_eq!(obj.len(), 2);
                assert!(obj.contains_key("name"));
                assert!(obj.contains_key("age"));
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
}
