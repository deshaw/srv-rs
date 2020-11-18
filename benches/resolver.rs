use criterion::{criterion_group, criterion_main, Criterion};
use srv_rs::resolver::{libresolv::LibResolv, SrvResolver};
use trust_dns_resolver::{AsyncResolver, TokioAsyncResolver};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    let libresolv = LibResolv::default();
    // Disable trust-dns caching so benches are fair
    let (conf, mut opts) = trust_dns_resolver::system_conf::read_system_conf().unwrap();
    opts.cache_size = 0;
    let trust_dns = runtime.block_on(AsyncResolver::tokio(conf, opts)).unwrap();

    let mut group = c.benchmark_group(format!("resolve {}", srv_rs::EXAMPLE_SRV));
    group.bench_function("libresolv", |b| {
        b.iter(|| {
            runtime
                .block_on(libresolv.get_srv_records_unordered(srv_rs::EXAMPLE_SRV))
                .unwrap()
        })
    });
    group.bench_function("trust-dns", |b| {
        b.iter(|| {
            runtime
                .block_on(trust_dns.get_srv_records_unordered(srv_rs::EXAMPLE_SRV))
                .unwrap()
        })
    });
    drop(group);

    let gmail = "_imaps._tcp.gmail.com";
    let mut group = c.benchmark_group(format!("resolve {}", gmail));
    group.bench_function("libresolv", |b| {
        b.iter(|| {
            runtime
                .block_on(libresolv.get_srv_records_unordered(gmail))
                .unwrap()
        })
    });
    group.bench_function("trust-dns", |b| {
        b.iter(|| {
            runtime
                .block_on(trust_dns.get_srv_records_unordered(gmail))
                .unwrap()
        })
    });
    drop(group);

    let mut group = c.benchmark_group(format!("order {} records", srv_rs::EXAMPLE_SRV));
    let mut rng = rand::thread_rng();
    let (records, _) = runtime
        .block_on(libresolv.get_srv_records_unordered(srv_rs::EXAMPLE_SRV))
        .unwrap();
    group.bench_function("libresolv", |b| {
        b.iter(|| LibResolv::order_srv_records(&mut records.clone(), &mut rng))
    });
    let (records, _) = runtime
        .block_on(trust_dns.get_srv_records_unordered(srv_rs::EXAMPLE_SRV))
        .unwrap();
    group.bench_function("trust-dns", |b| {
        b.iter(|| TokioAsyncResolver::order_srv_records(&mut records.clone(), &mut rng))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
