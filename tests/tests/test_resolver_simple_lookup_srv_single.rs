//! A test that looks up a single SRV record.

use srv_rs::resolver::SrvResolver;

use crate::{
    harness::{MockSrv, Test, TestConfig},
    tests::helpers::{run_with_all_resolvers, DEFAULT_MOCK_FILES},
};

/// Look up a single SRV record in a world where only one SRV record is present.
pub static TEST_RESOLVER_SIMPLE_LOOKUP_SRV_SINGLE: Test = Test {
    name: "test_resolver_simple_lookup_srv_single",
    run: test_resolver_simple_lookup_srv_single,
    config: &TestConfig {
        mock_files: DEFAULT_MOCK_FILES,
        dns_records: &[MockSrv::new(
            "_http._tcp.test.local.",
            10,
            100,
            8080,
            "server1.test.local.",
            300,
        )],
    },
};

/// Test implementation of [`TEST_RESOLVER_SIMPLE_LOOKUP_SRV_SINGLE`].
fn test_resolver_simple_lookup_srv_single() {
    async fn test(resolver: impl SrvResolver) {
        // Get all SRV records sorted by priority and weight
        let (records, _valid_until) = resolver
            .get_srv_records("_http._tcp.test.local.")
            .await
            .expect("SRV lookup failed");

        // Get the expected SRV records to compare against
        let expected = TEST_RESOLVER_SIMPLE_LOOKUP_SRV_SINGLE.config.dns_records;

        // Assert the correct number of records were returned
        assert_eq!(
            records.len(),
            expected.len(),
            "expected {} SRV records, got {}",
            expected.len(),
            records.len()
        );

        // Assert the found record is the same as the expected record
        let record = records.first().unwrap();
        assert!(expected.first().unwrap() == record, "SRV record mismatch");
    }

    TEST_RESOLVER_SIMPLE_LOOKUP_SRV_SINGLE.config.validate();
    run_with_all_resolvers!(test);
}
