//! A test that looks up a single SRV record.

mod sandbox;

use sandbox::Sandbox;
use sandbox::components::dns::{MockDns, MockSrv};
use srv_rs::SrvRecord;
use srv_rs::resolver::{SrvResolver, libresolv::LibResolv};

#[test]
fn simple_lookup_srv_single() {
    Sandbox::new()
        .component(MockDns::new(&[MockSrv::new(
            "_http._tcp.test.local.",
            10,
            100,
            8080,
            "server1.test.local.",
            300,
        )]))
        .run_with_tokio(|| async {
            test_simple_lookup_srv_single(LibResolv).await;
            test_simple_lookup_srv_single(
                hickory_resolver::Resolver::builder_tokio().unwrap().build(),
            )
            .await;
        });
}

async fn test_simple_lookup_srv_single(resolver: impl SrvResolver) {
    let (records, _valid_until) = resolver
        .get_srv_records_unordered("_http._tcp.test.local.")
        .await
        .expect("SRV lookup failed");
    assert_eq!(records.len(), 1, "expected 1 SRV record");
    let record = records.first().unwrap();
    assert_eq!(record.priority(), 10);
    assert_eq!(record.weight(), 100);
    assert_eq!(record.port(), 8080);
    assert_eq!(
        record.target().to_string().trim_end_matches('.'),
        "server1.test.local"
    );
}
