//! Clients based on SRV lookups.

use crate::{record::SrvRecord, resolver::SrvResolver};
use arc_swap::ArcSwap;
use futures_util::{
    stream::{self, Stream, StreamExt},
    task::{Context, Poll},
    FutureExt,
};
use http::uri::{Scheme, Uri};
use std::{
    error::Error, fmt::Debug, future::Future, iter::FromIterator, pin::Pin, sync::Arc,
    time::Duration,
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
    #[error(transparent)]
    NoTargets(#[from] NoSRVTargets),
}

/// Error produced when there are no SRV targets to use--i.e., when no SRV
/// records were found.
#[derive(Debug, thiserror::Error)]
#[error("no SRV targets to use")]
pub struct NoSRVTargets;

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

impl Default for ExecutionMode {
    fn default() -> Self {
        Self::Serial
    }
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
    pub async fn execute<'a, T, E: Error, Fut>(
        &'a self,
        execution_mode: ExecutionMode,
        func: impl FnMut(Uri) -> Fut + 'a,
    ) -> Result<ExecuteResults<impl Stream<Item = Result<T, E>> + 'a>, SrvError<Resolver::Error>>
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
        Ok(ExecuteResults(results))
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

/// Stream of results.
pub struct ExecuteResults<S>(S);

impl<S: Stream> Stream for ExecuteResults<S> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // This is okay because the inner stream is pinned when `self` is.
        let stream = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
        stream.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<S> ExecuteResults<S> {
    /// Returns the first `Result::Ok` or the last `Result::Err` in a stream
    /// of results, or `None` if there are no results at all.
    pub async fn first_success<T, E>(self) -> Result<Result<T, E>, NoSRVTargets>
    where
        S: Stream<Item = Result<T, E>>,
    {
        let results = self;
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
            Err(NoSRVTargets)
        }
    }
}

/// Performs an operation on a [`SrvClient`]'s SRV targets.
///
/// Operations must implement `FnMut(Uri) -> Result<T, E>`.
///
/// # Examples
///
/// ```
/// # use srv_rs::{EXAMPLE_SRV, client::{SrvClient, SrvError}};
/// # use srv_rs::resolver::libresolv::{LibResolv, LibResolvError};
/// # use std::convert::Infallible;
/// # #[tokio::main]
/// # async fn main() -> Result<(), SrvError<LibResolvError>> {
/// let client = SrvClient::<LibResolv>::new(EXAMPLE_SRV);
///
/// let res = srv_rs::execute!(client, |address: http::Uri| async move {
///     Ok::<_, Infallible>(address.to_string())
/// })
/// .await?;
/// assert!(res.is_ok());
///
/// let res = srv_rs::execute!(client, |address| async move {
///     address.to_string().parse::<usize>()
/// })
/// .await?;
/// assert!(res.is_err());
/// # Ok(())
/// # }
/// ```
///
/// ## SRV Target Selection Policies
///
/// SRV target selection order is determined by a [`SrvClient`]'s [`Policy`],
/// and can be set on client construction:
///
/// ```
/// # use srv_rs::{EXAMPLE_SRV, resolver::libresolv::LibResolv};
/// # use srv_rs::client::{SrvClient, policy::Rfc2782};
/// # fn main() {
/// let client = SrvClient::<LibResolv>::new(EXAMPLE_SRV).policy(Rfc2782);
/// # }
/// ```
///
/// ## Execution Modes
///
/// By default, the operation will be executed on SRV targets *serially* (i.e.
/// one after another). To execute the operations *concurrently*, an [`ExecutionMode`]
/// can be specified as the second argument:
///
/// ```
/// # use srv_rs::{EXAMPLE_SRV, client::{SrvClient, SrvError}};
/// # use srv_rs::resolver::libresolv::{LibResolv, LibResolvError};
/// # use std::convert::Infallible;
/// # #[tokio::main]
/// # async fn main() -> Result<(), SrvError<LibResolvError>> {
/// # let client = SrvClient::<LibResolv>::new(EXAMPLE_SRV);
/// use srv_rs::client::ExecutionMode;
/// let res = srv_rs::execute!(client, ExecutionMode::Concurrent, |address| async move {
///     Ok::<_, Infallible>(address.to_string())
/// })
/// .await?;
/// assert!(res.is_ok());
/// # Ok(())
/// # }
/// ```
///
/// **Note:** *concurrent* does not imply *parallel*--no tasks are spawned in
/// the concurrent execution mode.
///
/// ## Streaming Results
///
/// By default, `execute` will return the first sucessful result produced by
/// the operation or the last unsuccessful one if there were no successes.
/// To get a [`Stream`] of results, the `=> stream` syntax can be used:
///
/// ```
/// # use srv_rs::{EXAMPLE_SRV, client::{SrvClient, SrvError}};
/// # use srv_rs::resolver::libresolv::{LibResolv, LibResolvError};
/// # use std::convert::Infallible;
/// # #[tokio::main]
/// # async fn main() -> Result<(), SrvError<LibResolvError>> {
/// # let client = SrvClient::<LibResolv>::new(EXAMPLE_SRV);
/// let results_stream = srv_rs::execute!(client => stream, |address| async move {
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
/// [`ExecutionMode`]: client/enum.ExecutionMode.html
/// [`SrvClient`]: client/struct.SrvClient.html
/// [`Policy`]: client/policy/trait.Policy.html
/// [`Stream`]: ../futures_core/stream/trait.Stream.html
#[macro_export]
macro_rules! execute {
    ($client:expr, $f:expr) => {
        $crate::execute!($client, Default::default(), $f)
    };
    ($client:expr, $mode:expr, $f:expr) => {
        async {
            match $crate::execute!($client => stream, $mode, $f).await {
                Ok(results) => results.first_success().await.map_err(Into::into),
                Err(e) => Err(e),
            }
        }
    };
    ($client:expr => stream, $f:expr) => {
        $client.execute(Default::default(), $f)
    };
    ($client:expr => stream, $mode:expr, $f:expr) => {
        $client.execute($mode, $f)
    };
}
