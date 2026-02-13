// Quick test to show current multi-file behavior
use std::fs;

fn main() {
    let user_schema = fs::read_to_string("tests/fixtures/multi-file/user.avsc")
        .expect("Failed to read user.avsc");

    println!("Testing user.avsc which references 'Address' type:");
    println!("{}\n", user_schema);

    // Simulate what parse_and_validate does
    let diagnostics = avro_lsp::handlers::diagnostics::parse_and_validate(&user_schema);

    if diagnostics.is_empty() {
        println!("✓ No diagnostics (validation passed)");
    } else {
        println!("✗ Found {} diagnostic(s):", diagnostics.len());
        for diag in diagnostics {
            println!("  - {}", diag.message);
        }
    }
}
