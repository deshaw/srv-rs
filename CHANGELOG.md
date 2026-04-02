# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 1.0.0 - 2026-04-08

### Added

- `hickory` resolver backend via `hickory-resolver` (replaces `trust-dns`)
- Static resolver backend via `resolver::manual::StaticResolver`
- `Clone`, `Copy`, `PartialEq`, `Eq`, and `Hash` derived on most public types
- Integration test harness with sandboxed DNS via bubblewrap

### Changed

- Removed `trust-dns` resolver backend and feature flag, please use `hickory` instead
- `SrvClient::new` and builder methods now accept `impl Into<String>` instead of `&impl ToString`
- Replaced `libresolv-sys` with `resolv` crate for the `libresolv` backend
- Upgraded to Rust edition 2024
- Updated dependencies
- MSRV is now 1.85

### Fixed

- `Error::Lookup` now correctly exposes its inner error

## 0.2.0 - 2020-12-18

### Changed

- Flattened `crate::client` and `crate::record` modules in public API
- Renamed `SrvError` to `Error`
- Exported `Cache` from `crate::policy`
- Hid empty `crate::resolver::trust_dns` module
- Updated README contributing section and `crates.io` dependency

## 0.1.1 - 2020-12-18

### Changed

- Author email in Cargo.toml

### Fixed

- `docs.rs`: document with `--all-features`

## 0.1.0 - 2020-12-18

### Added

- Abstraction of SRV records
- Abstraction of SRV DNS resolvers
  - `libresolv` and `trust-dns`-based implementations
- Abstraction of SRV target selection policies
- Client for communicating with SRV-located services
