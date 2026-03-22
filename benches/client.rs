use criterion::{criterion_group, criterion_main, Criterion};
use rand::Rng;
use srv_rs::{policy::Rfc2782, resolver::libresolv::LibResolv, Execution, SrvClient};

const SRV_NAME: &str = srv_rs::EXAMPLE_SRV;
const SRV_DESCRIPTION: &str = SRV_NAME;

/// Benchmark the performance of the client.
#[allow(clippy::missing_panics_doc)]
pub fn criterion_benchmark(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let client = SrvClient::<LibResolv>::new(SRV_NAME);
    let rfc2782_client = SrvClient::<LibResolv>::new(SRV_NAME).policy(Rfc2782);

    let succeed = || Ok::<_, std::convert::Infallible>(());
    let fail = || "".parse::<usize>();
    let random = || {
        if rand::rng().random_bool(0.5) {
            fail()
        } else {
            Ok(0)
        }
    };

    let mut group = c.benchmark_group(format!("execute ({SRV_DESCRIPTION}, first succeeds)"));
    group.bench_function("Policy::Affinity", |b| {
        b.iter(|| runtime.block_on(client.execute(Execution::Serial, |_| async { succeed() })));
    });
    group.bench_function("Policy::Rfc2782", |b| {
        b.iter(|| {
            runtime.block_on(rfc2782_client.execute(Execution::Serial, |_| async { succeed() }))
        });
    });
    drop(group);

    let mut group = c.benchmark_group(format!("execute ({SRV_DESCRIPTION}, all fail)"));
    group.bench_function("Policy::Affinity", |b| {
        b.iter(|| runtime.block_on(client.execute(Execution::Serial, |_| async { fail() })));
    });
    group.bench_function("Policy::Rfc2782", |b| {
        b.iter(|| {
            runtime.block_on(rfc2782_client.execute(Execution::Serial, |_| async { fail() }))
        });
    });
    drop(group);

    let mut group = c.benchmark_group(format!("execute ({SRV_DESCRIPTION}, half fail)"));
    group.bench_function("Policy::Affinity", |b| {
        b.iter(|| runtime.block_on(client.execute(Execution::Serial, |_| async { random() })));
    });
    group.bench_function("Policy::Rfc2782", |b| {
        b.iter(|| {
            runtime.block_on(rfc2782_client.execute(Execution::Serial, |_| async { random() }))
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
