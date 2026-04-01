//! Tests that resolvers return errors for nonexistent, malformed, and otherwise invalid names.

mod sandbox;

use sandbox::Sandbox;
use sandbox::components::dns::MockDns;
use srv_rs::resolver::{SrvResolver, libresolv::LibResolv};

#[test]
fn lookup_nonexistent_host() {
    Sandbox::new()
        .component(MockDns::new(&[]))
        .run_with_tokio(|| async {
            test_lookup_nonexistent_host(LibResolv).await;
            test_lookup_nonexistent_host(
                hickory_resolver::Resolver::builder_tokio().unwrap().build(),
            )
            .await;
        });
}

async fn test_lookup_nonexistent_host(resolver: impl SrvResolver) {
    let result = resolver
        .get_srv_records("_http._tcp.foobar.test.local.")
        .await;
    assert!(result.is_err(), "expected error for nonexistent host");
}

#[test]
fn lookup_malformed_srv_name() {
    Sandbox::new()
        .component(MockDns::new(&[]))
        .run_with_tokio(|| async {
            test_lookup_malformed_srv_name(LibResolv).await;
            test_lookup_malformed_srv_name(
                hickory_resolver::Resolver::builder_tokio().unwrap().build(),
            )
            .await;
        });
}

async fn test_lookup_malformed_srv_name(resolver: impl SrvResolver) {
    let result = resolver.get_srv_records("_http.foobar.test.local.").await;
    assert!(result.is_err(), "expected error for malformed SRV name");
}

#[test]
fn lookup_very_malformed_srv_name() {
    Sandbox::new()
        .component(MockDns::new(&[]))
        .run_with_tokio(|| async {
            test_lookup_very_malformed_srv_name(LibResolv).await;
            test_lookup_very_malformed_srv_name(
                hickory_resolver::Resolver::builder_tokio().unwrap().build(),
            )
            .await;
        });
}

async fn test_lookup_very_malformed_srv_name(resolver: impl SrvResolver) {
    let result = resolver.get_srv_records("  @#*^[_hsd flt.com").await;
    assert!(
        result.is_err(),
        "expected error for very malformed SRV name"
    );
}

#[test]
fn lookup_srv_name_containing_nul() {
    Sandbox::new()
        .component(MockDns::new(&[]))
        .run_with_tokio(|| async {
            test_lookup_srv_name_containing_nul(LibResolv).await;
            test_lookup_srv_name_containing_nul(
                hickory_resolver::Resolver::builder_tokio().unwrap().build(),
            )
            .await;
        });
}

async fn test_lookup_srv_name_containing_nul(resolver: impl SrvResolver) {
    let result = resolver.get_srv_records("_http.\0_tcp.foo.com").await;
    assert!(
        result.is_err(),
        "expected error for SRV name containing NUL"
    );
}
