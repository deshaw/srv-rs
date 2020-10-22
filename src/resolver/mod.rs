//! SRV resolvers.

use crate::record::SrvRecord;
use async_trait::async_trait;
use rand::Rng;

#[cfg(feature = "libresolv")]
pub mod libresolv;

#[cfg(feature = "trust-dns")]
pub mod trust_dns;

/// Represents the ability to act as a SRV resolver.
#[async_trait]
pub trait SrvResolver: Send + Sync {
    /// SRV record representation produced by the resolver.
    type Record: SrvRecord;

    /// Errors encountered during SRV resolution.
    type Error: std::error::Error + 'static;

    /// Gets the records corresponding to a srv name without sorting by priority
    /// or shuffling based on weight.
    async fn get_srv_records_unordered(&self, srv: &str) -> Result<Vec<Self::Record>, Self::Error>;

    /// Gets the records corresponding to a srv name, sorting by priority and
    /// shuffling based on weight.
    async fn get_srv_records(&self, srv: &str) -> Result<Vec<Self::Record>, Self::Error> {
        let mut records = self.get_srv_records_unordered(srv).await?;
        Self::order_srv_records(&mut records, rand::thread_rng());
        Ok(records)
    }

    /// Sorts SRV records by priority and weight per RFC 2782.
    fn order_srv_records(records: &mut [Self::Record], mut rng: impl Rng) {
        records.sort_by_cached_key(|record| record.sort_key(&mut rng));
    }
}
