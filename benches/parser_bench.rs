use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use avro_lsp::schema::json_parser::parse_json;

/// Benchmark parsing small schemas (~10 lines)
/// Uses simple_record.avsc - basic record with 2 fields
fn bench_parse_small_schema(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let input_size = input.len();

    let mut group = c.benchmark_group("parse_small_schema");
    group.throughput(Throughput::Bytes(input_size as u64));

    group.bench_function("simple_record", |b| {
        b.iter(|| {
            let result = parse_json(black_box(input));
            // Ensure the result is used to prevent optimization
            black_box(result).expect("Valid schema should parse")
        });
    });

    group.finish();
}

/// Benchmark parsing medium schemas (~30 lines)
/// Uses nested_record.avsc - nested record structure with 3 fields
fn bench_parse_medium_schema(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let input_size = input.len();

    let mut group = c.benchmark_group("parse_medium_schema");
    group.throughput(Throughput::Bytes(input_size as u64));

    group.bench_function("nested_record", |b| {
        b.iter(|| {
            let result = parse_json(black_box(input));
            black_box(result).expect("Valid schema should parse")
        });
    });

    group.finish();
}

/// Benchmark parsing large schemas (~80 lines)
/// Uses all_logical_types.avsc - complex schema with 10 fields and various logical types
fn bench_parse_large_schema(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/all_logical_types.avsc");
    let input_size = input.len();

    let mut group = c.benchmark_group("parse_large_schema");
    group.throughput(Throughput::Bytes(input_size as u64));

    group.bench_function("all_logical_types", |b| {
        b.iter(|| {
            let result = parse_json(black_box(input));
            black_box(result).expect("Valid schema should parse")
        });
    });

    group.finish();
}

/// Comparative benchmark showing relative performance across schema sizes
fn bench_parse_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_schema_comparison");

    // Small schema (~10 lines)
    let small = include_str!("../tests/fixtures/valid/simple_record.avsc");
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("schema_size", "small_10_lines"),
        &small,
        |b, input| {
            b.iter(|| {
                let result = parse_json(black_box(input));
                black_box(result).expect("Valid schema should parse")
            });
        },
    );

    // Medium schema (~30 lines)
    let medium = include_str!("../tests/fixtures/valid/nested_record.avsc");
    group.throughput(Throughput::Bytes(medium.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("schema_size", "medium_30_lines"),
        &medium,
        |b, input| {
            b.iter(|| {
                let result = parse_json(black_box(input));
                black_box(result).expect("Valid schema should parse")
            });
        },
    );

    // Large schema (~80 lines)
    let large = include_str!("../tests/fixtures/valid/all_logical_types.avsc");
    group.throughput(Throughput::Bytes(large.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("schema_size", "large_80_lines"),
        &large,
        |b, input| {
            b.iter(|| {
                let result = parse_json(black_box(input));
                black_box(result).expect("Valid schema should parse")
            });
        },
    );

    group.finish();
}

/// Benchmark parsing various other valid schemas to get broader performance picture
fn bench_parse_various_schemas(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_various_schemas");

    // Union example
    let union = include_str!("../tests/fixtures/valid/union_example.avsc");
    group.throughput(Throughput::Bytes(union.len() as u64));
    group.bench_function("union_example", |b| {
        b.iter(|| {
            let result = parse_json(black_box(union));
            black_box(result).expect("Valid schema should parse")
        });
    });

    // Fixed type example
    let fixed = include_str!("../tests/fixtures/valid/fixed_example.avsc");
    group.throughput(Throughput::Bytes(fixed.len() as u64));
    group.bench_function("fixed_example", |b| {
        b.iter(|| {
            let result = parse_json(black_box(fixed));
            black_box(result).expect("Valid schema should parse")
        });
    });

    // Array and map example
    let array_map = include_str!("../tests/fixtures/valid/array_map_example.avsc");
    group.throughput(Throughput::Bytes(array_map.len() as u64));
    group.bench_function("array_map_example", |b| {
        b.iter(|| {
            let result = parse_json(black_box(array_map));
            black_box(result).expect("Valid schema should parse")
        });
    });

    group.finish();
}

/// Benchmark error detection on invalid schemas
/// This measures how quickly the parser can detect and report errors
fn bench_parse_invalid_schemas(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_invalid_schemas");

    // Missing fields
    let missing_fields = include_str!("../tests/fixtures/invalid/missing_fields.avsc");
    group.throughput(Throughput::Bytes(missing_fields.len() as u64));
    group.bench_function("missing_fields_detection", |b| {
        b.iter(|| {
            let result = parse_json(black_box(missing_fields));
            // Should return error
            black_box(result)
        });
    });

    // Duplicate symbols
    let duplicate_symbols = include_str!("../tests/fixtures/invalid/duplicate_symbols.avsc");
    group.throughput(Throughput::Bytes(duplicate_symbols.len() as u64));
    group.bench_function("duplicate_symbols_detection", |b| {
        b.iter(|| {
            let result = parse_json(black_box(duplicate_symbols));
            black_box(result)
        });
    });

    group.finish();
}

// Configure criterion to run all benchmark groups
criterion_group!(
    benches,
    bench_parse_small_schema,
    bench_parse_medium_schema,
    bench_parse_large_schema,
    bench_parse_comparison,
    bench_parse_various_schemas,
    bench_parse_invalid_schemas,
);

criterion_main!(benches);
