//! Caches for SRV record targets.

use std::time::Instant;

#[derive(Debug)]
/// A cache of items valid for a limited period of time.
pub struct Cache<T> {
    valid_until: Instant,
    items: Box<[T]>,
}

impl<T> Cache<T> {
    /// Creates a new cache of items valid until some time.
    pub fn new(items: impl Into<Box<[T]>>, valid_until: Instant) -> Self {
        let items = items.into();
        Self { valid_until, items }
    }

    /// Determines if a cache is valid.
    pub fn valid(&self) -> bool {
        !self.items.is_empty() && Instant::now() <= self.valid_until
    }

    /// Gets the items stored in a cache.
    pub fn items(&self) -> &[T] {
        &self.items
    }
}

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self::new(Vec::new(), Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn default_is_invalid() {
        assert!(!Cache::<()>::default().valid());
    }

    #[test]
    fn empty_is_invalid() {
        let cache = Cache::<()>::new(vec![], Instant::now() + Duration::from_secs(1));
        assert!(!cache.valid());
    }

    #[test]
    fn expired_is_invalid() {
        let cache = Cache::new(vec![()], Instant::now() - Duration::from_secs(1));
        assert!(!cache.valid());
    }

    #[test]
    fn nonempty_and_fresh_is_valid() {
        let cache = Cache::new(vec![()], Instant::now() + Duration::from_secs(1));
        assert!(cache.valid());
    }
}
