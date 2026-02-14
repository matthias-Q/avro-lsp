use async_lsp::lsp_types::{Position, Range};

/// Find the precise range of a nested union [[...]] within a field range
pub(super) fn find_nested_union_range(text: &str, field_range: Range) -> Option<Range> {
    let lines: Vec<&str> = text.lines().collect();

    // Search within the field range for the nested union pattern [[
    for line_idx in field_range.start.line..=field_range.end.line {
        if let Some(line) = lines.get(line_idx as usize) {
            // Look for [[ pattern which indicates nested union
            if let Some(pos) = line.find("[[") {
                // We want to highlight the OUTER array to make it easy to trigger
                // the "flatten" code action anywhere in the union
                let outer_start = pos;
                let start_char = outer_start as u32;

                // Find the matching ]] for the outer array
                let mut bracket_count = 0;
                let mut end_pos = outer_start;

                for (idx, ch) in line[outer_start..].char_indices() {
                    if ch == '[' {
                        bracket_count += 1;
                    } else if ch == ']' {
                        bracket_count -= 1;
                        if bracket_count == 0 {
                            end_pos = outer_start + idx + 1;
                            break;
                        }
                    }
                }

                return Some(Range {
                    start: Position {
                        line: line_idx,
                        character: start_char,
                    },
                    end: Position {
                        line: line_idx,
                        character: end_pos as u32,
                    },
                });
            }
        }
    }

    None
}
