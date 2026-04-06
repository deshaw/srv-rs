//! Static SRV resolver that returns manually pre-configured records without DNS lookups.

use super::SrvResolver;
use crate::SrvRecord;
use async_trait::async_trait;
use std::{convert::Infallible, time::Instant};

/// SRV resolver that returns a static, manually specified set of records without performing DNS lookups.
///
/// # Examples
///
/// ```
/// use srv_rs::{SrvClient, Execution};
/// use srv_rs::resolver::manual::StaticResolver;
///
/// # #[tokio::main]
/// # async fn main() {
/// let resolver = StaticResolver::new_from_single_target("server.example.com", 8080);
/// let client: SrvClient<_> = SrvClient::new_with_resolver("unused", resolver);
///
/// let result = client.execute(Execution::Serial, |uri| async move {
///     Ok::<_, std::io::Error>(uri.to_string())
/// }).await;
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct StaticResolver {
    records: Vec<StaticSrvRecord>,
}

impl StaticResolver {
    /// Create a new static resolver with the given records that will not perform any DNS lookups.
    pub fn new(records: impl IntoIterator<Item = StaticSrvRecord>) -> Self {
        Self {
            records: records.into_iter().collect(),
        }
    }

    /// Create a new static resolver pointing to a single target.
    /// Internally, the record is assigned a priority and weight of 0.
    pub fn new_from_single_target(target: impl Into<String>, port: u16) -> Self {
        Self::new([StaticSrvRecord {
            target: target.into(),
            port,
            priority: 0,
            weight: 0,
        }])
    }

    /// Return all configured records without sorting by priority or shuffling by weight.
    ///
    /// Unlike [`SrvResolver::get_srv_records_unordered`], this method requires no SRV name
    /// and is synchronous. This is useful when working with a [`StaticResolver`] directly.
    #[must_use]
    pub fn get_static_srv_records_unordered(&self) -> Vec<StaticSrvRecord> {
        self.records.clone()
    }

    /// Return all configured records sorted by priority and shuffled by weight per RFC 2782.
    ///
    /// Unlike [`SrvResolver::get_srv_records`], this method requires no SRV name and is
    /// synchronous. This is useful when working with a [`StaticResolver`] directly.
    #[must_use]
    pub fn get_static_srv_records(&self) -> Vec<StaticSrvRecord> {
        let mut records = self.get_static_srv_records_unordered();
        Self::order_srv_records(&mut records, rand::rng());
        records
    }
}

#[async_trait]
impl SrvResolver for StaticResolver {
    type Record = StaticSrvRecord;
    type Error = Infallible;

    async fn get_srv_records_unordered(
        &self,
        _srv: &str,
    ) -> Result<(Vec<Self::Record>, Instant), Self::Error> {
        Ok((self.get_static_srv_records_unordered(), Instant::now()))
    }
}

/// A manual SRV record with pre-configured target, port, priority, and weight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticSrvRecord {
    /// Record's target hostname.
    pub target: String,
    /// Record's port.
    pub port: u16,
    /// Record's priority.
    pub priority: u16,
    /// Record's weight.
    pub weight: u16,
}

impl SrvRecord for StaticSrvRecord {
    type Target = str;

    fn target(&self) -> &Self::Target {
        &self.target
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn priority(&self) -> u16 {
        self.priority
    }

    fn weight(&self) -> u16 {
        self.weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test a static resolver configured for a single target.
    #[test]
    fn test_new_from_single_target() {
        let resolver = StaticResolver::new_from_single_target("host.example.com", 8080);
        let records = resolver.get_static_srv_records_unordered();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].target(), "host.example.com");
        assert_eq!(records[0].port(), 8080);
        assert_eq!(records[0].priority(), 0);
        assert_eq!(records[0].weight(), 0);
    }

    /// Test a static resolver configured for multiple targets.
    #[test]
    fn test_get_static_srv_records() {
        let low = StaticSrvRecord {
            target: "low-priority.example.com".into(),
            port: 9090,
            priority: 10,
            weight: 5,
        };
        let high = StaticSrvRecord {
            target: "high-priority.example.com".into(),
            port: 8080,
            priority: 1,
            weight: 20,
        };
        let resolver = StaticResolver::new([low.clone(), high.clone()]);

        let mut unordered = resolver.get_static_srv_records_unordered();
        unordered.sort_by(|a, b| a.target.cmp(&b.target));
        assert_eq!(unordered, [high.clone(), low.clone()]);

        let ordered = resolver.get_static_srv_records();
        assert_eq!(ordered, [high, low]);
    }
}
