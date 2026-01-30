use srv_rs::{resolver::SrvResolver, SrvRecord};

use crate::harness::{MockFile, MockSrv, TestConfig};

/// Files to use for all tests unless otherwise specified.
pub static DEFAULT_MOCK_FILES: &[MockFile] = &[
    MockFile::new("/etc/resolv.conf", b"nameserver 127.0.0.1\n"),
    MockFile::new("/etc/hosts", b"127.0.0.1 localhost\n"),
    MockFile::new("/etc/nsswitch.conf", b"hosts: files dns\n"),
];

/// Configuration to use for all tests unless otherwise specified.
pub static DEFAULT_TEST_CONFIG: TestConfig = TestConfig {
    mock_files: DEFAULT_MOCK_FILES,
    dns_records: &[],
};

/// Runs a function with a single resolver.
pub fn run_with_resolver<R, F, Fut>(resolver: R, f: &F)
where
    R: SrvResolver,
    F: Fn(R) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(f(resolver));
}

/// Runs a test function against all resolver implementations.
#[macro_export]
macro_rules! run_with_all_resolvers {
    ($f:expr) => {
        // Run with LibResolv
        $crate::tests::helpers::run_with_resolver(srv_rs::resolver::libresolv::LibResolv, &$f);
        // Run with Hickory
        $crate::tests::helpers::run_with_resolver(
            hickory_resolver::Resolver::builder_tokio()
                .expect("failed to create hickory resolver")
                .build(),
            &$f,
        );
    };
}
pub use run_with_all_resolvers;

/// [`PartialEq`] implementation for [`MockSrv`] and anything that implements [`SrvRecord`].
/// This is a convenience implementation for testing purposes.
/// It omits:
/// - Record name (not exposed in the [`SrvRecord`] trait)
/// - TTL (not exposed in the [`SrvRecord`] trait)
/// - Trailing dot on the target, if one exists
impl<Srv: SrvRecord> PartialEq<Srv> for MockSrv {
    fn eq(&self, other: &Srv) -> bool {
        self.priority == other.priority()
            && self.weight == other.weight()
            && self.port == other.port()
            && self.target.trim_end_matches('.') == other.target().to_string().trim_end_matches('.')
    }
}
