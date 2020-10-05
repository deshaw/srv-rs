#![warn(missing_docs)]

/*!
Rust client for communicating with services located by DNS SRV records.

# Introduction

SRV Records, as defined in [RFC 2782](https://tools.ietf.org/html/rfc2782),
are DNS records of the form

`_Service._Proto.Name TTL Class SRV Priority Weight Port Target`

For instance, a DNS server might respond with the following SRV records for
`_http._tcp.example.com`:

```text
_http._tcp.example.com. 60 IN SRV 1 100 8000 test.example.com.
_http._tcp.example.com. 60 IN SRV 2 50  8001 test.example.com.
_http._tcp.example.com. 60 IN SRV 2 50  8002 test.example.com.
```

A client wanting to communicate with this example service would first try to
communicate with `test.example.com:8000` (the record with the lowest
priority), then with the other two (in a random order, since they are of the
same priority) should the first be unavailable.

`srv-rs` handles the lookup and caching of SRV records as well as the ordered
selection of targets to use for communication with SRV-located services.
It presents this service in the following interface:

```
# #[tokio::main]
# async fn main() {
use srv_rs::{client::{SrvClient, Execution}, resolver::libresolv::LibResolv};
let client = SrvClient::<LibResolv>::new("_http._tcp.example.com");
client.execute_one(Execution::Serial, |address: http::Uri| async move {
    // Communicate with the service at `address`
    // `hyper` is used here as an example, but it is in no way required
    hyper::Client::new().get(address).await
})
.await;
# }
```

[`SrvClient::new`] creates a client (that should be reused to take advantage of
caching) for communicating with the service located by `_http._tcp.example.com`.
The [`execute`] macro takes in a future-producing closure (emulating async
closures, which are currently unstable) and executes the closure on a series of
targets parsed from the discovered SRV records, stopping and returning the
first `Ok` or last `Err` it obtains.

# Alternative Resolvers and Target Selection Policies

By default, `srv-rs` makes use of `libresolv` for SRV lookup and uses a
target selection policy that maintains affinity for the last target it has used
successfully. Both of these behaviors can be changed by implementing the
[`SrvResolver`] and [`Policy`] traits, respectively.

[`SrvClient::new`]: client/struct.SrvClient.html#method.new
[`execute`]: macro.execute.html
[`SrvResolver`]: resolver/trait.SrvResolver.html
[`Policy`]: client/policy/trait.Policy.html
*/

pub mod client;

pub mod record;

pub mod resolver;

#[doc(hidden)]
pub const EXAMPLE_SRV: &str = "_http._tcp.srv-client-rust.deshaw.org";
