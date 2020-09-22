//! Clients based on SRV lookups.

use crate::{record::SrvRecord, resolver::SrvResolver};
use arc_swap::{ArcSwap, Guard};
use http::uri::{Scheme, Uri};
use std::{future::Future, ops::Deref, sync::Arc, time::Duration};

mod cache;
use cache::Cache;

/// SRV target selection policies.
pub mod policy;
use policy::IntoUriIter;

/// Errors encountered during SRV record resolution
#[derive(Debug, thiserror::Error)]
pub enum SrvError<Lookup: std::error::Error + 'static> {
    /// Srv lookup errors
    #[error("srv lookup error")]
    Lookup(Lookup),
    /// Srv record parsing errors
    #[error("building uri from srv record: {0}")]
    RecordParsing(#[from] http::Error),
}

/// Errors encountered by the SrvClient
#[derive(Debug, thiserror::Error)]
pub enum SrvClientError<Lookup: std::error::Error + 'static> {
    /// Produced when there are no URI candidates for a client to use
    #[error("no uri candidates")]
    NoUriCandidates,
    /// Srv resolution errors
    #[error(transparent)]
    Srv(#[from] SrvError<Lookup>),
}

/// Client for intelligently performing operations on a service located by SRV records.
#[derive(Debug)]
pub struct SrvClient<Resolver, Policy: policy::Policy = policy::Affinity> {
    srv: String,
    resolver: Resolver,
    http_scheme: Scheme,
    path_prefix: String,
    policy: Policy,
    cache: ArcSwap<Cache<Policy::CacheItem>>,
}

impl<Resolver: SrvResolver + Default, Policy: policy::Policy + Default>
    SrvClient<Resolver, Policy>
{
    /// Creates a new client for communicating with services located by `srv_name`.
    pub fn new(srv_name: impl ToString) -> Self {
        Self::new_with_resolver(srv_name, Resolver::default())
    }
}

impl<Resolver: SrvResolver, Policy: policy::Policy + Default> SrvClient<Resolver, Policy> {
    /// Creates a new client for communicating with services located by `srv_name`.
    pub fn new_with_resolver(srv_name: impl ToString, resolver: Resolver) -> Self {
        Self {
            srv: srv_name.to_string(),
            resolver,
            http_scheme: Scheme::HTTPS,
            path_prefix: String::from("/"),
            policy: Default::default(),
            cache: Default::default(),
        }
    }

    async fn get_srv_records(&self) -> Result<Vec<Resolver::Record>, SrvError<Resolver::Error>> {
        self.resolver
            .get_srv_records(&self.srv)
            .await
            .map_err(SrvError::Lookup)
    }

    async fn get_fresh_uri_candidates(
        &self,
    ) -> Result<(Vec<Uri>, Duration), SrvError<Resolver::Error>> {
        // Query DNS for the SRV record
        let records = self.get_srv_records().await?;

        // Create URIs from SRV records
        let min_ttl = records.iter().map(|r| r.ttl()).min().unwrap_or_default();
        let uris = records
            .iter()
            .map(|record| self.parse_record(record))
            .collect::<Result<Vec<Uri>, _>>()?;

        Ok((uris, min_ttl))
    }

    async fn refresh_cache(
        &self,
    ) -> Result<Arc<Cache<Policy::CacheItem>>, SrvError<Resolver::Error>> {
        let new_cache = Arc::new(self.policy.refresh_cache(self).await?);
        self.cache.store(new_cache.clone());
        Ok(new_cache)
    }

    /// Gets a client's cached items, refreshing the existing cache if it is invalid.
    pub async fn get_valid_cache_items(
        &self,
    ) -> Result<impl Deref<Target = [Policy::CacheItem]>, SrvError<Resolver::Error>> {
        let cache = match self.cache.load() {
            cache if cache.valid() => cache,
            _ => Guard::from_inner(self.refresh_cache().await?),
        };
        Ok(Cache::items(cache))
    }

    /// Determines an ordering of URIs to select based on the current [`Policy`].
    ///
    /// [`Policy`]: policy/trait.Policy.html
    pub fn uris<'a>(&'a self, cached: &'a [Policy::CacheItem]) -> <Policy as IntoUriIter<'a>>::Iter
    where
        Policy: IntoUriIter<'a>,
    {
        IntoUriIter::uris(&self.policy, cached)
    }

    /// Performs an operation serially for each base URI in an iterable,
    /// stopping and returning the first success or returning the last error.
    pub async fn execute_with<'a, F, T, E>(
        &self,
        mut func: impl FnMut(&'a Uri) -> F,
        candidates: impl IntoIterator<Item = &'a Uri>,
    ) -> Result<Result<T, E>, SrvClientError<Resolver::Error>>
    where
        E: std::error::Error,
        F: 'a + Future<Output = Result<T, E>>,
    {
        let mut err = None;
        for candidate in candidates {
            match func(candidate).await {
                Ok(res) => {
                    #[cfg(feature = "log")]
                    tracing::info!(URI = %candidate, "(serial) execution attempt succeeded");
                    self.policy.note_success(candidate);
                    return Ok(Ok(res));
                }
                Err(e) => {
                    #[cfg(feature = "log")]
                    tracing::info!(URI = %candidate, error = %e, "(serial) execution attempt failed");
                    self.policy.note_failure(candidate);
                    err = Some(e)
                }
            }
        }

        if let Some(err) = err {
            Ok(Err(err))
        } else {
            Err(SrvClientError::NoUriCandidates)
        }
    }

    fn parse_record(&self, record: &Resolver::Record) -> Result<Uri, http::Error> {
        record.parse(self.http_scheme.clone(), self.path_prefix.as_str())
    }

    /// Sets the SRV name of the client.
    pub fn srv_name(self, srv_name: impl ToString) -> Self {
        Self {
            srv: srv_name.to_string(),
            ..self
        }
    }

    /// Sets the resolver of the client.
    pub fn resolver<R>(self, resolver: R) -> SrvClient<R, Policy> {
        SrvClient {
            resolver,
            cache: Default::default(),
            policy: self.policy,
            srv: self.srv,
            http_scheme: self.http_scheme,
            path_prefix: self.path_prefix,
        }
    }

    /// Sets the policy of the client.
    pub fn policy<P: policy::Policy>(self, policy: P) -> SrvClient<Resolver, P> {
        SrvClient {
            policy,
            cache: Default::default(),
            resolver: self.resolver,
            srv: self.srv,
            http_scheme: self.http_scheme,
            path_prefix: self.path_prefix,
        }
    }

    /// Sets the http scheme of the client.
    pub fn http_scheme(self, http_scheme: Scheme) -> Self {
        Self {
            http_scheme,
            ..self
        }
    }

    /// Sets the path prefix of the client.
    pub fn path_prefix(self, path_prefix: impl ToString) -> Self {
        Self {
            path_prefix: path_prefix.to_string(),
            ..self
        }
    }
}

/// Perform some operation serially for each target server in the SRV record,
/// stopping and returning the first success or returning the last error.
///
/// # Examples
///
/// ```
/// # use srv_rs::{client::{SrvClient, SrvClientError}};
/// # use srv_rs::resolver::libresolv::{LibResolv, LibResolvError};
/// # use std::convert::Infallible;
/// # #[tokio::main]
/// # async fn main() -> Result<(), SrvClientError<LibResolvError>> {
/// let client = SrvClient::<LibResolv>::new("_http._tcp.srv-client-rust.deshaw.org");
///
/// let res = srv_rs::execute!(client, |address: &http::Uri| async move {
///     Ok::<_, Infallible>(address.to_string())
/// }).await?;
/// assert!(res.is_ok());
///
/// let res = srv_rs::execute!(client, |address: &http::Uri| async move {
///     address.to_string().parse::<usize>()
/// }).await?;
/// assert!(res.is_err());
/// # Ok(())
/// # }
/// ```
///
/// ## Custom Policy
/// ```
/// # use srv_rs::{client::{SrvClient, SrvClientError, policy::{Policy, Rfc2782}}};
/// # use srv_rs::resolver::libresolv::{LibResolv, LibResolvError};
/// # use std::convert::Infallible;
/// # #[tokio::main]
/// # async fn main() -> Result<(), SrvClientError<LibResolvError>> {
/// let client = SrvClient::<LibResolv>::new("_http._tcp.srv-client-rust.deshaw.org").policy(Rfc2782);
/// let res = srv_rs::execute!(client, |address| async move {
///     Ok::<_, Infallible>(address.to_string())
/// }).await?;
/// assert!(res.is_ok());
/// # Ok(())
/// # }
/// ```
#[macro_export]
macro_rules! execute {
    ($client:expr, $f:expr) => {
        async {
            let cache = $client.get_valid_cache_items().await?;
            $client.execute_with($f, $client.uris(&cache)).await
        }
    };
}
