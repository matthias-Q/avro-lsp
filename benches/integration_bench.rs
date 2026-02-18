use async_lsp::lsp_types::{Position, Url};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

use avro_lsp::handlers::{completion, formatting, hover, semantic_tokens, symbols};
use avro_lsp::schema::{AvroParser, AvroValidator};
use avro_lsp::workspace::Workspace;

/// Benchmark file open workflow
/// Simulates what happens when a user opens a file in the editor:
/// Parse → Validate → Generate semantic tokens → Generate symbols
fn bench_file_open_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_open_workflow");

    // Simple schema
    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");
    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_function("simple_record", |b| {
        b.iter(|| {
            // Parse
            let mut parser = AvroParser::new();
            let schema = parser.parse(black_box(simple)).expect("Valid schema");

            // Validate
            let validator = AvroValidator::new();
            let _diagnostics = validator.validate(black_box(&schema));

            // Generate semantic tokens
            let _tokens = semantic_tokens::build_semantic_tokens(black_box(&schema));

            // Generate symbols
            let _symbols = symbols::create_document_symbols(black_box(&schema), black_box(simple));

            black_box(())
        });
    });

    // Nested schema
    let nested = include_str!("../tests/fixtures/valid/nested_record.avsc");
    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_function("nested_record", |b| {
        b.iter(|| {
            let mut parser = AvroParser::new();
            let schema = parser.parse(black_box(nested)).expect("Valid schema");
            let validator = AvroValidator::new();
            let _diagnostics = validator.validate(black_box(&schema));
            let _tokens = semantic_tokens::build_semantic_tokens(black_box(&schema));
            let _symbols = symbols::create_document_symbols(black_box(&schema), black_box(nested));
            black_box(())
        });
    });

    // Comprehensive schema
    let comprehensive = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");
    group.throughput(Throughput::Bytes(comprehensive.len() as u64));
    group.bench_function("comprehensive_types", |b| {
        b.iter(|| {
            let mut parser = AvroParser::new();
            let schema = parser
                .parse(black_box(comprehensive))
                .expect("Valid schema");
            let validator = AvroValidator::new();
            let _diagnostics = validator.validate(black_box(&schema));
            let _tokens = semantic_tokens::build_semantic_tokens(black_box(&schema));
            let _symbols =
                symbols::create_document_symbols(black_box(&schema), black_box(comprehensive));
            black_box(())
        });
    });

    group.finish();
}

/// Benchmark incremental editing workflow
/// Simulates what happens on each keystroke:
/// Reparse → Revalidate → Update diagnostics
fn bench_incremental_edit_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_edit_workflow");

    let input = include_str!("../tests/fixtures/valid/simple_record.avsc");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("simple_edit", |b| {
        b.iter(|| {
            // Simulate change to the file (in reality would be incremental)
            let mut parser = AvroParser::new();
            let schema = parser.parse(black_box(input)).expect("Valid schema");

            // Revalidate
            let validator = AvroValidator::new();
            let _diagnostics = validator.validate(black_box(&schema));

            black_box(())
        });
    });

    group.finish();
}

/// Benchmark code completion workflow
/// Simulates typing and getting completions:
/// Parse → Get completion context → Generate suggestions
fn bench_completion_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("completion_workflow");

    let input = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).ok();

    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("trigger_completion", |b| {
        b.iter(|| {
            // User types and triggers completion
            let pos = Position::new(2, 12); // After "type":
            let _completions = completion::get_completions(
                black_box(input),
                black_box(pos),
                black_box(schema.as_ref()),
            );
            black_box(())
        });
    });

    group.finish();
}

/// Benchmark hover workflow
/// Simulates hovering over a symbol:
/// Find word at position → Lookup type info → Format hover content
fn bench_hover_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("hover_workflow");

    let input = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let mut parser = AvroParser::new();
    let schema = parser.parse(input).expect("Valid schema");

    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("hover_on_type", |b| {
        b.iter(|| {
            // Find word at cursor position
            let pos = Position::new(5, 20);
            let word = hover::get_word_at_position(black_box(input), black_box(pos));

            // Generate hover if word found
            if let Some(w) = word {
                let _hover = hover::generate_hover(black_box(&schema), black_box(input), &w);
            }

            black_box(())
        });
    });

    group.finish();
}

/// Benchmark formatting workflow
/// Simulates format document command:
/// Parse → Format → Replace text
fn bench_format_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("format_workflow");

    let input = include_str!("../tests/fixtures/valid/nested_record.avsc");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("format_document", |b| {
        b.iter(|| {
            // Format the document
            let _formatted = formatting::format_document(black_box(input));
            black_box(())
        });
    });

    group.finish();
}

/// Benchmark workspace project initialization
/// Simulates opening a project with multiple schema files:
/// Scan directory → Parse all files → Build type index → Validate all
fn bench_project_initialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("project_initialization");

    let common = include_str!("../tests/fixtures/workspace/common_types.avsc");
    let user = include_str!("../tests/fixtures/workspace/user.avsc");
    let order = include_str!("../tests/fixtures/workspace/order.avsc");
    let product = include_str!("../tests/fixtures/workspace/product.avsc");
    let event = include_str!("../tests/fixtures/workspace/event.avsc");

    let total_bytes = common.len() + user.len() + order.len() + product.len() + event.len();
    group.throughput(Throughput::Bytes(total_bytes as u64));

    group.bench_function("init_5_file_project", |b| {
        b.iter(|| {
            let mut workspace = Workspace::new();

            // Load all files (simulates scanning directory)
            workspace
                .update_file(
                    Url::parse("file:///workspace/common_types.avsc").unwrap(),
                    common.to_string(),
                )
                .unwrap();
            workspace
                .update_file(
                    Url::parse("file:///workspace/user.avsc").unwrap(),
                    user.to_string(),
                )
                .unwrap();
            workspace
                .update_file(
                    Url::parse("file:///workspace/order.avsc").unwrap(),
                    order.to_string(),
                )
                .unwrap();
            workspace
                .update_file(
                    Url::parse("file:///workspace/product.avsc").unwrap(),
                    product.to_string(),
                )
                .unwrap();
            workspace
                .update_file(
                    Url::parse("file:///workspace/event.avsc").unwrap(),
                    event.to_string(),
                )
                .unwrap();

            // Validate all files
            let _diagnostics = workspace.validate_all();

            black_box(workspace)
        });
    });

    group.finish();
}

/// Benchmark cross-file navigation workflow
/// Simulates go-to-definition across files:
/// Find type reference → Resolve in workspace → Return location
fn bench_cross_file_navigation(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_file_navigation");

    // Setup workspace
    let mut workspace = Workspace::new();
    let common = include_str!("../tests/fixtures/workspace/common_types.avsc");
    let user = include_str!("../tests/fixtures/workspace/user.avsc");

    workspace
        .update_file(
            Url::parse("file:///workspace/common_types.avsc").unwrap(),
            common.to_string(),
        )
        .unwrap();
    workspace
        .update_file(
            Url::parse("file:///workspace/user.avsc").unwrap(),
            user.to_string(),
        )
        .unwrap();

    let user_uri = Url::parse("file:///workspace/user.avsc").unwrap();

    group.bench_function("goto_definition", |b| {
        b.iter(|| {
            // User clicks on "Address" type reference in user.avsc
            let type_info = workspace.resolve_type(black_box("Address"), black_box(&user_uri));

            // Get definition location
            if let Some(info) = type_info {
                let _location = info.defined_in.clone();
            }

            black_box(())
        });
    });

    group.finish();
}

/// Benchmark find references workflow
/// Simulates find-all-references command:
/// Search all files → Collect matches → Return locations
fn bench_find_references_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("find_references_workflow");

    // Setup workspace with multiple files that reference Address
    let mut workspace = Workspace::new();
    let common = include_str!("../tests/fixtures/workspace/common_types.avsc");
    let user = include_str!("../tests/fixtures/workspace/user.avsc");
    let order = include_str!("../tests/fixtures/workspace/order.avsc");

    workspace
        .update_file(
            Url::parse("file:///workspace/common_types.avsc").unwrap(),
            common.to_string(),
        )
        .unwrap();
    workspace
        .update_file(
            Url::parse("file:///workspace/user.avsc").unwrap(),
            user.to_string(),
        )
        .unwrap();
    workspace
        .update_file(
            Url::parse("file:///workspace/order.avsc").unwrap(),
            order.to_string(),
        )
        .unwrap();

    group.bench_function("find_all_refs", |b| {
        b.iter(|| {
            // User invokes find-all-references on "Address"
            let _locations = workspace.find_all_references(black_box("Address"));
            black_box(())
        });
    });

    group.finish();
}

/// Comparative benchmark showing different workflow complexities
fn bench_workflow_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow_comparison");

    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let nested = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let comprehensive = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");

    // File open workflow comparison
    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("file_open", "simple"),
        &simple,
        |b, input| {
            b.iter(|| {
                let mut parser = AvroParser::new();
                let schema = parser.parse(black_box(input)).expect("Valid schema");
                let validator = AvroValidator::new();
                let _diagnostics = validator.validate(black_box(&schema));
                let _tokens = semantic_tokens::build_semantic_tokens(black_box(&schema));
                let _symbols =
                    symbols::create_document_symbols(black_box(&schema), black_box(input));
                black_box(())
            });
        },
    );

    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("file_open", "nested"),
        &nested,
        |b, input| {
            b.iter(|| {
                let mut parser = AvroParser::new();
                let schema = parser.parse(black_box(input)).expect("Valid schema");
                let validator = AvroValidator::new();
                let _diagnostics = validator.validate(black_box(&schema));
                let _tokens = semantic_tokens::build_semantic_tokens(black_box(&schema));
                let _symbols =
                    symbols::create_document_symbols(black_box(&schema), black_box(input));
                black_box(())
            });
        },
    );

    group.throughput(Throughput::Bytes(comprehensive.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("file_open", "comprehensive"),
        &comprehensive,
        |b, input| {
            b.iter(|| {
                let mut parser = AvroParser::new();
                let schema = parser.parse(black_box(input)).expect("Valid schema");
                let validator = AvroValidator::new();
                let _diagnostics = validator.validate(black_box(&schema));
                let _tokens = semantic_tokens::build_semantic_tokens(black_box(&schema));
                let _symbols =
                    symbols::create_document_symbols(black_box(&schema), black_box(input));
                black_box(())
            });
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_file_open_workflow,
    bench_incremental_edit_workflow,
    bench_completion_workflow,
    bench_hover_workflow,
    bench_format_workflow,
    bench_project_initialization,
    bench_cross_file_navigation,
    bench_find_references_workflow,
    bench_workflow_comparison,
);

criterion_main!(benches);
