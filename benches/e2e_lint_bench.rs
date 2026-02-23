use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};

use avro_lsp::schema::{AvroParser, AvroValidator};

/// End-to-end benchmark for linting the comprehensive deeply nested schema
/// This simulates the complete `avro-lsp lint` workflow:
/// 1. Parse the schema
/// 2. Validate the schema
/// 3. Generate diagnostics
///
/// This benchmark uses the examples/comprehensive_deeply_nested.avsc file which contains:
/// - All Avro primitive types (null, boolean, int, long, float, double, bytes, string)
/// - All complex types (record, enum, array, map, union, fixed)
/// - 10 levels of deeply nested records
/// - Complex combinations (arrays of maps of records, etc.)
/// - 500+ lines of JSON schema definition
fn bench_lint_comprehensive_deeply_nested(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_lint");

    let comprehensive_deep = include_str!("../examples/comprehensive_deeply_nested.avsc");

    // Set throughput to measure performance per byte
    group.throughput(Throughput::Bytes(comprehensive_deep.len() as u64));

    // Benchmark the complete lint workflow
    group.bench_function("comprehensive_deeply_nested", |b| {
        b.iter(|| {
            // Step 1: Parse the schema
            let mut parser = AvroParser::new();
            let schema = parser
                .parse(black_box(comprehensive_deep))
                .expect("Schema should be valid");

            // Step 2: Validate the schema
            let validator = AvroValidator::new();
            let diagnostics = validator.validate(black_box(&schema));

            // Ensure diagnostics are computed
            black_box(diagnostics)
        });
    });

    group.finish();
}

/// Benchmark parsing only (no validation) for the deeply nested schema
/// This isolates the parser performance from validation
fn bench_parse_only_comprehensive_deeply_nested(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_parse_only");

    let comprehensive_deep = include_str!("../examples/comprehensive_deeply_nested.avsc");

    group.throughput(Throughput::Bytes(comprehensive_deep.len() as u64));

    group.bench_function("comprehensive_deeply_nested", |b| {
        b.iter(|| {
            let mut parser = AvroParser::new();
            let schema = parser
                .parse(black_box(comprehensive_deep))
                .expect("Schema should be valid");

            black_box(schema)
        });
    });

    group.finish();
}

/// Benchmark validation only (pre-parsed) for the deeply nested schema
/// This isolates the validator performance from parsing
fn bench_validate_only_comprehensive_deeply_nested(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_validate_only");

    let comprehensive_deep = include_str!("../examples/comprehensive_deeply_nested.avsc");

    // Pre-parse the schema once
    let mut parser = AvroParser::new();
    let schema = parser
        .parse(comprehensive_deep)
        .expect("Schema should be valid");

    group.throughput(Throughput::Bytes(comprehensive_deep.len() as u64));

    group.bench_function("comprehensive_deeply_nested", |b| {
        b.iter(|| {
            let validator = AvroValidator::new();
            let diagnostics = validator.validate(black_box(&schema));

            black_box(diagnostics)
        });
    });

    group.finish();
}

/// Compare lint performance across different schema complexities
/// Shows how performance scales with schema size and complexity
fn bench_lint_complexity_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_lint_comparison");

    // Simple schema
    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");

    // Medium complexity
    let nested = include_str!("../tests/fixtures/valid/nested_record.avsc");

    // High complexity with all types
    let comprehensive = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");

    // Extreme complexity with deep nesting
    let comprehensive_deep = include_str!("../examples/comprehensive_deeply_nested.avsc");

    // Benchmark simple schema
    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_function("simple_record", |b| {
        b.iter(|| {
            let mut parser = AvroParser::new();
            let schema = parser.parse(black_box(simple)).expect("Valid schema");
            let validator = AvroValidator::new();
            let diagnostics = validator.validate(black_box(&schema));
            black_box(diagnostics)
        });
    });

    // Benchmark nested schema
    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_function("nested_record", |b| {
        b.iter(|| {
            let mut parser = AvroParser::new();
            let schema = parser.parse(black_box(nested)).expect("Valid schema");
            let validator = AvroValidator::new();
            let diagnostics = validator.validate(black_box(&schema));
            black_box(diagnostics)
        });
    });

    // Benchmark comprehensive schema
    group.throughput(Throughput::Bytes(comprehensive.len() as u64));
    group.bench_function("comprehensive_types", |b| {
        b.iter(|| {
            let mut parser = AvroParser::new();
            let schema = parser
                .parse(black_box(comprehensive))
                .expect("Valid schema");
            let validator = AvroValidator::new();
            let diagnostics = validator.validate(black_box(&schema));
            black_box(diagnostics)
        });
    });

    // Benchmark deeply nested comprehensive schema
    group.throughput(Throughput::Bytes(comprehensive_deep.len() as u64));
    group.bench_function("comprehensive_deeply_nested", |b| {
        b.iter(|| {
            let mut parser = AvroParser::new();
            let schema = parser
                .parse(black_box(comprehensive_deep))
                .expect("Valid schema");
            let validator = AvroValidator::new();
            let diagnostics = validator.validate(black_box(&schema));
            black_box(diagnostics)
        });
    });

    group.finish();
}

/// Benchmark memory allocation patterns during lint
/// Useful for identifying allocation hotspots
fn bench_lint_with_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("e2e_lint_stats");

    let comprehensive_deep = include_str!("../examples/comprehensive_deeply_nested.avsc");

    group.throughput(Throughput::Bytes(comprehensive_deep.len() as u64));

    // Configure for more detailed statistics
    group.sample_size(100); // More samples for better statistics

    group.bench_function("comprehensive_deeply_nested_detailed", |b| {
        b.iter(|| {
            let mut parser = AvroParser::new();
            let schema = parser
                .parse(black_box(comprehensive_deep))
                .expect("Schema should be valid");

            let validator = AvroValidator::new();
            let diagnostics = validator.validate(black_box(&schema));

            // Return both to ensure they're not optimized away
            black_box((schema, diagnostics))
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_lint_comprehensive_deeply_nested,
    bench_parse_only_comprehensive_deeply_nested,
    bench_validate_only_comprehensive_deeply_nested,
    bench_lint_complexity_comparison,
    bench_lint_with_stats,
);

criterion_main!(benches);
