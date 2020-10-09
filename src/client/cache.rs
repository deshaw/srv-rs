//! Caches for SRV record targets.

use std::time::{Duration, Instant};

#[derive(Debug)]
/// A cache of items valid for a limited period of time.
pub struct Cache<T> {
    created: Instant,
    max_age: Duration,
    items: Vec<T>,
}

impl<T> Cache<T> {
    /// Creates a new cache of items valid for `max_age`.
    pub fn new(items: Vec<T>, max_age: Duration) -> Self {
        Self {
            created: Instant::now(),
            max_age,
            items,
        }
    }

    /// Determines if a cache is valid.
    pub fn valid(&self) -> bool {
        !self.items.is_empty() && self.created.elapsed() <= self.max_age
    }

    /// Gets the items stored in a cache.
    pub fn items(&self) -> &[T] {
        &self.items
    }
}

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self::new(Vec::new(), Duration::new(0, 0))
    }
}
