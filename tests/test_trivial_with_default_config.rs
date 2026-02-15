//! A trivial test that checks that the sandbox harness is working.

mod sandbox;

use sandbox::Sandbox;

#[test]
fn test_trivial() {
    Sandbox::new().run(|| {
        print!("stdout check");
        eprint!("stderr check");
    });
}
