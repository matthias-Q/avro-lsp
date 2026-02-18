# avro-lsp Benchmarks

This directory contains comprehensive benchmarks for the avro-lsp language server, covering parsing, validation, LSP handlers, workspace operations, and end-to-end workflows.

## Overview

The benchmarks use [Criterion.rs](https://github.com/bheisler/criterion.rs) to measure parsing performance with:
- Statistical analysis and confidence intervals
- Throughput measurements (MiB/s)
- HTML reports with graphs
- Comparison between runs

## Benchmark Suites

### 1. Parser Benchmarks (`parser_bench.rs`)
Tests JSON parsing performance with the nom-based parser.

#### Groups:
- `parse_small_schema` - Small schemas (~10 lines)
- `parse_medium_schema` - Medium schemas (~30 lines)
- `parse_large_schema` - Large schemas (~80 lines)
- `parse_schema_comparison` - Side-by-side comparison
- `parse_various_schemas` - Different schema types (union, fixed, array/map)
- `parse_invalid_schemas` - Error detection performance

### 2. Validator Benchmarks (`validator_bench.rs`)
Tests schema validation performance across different schema types and complexities.

#### Groups:
- `validate_simple` - Basic record validation
- `validate_nested` - Nested record structures
- `validate_comprehensive` - All Avro types and features
- `validate_logical_types` - Logical type validation (decimal, timestamp, etc.)
- `validate_defaults` - Default value validation
- `validate_unions` - Union type validation
- `validate_enums` - Enum symbol validation
- `validate_collections` - Array and map validation
- `validate_invalid` - Error detection on invalid schemas
- `validate_comparison` - Comparative validation across complexities

### 3. Handler Benchmarks (`handlers_bench.rs`)
Tests LSP request handler performance for editor interactions.

#### Groups:
- `hover` - Type information lookup (type names, primitives, fields)
- `completion` - Suggestion generation (keys, type values)
- `symbols` - Document outline/symbol tree creation
- `semantic_tokens` - Full document semantic highlighting
- `inlay_hints` - Inline type hint generation
- `folding_ranges` - Code folding region calculation
- `formatting` - JSON pretty-printing
- `handler_comparison` - Cross-handler performance comparison

### 4. Workspace Benchmarks (`workspace_bench.rs`)
Tests multi-file workspace operations and cross-file features.

#### Groups:
- `add_file` - Single file parsing and type registration
- `initialize_workspace` - Multi-file workspace indexing (5, 10 files)
- `type_resolution` - Cross-file type lookup
- `find_references` - Workspace-wide reference search
- `remove_file` - File cleanup and deregistration
- `validate_workspace` - Cross-file validation
- `workspace_comparison` - Operation comparison

### 5. Integration Benchmarks (`integration_bench.rs`)
Tests realistic end-to-end workflows combining multiple operations.

#### Groups:
- `file_open_workflow` - Parse → Validate → Tokens → Symbols
- `incremental_edit_workflow` - Reparse → Revalidate on keystroke
- `completion_workflow` - Trigger → Context → Suggestions
- `hover_workflow` - Word lookup → Type info → Format
- `format_workflow` - Parse → Format → Replace
- `project_initialization` - Scan → Parse all → Index → Validate
- `cross_file_navigation` - Find reference → Resolve → Location
- `find_references_workflow` - Search all → Collect → Return locations
- `workflow_comparison` - Complexity comparison across workflows

## Running Benchmarks

### Run all benchmarks
```bash
cargo bench
```

### Run specific benchmark suite
```bash
cargo bench --bench parser_bench      # Parser only
cargo bench --bench validator_bench   # Validator only
cargo bench --bench handlers_bench    # LSP handlers only
cargo bench --bench workspace_bench   # Workspace operations only
cargo bench --bench integration_bench # End-to-end workflows only
```

### Run specific benchmark group
```bash
cargo bench validate_simple            # Specific group
cargo bench hover                      # All hover benchmarks
cargo bench file_open_workflow         # Integration workflow
```

### Quick run (faster, less accurate)
```bash
cargo bench -- --quick
```

### Verbose output
```bash
cargo bench -- --verbose
```

### Run benchmark tests (verify they compile/work)
```bash
cargo bench -- --test
```

## Baseline Comparisons

Save a baseline to compare future changes:

```bash
# Save current performance as baseline
cargo bench -- --save-baseline main

# Make code changes...

# Compare against baseline
cargo bench -- --baseline main
```

This will show you if your changes improved or degraded performance.

## Understanding Output

### Terminal Output
```
parse_small_schema/simple_record
                        time:   [3.76 µs 3.78 µs 3.87 µs]
                        thrpt:  [33.29 MiB/s 34.02 MiB/s 34.20 MiB/s]
```

- **time**: Mean parsing time with confidence interval
  - Lower is better
- **thrpt**: Throughput in MiB/s
  - Higher is better
  - Shows how many megabytes per second can be parsed

### HTML Reports

After running benchmarks, view detailed reports:
```bash
open target/criterion/report/index.html
```

Reports include:
- Performance graphs
- Distribution plots
- Statistical analysis
- Historical comparisons

## Performance Expectations

Based on typical results (varies by machine):

### Parser Performance
| Schema Size | Lines | Time (µs) | Throughput (MiB/s) |
|-------------|-------|-----------|-------------------|
| Small       | ~10   | 3-5       | 30-40             |
| Medium      | ~30   | 10-15     | 40-50             |
| Large       | ~80   | 25-35     | 45-55             |

### Validator Performance
| Schema Type      | Time (µs) | Notes                          |
|------------------|-----------|--------------------------------|
| Simple           | 1-3       | Basic record with 2 fields     |
| Nested           | 5-10      | Nested structures              |
| Comprehensive    | 15-25     | All types and features         |
| Logical types    | 10-20     | Decimal, timestamp validation  |

### LSP Handler Performance
| Handler         | Time (µs) | Target    | Notes                      |
|-----------------|-----------|-----------|----------------------------|
| Hover           | 1-5       | <50       | Type lookup                |
| Completion      | 5-15      | <50       | Suggestion generation      |
| Symbols         | 3-10      | <50       | Document outline           |
| Semantic tokens | 10-30     | <100      | Full document highlighting |
| Formatting      | 5-15      | <50       | JSON pretty-print          |

### Workspace Performance
| Operation           | Files | Time (µs) | Target  | Notes                    |
|---------------------|-------|-----------|---------|--------------------------|
| Add file            | 1     | 5-15      | <50     | Parse + register types   |
| Initialize workspace| 5     | 50-150    | <500    | Index all files          |
| Type resolution     | -     | 0.5-2     | <10     | HashMap lookup           |
| Find references     | 3     | 10-30     | <100    | Cross-file search        |

### Integration Workflow Performance
| Workflow              | Time (µs) | Target | Notes                              |
|-----------------------|-----------|--------|------------------------------------|
| File open             | 20-50     | <100   | Parse + validate + tokens + symbols|
| Incremental edit      | 5-15      | <50    | Reparse + revalidate               |
| Project init (5 files)| 100-300   | <500   | Full workspace setup               |
| Cross-file navigation | 5-15      | <50    | Type resolution + location         |

**Key Observations:**
- Parser scales well with input size (throughput increases)
- Validation is fast for typical schemas (<20 µs)
- LSP handlers meet <100ms target for good UX
- Workspace operations are efficient even with multiple files
- End-to-end workflows complete in <1ms for most cases

## Optimization Tips

When optimizing avro-lsp performance:

1. **Run full benchmarks** (not `--quick`) for accurate measurements
2. **Save baseline** before making changes
3. **Compare against baseline** after changes
4. **Check confidence intervals** - smaller is better
5. **Look for regressions** across all benchmark suites, not just one
6. **Focus on critical paths**:
   - Parser and validator (run on every change)
   - Semantic tokens (run on every file open/change)
   - Completion and hover (run on user interaction)
7. **Profile before optimizing** - use benchmarks to identify bottlenecks
8. **Consider algorithmic improvements** over micro-optimizations

## Troubleshooting

### "Gnuplot not found, using plotters backend"
This is normal. Criterion will use the Rust-based plotters backend instead.
Install gnuplot if you want: `sudo apt install gnuplot` (Linux) or `brew install gnuplot` (macOS)

### Benchmarks take too long
Use `--quick` flag for faster (less accurate) results during development.

### Need more samples
Criterion automatically adjusts sample size. To force a specific sample count:
```rust
group.sample_size(1000);  // Add to benchmark code
```

## Adding New Benchmarks

### Add to existing suite

1. Add test schema to `tests/fixtures/valid/` or `tests/fixtures/invalid/`
2. Add benchmark function to appropriate file:

```rust
fn bench_my_new_test(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/my_schema.avsc");
    let schema = parse_json(input).expect("Valid schema");
    
    let mut group = c.benchmark_group("my_group");
    group.bench_function("my_test", |b| {
        b.iter(|| {
            let validator = AvroValidator::new();
            let result = validator.validate(black_box(&schema));
            black_box(result)
        });
    });
    group.finish();
}
```

3. Add to `criterion_group!` macro at bottom of file
4. Run: `cargo bench my_group`

### Add new benchmark suite

1. Create new file: `benches/my_bench.rs`
2. Add to `Cargo.toml`:
```toml
[[bench]]
name = "my_bench"
harness = false
```
3. Implement benchmark groups with `criterion_group!` and `criterion_main!`
4. Run: `cargo bench --bench my_bench`

## Test Fixtures

Benchmarks use schemas from `tests/fixtures/`:

### Single-file Schemas (`tests/fixtures/valid/`)
- `simple_record.avsc` - Basic record with 2 fields (~10 lines)
- `nested_record.avsc` - Nested record structure (~30 lines)
- `comprehensive_types.avsc` - All Avro types (~80 lines)
- `all_logical_types.avsc` - Logical types (decimal, timestamp, etc.)
- `union_example.avsc` - Union types
- `enum_example.avsc` - Enum type
- `fixed_example.avsc` - Fixed-length binary type
- `array_map_example.avsc` - Collections
- `valid_default_values.avsc` - Default value examples

### Multi-file Workspace (`tests/fixtures/workspace/`)
- `common_types.avsc` - Shared Address type
- `user.avsc` - User record (references Address)
- `order.avsc` - Order record (references Address)
- `product.avsc` - Product record
- `event.avsc` - Event record with enum

These fixtures test cross-file type resolution and workspace features.

### Invalid Schemas (`tests/fixtures/invalid/`)
Used to benchmark error detection performance.

## CI Integration

To use in CI:
```bash
# Run benchmarks and save as baseline
cargo bench -- --save-baseline ci

# On subsequent runs, compare
cargo bench -- --baseline ci
```

Consider using [criterion-table](https://github.com/nu11ptr/criterion-table) or similar tools to generate markdown tables for PR comments.

## References

- [Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/)
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [nom Parser Combinators](https://github.com/rust-bakery/nom)
