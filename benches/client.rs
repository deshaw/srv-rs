use criterion::{criterion_group, criterion_main, Criterion};
use rand::Rng;
use srv_rs::{
    client::{policy::Rfc2782, Execution, SrvClient},
    resolver::libresolv::LibResolv,
};

const SRV_NAME: &str = srv_rs::EXAMPLE_SRV;
const SRV_DESCRIPTION: &str = SRV_NAME;

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    let client = SrvClient::<LibResolv>::new(SRV_NAME);
    let rfc2782_client = SrvClient::<LibResolv>::new(SRV_NAME).policy(Rfc2782);

    let succeed = || Ok::<_, std::convert::Infallible>(());
    let fail = || "".parse::<usize>();
    let random = || {
        if rand::thread_rng().gen_bool(0.5) {
            fail()
        } else {
            Ok(0)
        }
    };

    let mut group = c.benchmark_group(format!("execute ({}, first succeeds)", SRV_DESCRIPTION));
    group.bench_function("Policy::Affinity", |b| {
        b.iter(|| runtime.block_on(client.execute_one(Execution::Serial, |_| async { succeed() })))
    });
    group.bench_function("Policy::Rfc2782", |b| {
        b.iter(|| {
            runtime.block_on(rfc2782_client.execute_one(Execution::Serial, |_| async { succeed() }))
        })
    });
    drop(group);

    let mut group = c.benchmark_group(format!("execute ({}, all fail)", SRV_DESCRIPTION));
    group.bench_function("Policy::Affinity", |b| {
        b.iter(|| runtime.block_on(client.execute_one(Execution::Serial, |_| async { fail() })))
    });
    group.bench_function("Policy::Rfc2782", |b| {
        b.iter(|| {
            runtime.block_on(rfc2782_client.execute_one(Execution::Serial, |_| async { fail() }))
        })
    });
    drop(group);

    let mut group = c.benchmark_group(format!("execute ({}, half fail)", SRV_DESCRIPTION));
    group.bench_function("Policy::Affinity", |b| {
        b.iter(|| runtime.block_on(client.execute_one(Execution::Serial, |_| async { random() })))
    });
    group.bench_function("Policy::Rfc2782", |b| {
        b.iter(|| {
            runtime.block_on(rfc2782_client.execute_one(Execution::Serial, |_| async { random() }))
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
