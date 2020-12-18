//! SRV records.

use http::uri::{PathAndQuery, Scheme, Uri};
use rand::Rng;
use std::{cmp::Reverse, convert::TryInto, fmt::Display};

/// Representation of types that contain the fields of a SRV record.
pub trait SrvRecord {
    /// Type representing the SRV record's target. Must implement `Display` so
    /// it can be used to create a `Uri`.
    type Target: Display + ?Sized;

    /// Gets a SRV record's target.
    fn target(&self) -> &Self::Target;

    /// Gets a SRV record's port.
    fn port(&self) -> u16;

    /// Gets a SRV record's priority.
    fn priority(&self) -> u16;

    /// Gets a SRV record's weight.
    fn weight(&self) -> u16;

    /// Parses a SRV record into a URI with a given scheme (e.g. https) and
    /// `path_and_query` (used as a suffix in the URI).
    ///
    /// ```
    /// # fn srv_record_parse() -> Result<(), http::Error> {
    /// use srv_rs::{resolver::libresolv::LibResolvSrvRecord, SrvRecord};
    /// let record = LibResolvSrvRecord {
    ///     priority: 1,
    ///     weight: 100,
    ///     port: 8211,
    ///     target: String::from("srv-client-rust.deshaw.org"),
    /// };
    /// assert_eq!(
    ///     &record.parse("https", "/")?.to_string(),
    ///     "https://srv-client-rust.deshaw.org:8211/"
    /// );
    /// assert_eq!(
    ///     &record.parse("http", "/bar")?.to_string(),
    ///     "http://srv-client-rust.deshaw.org:8211/bar"
    /// );
    /// # Ok(())
    /// # }
    /// ```
    fn parse(
        &self,
        scheme: impl TryInto<Scheme, Error = impl Into<http::Error>>,
        path_and_query: impl TryInto<PathAndQuery, Error = impl Into<http::Error>>,
    ) -> Result<Uri, http::Error> {
        let scheme: Scheme = scheme.try_into().map_err(Into::into)?;
        let path_and_query: PathAndQuery = path_and_query.try_into().map_err(Into::into)?;
        Uri::builder()
            .scheme(scheme)
            .path_and_query(path_and_query)
            .authority(format!("{}:{}", self.target(), self.port()).as_str())
            .build()
    }

    /// Generates a key to sort a SRV record by priority and weight per RFC 2782.
    fn sort_key(&self, rng: impl Rng) -> (u16, Reverse<u32>) {
        sort_key(self.priority(), self.weight(), rng)
    }
}

/// Generates a key to sort a SRV record by priority and weight per RFC 2782.
pub(crate) fn sort_key(priority: u16, weight: u16, mut rng: impl Rng) -> (u16, Reverse<u32>) {
    // Sort ascending by priority, then descending (hence `Reverse`) by randomized weight
    let rand = rng.gen::<u16>() as u32;
    (priority, Reverse(weight as u32 * rand))
}
