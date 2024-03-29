# srv-rs

[![Test Status](https://github.com/deshaw/srv-rs/workflows/Rust/badge.svg?event=push)](https://github.com/deshaw/srv-rs/actions)
[![Crate](https://img.shields.io/crates/v/srv-rs.svg)](https://crates.io/crates/srv-rs)

Rust client for communicating with services located by DNS SRV records.

## Introduction

SRV Records, as defined in [RFC 2782](https://tools.ietf.org/html/rfc2782),
are DNS records of the form

`_Service._Proto.Name TTL Class SRV Priority Weight Port Target`

For instance, a DNS server might respond with the following SRV records for
`_http._tcp.example.com`:

```
_http._tcp.example.com. 60 IN SRV 1 100 443 test1.example.com.
_http._tcp.example.com. 60 IN SRV 2 50  443 test2.example.com.
_http._tcp.example.com. 60 IN SRV 2 50  443 test3.example.com.
```

A client wanting to communicate with this example service would first try to
communicate with `test1.example.com:443` (the record with the lowest
priority), then with the other two (in a random order, since they are of the
same priority) should the first be unavailable.

`srv-rs` handles the lookup and caching of SRV records as well as the ordered
selection of targets to use for communication with SRV-located services.
It presents this service in the following interface:

```rust
use srv_rs::{SrvClient, Execution, resolver::libresolv::LibResolv};
let client = SrvClient::<LibResolv>::new("_http._tcp.example.com");
client.execute(Execution::Serial, |address: http::Uri| async move {
    // Communicate with the service at `address`
    // `hyper` is used here as an example, but it is in no way required
    hyper::Client::new().get(address).await
})
.await;
```

[`SrvClient::new`] creates a client (that should be reused to take advantage of
caching) for communicating with the service located by `_http._tcp.example.com`.
[`SrvClient::execute`] takes in a future-producing closure (emulating async
closures, which are currently unstable) and executes the closure on a series of
targets parsed from the discovered SRV records, stopping and returning the
first `Ok` or last `Err` it obtains.

## Alternative Resolvers and Target Selection Policies

`srv-rs` provides multiple resolver backends for SRV lookup and by default uses
a target selection policy that maintains affinity for the last target it has
used successfully. Both of these behaviors can be changed by implementing the
[`SrvResolver`] and [`Policy`] traits, respectively.

The provided resolver backends are enabled by the following features:

- `libresolv` (via [`LibResolv`])
- `trust-dns` (via [`trust_dns_resolver::AsyncResolver`])

[`SrvResolver`]: resolver::SrvResolver
[`Policy`]: policy::Policy
[`LibResolv`]: resolver::libresolv::LibResolv

## Usage

Add srv-rs to your dependencies in `Cargo.toml`, enabling at least one of
the DNS resolver backends (see [Alternative Resolvers](README.md#alternative-resolvers-and-target-selection-policies)).
`libresolv` is enabled here as an example, but it is not required.

```toml
[dependencies]
srv-rs = { version = "0.2.0", features = ["libresolv"] }
```

## Contributing

1. Clone the repo
2. Make some changes
3. Test: `cargo test --all-features`
4. Format: `cargo fmt`
5. Clippy: `cargo clippy --all-features --tests -- -Dclippy::all`
6. Bench: `cargo bench --all-features`
7. If modifying crate-level docs (`src/lib.rs`) or `README.tpl`, update `README.md`:
    1. `cargo install cargo-readme`
    2. `cargo readme > README.md`

We love contributions! Before you can contribute, please sign and submit this [Contributor License Agreement (CLA)](https://www.deshaw.com/oss/cla).
This CLA is in place to protect all users of this project.

## History

This project was contributed back to the community by the [D. E. Shaw group](https://www.deshaw.com/).

<p align="center">
    <a href="https://www.deshaw.com">
       <img src="https://www.deshaw.com/assets/logos/blue_logo_417x125.png" alt="D. E. Shaw Logo" height="75" >
    </a>
</p>
