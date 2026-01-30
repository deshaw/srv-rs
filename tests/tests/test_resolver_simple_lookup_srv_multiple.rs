//! A test that looks up multiple SRV records.

use srv_rs::{resolver::SrvResolver, SrvRecord};

use crate::{
    harness::{MockSrv, Test, TestConfig},
    tests::helpers::{run_with_all_resolvers, DEFAULT_MOCK_FILES},
};

pub static TEST_RESOLVER_SIMPLE_LOOKUP_SRV_MULTIPLE: Test = Test {
    name: "test_resolver_simple_lookup_srv_multiple",
    run: test_resolver_simple_lookup_srv_multiple,
    config: &TestConfig {
        mock_files: DEFAULT_MOCK_FILES,
        dns_records: &[
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
        ],
    },
};

fn test_resolver_simple_lookup_srv_multiple() {
    async fn test(resolver: impl SrvResolver) {
        // Get all SRV records sorted by priority and weight
        let (sorted, _valid_until) = resolver
            .get_srv_records("_http._tcp.multi.local.")
            .await
            .expect("SRV lookup failed");

        // Get all SRV records without sorting them
        let (unordered, _valid_until) = resolver
            .get_srv_records_unordered("_http._tcp.multi.local.")
            .await
            .expect("SRV lookup failed");

        // Get the expected SRV records to compare against
        let expected = TEST_RESOLVER_SIMPLE_LOOKUP_SRV_MULTIPLE.config.dns_records;

        // Assert the correct number of sorted records were returned
        assert_eq!(
            sorted.len(),
            expected.len(),
            "expected {} SRV records, got {}",
            expected.len(),
            sorted.len()
        );

        // Assert the correct number of unordered records were returned
        assert_eq!(
            unordered.len(),
            expected.len(),
            "expected {} SRV records, got {}",
            expected.len(),
            unordered.len()
        );

        // Assert the sorted records are the same as the expected records
        assert!(
            expected
                .iter()
                .all(|record| sorted.iter().any(|s| record == s)),
            "sorted SRV records do not contain all expected records"
        );

        // Assert the unordered records are the same as the expected records
        assert!(
            expected
                .iter()
                .all(|record| unordered.iter().any(|s| record == s)),
            "unordered SRV records do not contain all expected records"
        );

        // Assert the records have been correctly sorted by priority per RFC 2782:
        // Records MUST be ordered by ascending priority (lower priority is more preferred).
        // Within the same priority, weight determines selection probability, but is non-deterministic.
        assert!(
            sorted
                .windows(2)
                .all(|w| w[0].priority() <= w[1].priority()),
            "sorted SRV records are not in ascending priority order"
        );
    }

    TEST_RESOLVER_SIMPLE_LOOKUP_SRV_MULTIPLE.config.validate();
    run_with_all_resolvers!(test);
}
