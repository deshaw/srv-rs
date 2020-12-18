//! Clients based on SRV lookups.

use crate::{SrvRecord, SrvResolver};
use arc_swap::ArcSwap;
use futures_util::{
    pin_mut,
    stream::{self, Stream, StreamExt},
    FutureExt,
};
use http::uri::{Scheme, Uri};
use std::{error::Error, fmt::Debug, future::Future, iter::FromIterator, sync::Arc, time::Instant};

pub mod cache;
use cache::Cache;

/// SRV target selection policies.
pub mod policy;

/// Errors encountered during SRV record resolution
#[derive(Debug, thiserror::Error)]
pub enum SrvError<Lookup: Debug> {
    /// SRV lookup errors
    #[error("SRV lookup error")]
    Lookup(Lookup),
    /// SRV record parsing errors
    #[error("building uri from SRV record: {0}")]
    RecordParsing(#[from] http::Error),
    /// Produced when there are no SRV targets for a client to use
    #[error("no SRV targets to use")]
    NoTargets,
}

/// Client for intelligently performing operations on a service located by SRV records.
///
/// # Usage
///
/// After being created by [`SrvClient::new`] or [`SrvClient::new_with_resolver`],
/// operations can be performed on the service pointed to by a [`SrvClient`] with
/// the [`execute`] and [`execute_stream`] methods.
///
/// ## DNS Resolvers
///
/// The resolver used to lookup SRV records is determined by a client's
/// [`SrvResolver`], and can be set with [`SrvClient::resolver`].
///
/// ## SRV Target Selection Policies
///
/// SRV target selection order is determined by a client's [`Policy`],
/// and can be set with [`SrvClient::policy`].
///
/// [`execute`]: SrvClient::execute()
/// [`execute_stream`]: SrvClient::execute_stream()
/// [`Policy`]: policy::Policy
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
    /// # use srv_rs::EXAMPLE_SRV;
    /// use srv_rs::{client::SrvClient, resolver::libresolv::LibResolv};
    /// let client = SrvClient::<LibResolv>::new("_http._tcp.example.com");
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
    /// Gets a fresh set of SRV records from a client's DNS resolver, returning
    /// them along with the time they're valid until.
    pub async fn get_srv_records(
        &self,
    ) -> Result<(Vec<Resolver::Record>, Instant), SrvError<Resolver::Error>> {
        self.resolver
            .get_srv_records(&self.srv)
            .await
            .map_err(SrvError::Lookup)
    }

    /// Gets a fresh set of SRV records from a client's DNS resolver and parses
    /// their target/port pairs into URIs, which are returned along with the
    /// time they're valid until--i.e., the time a cache containing these URIs
    /// should expire.
    pub async fn get_fresh_uri_candidates(
        &self,
    ) -> Result<(Vec<Uri>, Instant), SrvError<Resolver::Error>> {
        // Query DNS for the SRV record
        let (records, valid_until) = self.get_srv_records().await?;

        // Create URIs from SRV records
        let uris = records
            .iter()
            .map(|record| self.parse_record(record))
            .collect::<Result<Vec<Uri>, _>>()?;

        Ok((uris, valid_until))
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

    /// Performs an operation on all of a client's SRV targets, producing a
    /// stream of results (one for each target). If the serial execution mode is
    /// specified, the operation will be performed on each target in the order
    /// determined by the current [`Policy`], and the results will be returned
    /// in the same order. If the concurrent execution mode is specified, the
    /// operation will be performed on all targets concurrently, and results
    /// will be returned in the order they become available.
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
    /// let results_stream = client.execute_stream(Execution::Serial, |address| async move {
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
    ///
    /// [`Policy`]: policy::Policy
    pub async fn execute_stream<'a, T, E: Error, Fut>(
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
            #[allow(clippy::from_iter_instead_of_collect)]
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
    /// let res = client.execute(Execution::Serial, |address| async move {
    ///     Ok::<_, Infallible>(address.to_string())
    /// })
    /// .await?;
    /// assert!(res.is_ok());
    ///
    /// let res = client.execute(Execution::Concurrent, |address| async move {
    ///     address.to_string().parse::<usize>()
    /// })
    /// .await?;
    /// assert!(res.is_err());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute<T, E: Error, Fut>(
        &self,
        execution_mode: Execution,
        func: impl FnMut(Uri) -> Fut,
    ) -> Result<Result<T, E>, SrvError<Resolver::Error>>
    where
        Fut: Future<Output = Result<T, E>>,
    {
        let results = self.execute_stream(execution_mode, func).await?;
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
    /// # use srv_rs::{EXAMPLE_SRV, };
    /// use srv_rs::{client::{SrvClient, policy::Rfc2782}, resolver::libresolv::LibResolv};
    /// let client = SrvClient::<LibResolv>::new(EXAMPLE_SRV).policy(Rfc2782);
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
