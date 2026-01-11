//! A test that looks up multiple SRV records.

use srv_rs::resolver::{libresolv::LibResolv, SrvResolver};

use crate::{
    harness::{MockSrv, Test, TestConfig},
    tests::DEFAULT_MOCK_FILES,
};

pub static TEST_SIMPLE_LOOKUP_SRV_MULTIPLE: Test = Test {
    name: "test_simple_lookup_srv_multiple",
    run: test_simple_lookup_srv_multiple,
    config: &TestConfig {
        mock_files: DEFAULT_MOCK_FILES,
        dns_records: &[
            MockSrv::new(
                "_http._tcp.multi.local",
                10,
                100,
                8080,
                "primary.multi.local",
                300,
            ),
            MockSrv::new(
                "_http._tcp.multi.local",
                20,
                50,
                8081,
                "secondary.multi.local",
                300,
            ),
            MockSrv::new(
                "_http._tcp.multi.local",
                10,
                25,
                8082,
                "backup.multi.local",
                300,
            ),
        ],
    },
};

fn test_simple_lookup_srv_multiple() {
    TEST_SIMPLE_LOOKUP_SRV_MULTIPLE.config.validate();
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let resolver = LibResolv::default();
        let (records, _valid_until) = resolver
            .get_srv_records("_http._tcp.multi.local")
            .await
            .expect("SRV lookup failed");

        assert_eq!(records.len(), 3, "expected 3 SRV records");

        assert!(
            records.iter().map(|r| r.priority).is_sorted(),
            "records should be sorted by priority: {:?}",
            records.iter().map(|r| r.priority).collect::<Vec<_>>()
        );

        println!("SRV lookup returned {} records:", records.len());
        for record in &records {
            println!("SRV lookup successful: {record:?}",);
        }
    });
}
