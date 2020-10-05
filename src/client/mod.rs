//! Clients based on SRV lookups.

use crate::{record::SrvRecord, resolver::SrvResolver};
use arc_swap::ArcSwap;
use futures_util::{
    pin_mut,
    stream::{self, Stream, StreamExt},
    FutureExt,
};
use http::uri::{Scheme, Uri};
use std::{
    error::Error, fmt::Debug, future::Future, iter::FromIterator, sync::Arc, time::Duration,
};

mod cache;
use cache::Cache;

/// SRV target selection policies.
pub mod policy;

/// Errors encountered during SRV record resolution
#[derive(Debug, thiserror::Error)]
pub enum SrvError<Lookup: Debug> {
    /// Srv lookup errors
    #[error("srv lookup error")]
    Lookup(Lookup),
    /// Srv record parsing errors
    #[error("building uri from srv record: {0}")]
    RecordParsing(#[from] http::Error),
    /// Produced when there are no SRV targets for a client to use
    #[error("no SRV targets to use")]
    NoTargets,
}

/// Client for intelligently performing operations on a service located by SRV records.
///
/// ## SRV Target Selection Policies
///
/// SRV target selection order is determined by a client's [`Policy`],
/// and can be set with [`SrvClient::policy`].
///
/// [`Policy`]: policy/trait.Policy.html
/// [`SrvClient::policy`]: struct.SrvClient.html#method.policy
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
pub enum Execution {
    /// Operations are performed *serially* (i.e. one after the other).
    Serial,
    /// Operations are performed *concurrently* (i.e. all at once).
    /// Note that this does not imply parallelism--no additional tasks are spawned.
    Concurrent,
}

impl Default for Execution {
    fn default() -> Self {
        Self::Serial
    }
}

impl<Resolver: Default, Policy: policy::Policy + Default> SrvClient<Resolver, Policy> {
    /// Creates a new client for communicating with services located by `srv_name`.
    ///
    /// # Examples
    /// ```
    /// # use srv_rs::{EXAMPLE_SRV, client::SrvClient, resolver::libresolv::LibResolv};
    /// # fn main() {
    /// let client = SrvClient::<LibResolv>::new("_http._tcp.example.com");
    /// # }
    /// ```
    pub fn new(srv_name: impl ToString) -> Self {
        Self::new_with_resolver(srv_name, Resolver::default())
    }
}

impl<Resolver, Policy: policy::Policy + Default> SrvClient<Resolver, Policy> {
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
}

impl<Resolver: SrvResolver, Policy: policy::Policy> SrvClient<Resolver, Policy> {
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
    async fn get_valid_cache(
        &self,
    ) -> Result<Arc<Cache<Policy::CacheItem>>, SrvError<Resolver::Error>> {
        match self.cache.load_full() {
            cache if cache.valid() => Ok(cache),
            _ => self.refresh_cache().await,
        }
    }

    /// Performs an operation on a client's SRV targets, producing a stream of
    /// results.
    ///
    /// # Examples
    ///
    /// ```
    /// # use srv_rs::{EXAMPLE_SRV, client::{SrvClient, SrvError, Execution}};
    /// # use srv_rs::resolver::libresolv::{LibResolv, LibResolvError};
    /// # use std::convert::Infallible;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), SrvError<LibResolvError>> {
    /// # let client = SrvClient::<LibResolv>::new(EXAMPLE_SRV);
    /// let results_stream = client.execute(Execution::Serial, |address| async move {
    ///     Ok::<_, Infallible>(address.to_string())
    /// })
    /// .await?;
    /// // Do something with the stream, for example collect all results into a `Vec`:
    /// use futures::stream::StreamExt;
    /// let results: Vec<Result<_, _>> = results_stream.collect().await;
    /// for result in results {
    ///     assert!(result.is_ok());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute<'a, T, E: Error, Fut>(
        &'a self,
        execution_mode: Execution,
        func: impl FnMut(Uri) -> Fut + 'a,
    ) -> Result<impl Stream<Item = Result<T, E>> + 'a, SrvError<Resolver::Error>>
    where
        Fut: Future<Output = Result<T, E>> + 'a,
    {
        let mut func = func;
        let cache = self.get_valid_cache().await?;
        let order = self.policy.order(cache.items());
        let func = {
            let cache = cache.clone();
            move |idx| {
                let candidate = Policy::cache_item_to_uri(&cache.items()[idx]);
                func(candidate.to_owned()).map(move |res| (idx, res))
            }
        };
        let results = match execution_mode {
            Execution::Serial => stream::iter(order).then(func).left_stream(),
            Execution::Concurrent => {
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

    /// Performs an operation on a client's SRV targets, producing the first
    /// successful result or the last error encountered if every execution of
    /// the operation was unsuccessful.
    ///
    /// # Examples
    ///
    /// ```
    /// # use srv_rs::{EXAMPLE_SRV, client::{SrvClient, SrvError, Execution}};
    /// # use srv_rs::resolver::libresolv::{LibResolv, LibResolvError};
    /// # use std::convert::Infallible;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), SrvError<LibResolvError>> {
    /// let client = SrvClient::<LibResolv>::new(EXAMPLE_SRV);
    ///
    /// let res = client.execute_one(Execution::Serial, |address| async move {
    ///     Ok::<_, Infallible>(address.to_string())
    /// })
    /// .await?;
    /// assert!(res.is_ok());
    ///
    /// let res = client.execute_one(Execution::Concurrent, |address| async move {
    ///     address.to_string().parse::<usize>()
    /// })
    /// .await?;
    /// assert!(res.is_err());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_one<T, E: Error, Fut>(
        &self,
        execution_mode: Execution,
        func: impl FnMut(Uri) -> Fut,
    ) -> Result<Result<T, E>, SrvError<Resolver::Error>>
    where
        Fut: Future<Output = Result<T, E>>,
    {
        let results = self.execute(execution_mode, func).await?;
        pin_mut!(results);

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
            Err(SrvError::NoTargets)
        }
    }

    fn parse_record(&self, record: &Resolver::Record) -> Result<Uri, http::Error> {
        record.parse(self.http_scheme.clone(), self.path_prefix.as_str())
    }
}

impl<Resolver, Policy: policy::Policy> SrvClient<Resolver, Policy> {
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
    ///
    /// # Examples
    ///
    /// ```
    /// # use srv_rs::{EXAMPLE_SRV, resolver::libresolv::LibResolv};
    /// # use srv_rs::client::{SrvClient, policy::Rfc2782};
    /// # fn main() {
    /// let client = SrvClient::<LibResolv>::new(EXAMPLE_SRV).policy(Rfc2782);
    /// # }
    /// ```
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
