//! SRV resolver backed by `trust-dns-resolver`.

use super::SrvResolver;
use crate::record::SrvRecord;
use async_trait::async_trait;
use std::time::{Duration, Instant};
use trust_dns_resolver::{
    error::ResolveError,
    proto::{rr::rdata::SRV, DnsHandle},
    AsyncResolver, ConnectionProvider, Name,
};

#[async_trait]
impl<C, P> SrvResolver for AsyncResolver<C, P>
where
    C: DnsHandle,
    P: ConnectionProvider<Conn = C>,
{
    type Record = (SRV, Duration);
    type Error = ResolveError;

    async fn get_srv_records_unordered(&self, srv: &str) -> Result<Vec<Self::Record>, Self::Error> {
        let lookup = self.srv_lookup(srv).await?;
        // TODO: it'd be cleaner not to duplicate this duration--maybe we should
        // have the method return (Vec<Self::Record>, Duration).
        let ttl = lookup.as_lookup().valid_until() - Instant::now();
        Ok(lookup.into_iter().zip(std::iter::repeat(ttl)).collect())
    }
}

impl SrvRecord for (SRV, Duration) {
    type Target = Name;

    fn ttl(&self) -> Duration {
        self.1
    }

    fn target(&self) -> &Self::Target {
        self.0.target()
    }

    fn port(&self) -> u16 {
        self.0.port()
    }

    fn priority(&self) -> u16 {
        self.0.priority()
    }

    fn weight(&self) -> u16 {
        self.0.weight()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn srv_lookup() -> Result<(), ResolveError> {
        let records = AsyncResolver::tokio_from_system_conf()
            .await?
            .get_srv_records_unordered(crate::EXAMPLE_SRV)
            .await?;
        assert_ne!(records.len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn srv_lookup_ordered() -> Result<(), ResolveError> {
        let records = AsyncResolver::tokio_from_system_conf()
            .await?
            .get_srv_records(crate::EXAMPLE_SRV)
            .await?;
        assert_ne!(records.len(), 0);
        assert!((0..records.len() - 1).all(|i| records[i].priority() <= records[i + 1].priority()));
        Ok(())
    }

    #[tokio::test]
    async fn get_fresh_uris() -> Result<(), ResolveError> {
        use crate::client::{policy::Affinity, SrvClient};
        let resolver = AsyncResolver::tokio_from_system_conf().await?;
        let client = SrvClient::<_, Affinity>::new_with_resolver(crate::EXAMPLE_SRV, resolver);
        assert_ne!(
            client.get_fresh_uri_candidates().await.unwrap().0,
            Vec::<http::Uri>::new()
        );
        Ok(())
    }

    #[tokio::test]
    async fn invalid_host() {
        AsyncResolver::tokio_from_system_conf()
            .await
            .unwrap()
            .get_srv_records("_http._tcp.foobar.deshaw.com")
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn malformed_srv_name() {
        AsyncResolver::tokio_from_system_conf()
            .await
            .unwrap()
            .get_srv_records("_http.foobar.deshaw.com")
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn very_malformed_srv_name() {
        AsyncResolver::tokio_from_system_conf()
            .await
            .unwrap()
            .get_srv_records("  @#*^[_hsd flt.com")
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn srv_name_containing_nul_terminator() {
        AsyncResolver::tokio_from_system_conf()
            .await
            .unwrap()
            .get_srv_records("_http.\0_tcp.foo.com")
            .await
            .unwrap_err();
    }
}
