use criterion::{criterion_group, criterion_main, Criterion};
use srv_rs::resolver::{libresolv::LibResolv, SrvResolver};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function(
        "resolve _http._tcp.srv-client-rust.deshaw.org (libresolv)",
        |b| {
            b.iter(|| {
                runtime
                    .block_on(
                        LibResolv::default()
                            .get_srv_records_unordered("_http._tcp.srv-client-rust.deshaw.org"),
                    )
                    .unwrap()
            })
        },
    );

    c.bench_function("resolve _imaps._tcp.gmail.com (libresolv)", |b| {
        b.iter(|| {
            runtime
                .block_on(LibResolv::default().get_srv_records_unordered("_imaps._tcp.gmail.com"))
                .unwrap()
        })
    });

    let records = runtime
        .block_on(
            LibResolv::default().get_srv_records_unordered("_http._tcp.srv-client-rust.deshaw.org"),
        )
        .unwrap();
    let mut rng = rand::thread_rng();
    c.bench_function("order _http._tcp.srv-client-rust.deshaw.org records", |b| {
        b.iter(|| LibResolv::order_srv_records(&mut records.clone(), &mut rng))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
