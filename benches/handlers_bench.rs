use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use async_lsp::lsp_types::Position;
use avro_lsp::handlers::{
    completion, folding_ranges, formatting, hover, inlay_hints, semantic_tokens, symbols,
};
use avro_lsp::schema::AvroParser;

/// Benchmark hover information generation
/// Tests type information lookup at various positions
fn bench_hover(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).expect("Valid schema should parse");

    let mut group = c.benchmark_group("hover");
    group.throughput(Throughput::Bytes(input.len() as u64));

    // Hover on type name
    group.bench_function("type_name", |b| {
        b.iter(|| {
            let word = black_box("Address");
            let result = hover::generate_hover(black_box(&schema), black_box(input), word);
            black_box(result)
        });
    });

    // Hover on primitive type
    group.bench_function("primitive_type", |b| {
        b.iter(|| {
            let word = black_box("string");
            let result = hover::generate_hover(black_box(&schema), black_box(input), word);
            black_box(result)
        });
    });

    // Hover on field name
    group.bench_function("field_name", |b| {
        b.iter(|| {
            let word = black_box("name");
            let result = hover::generate_hover(black_box(&schema), black_box(input), word);
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark completion generation
/// Tests suggestion generation at various cursor positions
fn bench_completion(c: &mut Criterion) {
    let input = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).ok();

    let mut group = c.benchmark_group("completion");
    group.throughput(Throughput::Bytes(input.len() as u64));

    // Completion after opening brace (key suggestions)
    group.bench_function("json_key", |b| {
        b.iter(|| {
            let pos = Position::new(1, 2); // After "{"
            let result = completion::get_completions(
                black_box(input),
                black_box(pos),
                black_box(schema.as_ref()),
            );
            black_box(result)
        });
    });

    // Completion after "type": (value suggestions)
    group.bench_function("type_value", |b| {
        b.iter(|| {
            let pos = Position::new(2, 12); // After "type":
            let result = completion::get_completions(
                black_box(input),
                black_box(pos),
                black_box(schema.as_ref()),
            );
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark document symbol generation
/// Tests outline/symbol tree creation
fn bench_symbols(c: &mut Criterion) {
    let mut group = c.benchmark_group("symbols");

    // Simple schema
    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let mut parser = AvroParser::new();
    let simple_schema = parser.parse(simple).expect("Valid schema");
    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_function("simple_record", |b| {
        b.iter(|| {
            let result =
                symbols::create_document_symbols(black_box(&simple_schema), black_box(simple));
            black_box(result)
        });
    });

    // Nested schema
    let nested = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let mut parser = AvroParser::new();
    let nested_schema = parser.parse(nested).expect("Valid schema");
    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_function("nested_record", |b| {
        b.iter(|| {
            let result =
                symbols::create_document_symbols(black_box(&nested_schema), black_box(nested));
            black_box(result)
        });
    });

    // Comprehensive schema
    let comprehensive = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");
    let mut parser = AvroParser::new();
    let comprehensive_schema = parser.parse(comprehensive).expect("Valid schema");
    group.throughput(Throughput::Bytes(comprehensive.len() as u64));
    group.bench_function("comprehensive_types", |b| {
        b.iter(|| {
            let result = symbols::create_document_symbols(
                black_box(&comprehensive_schema),
                black_box(comprehensive),
            );
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark semantic token generation
/// Tests full document semantic highlighting
fn bench_semantic_tokens(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_tokens");

    // Simple schema
    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let mut parser = AvroParser::new();
    let simple_schema = parser.parse(simple).expect("Valid schema");
    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_function("simple_record", |b| {
        b.iter(|| {
            let result = semantic_tokens::build_semantic_tokens(black_box(&simple_schema));
            black_box(result)
        });
    });

    // Nested schema
    let nested = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let mut parser = AvroParser::new();
    let nested_schema = parser.parse(nested).expect("Valid schema");
    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_function("nested_record", |b| {
        b.iter(|| {
            let result = semantic_tokens::build_semantic_tokens(black_box(&nested_schema));
            black_box(result)
        });
    });

    // Comprehensive schema
    let comprehensive = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");
    let mut parser = AvroParser::new();
    let comprehensive_schema = parser.parse(comprehensive).expect("Valid schema");
    group.throughput(Throughput::Bytes(comprehensive.len() as u64));
    group.bench_function("comprehensive_types", |b| {
        b.iter(|| {
            let result = semantic_tokens::build_semantic_tokens(black_box(&comprehensive_schema));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark inlay hints generation
/// Tests inline type hint creation
fn bench_inlay_hints(c: &mut Criterion) {
    let mut group = c.benchmark_group("inlay_hints");

    // Simple schema
    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let mut parser = AvroParser::new();
    let simple_schema = parser.parse(simple).expect("Valid schema");
    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_function("simple_record", |b| {
        b.iter(|| {
            let result =
                inlay_hints::generate_inlay_hints(black_box(&simple_schema), black_box(simple));
            black_box(result)
        });
    });

    // Nested schema with more fields
    let nested = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let mut parser = AvroParser::new();
    let nested_schema = parser.parse(nested).expect("Valid schema");
    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_function("nested_record", |b| {
        b.iter(|| {
            let result =
                inlay_hints::generate_inlay_hints(black_box(&nested_schema), black_box(nested));
            black_box(result)
        });
    });

    // Schema with unions and complex types
    let union = include_str!("../tests/fixtures/valid/union_example.avsc");
    let mut parser = AvroParser::new();
    let union_schema = parser.parse(union).expect("Valid schema");
    group.throughput(Throughput::Bytes(union.len() as u64));
    group.bench_function("union_example", |b| {
        b.iter(|| {
            let result =
                inlay_hints::generate_inlay_hints(black_box(&union_schema), black_box(union));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark folding ranges generation
/// Tests code folding region calculation
fn bench_folding_ranges(c: &mut Criterion) {
    let mut group = c.benchmark_group("folding_ranges");

    // Simple schema
    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let mut parser = AvroParser::new();
    let simple_schema = parser.parse(simple).expect("Valid schema");
    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_function("simple_record", |b| {
        b.iter(|| {
            let result =
                folding_ranges::get_folding_ranges(black_box(&simple_schema), black_box(simple));
            black_box(result)
        });
    });

    // Nested schema with more foldable regions
    let nested = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let mut parser = AvroParser::new();
    let nested_schema = parser.parse(nested).expect("Valid schema");
    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_function("nested_record", |b| {
        b.iter(|| {
            let result =
                folding_ranges::get_folding_ranges(black_box(&nested_schema), black_box(nested));
            black_box(result)
        });
    });

    // Comprehensive schema with many regions
    let comprehensive = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");
    let mut parser = AvroParser::new();
    let comprehensive_schema = parser.parse(comprehensive).expect("Valid schema");
    group.throughput(Throughput::Bytes(comprehensive.len() as u64));
    group.bench_function("comprehensive_types", |b| {
        b.iter(|| {
            let result = folding_ranges::get_folding_ranges(
                black_box(&comprehensive_schema),
                black_box(comprehensive),
            );
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark document formatting
/// Tests JSON pretty-printing performance
fn bench_formatting(c: &mut Criterion) {
    let mut group = c.benchmark_group("formatting");

    // Simple schema
    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");
    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_function("simple_record", |b| {
        b.iter(|| {
            let result = formatting::format_document(black_box(simple));
            black_box(result)
        });
    });

    // Nested schema
    let nested = include_str!("../tests/fixtures/valid/nested_record.avsc");
    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_function("nested_record", |b| {
        b.iter(|| {
            let result = formatting::format_document(black_box(nested));
            black_box(result)
        });
    });

    // Large schema
    let large = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");
    group.throughput(Throughput::Bytes(large.len() as u64));
    group.bench_function("comprehensive_types", |b| {
        b.iter(|| {
            let result = formatting::format_document(black_box(large));
            black_box(result)
        });
    });

    group.finish();
}

/// Comparative benchmark showing handler performance across schema sizes
fn bench_handler_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("handler_comparison");

    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let mut parser = AvroParser::new();
    let simple_schema = parser.parse(simple).expect("Valid schema");

    let nested = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let mut parser = AvroParser::new();
    let nested_schema = parser.parse(nested).expect("Valid schema");

    let comprehensive = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");
    let mut parser = AvroParser::new();
    let comprehensive_schema = parser.parse(comprehensive).expect("Valid schema");

    // Semantic tokens comparison
    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("semantic_tokens", "simple"),
        &simple_schema,
        |b, schema| {
            b.iter(|| {
                let result = semantic_tokens::build_semantic_tokens(black_box(schema));
                black_box(result)
            });
        },
    );

    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("semantic_tokens", "nested"),
        &nested_schema,
        |b, schema| {
            b.iter(|| {
                let result = semantic_tokens::build_semantic_tokens(black_box(schema));
                black_box(result)
            });
        },
    );

    group.throughput(Throughput::Bytes(comprehensive.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("semantic_tokens", "comprehensive"),
        &comprehensive_schema,
        |b, schema| {
            b.iter(|| {
                let result = semantic_tokens::build_semantic_tokens(black_box(schema));
                black_box(result)
            });
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_hover,
    bench_completion,
    bench_symbols,
    bench_semantic_tokens,
    bench_inlay_hints,
    bench_folding_ranges,
    bench_formatting,
    bench_handler_comparison,
);

criterion_main!(benches);
