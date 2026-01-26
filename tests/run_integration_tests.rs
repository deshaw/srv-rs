use crate::harness::{Test, TestHarness};

mod harness;
mod tests;

/// Complete list of integration tests to run.
static TESTS: &[&Test] = &[
    &tests::TEST_TRIVIAL_WITH_DEFAULT_CONFIG,
    &tests::TEST_SIMPLE_LOOKUP_SRV_SINGLE,
    &tests::TEST_SIMPLE_LOOKUP_SRV_MULTIPLE,
];

fn main() -> std::process::ExitCode {
    TestHarness::setup(TESTS)
}
