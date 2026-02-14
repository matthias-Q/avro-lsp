use regex::Regex;

use crate::schema::error::{Result, SchemaError};

#[allow(dead_code)]
pub fn validate_name(name: &str, name_regex: &Regex) -> Result<()> {
    validate_name_with_range(name, None, name_regex)
}

pub fn validate_name_with_range(
    name: &str,
    range: Option<async_lsp::lsp_types::Range>,
    name_regex: &Regex,
) -> Result<()> {
    if !name_regex.is_match(name) {
        let suggested = fix_invalid_name(name);
        return Err(SchemaError::InvalidName {
            name: name.to_string(),
            range,
            suggested: Some(suggested),
        });
    }
    Ok(())
}

#[allow(dead_code)]
pub fn validate_namespace(namespace: &str, name_regex: &Regex) -> Result<()> {
    validate_namespace_with_range(namespace, None, name_regex)
}

pub fn validate_namespace_with_range(
    namespace: &str,
    range: Option<async_lsp::lsp_types::Range>,
    name_regex: &Regex,
) -> Result<()> {
    if namespace.is_empty() {
        return Ok(());
    }

    for part in namespace.split('.') {
        if !name_regex.is_match(part) {
            let suggested = fix_invalid_namespace(namespace);
            return Err(SchemaError::InvalidNamespace {
                namespace: namespace.to_string(),
                range,
                suggested: Some(suggested),
            });
        }
    }
    Ok(())
}

fn fix_invalid_name(name: &str) -> String {
    if name.is_empty() {
        return "field".to_string();
    }

    let mut result = String::new();

    for (i, ch) in name.chars().enumerate() {
        if i == 0 {
            if ch.is_ascii_alphabetic() || ch == '_' {
                result.push(ch);
            } else if ch.is_ascii_digit() {
                result.push('_');
                result.push(ch);
            } else {
                result.push('_');
            }
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            result.push(ch);
        } else {
            result.push('_');
        }
    }

    if result.is_empty() {
        result = "field".to_string();
    }

    result
}

fn fix_invalid_namespace(namespace: &str) -> String {
    let name_regex = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();

    let valid_parts: Vec<&str> = namespace
        .split('.')
        .filter(|part| name_regex.is_match(part))
        .collect();

    if valid_parts.is_empty() {
        String::new()
    } else {
        valid_parts.join(".")
    }
}
