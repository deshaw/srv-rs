//! Sandbox components and the [`SandboxComponent`] trait.

use std::path::{Path, PathBuf};

/// Minimal mock DNS server for testing SRV record resolution.
pub mod dns;
use dns::MockDns;

/// Extend the sandbox environment with optional components (e.g., DNS, naming, certificate authorities, etc.)
pub trait SandboxComponent {
    /// A component may require additional capabilities and mounts to run correctly inside the sandbox.
    /// This method returns a list of those requirements.
    fn configure_sandbox(&self, tempdir: &Path) -> Vec<SandboxRequirement>;

    /// Start the component inside the sandbox and return a handle to it.
    fn start(&self) -> Box<dyn std::any::Any>;
}

impl SandboxComponent for MockDns {
    fn configure_sandbox(&self, tempdir: &Path) -> Vec<SandboxRequirement> {
        let mut reqs = vec![
            // Required to bring up the loopback interface.
            SandboxRequirement::Capability(Capability::NetAdmin),
            // Required to bind a socket to port 53.
            SandboxRequirement::Capability(Capability::NetBindService),
        ];
        for &(desired_path, contents) in Self::config_files() {
            let host_path = tempdir.join(Path::new(desired_path).file_name().unwrap());
            std::fs::write(&host_path, contents).expect("failed to write mock file to tempdir");
            reqs.push(SandboxRequirement::BindMountReadOnly {
                host_path,
                desired_path: PathBuf::from(desired_path),
            });
        }
        reqs
    }

    /// Start the DNS server and return a handle to it.
    fn start(&self) -> Box<dyn std::any::Any> {
        // Validate that required configuration files were mounted correctly.
        for &(desired_path, contents) in Self::config_files() {
            let actual = std::fs::read(desired_path)
                .unwrap_or_else(|e| panic!("failed to read mock file {}: {}", desired_path, e));
            assert_eq!(
                actual, contents,
                "mock file {desired_path} contents mismatch",
            );
        }

        // Start the DNS server.
        Box::new(self.spawn().expect("failed to start mock DNS server"))
    }
}

/// Requirements that a component may need to run correctly inside the sandbox.
pub enum SandboxRequirement {
    /// Add a [`Capability`] to the sandbox.
    Capability(Capability),
    /// Read-only bind mount the host path to the desired path in the sandbox.
    BindMountReadOnly {
        host_path: PathBuf,
        desired_path: PathBuf,
    },
}

/// Capabilities that may be added to the sandbox.
/// New capabilities are added to this enum as needed.
pub enum Capability {
    /// `CAP_NET_ADMIN`
    NetAdmin,
    /// `CAP_NET_BIND_SERVICE`
    NetBindService,
}

impl From<&Capability> for &'static str {
    fn from(value: &Capability) -> Self {
        match value {
            Capability::NetAdmin => "CAP_NET_ADMIN",
            Capability::NetBindService => "CAP_NET_BIND_SERVICE",
        }
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.into())
    }
}
