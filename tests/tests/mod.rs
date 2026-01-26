use crate::harness::{MockFile, TestConfig};

/// Files to use for all tests unless otherwise specified.
static DEFAULT_MOCK_FILES: &[MockFile] = &[
    MockFile::new("/etc/resolv.conf", b"nameserver 127.0.0.1\n"),
    MockFile::new("/etc/hosts", b"127.0.0.1 localhost\n"),
    MockFile::new("/etc/nsswitch.conf", b"hosts: files dns\n"),
];

/// Configuration to use for all tests unless otherwise specified.
static DEFAULT_TEST_CONFIG: TestConfig = TestConfig {
    mock_files: DEFAULT_MOCK_FILES,
    dns_records: &[],
};

mod test_simple_lookup_srv_multiple;
mod test_simple_lookup_srv_single;
mod test_trivial_with_default_config;

pub use test_simple_lookup_srv_multiple::TEST_SIMPLE_LOOKUP_SRV_MULTIPLE;
pub use test_simple_lookup_srv_single::TEST_SIMPLE_LOOKUP_SRV_SINGLE;
pub use test_trivial_with_default_config::TEST_TRIVIAL_WITH_DEFAULT_CONFIG;
