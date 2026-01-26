//! SRV resolvers.

use crate::SrvRecord;
use async_trait::async_trait;
use rand::Rng;
use std::time::Instant;

#[cfg(feature = "libresolv")]
pub mod libresolv;

#[cfg(feature = "hickory")]
mod hickory;

/// Represents the ability to act as a SRV resolver.
#[async_trait]
pub trait SrvResolver: Send + Sync {
    /// SRV record representation produced by the resolver.
    type Record: SrvRecord;

    /// Errors encountered during SRV resolution.
    type Error: std::error::Error + 'static;

    /// Gets the records corresponding to a srv name without sorting by priority
    /// or shuffling based on weight, returning them along with the time they're
    /// valid until.
    async fn get_srv_records_unordered(
        &self,
        srv: &str,
    ) -> Result<(Vec<Self::Record>, Instant), Self::Error>;

    /// Gets the records corresponding to a srv name, sorting by priority and
    /// shuffling based on weight, returning them along with the time they're
    /// valid until.
    async fn get_srv_records(
        &self,
        srv: &str,
    ) -> Result<(Vec<Self::Record>, Instant), Self::Error> {
        let (mut records, valid_until) = self.get_srv_records_unordered(srv).await?;
        Self::order_srv_records(&mut records, rand::rng());
        Ok((records, valid_until))
    }

    /// Sorts SRV records by priority and weight per RFC 2782.
    fn order_srv_records(records: &mut [Self::Record], mut rng: impl Rng) {
        records.sort_by_cached_key(|record| record.sort_key(&mut rng));
    }
}
