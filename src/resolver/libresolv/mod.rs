//! SRV Resolver backed by `libresolv`.

use super::SrvResolver;
use crate::SrvRecord;
use async_trait::async_trait;
use resolv::{Record, Resolver};
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

// Per-thread libresolv resolver, where each resolver has a stable address.
// This is required because:
// - libresolv's res_state contains raw pointers, some of which are self-referential.
// - The intended usage is to manage per-thread resolver state.
//
// From the man page for resolver(3):
//
//     The traditional resolver interfaces such as res_init() and res_query()
//     use some static (global) state stored in the _res structure, rendering
//     these functions non-thread-safe.
//
//     BIND 8.2 introduced a set of new interfaces res_ninit(), res_nquery(),
//     and so on, which take a res_state as their first argument, so you can use
//     a per-thread resolver state.
//
thread_local!(static RESOLVER: RefCell<Resolver> =
    RefCell::new(Resolver::new().expect("unable to initialize libresolv state"))
);

/// Errors encountered by [`LibResolv`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LibResolvError {
    /// SRV resolver errors.
    #[error("resolver: {0}")]
    Resolver(#[from] resolv::error::Error),
    /// Tried to parse non-SRV record as SRV.
    #[error("record type is not SRV")]
    NotSrv,
}

/// SRV Resolver backed by `libresolv`.
///
/// # Panics
///
/// Panics on first use if the per-thread resolver cannot be initialized.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LibResolv;

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
            let mut response: resolv::Response =
                resolver.search(srv.as_bytes(), resolv::Class::IN, resolv::RecordType::SRV)?;
            let response_time = Instant::now();
            let (ttls, srvs): (Vec<Duration>, Vec<Self::Record>) = response
                .answers::<resolv::record::SRV>()
                .map(|x| (Duration::from_secs(u64::from(x.ttl)), Self::Record::from(x)))
                .unzip();
            let min_ttl = ttls.into_iter().min().unwrap_or(Duration::ZERO);
            Ok((srvs, response_time + min_ttl))
        })
    }
}

/// Representation of SRV records used by [`LibResolv`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LibResolvSrvRecord {
    /// Record's target.
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
            priority: record.data.priority,
            weight: record.data.weight,
        }
    }
}
