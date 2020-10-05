use crate::client::{Cache, SrvClient, SrvError, SrvRecord, SrvResolver};
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use http::Uri;
use std::sync::Arc;

/// Policy for [`SrvClient`] to use when selecting SRV targets to recommend.
///
/// [`SrvClient`]: ../struct.SrvClient.html
#[async_trait]
pub trait Policy: Sized {
    /// Type of item stored in a client's cache.
    type CacheItem;

    /// Obtains a refreshed cache for a client.
    async fn refresh_cache<Resolver: SrvResolver>(
        &self,
        client: &SrvClient<Resolver, Self>,
    ) -> Result<Cache<Self::CacheItem>, SrvError<Resolver::Error>>;

    /// Makes any policy adjustments following a successful execution on `uri`.
    #[allow(unused_variables)]
    fn note_success(&self, uri: &Uri) {}

    /// Makes any policy adjustments following a failed execution on `uri`.
    #[allow(unused_variables)]
    fn note_failure(&self, uri: &Uri) {}
}

/// Ability of a [`Policy`] to define an ordering of SRV targets to
/// select from the items of a [`SrvClient`]'s cache.
///
/// When [Generic Associated Types](https://github.com/rust-lang/rust/issues/44265)
/// are stable, this should be combined with [`Policy`] by adding an associated
/// type `Policy::UriIter<'a>` that is produced by a method `Policy::uris`.
///
/// [`Policy`]: trait.Policy.html
/// [`SrvClient`]: ../struct.SrvClient.html
pub trait IntoUriIter<'a>: Policy {
    /// Kind of iterator produced.
    type Iter: Iterator<Item = &'a Uri>;

    /// Creates an iterator of `Uri`s in the order a [`SrvClient`] should use them.
    ///
    /// [`SrvClient`]: ../struct.SrvClient.html
    fn uris(&'a self, cached: &'a [Self::CacheItem]) -> Self::Iter;
}

/// Policy that selects targets based on past successes--if a target was used
/// successfully in a past execution, it will be recommended first.
#[derive(Default)]
pub struct Affinity {
    last_working_target: ArcSwapOption<Uri>,
}

#[async_trait]
impl Policy for Affinity {
    type CacheItem = Uri;

    async fn refresh_cache<Resolver: SrvResolver>(
        &self,
        client: &SrvClient<Resolver, Self>,
    ) -> Result<Cache<Self::CacheItem>, SrvError<Resolver::Error>> {
        let (uris, min_ttl) = client.get_fresh_uri_candidates().await?;
        Ok(Cache::new(uris, min_ttl))
    }

    fn note_success(&self, uri: &Uri) {
        self.last_working_target.store(Some(Arc::new(uri.clone())));
    }
}

impl<'a> IntoUriIter<'a> for Affinity {
    type Iter = AffinityUriIter<'a>;

    fn uris(&'a self, cached: &'a [Uri]) -> Self::Iter {
        let preferred = self.last_working_target.load();
        Affinity::uris_preferring(cached, preferred.as_deref())
    }
}

impl Affinity {
    fn uris_preferring<'a>(cached: &'a [Uri], preferred: Option<&Uri>) -> AffinityUriIter<'a> {
        let preferred = preferred
            .as_deref()
            .and_then(|preferred| cached.iter().position(|uri| uri == preferred))
            .unwrap_or(0);
        AffinityUriIter {
            uris: &cached,
            preferred,
            next: None,
        }
    }
}

/// Iterator over `Uri`s based on affinity. See [`Affinity`].
///
/// [`Affinity`]: struct.Affinity.html
pub struct AffinityUriIter<'a> {
    uris: &'a [Uri],
    /// Index of the URI to produce first (i.e. the preferred URI).
    /// `0` if the first is preferred or there is no preferred URI at all.
    preferred: usize,
    /// Index of the next URI to be produced.
    /// If `None`, the preferred URI will be produced.
    next: Option<usize>,
}

impl<'a> Iterator for AffinityUriIter<'a> {
    type Item = &'a Uri;

    fn next(&mut self) -> Option<Self::Item> {
        let (idx, next) = match self.next {
            // If no URIs have been produced, produce the preferred URI then go back to the first
            None => (self.preferred, 0),
            // If `preferred` is next, skip past it since it was produced already (`self.next != None`)
            Some(next) if next == self.preferred => (next + 1, next + 2),
            // Otherwise, advance normally
            Some(next) => (next, next + 1),
        };
        self.next = Some(next);
        self.uris.get(idx)
    }
}

/// Policy that selects targets based on the algorithm in RFC 2782, reshuffling
/// by weight for each selection.
#[derive(Default)]
pub struct Rfc2782;

/// Representation of a SRV record with its target and port parsed into a `Uri`.
pub struct ParsedRecord {
    uri: Uri,
    priority: u16,
    weight: u16,
}

impl ParsedRecord {
    fn new<Record: SrvRecord>(record: &Record, uri: Uri) -> Self {
        Self {
            uri,
            priority: record.priority(),
            weight: record.weight(),
        }
    }
}

#[async_trait]
impl Policy for Rfc2782 {
    type CacheItem = ParsedRecord;

    async fn refresh_cache<Resolver: SrvResolver>(
        &self,
        client: &SrvClient<Resolver, Self>,
    ) -> Result<Cache<Self::CacheItem>, SrvError<Resolver::Error>> {
        let records = client.get_srv_records().await?;
        let parsed = records
            .iter()
            .map(|record| {
                client
                    .parse_record(record)
                    .map(|uri| ParsedRecord::new(record, uri))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let min_ttl = records.iter().map(|record| record.ttl()).min();
        Ok(Cache::new(parsed, min_ttl.unwrap_or_default()))
    }
}

impl<'a> IntoUriIter<'a> for Rfc2782 {
    type Iter = Rfc2782UriIter<'a, <Vec<usize> as IntoIterator>::IntoIter>;

    fn uris(&'a self, cached: &'a [ParsedRecord]) -> Self::Iter {
        let mut indices = (0..cached.len()).collect::<Vec<_>>();
        let mut rng = rand::thread_rng();
        indices.sort_by_cached_key(|&idx| {
            let (priority, weight) = (cached[idx].priority, cached[idx].weight);
            crate::record::sort_key(priority, weight, &mut rng)
        });
        Rfc2782UriIter {
            uris: cached,
            order: indices.into_iter(),
        }
    }
}

/// Iterator over `Uri`s with order determined by the algorithm in RFC 2782.
/// See [`Rfc2782`].
///
/// [`Rfc2782`]: struct.Rfc2782.html
pub struct Rfc2782UriIter<'a, T: Iterator<Item = usize>> {
    uris: &'a [ParsedRecord],
    order: T,
}

impl<'a, T: Iterator<Item = usize>> Iterator for Rfc2782UriIter<'a, T> {
    type Item = &'a Uri;

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.order.next()?;
        self.uris.get(idx).map(|parsed| &parsed.uri)
    }
}

#[test]
fn affinity_uris_iter_order() {
    let google: Uri = "https://google.com".parse().unwrap();
    let amazon: Uri = "https://amazon.com".parse().unwrap();
    let desco: Uri = "https://deshaw.com".parse().unwrap();
    let cache = vec![google.clone(), amazon.clone(), desco.clone()];
    assert_eq!(
        Affinity::uris_preferring(&cache, None).collect::<Vec<_>>(),
        vec![&google, &amazon, &desco]
    );
    assert_eq!(
        Affinity::uris_preferring(&cache, Some(&google)).collect::<Vec<_>>(),
        vec![&google, &amazon, &desco]
    );
    assert_eq!(
        Affinity::uris_preferring(&cache, Some(&amazon)).collect::<Vec<_>>(),
        vec![&amazon, &google, &desco]
    );
    assert_eq!(
        Affinity::uris_preferring(&cache, Some(&desco)).collect::<Vec<_>>(),
        vec![&desco, &google, &amazon]
    );
}

#[test]
fn balance_uris_iter_order() {
    // Clippy doesn't like that Uri has interior mutability and is being used
    // as a HashMap key but we aren't doing anything naughty in the test
    #[allow(clippy::mutable_key_type)]
    let mut priorities = std::collections::HashMap::new();
    priorities.insert("https://google.com".parse::<Uri>().unwrap(), 2);
    priorities.insert("https://cloudflare.com".parse().unwrap(), 2);
    priorities.insert("https://amazon.com".parse().unwrap(), 1);
    priorities.insert("https://deshaw.com".parse().unwrap(), 1);

    let cache = priorities
        .iter()
        .map(|(uri, &priority)| ParsedRecord {
            uri: uri.clone(),
            priority,
            weight: rand::random::<u8>() as u16,
        })
        .collect::<Vec<_>>();

    let ordered = |iter| {
        let mut last = None;
        for item in iter {
            if let Some(last) = last {
                assert!(priorities[last] <= priorities[item]);
            }
            last = Some(item);
        }
    };

    for _ in 0..5 {
        ordered(Rfc2782.uris(&cache));
    }
}
