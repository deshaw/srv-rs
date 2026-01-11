use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

use owo_colors::OwoColorize;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::harness::dns_server::DnsServer;

pub mod dns_server;

/// Test harness implementation.
pub struct TestHarness;

impl TestHarness {
    /// Pass the test name as an argument to the test binary to run only that test.
    /// This is used internally by the test harness itself, not humans.
    const TEST_ARG: &str = "--test-name=";

    /// Setup the test harness.
    /// Run all the tests or run a single test if a test name is provided.
    pub fn setup(tests: &[&Test]) {
        // Run a single test if one is specified
        let args: Vec<String> = std::env::args().collect();
        if let Some(test_name) = args.get(1).and_then(|s| s.strip_prefix(Self::TEST_ARG)) {
            let test = tests
                .iter()
                .find(|t| t.name == test_name)
                .unwrap_or_else(|| panic!("unknown test: {}", test_name));
            let _dns =
                DnsServer::spawn(test.config.dns_records).expect("failed to start DNS server");
            (test.run)();
            return;
        }

        // General case: run all tests in parallel, isolated processes
        let self_exe = std::env::current_exe().unwrap();
        let results: Vec<(&str, std::io::Result<Output>)> = tests
            .par_iter()
            .map(|test| {
                let arg = format!("{}{}", Self::TEST_ARG, test.name);
                let test_output = Self::execute_test_in_sandbox(test, &self_exe, &[&arg]);
                (test.name, test_output)
            })
            .collect();

        // Process the test results and print a summary
        let mut passed = 0;
        for (test_name, test_result) in results {
            passed += usize::from(Self::process_test_result(test_name, test_result));
        }
        println!("{}{passed}", "passed: ".green());
        let failed = tests.len() - passed;
        if failed > 0 {
            println!("{}{failed}", "failed: ".red());
        } else {
            println!("{}", "all tests passed".green());
        }
    }

    /// Execute a test in an isolated sandbox environment.
    fn execute_test_in_sandbox(
        test: &Test,
        binary: impl AsRef<OsStr>,
        args: &[&str],
    ) -> std::io::Result<Output> {
        let tempdir = tempfile::tempdir()?;
        let mut cmd = Command::new("bwrap");
        cmd.args(["--unshare-all"]) // Unshare every namespace (net, user, uts, pid, etc.)
            .args(["--cap-add", "CAP_NET_ADMIN"]) // Required to bring up loopback interface
            .args(["--cap-add", "CAP_NET_BIND_SERVICE"]) // Required for test DNS server
            .args(["--dev-bind", "/", "/"]) // This can be narrowed
            .arg("--die-with-parent"); // Die if the test harness exits unexpectedly

        // Mount any test files into the environment
        for f in test.config.mock_files {
            let mock_path = tempdir
                .path()
                .join(Path::new(f.desired_path).file_name().unwrap());
            std::fs::write(&mock_path, f.contents).unwrap();
            cmd.args(["--ro-bind", &mock_path.to_string_lossy(), f.desired_path]);
        }

        cmd.arg("--").arg(binary).args(args).output()
    }

    /// Process a test result: print output and return whether the test passed.
    fn process_test_result(test_name: &str, result: std::io::Result<Output>) -> bool {
        let success = match result {
            Ok(out) => {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    println!("{test_name}: stdout: {line}");
                }
                for line in String::from_utf8_lossy(&out.stderr).lines() {
                    eprintln!("{}", format!("{test_name}: stderr: {line}").yellow());
                }
                out.status.success()
            }
            Err(e) => {
                eprintln!("{}", format!("{test_name}: error: {e}").red());
                false
            }
        };

        if success {
            println!("{}{test_name}\n", "passed: ".green());
        } else {
            println!("{}{test_name}\n", "failed: ".red());
        }
        success
    }
}

/// Integration test definition struct.
/// Create one of these to each new integration test.
pub struct Test {
    /// Unique name of the test
    pub name: &'static str,
    /// Function to run the test
    pub run: fn(),
    /// Test configuration
    pub config: &'static TestConfig,
}

/// Test configuration struct.
/// Use this to define the state of the world for a test.
#[non_exhaustive]
pub struct TestConfig {
    /// Files to mock in the test environment
    pub mock_files: &'static [MockFile],
    /// DNS records to mock in the test environment
    pub dns_records: &'static [MockSrv],
}

/// Static SRV record definition for use in test configurations.
#[derive(Clone, Debug)]
pub struct MockSrv {
    /// The SRV name (e.g., "_http._tcp.example.com")
    pub name: &'static str,
    /// Priority value
    pub priority: u16,
    /// Weight value
    pub weight: u16,
    /// Port number
    pub port: u16,
    /// Target hostname
    pub target: &'static str,
    /// TTL in seconds
    pub ttl: u32,
}

impl MockSrv {
    /// Create a new SRV record.
    pub const fn new(
        name: &'static str,
        priority: u16,
        weight: u16,
        port: u16,
        target: &'static str,
        ttl: u32,
    ) -> Self {
        Self {
            name,
            priority,
            weight,
            port,
            target,
            ttl,
        }
    }
}

impl TestConfig {
    /// Run this from within a test to validate the test configuration.
    pub fn validate(&self) {
        for f in self.mock_files {
            let contents = std::fs::read(f.desired_path).unwrap();
            assert_eq!(
                contents, f.contents,
                "mock file {} contents mismatch",
                f.desired_path
            );
        }
    }
}

/// Defines how to mock a file in the test environment.
pub struct MockFile {
    /// The path to the file in the test environment.
    pub desired_path: &'static str,
    /// The contents of the file in the test environment.
    pub contents: &'static [u8],
}

impl MockFile {
    /// Create a new mocked file.
    #[must_use]
    pub const fn new(desired_path: &'static str, contents: &'static [u8]) -> Self {
        Self {
            desired_path,
            contents,
        }
    }
}
