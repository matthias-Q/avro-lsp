use async_lsp::lsp_types::Url;
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

use avro_lsp::workspace::Workspace;

/// Benchmark adding a single file to workspace
/// Tests file parsing and type registration
fn bench_add_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("add_file");

    // Simple schema
    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let uri = Url::parse("file:///test/simple_record.avsc").unwrap();
    group.throughput(Throughput::Bytes(simple.len() as u64));
    group.bench_function("simple_record", |b| {
        b.iter(|| {
            let mut workspace = Workspace::new();
            let result =
                workspace.update_file(black_box(uri.clone()), black_box(simple.to_string()));
            black_box(result)
        });
    });

    // Nested schema
    let nested = include_str!("../tests/fixtures/valid/nested_record.avsc");
    let uri = Url::parse("file:///test/nested_record.avsc").unwrap();
    group.throughput(Throughput::Bytes(nested.len() as u64));
    group.bench_function("nested_record", |b| {
        b.iter(|| {
            let mut workspace = Workspace::new();
            let result =
                workspace.update_file(black_box(uri.clone()), black_box(nested.to_string()));
            black_box(result)
        });
    });

    // Comprehensive schema
    let comprehensive = include_str!("../tests/fixtures/valid/comprehensive_types.avsc");
    let uri = Url::parse("file:///test/comprehensive_types.avsc").unwrap();
    group.throughput(Throughput::Bytes(comprehensive.len() as u64));
    group.bench_function("comprehensive_types", |b| {
        b.iter(|| {
            let mut workspace = Workspace::new();
            let result =
                workspace.update_file(black_box(uri.clone()), black_box(comprehensive.to_string()));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark workspace initialization with multiple files
/// Tests indexing performance with varying workspace sizes
fn bench_initialize_workspace(c: &mut Criterion) {
    let mut group = c.benchmark_group("initialize_workspace");

    // Load test schemas
    let common = include_str!("../tests/fixtures/workspace/common_types.avsc");
    let user = include_str!("../tests/fixtures/workspace/user.avsc");
    let order = include_str!("../tests/fixtures/workspace/order.avsc");
    let product = include_str!("../tests/fixtures/workspace/product.avsc");
    let event = include_str!("../tests/fixtures/workspace/event.avsc");

    // Benchmark with 5 files
    let total_bytes = common.len() + user.len() + order.len() + product.len() + event.len();
    group.throughput(Throughput::Bytes(total_bytes as u64));
    group.bench_function("5_files", |b| {
        b.iter(|| {
            let mut workspace = Workspace::new();

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

            black_box(workspace)
        });
    });

    group.finish();
}

/// Benchmark type resolution across files
/// Tests cross-file type lookup performance
fn bench_type_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("type_resolution");

    // Setup workspace with multiple files
    let mut workspace = Workspace::new();
    let common = include_str!("../tests/fixtures/workspace/common_types.avsc");
    let user = include_str!("../tests/fixtures/workspace/user.avsc");

    let common_uri = Url::parse("file:///workspace/common_types.avsc").unwrap();
    let user_uri = Url::parse("file:///workspace/user.avsc").unwrap();

    workspace
        .update_file(common_uri.clone(), common.to_string())
        .unwrap();
    workspace
        .update_file(user_uri.clone(), user.to_string())
        .unwrap();

    group.bench_function("resolve_local_type", |b| {
        b.iter(|| {
            let result = workspace.resolve_type(black_box("User"), black_box(&user_uri));
            black_box(result)
        });
    });

    group.bench_function("resolve_cross_file_type", |b| {
        b.iter(|| {
            let result = workspace.resolve_type(black_box("Address"), black_box(&user_uri));
            black_box(result)
        });
    });

    group.bench_function("resolve_qualified_name", |b| {
        b.iter(|| {
            let result = workspace.resolve_type(
                black_box("com.example.common.Address"),
                black_box(&user_uri),
            );
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark finding all references to a type
/// Tests workspace-wide reference search performance
fn bench_find_references(c: &mut Criterion) {
    let mut group = c.benchmark_group("find_references");

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

    // Benchmark finding references to Address (should find 2 references)
    group.bench_function("find_address_refs", |b| {
        b.iter(|| {
            let result = workspace.find_all_references(black_box("Address"));
            black_box(result)
        });
    });

    // Benchmark finding references to User (no references)
    group.bench_function("find_user_refs", |b| {
        b.iter(|| {
            let result = workspace.find_all_references(black_box("User"));
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark file removal from workspace
/// Tests cleanup and deregistration performance
fn bench_remove_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("remove_file");

    let simple = include_str!("../tests/fixtures/valid/simple_record.avsc");
    let uri = Url::parse("file:///test/simple_record.avsc").unwrap();

    group.bench_function("remove_simple", |b| {
        b.iter_batched(
            || {
                let mut workspace = Workspace::new();
                workspace
                    .update_file(uri.clone(), simple.to_string())
                    .unwrap();
                workspace
            },
            |mut workspace| {
                workspace.remove_file(black_box(&uri));
                black_box(workspace)
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// Benchmark workspace validation
/// Tests validation of all files with cross-file type resolution
fn bench_validate_workspace(c: &mut Criterion) {
    let mut group = c.benchmark_group("validate_workspace");

    // Setup workspace with multiple files
    let mut workspace = Workspace::new();
    let common = include_str!("../tests/fixtures/workspace/common_types.avsc");
    let user = include_str!("../tests/fixtures/workspace/user.avsc");
    let order = include_str!("../tests/fixtures/workspace/order.avsc");
    let product = include_str!("../tests/fixtures/workspace/product.avsc");
    let event = include_str!("../tests/fixtures/workspace/event.avsc");

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

    let total_bytes = common.len() + user.len() + order.len() + product.len() + event.len();
    group.throughput(Throughput::Bytes(total_bytes as u64));

    group.bench_function("validate_5_files", |b| {
        b.iter(|| {
            let result = workspace.validate_all();
            black_box(result)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_add_file,
    bench_initialize_workspace,
    bench_type_resolution,
    bench_find_references,
    bench_remove_file,
    bench_validate_workspace,
);

criterion_main!(benches);
