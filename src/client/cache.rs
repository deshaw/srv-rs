//! Caches for SRV record targets.

use arc_swap::Guard;
use std::{
    ops::Deref,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct Cache<T> {
    created: Instant,
    max_age: Duration,
    items: Vec<T>,
}

impl<T> Cache<T> {
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

    pub fn items(&self) -> &[T] {
        &self.items
    }
}

impl<T> Default for Cache<T> {
    fn default() -> Self {
        Self::new(Vec::new(), Duration::new(0, 0))
    }
}

pub struct CacheItemsHandle<'a, T>(Guard<'a, Arc<Cache<T>>>);

impl<'a, T> Deref for CacheItemsHandle<'a, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0.items
    }
}
