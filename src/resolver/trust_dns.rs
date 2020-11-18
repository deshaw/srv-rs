//! SRV resolver backed by `trust-dns-resolver`.

use super::SrvResolver;
use crate::record::SrvRecord;
use async_trait::async_trait;
use std::time::Instant;
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
    type Record = SRV;
    type Error = ResolveError;

    async fn get_srv_records_unordered(
        &self,
        srv: &str,
    ) -> Result<(Vec<Self::Record>, Instant), Self::Error> {
        let lookup = self.srv_lookup(srv).await?;
        let valid_until = lookup.as_lookup().valid_until();
        Ok((lookup.into_iter().collect(), valid_until))
    }
}

impl SrvRecord for SRV {
    type Target = Name;

    fn target(&self) -> &Self::Target {
        self.target()
    }

    fn port(&self) -> u16 {
        self.port()
    }

    fn priority(&self) -> u16 {
        self.priority()
    }

    fn weight(&self) -> u16 {
        self.weight()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn srv_lookup() -> Result<(), ResolveError> {
        let (records, _) = AsyncResolver::tokio_from_system_conf()
            .await?
            .get_srv_records_unordered(crate::EXAMPLE_SRV)
            .await?;
        assert_ne!(records.len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn srv_lookup_ordered() -> Result<(), ResolveError> {
        let (records, _) = AsyncResolver::tokio_from_system_conf()
            .await?
            .get_srv_records(crate::EXAMPLE_SRV)
            .await?;
        assert_ne!(records.len(), 0);
        assert!((0..records.len() - 1).all(|i| records[i].priority() <= records[i + 1].priority()));
        Ok(())
    }

    #[tokio::test]
    async fn get_fresh_uris() -> Result<(), ResolveError> {
        let resolver = AsyncResolver::tokio_from_system_conf().await?;
        let client = crate::client::SrvClient::<_>::new_with_resolver(crate::EXAMPLE_SRV, resolver);
        let (uris, _) = client.get_fresh_uri_candidates().await.unwrap();
        assert_ne!(uris, Vec::<http::Uri>::new());
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
