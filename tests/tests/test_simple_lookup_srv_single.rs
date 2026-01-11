//! A test that looks up a single SRV record.

use srv_rs::resolver::{libresolv::LibResolv, SrvResolver};

use crate::{
    harness::{MockSrv, Test, TestConfig},
    tests::DEFAULT_MOCK_FILES,
};

pub static TEST_SIMPLE_LOOKUP_SRV_SINGLE: Test = Test {
    name: "test_simple_lookup_srv_single",
    run: test_simple_lookup_srv_single,
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

fn test_simple_lookup_srv_single() {
    TEST_SIMPLE_LOOKUP_SRV_SINGLE.config.validate();
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let resolver = LibResolv::default();
        let (records, _valid_until) = resolver
            .get_srv_records_unordered("_http._tcp.test.local.")
            .await
            .expect("SRV lookup failed");

        assert_eq!(records.len(), 1, "expected 1 SRV record");
        let record = records.first().unwrap();
        assert_eq!(record.priority, 10);
        assert_eq!(record.weight, 100);
        assert_eq!(record.port, 8080);
        assert_eq!(record.target, "server1.test.local");
        println!("SRV lookup successful: {record:?}");
    });
}
