// Shared test infrastructure — not every test binary uses every item.
#![allow(dead_code)]

use std::{
    io::{stderr, stdout, Write},
    process::Command,
};

pub mod components;

use components::{SandboxComponent, SandboxRequirement};

/// A sandbox test environment.
pub struct Sandbox {
    /// Optional components to extend the sandbox environment (e.g., DNS, naming, certificate authorities, etc.)
    components: Vec<Box<dyn SandboxComponent>>,
}

impl Sandbox {
    /// Environment variable set within the sandboxed child process.
    const INSIDE_SANDBOX: &str = "SRV_RS_SANDBOX";

    /// Create a new, empty [`Sandbox`].
    pub const fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    /// Add a [`SandboxComponent`] to the sandbox.
    pub fn component(mut self, component: impl SandboxComponent + 'static) -> Self {
        self.components.push(Box::new(component));
        self
    }

    /// Execute the given test body within the sandbox.
    pub fn run(self, test_body: impl FnOnce()) {
        if std::env::var(Self::INSIDE_SANDBOX).is_ok() {
            // Child: we're inside the sandbox; run the test body
            let _started_components: Vec<_> = self.components.iter().map(|c| c.start()).collect();
            test_body();
        } else {
            // Parent: execute the test binary in the sandbox
            self.execute_in_sandbox();
        }
    }

    /// Execute the given test body within the sandbox using a Tokio runtime.
    pub fn run_with_tokio<F: std::future::Future<Output = ()>>(
        self,
        test_body: impl FnOnce() -> F,
    ) {
        self.run(|| {
            tokio::runtime::Runtime::new()
                .expect("failed to create Tokio runtime")
                .block_on(test_body());
        });
    }

    /// Execute the current test binary inside a sandboxed environment.
    fn execute_in_sandbox(self) {
        let self_exe = std::env::current_exe().expect("failed to determine test binary path");
        let tempdir = tempfile::tempdir().expect("failed to create tempdir for sandbox");
        let test_name = std::thread::current()
            .name()
            .expect("sandbox must be run from within a #[test] function")
            .to_string();

        // Process sandbox components into bwrap arguments
        let component_args: Vec<String> = self
            .components
            .iter()
            .flat_map(|c| c.configure_sandbox(tempdir.path()))
            .flat_map(|add| add.to_bwrap_args())
            .collect();

        // Build and execute a bubblewrap command
        let output = Command::new("bwrap")
            .args(["--unshare-all"]) // Unshare every namespace
            .args(["--dev-bind", "/", "/"]) // TODO: this can be narrowed
            .arg("--die-with-parent") // Die if the parent exits
            .args(&component_args) // Add component-specific arguments
            .arg("--")
            .arg(&self_exe)
            .args([&test_name, "--exact", "--nocapture"])
            .env(Self::INSIDE_SANDBOX, "1")
            .output()
            .expect("failed to execute bwrap");

        // Forward test output and success/failure to the Rust test harness
        let _ = stdout().write_all(&output.stdout);
        let _ = stderr().write_all(&output.stderr);
        assert!(
            output.status.success(),
            "test `{test_name}` failed inside sandbox (exit code: {:?})",
            output.status.code()
        );
    }
}

impl SandboxRequirement {
    /// Translate the requirement into a list of bwrap command line arguments.
    fn to_bwrap_args(&self) -> Vec<String> {
        match self {
            Self::Capability(cap) => vec![String::from("--cap-add"), cap.to_string()],
            Self::BindMountReadOnly {
                host_path: mock_path,
                desired_path,
            } => {
                vec![
                    String::from("--ro-bind"),
                    mock_path.to_string_lossy().to_string(),
                    desired_path.to_string_lossy().to_string(),
                ]
            }
        }
    }
}
