use criterion::{criterion_group, criterion_main, Criterion};
use hickory_resolver::{Resolver, TokioResolver};
use srv_rs::resolver::{libresolv::LibResolv, SrvResolver};

/// Benchmark the performance of the resolver.
#[allow(clippy::missing_panics_doc)]
pub fn criterion_benchmark(c: &mut Criterion) {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    let libresolv = LibResolv;
    // Disable hickory caching so benches are fair
    let mut hickory_builder = Resolver::builder_tokio().unwrap();
    hickory_builder.options_mut().cache_size = 0;
    let hickory = hickory_builder.build();

    let mut group = c.benchmark_group(format!("resolve {}", srv_rs::EXAMPLE_SRV));
    group.bench_function("libresolv", |b| {
        b.iter(|| {
            runtime
                .block_on(libresolv.get_srv_records_unordered(srv_rs::EXAMPLE_SRV))
                .unwrap()
        });
    });
    group.bench_function("hickory", |b| {
        b.iter(|| {
            runtime
                .block_on(hickory.get_srv_records_unordered(srv_rs::EXAMPLE_SRV))
                .unwrap()
        });
    });
    drop(group);

    let gmail = "_imaps._tcp.gmail.com";
    let mut group = c.benchmark_group(format!("resolve {gmail}"));
    group.bench_function("libresolv", |b| {
        b.iter(|| {
            runtime
                .block_on(libresolv.get_srv_records_unordered(gmail))
                .unwrap()
        });
    });
    group.bench_function("hickory", |b| {
        b.iter(|| {
            runtime
                .block_on(hickory.get_srv_records_unordered(gmail))
                .unwrap()
        });
    });
    drop(group);

    let mut group = c.benchmark_group(format!("order {} records", srv_rs::EXAMPLE_SRV));
    let mut rng = rand::rng();
    let (records, _) = runtime
        .block_on(libresolv.get_srv_records_unordered(srv_rs::EXAMPLE_SRV))
        .unwrap();
    group.bench_function("libresolv", |b| {
        b.iter(|| LibResolv::order_srv_records(&mut records.clone(), &mut rng));
    });
    let (records, _) = runtime
        .block_on(hickory.get_srv_records_unordered(srv_rs::EXAMPLE_SRV))
        .unwrap();
    group.bench_function("hickory", |b| {
        b.iter(|| TokioResolver::order_srv_records(&mut records.clone(), &mut rng));
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
