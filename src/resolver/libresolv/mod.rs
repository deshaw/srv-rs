//! SRV Resolver backed by `libresolv`.

use super::SrvResolver;
use crate::SrvRecord;
use async_trait::async_trait;
use resolv::{Class, Record, RecordType, Resolver, Section};
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

thread_local!(static RESOLVER: RefCell<Resolver> =
    RefCell::new(Resolver::new().expect("unable to initialize libresolv state"))
);

/// Errors encountered by [`LibResolv`].
#[derive(Debug, thiserror::Error)]
pub enum LibResolvError {
    /// SRV resolver errors.
    #[error("resolver: {0}")]
    Resolver(#[from] resolv::error::Error),
    /// Tried to parse non-SRV record as SRV.
    #[error("record type is not SRV")]
    NotSrv,
}

impl PartialEq for LibResolvError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (LibResolvError::NotSrv, LibResolvError::NotSrv) => true,
            (LibResolvError::Resolver(a), LibResolvError::Resolver(b)) => {
                // Compare based on the debug representation since resolv::error::Error
                // doesn't implement PartialEq
                format!("{a:?}") == format!("{b:?}")
            }
            _ => false,
        }
    }
}

impl Eq for LibResolvError {}

/// SRV Resolver backed by `libresolv`.
#[derive(Debug, Default)]
pub struct LibResolv;

impl LibResolv {
    /// Initializes a resolver.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SrvResolver for LibResolv {
    type Record = LibResolvSrvRecord;
    type Error = LibResolvError;

    async fn get_srv_records_unordered(
        &self,
        srv: &str,
    ) -> Result<(Vec<Self::Record>, Instant), Self::Error> {
        RESOLVER.with(|resolver| {
            let mut resolver = resolver.borrow_mut();
            let response_time = Instant::now();
            let mut response = resolver.search(srv.as_bytes(), Class::IN, RecordType::SRV)?;

            let num_records = response.get_section_count(Section::Answer);
            let mut records = Vec::with_capacity(num_records);
            let mut min_ttl: Option<Duration> = None;

            for idx in 0..num_records {
                let record: Record<resolv::record::SRV> =
                    response.get_record(Section::Answer, idx)?;
                let ttl = Duration::from_secs(u64::from(record.ttl));
                min_ttl = min_ttl.map_or(Some(ttl), |m| Some(m.min(ttl)));
                records.push(LibResolvSrvRecord::from(record));
            }

            Ok((records, response_time + min_ttl.unwrap_or_default()))
        })
    }
}

/// Representation of SRV records used by [`LibResolv`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibResolvSrvRecord {
    /// Records's target.
    pub target: String,
    /// Record's port.
    pub port: u16,
    /// Record's priority.
    pub priority: u16,
    /// Record's weight.
    pub weight: u16,
}

impl SrvRecord for LibResolvSrvRecord {
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

impl From<Record<resolv::record::SRV>> for LibResolvSrvRecord {
    fn from(record: Record<resolv::record::SRV>) -> Self {
        Self {
            target: record.data.name,
            port: record.data.port,
            // The resolv crate uses i16 for priority/weight but they should be u16
            priority: record.data.priority as u16,
            weight: record.data.weight as u16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn srv_lookup() -> Result<(), LibResolvError> {
        let (records, valid_until) = LibResolv::default()
            .get_srv_records_unordered(crate::EXAMPLE_SRV)
            .await?;
        assert_ne!(records.len(), 0);
        assert!(valid_until > Instant::now());
        Ok(())
    }

    #[tokio::test]
    async fn srv_lookup_ordered() -> Result<(), LibResolvError> {
        let (records, _) = LibResolv::default()
            .get_srv_records(crate::EXAMPLE_SRV)
            .await?;
        assert_ne!(records.len(), 0);
        assert!((0..records.len() - 1).all(|i| records[i].priority() <= records[i + 1].priority()));
        Ok(())
    }

    #[tokio::test]
    async fn invalid_host() {
        let result = LibResolv::default()
            .get_srv_records("_http._tcp.foobar.deshaw.com")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn malformed_srv_name() {
        let result = LibResolv::default()
            .get_srv_records("_http.foobar.deshaw.com")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn very_malformed_srv_name() {
        let result = LibResolv::default()
            .get_srv_records("  @#*^[_hsd flt.com")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn srv_name_containing_nul_terminator() {
        let result = LibResolv::default()
            .get_srv_records("_http.\0_tcp.foo.com")
            .await;
        assert!(result.is_err());
    }
}
