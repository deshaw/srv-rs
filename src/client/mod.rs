//! Clients based on SRV lookups.

use crate::{record::SrvRecord, resolver::SrvResolver};
use arc_swap::ArcSwap;
use futures_util::{
    stream::{self, Stream, StreamExt},
    FutureExt,
};
use http::uri::{Scheme, Uri};
use std::{error::Error, future::Future, iter::FromIterator, sync::Arc, time::Duration};

mod cache;
use cache::Cache;

/// SRV target selection policies.
pub mod policy;

/// Errors encountered during SRV record resolution
#[derive(Debug, thiserror::Error)]
pub enum SrvError<Lookup: Error + 'static> {
    /// Srv lookup errors
    #[error("srv lookup error")]
    Lookup(Lookup),
    /// Srv record parsing errors
    #[error("building uri from srv record: {0}")]
    RecordParsing(#[from] http::Error),
}

/// Errors encountered by the SrvClient
#[derive(Debug, thiserror::Error)]
pub enum SrvClientError<Lookup: Error + 'static> {
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

/// Execution mode to use when performing an operation on SRV targets.
pub enum ExecutionMode {
    /// Operations are performed *serially* (i.e. one after the other).
    Serial,
    /// Operations are performed *concurrently* (i.e. all at once).
    /// Note that this does not imply parallelism--no additional tasks are spawned.
    Concurrent,
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
    ) -> Result<Arc<Cache<Policy::CacheItem>>, SrvError<Resolver::Error>> {
        match self.cache.load_full() {
            cache if cache.valid() => Ok(cache),
            _ => self.refresh_cache().await,
        }
    }

    /// Performs an operation on a client's SRV targets, producing a stream of
    /// results.
    pub async fn execute<'a, T, E: Error, Fut>(
        &'a self,
        mut func: impl FnMut(Uri) -> Fut + 'a,
        execution_mode: ExecutionMode,
    ) -> Result<impl Stream<Item = Result<T, E>> + 'a, SrvClientError<Resolver::Error>>
    where
        Fut: Future<Output = Result<T, E>> + 'a,
    {
        let cache = self.get_valid_cache_items().await?;
        let order = self.policy.order(cache.items());
        let func = {
            let cache = cache.clone();
            move |idx| {
                let candidate = Policy::cache_item_to_uri(&cache.items()[idx]);
                func(candidate.to_owned()).map(move |res| (idx, res))
            }
        };
        let results = match execution_mode {
            ExecutionMode::Serial => stream::iter(order).then(func).left_stream(),
            ExecutionMode::Concurrent => {
                stream::FuturesUnordered::from_iter(order.map(func)).right_stream()
            }
        };
        let results = results.map(move |(candidate_idx, result)| {
            let candidate = Policy::cache_item_to_uri(&cache.items()[candidate_idx]);
            match result {
                Ok(res) => {
                    #[cfg(feature = "log")]
                    tracing::info!(URI = %candidate, "execution attempt succeeded");
                    self.policy.note_success(candidate);
                    Ok(res)
                }
                Err(err) => {
                    #[cfg(feature = "log")]
                    tracing::info!(URI = %candidate, error = %err, "execution attempt failed");
                    self.policy.note_failure(candidate);
                    Err(err)
                }
            }
        });
        Ok(results)
    }

    /// Returns the first `Ok` from a stream of `Result` or the last `Err` if
    /// there are no `Ok`s.
    pub async fn first_successful_result<T, E: Error>(
        &self,
        results: impl Stream<Item = Result<T, E>>,
    ) -> Result<Result<T, E>, SrvClientError<Resolver::Error>> {
        pin_utils::pin_mut!(results);

        let mut last_error = None;
        while let Some(result) = results.next().await {
            match result {
                Ok(res) => return Ok(Ok(res)),
                Err(err) => last_error = Some(err),
            }
        }

        if let Some(err) = last_error {
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
/// let res = srv_rs::execute!(client, |address: http::Uri| async move {
///     Ok::<_, Infallible>(address.to_string())
/// }).await?;
/// assert!(res.is_ok());
///
/// let res = srv_rs::execute!(client, |address: http::Uri| async move {
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
        $crate::execute!($client => one, $f)
    };
    ($client:expr => $target:tt, $f:expr) => {
        $crate::execute!($client => $target, $crate::client::ExecutionMode::Serial, $f)
    };
    ($client:expr, $mode:expr, $f:expr) => {
        $crate::execute!($client => one, $mode, $f)
    };
    ($client:expr => one, $mode:expr, $f:expr) => {
        async {
            let results = $crate::execute!($client => many, $mode, $f).await?;
            $client.first_successful_result(results).await
        }
    };
    ($client:expr => many, $mode:expr, $f:expr) => {
        $client.execute($f, $mode)
    };
}
