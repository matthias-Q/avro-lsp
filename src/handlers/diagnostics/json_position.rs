use async_lsp::lsp_types::Position;

/// Extract position from error message and adjust for JSON syntax errors
/// Returns (Position, was_adjusted)
pub(super) fn extract_error_position_with_context(error_msg: &str, text: &str) -> (Position, bool) {
    // serde_json errors often contain "line X, column Y"
    if let Some(line_pos) = error_msg.find("line ")
        && let Some(col_pos) = error_msg.find("column ")
    {
        let line_str = &error_msg[line_pos + 5..];
        let line_end = line_str
            .find(|c: char| !c.is_numeric())
            .unwrap_or(line_str.len());
        let line_num: u32 = line_str[..line_end].parse().unwrap_or(1);

        let col_str = &error_msg[col_pos + 7..];
        let col_end = col_str
            .find(|c: char| !c.is_numeric())
            .unwrap_or(col_str.len());
        let col_num: u32 = col_str[..col_end].parse().unwrap_or(0);

        let mut position = Position {
            line: line_num.saturating_sub(1), // LSP is 0-indexed
            character: col_num.saturating_sub(1),
        };

        let mut was_adjusted = false;
        let lines: Vec<&str> = text.lines().collect();

        // Try to find the actual location of the syntax error by looking for common patterns
        // Check if this looks like a missing comma error by scanning backwards for array/object elements
        if position.line > 0 && position.line < lines.len() as u32 {
            let error_line_idx = position.line as usize;

            // Check if we're in an array/object context and find missing commas
            // by looking for lines ending with } or ] without a comma before the next element
            for i in (0..error_line_idx).rev().take(10) {
                let line = lines[i].trim_end();

                // If we find a line ending with } or ] (end of an object/array element)
                // and the next non-empty line starts with { or contains a field/element
                // then this line is missing a comma
                if (line.ends_with('}') || line.ends_with(']')) && !line.ends_with(',') {
                    // Check if the next non-empty line looks like it starts a new element
                    if let Some(next_line) = lines.get(i + 1) {
                        let next_trimmed = next_line.trim();
                        if next_trimmed.starts_with('{') || next_trimmed.starts_with("\"") {
                            position = Position {
                                line: i as u32,
                                character: line.len() as u32,
                            };
                            was_adjusted = true;
                            tracing::debug!(
                                "Adjusted JSON parse error to missing comma location: line {}, after '{}'",
                                i,
                                line
                            );
                            break;
                        }
                    }
                }
            }
        }

        // Fallback: For JSON parse errors at the start of a line (column near 0),
        // adjust to the end of the previous line (likely missing comma/brace)
        if !was_adjusted
            && position.character <= 2
            && position.line > 0
            && let Some(prev_line) = lines.get(position.line as usize - 1)
        {
            let prev_line_len = prev_line.len() as u32;
            position = Position {
                line: position.line - 1,
                character: prev_line_len,
            };
            was_adjusted = true;
            tracing::debug!(
                "Adjusted JSON parse error position to end of previous line: {:?}",
                position
            );
        }

        return (position, was_adjusted);
    }

    (
        Position {
            line: 0,
            character: 0,
        },
        false,
    )
}

/// Improve error message with correct position and helpful hints
pub(super) fn improve_error_message(
    original_msg: &str,
    pos: &Position,
    was_adjusted: bool,
) -> String {
    let base_msg = if let Some(colon_pos) = original_msg.find(": ") {
        &original_msg[colon_pos + 2..]
    } else {
        original_msg
    };

    let location = format!("line {}, column {}", pos.line + 1, pos.character + 1);

    if was_adjusted {
        format!(
            "JSON syntax error at {}: Expected comma or closing brace",
            location
        )
    } else if base_msg.contains("Unexpected trailing content") {
        format!("JSON syntax error at {}: {}", location, base_msg)
    } else {
        format!("JSON parse error at {}", location)
    }
}
