//! A test that looks up multiple SRV records.

mod sandbox;

use sandbox::components::dns::{MockDns, MockSrv};
use sandbox::Sandbox;
use srv_rs::resolver::{libresolv::LibResolv, SrvResolver};
use srv_rs::SrvRecord;

#[test]
fn simple_lookup_srv_multiple() {
    Sandbox::new()
        .component(MockDns::new(&[
            MockSrv::new(
                "_http._tcp.multi.local.",
                10,
                100,
                8080,
                "primary.multi.local.",
                300,
            ),
            MockSrv::new(
                "_http._tcp.multi.local.",
                20,
                50,
                8081,
                "secondary.multi.local.",
                300,
            ),
            MockSrv::new(
                "_http._tcp.multi.local.",
                10,
                25,
                8082,
                "backup.multi.local.",
                300,
            ),
        ]))
        .run_with_tokio(|| test_simple_lookup_srv_multiple(LibResolv));
}

async fn test_simple_lookup_srv_multiple(resolver: impl SrvResolver) {
    let (records, _valid_until) = resolver
        .get_srv_records("_http._tcp.multi.local.")
        .await
        .expect("SRV lookup failed");
    assert_eq!(records.len(), 3, "expected 3 SRV records");
    assert!(
        records.iter().map(SrvRecord::priority).is_sorted(),
        "records should be sorted by priority",
    );
}
