//! A trivial test that just checks that the test harness is working.

use crate::{harness::Test, tests::DEFAULT_TEST_CONFIG};

pub static TEST_TRIVIAL_WITH_DEFAULT_CONFIG: Test = Test {
    name: "test_trivial_with_default_config",
    run: test_trivial_with_default_config,
    config: &DEFAULT_TEST_CONFIG,
};

fn test_trivial_with_default_config() {
    TEST_TRIVIAL_WITH_DEFAULT_CONFIG.config.validate();
    print!("stdout check");
    eprint!("stderr check");
}
