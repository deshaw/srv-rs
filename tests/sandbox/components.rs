//! Sandbox components and the [`SandboxComponent`] trait.

use std::path::{Path, PathBuf};

/// Minimal mock DNS server for testing SRV record resolution.
pub mod dns;

/// Extend the sandbox environment with optional components (e.g., DNS, naming, certificate authorities, etc.)
pub trait SandboxComponent {
    /// A component may require additional capabilities and mounts to run correctly inside the sandbox.
    /// This method returns a list of those requirements.
    fn configure_sandbox(&self, tempdir: &Path) -> Vec<SandboxRequirement>;

    /// Start the component inside the sandbox and return a handle to it.
    fn start(&self) -> Box<dyn std::any::Any>;
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
