use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

use avro_lsp::schema::{AvroParser, AvroValidator};

/// Benchmark simple schema validation
/// Tests basic record validation with minimal complexity
fn bench_validate_simple(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).expect("Valid schema should parse");

    let mut group = c.benchmark_group("validate_simple");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("simple_record", |b| {
        b.iter(|| {
            let validator = AvroValidator::new();
            let result = validator.validate(black_box(&schema));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark complex nested schema validation
/// Tests deeply nested records with multiple field types
fn bench_validate_nested(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).expect("Valid schema should parse");

    let mut group = c.benchmark_group("validate_nested");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("nested_record", |b| {
        b.iter(|| {
            let validator = AvroValidator::new();
            let result = validator.validate(black_box(&schema));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark comprehensive type validation
/// Tests schema with all Avro types and features
fn bench_validate_comprehensive(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).expect("Valid schema should parse");

    let mut group = c.benchmark_group("validate_comprehensive");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("comprehensive_types", |b| {
        b.iter(|| {
            let validator = AvroValidator::new();
            let result = validator.validate(black_box(&schema));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark logical types validation
/// Tests all logical type variants (decimal, date, timestamp, etc.)
fn bench_validate_logical_types(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/all_logical_types.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).expect("Valid schema should parse");

    let mut group = c.benchmark_group("validate_logical_types");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("all_logical_types", |b| {
        b.iter(|| {
            let validator = AvroValidator::new();
            let result = validator.validate(black_box(&schema));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark default value validation
/// Tests validation of default values across different types
fn bench_validate_defaults(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/default_values_comprehensive.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).expect("Valid schema should parse");

    let mut group = c.benchmark_group("validate_defaults");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("default_values_comprehensive", |b| {
        b.iter(|| {
            let validator = AvroValidator::new();
            let result = validator.validate(black_box(&schema));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark union validation
/// Tests union type validation with multiple branches
fn bench_validate_unions(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/union_example.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).expect("Valid schema should parse");

    let mut group = c.benchmark_group("validate_unions");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("union_example", |b| {
        b.iter(|| {
            let validator = AvroValidator::new();
            let result = validator.validate(black_box(&schema));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark enum validation
/// Tests enum symbol validation
fn bench_validate_enums(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/enum_example.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).expect("Valid schema should parse");

    let mut group = c.benchmark_group("validate_enums");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("enum_example", |b| {
        b.iter(|| {
            let validator = AvroValidator::new();
            let result = validator.validate(black_box(&schema));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark array and map validation
/// Tests collection type validation
fn bench_validate_collections(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/array_map_example.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).expect("Valid schema should parse");

    let mut group = c.benchmark_group("validate_collections");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("array_map_example", |b| {
        b.iter(|| {
            let validator = AvroValidator::new();
            let result = validator.validate(black_box(&schema));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark error detection on invalid schemas
/// Measures validation performance when errors are present
fn bench_validate_invalid(c: &mut Criterion) {
    let mut group = c.benchmark_group("validate_invalid");

    // Missing required fields
    let missing_fields = include_str!("../tests/fixtures/invalid/missing_fields.avsc");
    let mut parser = AvroParser::new();
    if let Ok(schema) = parser.parse(missing_fields) {
        group.throughput(Throughput::Bytes(missing_fields.len() as u64));
        group.bench_function("missing_fields", |b| {
            b.iter(|| {
                let validator = AvroValidator::new();
                let result = validator.validate(black_box(&schema));
                black_box(result)
            });
        });
    }

    // Duplicate symbols in enum
    let duplicate_symbols = include_str!("../tests/fixtures/invalid/duplicate_symbols.avsc");
    let mut parser = AvroParser::new();
    if let Ok(schema) = parser.parse(duplicate_symbols) {
        group.throughput(Throughput::Bytes(duplicate_symbols.len() as u64));
        group.bench_function("duplicate_symbols", |b| {
            b.iter(|| {
                let validator = AvroValidator::new();
                let result = validator.validate(black_box(&schema));
                black_box(result)
            });
        });
    }

    // Invalid default value
    let invalid_default = include_str!("../tests/fixtures/invalid/invalid_default_enum.avsc");
    let mut parser = AvroParser::new();
    if let Ok(schema) = parser.parse(invalid_default) {
        group.throughput(Throughput::Bytes(invalid_default.len() as u64));
        group.bench_function("invalid_default_enum", |b| {
            b.iter(|| {
                let validator = AvroValidator::new();
                let result = validator.validate(black_box(&schema));
                black_box(result)
            });
        });
    }

    group.finish();
}

/// Comparative benchmark showing validation time vs schema complexity
fn bench_validate_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("validate_comparison");

    // Simple schema
    let simple_input = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let mut parser = AvroParser::new();
    let simple_schema = parser.parse(simple_input).expect("Valid schema");
    group.throughput(Throughput::Bytes(simple_input.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("complexity", "simple"),
        &simple_schema,
        |b, schema| {
            b.iter(|| {
                let validator = AvroValidator::new();
                let result = validator.validate(black_box(schema));
                black_box(result)
            });
        },
    );

    // Nested schema
    let nested_input = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let mut parser = AvroParser::new();
    let nested_schema = parser.parse(nested_input).expect("Valid schema");
    group.throughput(Throughput::Bytes(nested_input.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("complexity", "nested"),
        &nested_schema,
        |b, schema| {
            b.iter(|| {
                let validator = AvroValidator::new();
                let result = validator.validate(black_box(schema));
                black_box(result)
            });
        },
    );

    // Comprehensive schema
    let comprehensive_input = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");
    let mut parser = AvroParser::new();
    let comprehensive_schema = parser.parse(comprehensive_input).expect("Valid schema");
    group.throughput(Throughput::Bytes(comprehensive_input.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("complexity", "comprehensive"),
        &comprehensive_schema,
        |b, schema| {
            b.iter(|| {
                let validator = AvroValidator::new();
                let result = validator.validate(black_box(schema));
                black_box(result)
            });
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_validate_simple,
    bench_validate_nested,
    bench_validate_comprehensive,
    bench_validate_logical_types,
    bench_validate_defaults,
    bench_validate_unions,
    bench_validate_enums,
    bench_validate_collections,
    bench_validate_invalid,
    bench_validate_comparison,
);

criterion_main!(benches);
