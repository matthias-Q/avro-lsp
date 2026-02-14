# Parser Benchmarks

This directory contains benchmarks for the nom-based JSON parser used in avro-lsp.

## Overview

The benchmarks use [Criterion.rs](https://github.com/bheisler/criterion.rs) to measure parsing performance with:
- Statistical analysis and confidence intervals
- Throughput measurements (MiB/s)
- HTML reports with graphs
- Comparison between runs

## Benchmark Groups

### 1. `parse_small_schema` 
Tests parsing performance on small schemas (~10 lines):
- **simple_record.avsc** - Basic record with 2 fields

### 2. `parse_medium_schema`
Tests parsing performance on medium schemas (~30 lines):
- **nested_record.avsc** - Nested record structure with 3 fields

### 3. `parse_large_schema`
Tests parsing performance on large schemas (~80 lines):
- **all_logical_types.avsc** - Complex schema with 10 fields and various logical types

### 4. `parse_schema_comparison`
Comparative benchmark showing relative performance across all three schema sizes side-by-side.

### 5. `parse_various_schemas`
Tests parsing performance on different schema types:
- **union_example.avsc** - Union types
- **fixed_example.avsc** - Fixed-length binary types
- **array_map_example.avsc** - Array and map collections

### 6. `parse_invalid_schemas`
Measures error detection performance on malformed schemas:
- **missing_fields.avsc** - Missing required fields
- **duplicate_symbols.avsc** - Duplicate enum symbols

## Running Benchmarks

### Run all benchmarks
```bash
cargo bench
```

### Run specific benchmark group
```bash
cargo bench parse_small_schema
cargo bench parse_comparison
cargo bench parse_invalid
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

Based on current results (quick run):

| Schema Size | Lines | Time (µs) | Throughput (MiB/s) |
|-------------|-------|-----------|-------------------|
| Small       | ~10   | 3.8       | 34                |
| Medium      | ~30   | 12.3      | 44                |
| Large       | ~80   | 27.0      | 51                |

**Key Observations:**
- Parser scales well with input size
- Throughput increases with larger schemas (better amortization of overhead)
- Error detection is fast (~1-2 µs for simple malformed schemas)

## Optimization Tips

When optimizing the parser:

1. **Run full benchmarks** (not `--quick`) for accurate measurements
2. **Save baseline** before making changes
3. **Compare against baseline** after changes
4. **Check confidence intervals** - smaller is better
5. **Look for regressions** across all benchmark groups, not just one

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

To add a new benchmark:

1. Add test schema to `tests/fixtures/valid/` or `tests/fixtures/invalid/`
2. Add benchmark function in `benches/parser_bench.rs`:
```rust
fn bench_my_new_test(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/my_schema.avsc");
    c.bench_function("my_test", |b| {
        b.iter(|| parse_json(black_box(input)))
    });
}
```
3. Add to `criterion_group!` macro at bottom of file
4. Run: `cargo bench bench_my_new_test`

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
