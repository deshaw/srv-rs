//! SRV resolver backed by [`hickory_resolver`].

use super::SrvResolver;
use crate::SrvRecord;
use async_trait::async_trait;
use hickory_resolver::{
    name_server::ConnectionProvider, proto::rr::rdata::SRV, Name, ResolveError, Resolver,
};
use std::time::Instant;

#[async_trait]
impl<P> SrvResolver for Resolver<P>
where
    P: ConnectionProvider,
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
